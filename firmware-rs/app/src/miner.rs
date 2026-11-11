//! Shared job board, statistics, share queues and persisted totals, plus the
//! portable software mining worker. Engines (software threads, the ESP32's
//! SHA peripheral, ASIC drivers) all report through `Miner`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use bdlminer_core::coin::Coin;
use bdlminer_core::safety::{self, SafetyLease};
use bdlminer_core::sha256::sha256d;
use bdlminer_core::stratum::{RejectReason, Share, ShareAck};
use bdlminer_core::work::{self, Job, Work};

use crate::platform::{Clock, Storage, CLOCK_SET};
use crate::settings::crc32;

pub const MAX_MINERS: usize = 3;
/// Hash rate history: one average every 5 minutes, last 24 hours.
pub const HISTORY_INTERVAL_S: u32 = 300;
pub const HISTORY_LEN: usize = 288;
pub const TOP_SHARES: usize = 10;
pub const BLOCK_BESTS: usize = 16;
const SHARE_LOG: usize = 20;
const SHARE_QUEUE: usize = 16;
const RATE_WINDOW: usize = 60;
const LIVE_WINDOW: usize = 10;
/// Candidates under this top-32-bit value (diff >= ~1.5e-5) are fully
/// evaluated: they feed the "best difficulty" statistic.
pub const BEST_DIFF_FILTER: u32 = 0x0000_ffff;
/// Software batch between two looks at the job board (~0.1 s on an ESP32).
pub const BATCH_SW: u32 = 4096;
pub const STATS_NAMESPACE: &str = "stats";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShareState {
    Pending,
    Accepted,
    Rejected,
    Unanswered,
}

impl ShareState {
    pub fn name(self) -> &'static str {
        match self {
            ShareState::Pending => "pending",
            ShareState::Accepted => "accepted",
            ShareState::Rejected => "rejected",
            ShareState::Unanswered => "unanswered",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ShareEntry {
    /// Unix time, 0 when the clock was not set.
    pub time: u32,
    pub difficulty: f32,
    pub state: ShareState,
    pub backup: bool,
    id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockState {
    Queued = 0,
    Submitted,
    ShareAccepted,
    ShareRejected,
    Unanswered,
    Stale,
    QueueFull,
}

impl BlockState {
    pub fn name(self) -> &'static str {
        ["queued", "submitted", "share_accepted", "share_rejected", "unanswered", "stale", "queue_full"][self as usize]
    }
    fn from_u8(v: u8) -> Option<Self> {
        use BlockState::*;
        [Queued, Submitted, ShareAccepted, ShareRejected, Unanswered, Stale, QueueFull].get(v as usize).copied()
    }
}

/// A locally verified block candidate (a pool ACK is not chain proof).
#[derive(Clone, Copy, Debug)]
pub struct BlockCandidate {
    pub number: u32,
    pub height: u32,
    /// Digest byte order; reverse for the usual block identifier.
    pub hash: [u8; 32],
    pub state: BlockState,
    pub previous_session: bool,
    pub coin: Coin,
    submission_id: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BestShare {
    pub difficulty: f64,
    pub time: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BlockBest {
    pub height: u32,
    pub best: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HistoryPoint {
    pub hashrate: f32,
    /// i16::MIN when no usable sample.
    pub temperature_deci_c: i16,
    pub power_percent: u8,
    pub coverage_percent: u8,
}

impl HistoryPoint {
    const OFF: HistoryPoint =
        HistoryPoint { hashrate: 0.0, temperature_deci_c: i16::MIN, power_percent: 0, coverage_percent: 0 };
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Profile {
    pub hashing_us: u64,
    pub rest_ms: u64,
    pub waiting_ms: u64,
    pub paused_ms: u64,
}

/// Lifetime statistics, also the backup format.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StatsSnapshot {
    pub best_diff: f64,
    pub hashes: u64,
    pub shares: u32,
    pub blocks: u32,
    pub seconds: u32,
    pub top: [BestShare; TOP_SHARES],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Throttle {
    Full,
    Half,
    Paused,
    Reduced,
}

/// A coherent copy of the statistics for the UI and the network.
#[derive(Clone, Debug, Default)]
pub struct MinerStats {
    pub hashes: [u64; MAX_MINERS],
    pub miner_count: usize,
    pub engine: [&'static str; MAX_MINERS],
    pub shares_found: u32,
    pub accepted: u32,
    pub rejected: u32,
    /// Sum of the pool difficulty of accepted shares, this boot.
    pub accepted_work: f64,
    pub rejected_reasons: [u32; RejectReason::COUNT],
    pub work_available: bool,
    pub version_mask: u32,
    /// Locally verified block candidates, not chain confirmations.
    pub blocks_found: u32,
    pub over32bits: u32,
    pub over64bits: u32,
    pub best_diff: f64,
    pub session_best_diff: f64,
    pub period_best_diff: f64,
    pub lifetime_hashes: u64,
    pub lifetime_shares: u32,
    pub lifetime_blocks: u32,
    pub lifetime_seconds: u32,
    pub block_height: u32,
    pub network_diff: f64,
    pub pool_diff: f64,
    pub pool_connected: bool,
    pub start_millis: u32,
}

/// Work with what is needed to check its results.
#[derive(Clone, Debug)]
pub struct MiningWork {
    pub work: Work,
    pub generation: u32,
    pub height: u32,
}

struct Board {
    job: Job,
    extranonce2: u64,
}

#[derive(Clone)]
struct Queued {
    share: Share,
    generation: u32,
}

#[derive(Default)]
struct Queues {
    regular: VecDeque<Queued>,
    /// Reserved: ordinary shares never displace a block candidate.
    blocks: VecDeque<Queued>,
}

struct Inner {
    stats: MinerStats,
    top: [BestShare; TOP_SHARES],
    block_bests: [BlockBest; BLOCK_BESTS],
    block_head: usize,
    candidates: VecDeque<BlockCandidate>,
    previous_candidate: Option<BlockCandidate>,
    candidate_revision: u32,
    saved_candidate_revision: u32,
    best_dirty: bool,
    last_hashes: [u64; MAX_MINERS],
    total_hashes: u64,
    rate_ring: VecDeque<(u32, [u64; MAX_MINERS])>,
    hashrate: f64,
    hashrate_live: f64,
    engine_rate: [f64; MAX_MINERS],
    last_tick_ms: u32,
    session_ms: u64,
    base: StatsSnapshot,
    saved_blocks: u32,
    history: VecDeque<HistoryPoint>,
    history_revision: u32,
    history_hashes: u64,
    history_ms: u32,
    history_saved_ms: u32,
    history_saved_revision: u32,
    temperature_sum: f32,
    temperature_samples: u32,
    power_sum: u32,
    observation_samples: u32,
    profiles: [Profile; MAX_MINERS],
    share_log: VecDeque<ShareEntry>,
    pool_connected_ms: u32,
    pool_since: u32,
    pool_sessions: u32,
    last_save: u32,
    retry_save_at: u32,
    stats_seq: u32,
    candidate_seq: u32,
    loaded: bool,
    frozen: bool,
    restore: Option<(u32, Vec<HistoryPoint>)>,
}

pub struct Miner {
    coin: Coin,
    clock: Arc<dyn Clock>,
    storage: Arc<Mutex<dyn Storage>>,
    board: Mutex<Board>,
    board_valid: AtomicBool,
    board_seq: AtomicU32,
    generation: AtomicU32,
    last_job_time: AtomicU32,
    queues: Mutex<Queues>,
    queue_signal: Condvar,
    inner: Mutex<Inner>,
    /// Per-engine hash counters, outside the lock for the hot path.
    hashes: [AtomicU64; MAX_MINERS],
    safety: SafetyLease,
    battery_limited: AtomicBool,
    ota_busy: AtomicBool,
    stop: AtomicBool,
    engines: AtomicU8,
    log: Box<dyn Fn(&str) + Send + Sync>,
}

impl Miner {
    pub fn new(
        coin: Coin,
        clock: Arc<dyn Clock>,
        storage: Arc<Mutex<dyn Storage>>,
        log: impl Fn(&str) + Send + Sync + 'static,
    ) -> Arc<Self> {
        let now = clock.millis();
        Arc::new(Miner {
            coin,
            board: Mutex::new(Board { job: Job::default(), extranonce2: 0 }),
            board_valid: AtomicBool::new(false),
            board_seq: AtomicU32::new(0),
            generation: AtomicU32::new(0),
            last_job_time: AtomicU32::new(0),
            queues: Mutex::new(Queues::default()),
            queue_signal: Condvar::new(),
            inner: Mutex::new(Inner {
                stats: MinerStats { start_millis: now, ..Default::default() },
                top: Default::default(),
                block_bests: Default::default(),
                block_head: 0,
                candidates: VecDeque::with_capacity(4),
                previous_candidate: None,
                candidate_revision: 0,
                saved_candidate_revision: 0,
                best_dirty: false,
                last_hashes: [0; MAX_MINERS],
                total_hashes: 0,
                rate_ring: VecDeque::with_capacity(RATE_WINDOW),
                hashrate: 0.0,
                hashrate_live: 0.0,
                engine_rate: [0.0; MAX_MINERS],
                last_tick_ms: now,
                session_ms: 0,
                base: StatsSnapshot::default(),
                saved_blocks: 0,
                history: VecDeque::with_capacity(HISTORY_LEN),
                history_revision: 0,
                history_hashes: 0,
                history_ms: 0,
                history_saved_ms: 0,
                history_saved_revision: 0,
                temperature_sum: 0.0,
                temperature_samples: 0,
                power_sum: 0,
                observation_samples: 0,
                profiles: Default::default(),
                share_log: VecDeque::with_capacity(SHARE_LOG),
                pool_connected_ms: 0,
                pool_since: 0,
                pool_sessions: 0,
                last_save: now,
                retry_save_at: now,
                stats_seq: 0,
                candidate_seq: 0,
                loaded: false,
                frozen: false,
                restore: None,
            }),
            hashes: Default::default(),
            safety: SafetyLease::default(),
            battery_limited: AtomicBool::new(false),
            ota_busy: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            engines: AtomicU8::new(0),
            clock,
            storage,
            log: Box::new(log),
        })
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn now(&self) -> u32 {
        self.clock.millis()
    }

    fn unix_time(&self) -> u32 {
        self.clock.unix_time().filter(|&t| t > CLOCK_SET).unwrap_or(0)
    }

    pub fn coin(&self) -> Coin {
        self.coin
    }

    /// Adds an engine ("sw", "hw", "BM1370"...); returns its stats index.
    pub fn register_engine(&self, name: &'static str) -> Option<usize> {
        let mut inner = self.inner();
        let i = inner.stats.miner_count;
        if i >= MAX_MINERS {
            return None;
        }
        inner.stats.engine[i] = name;
        inner.stats.miner_count += 1;
        self.engines.store(inner.stats.miner_count as u8, Ordering::Relaxed);
        Some(i)
    }

    /// Asks the workers to stop (host builds and tests).
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.queue_signal.notify_all();
    }

    pub fn stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    pub fn add_hashes(&self, engine: usize, count: u64) {
        if let Some(h) = self.hashes.get(engine) {
            h.fetch_add(count, Ordering::Relaxed);
        }
    }

    // ------------------------------------------------------------ job board

    /// Publishes a new job; `clean` makes the engines drop current work.
    pub fn publish_job(&self, job: &Job, clean: bool) {
        if !self.coin.job_compatible(job.version, job.version_mask) {
            self.invalidate();
            return;
        }
        let mut board = self.board.lock().unwrap();
        if clean {
            self.generation.fetch_add(1, Ordering::SeqCst);
            self.reset_queues();
        }
        board.job = job.clone();
        self.last_job_time.store(job.ntime, Ordering::Relaxed);
        let height = job.block_height(self.coin);
        {
            let mut inner = self.inner();
            inner.stats.block_height = height;
            let head = inner.block_head;
            if height != 0 && height != inner.block_bests[head].height {
                inner.block_head = (head + 1) % BLOCK_BESTS;
                let head = inner.block_head;
                inner.block_bests[head] = BlockBest { height, best: 0.0 };
            }
            inner.stats.network_diff = work::difficulty_from_bits(job.nbits);
            inner.stats.pool_diff = job.difficulty;
            inner.stats.work_available = true;
            inner.stats.version_mask = job.version_mask;
        }
        self.board_valid.store(true, Ordering::SeqCst);
        self.board_seq.fetch_add(1, Ordering::SeqCst);
    }

    /// Connection lost: engines idle until the next job.
    pub fn invalidate(&self) {
        let _board = self.board.lock().unwrap();
        self.board_valid.store(false, Ordering::SeqCst);
        {
            let mut inner = self.inner();
            inner.stats.work_available = false;
            inner.stats.version_mask = 0;
        }
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.reset_queues();
    }

    /// Called with the board lock held.
    fn reset_queues(&self) {
        let mut q = self.queues.lock().unwrap();
        q.regular.clear();
        q.blocks.clear();
        drop(q);
        let mut inner = self.inner();
        let mut changed = false;
        for c in inner.candidates.iter_mut().filter(|c| c.state == BlockState::Queued) {
            c.state = BlockState::Stale;
            changed = true;
        }
        if changed {
            inner.candidate_revision += 1;
        }
    }

    pub fn board_valid(&self) -> bool {
        self.board_valid.load(Ordering::SeqCst)
    }
    pub fn board_seq(&self) -> u32 {
        self.board_seq.load(Ordering::SeqCst)
    }
    /// ntime of the latest pool job (0 = none yet): a clock when NTP is blocked.
    pub fn job_time(&self) -> u32 {
        self.last_job_time.load(Ordering::Relaxed)
    }
    pub fn current(&self, generation: u32) -> bool {
        self.board_valid() && generation == self.generation.load(Ordering::SeqCst)
    }

    /// Fresh work with a unique extranonce2, and the board sequence it came from.
    pub fn fetch_work(&self) -> Option<(MiningWork, u32)> {
        let mut board = self.board.lock().unwrap();
        if !self.board_valid() {
            return None;
        }
        let seq = self.board_seq();
        let counter = board.extranonce2;
        board.extranonce2 = board.extranonce2.wrapping_add(1);
        let work = work::build(&board.job, counter);
        Some((
            MiningWork {
                work,
                generation: self.generation.load(Ordering::SeqCst),
                height: board.job.block_height(self.coin),
            },
            seq,
        ))
    }

    /// ASIC work: the chip ORs its rolling bits into the version, so only
    /// the negotiated bits are cleared.
    pub fn asic_work(&self) -> Option<MiningWork> {
        let (mut w, _) = self.fetch_work()?;
        let version = w.work.version() & !w.work.version_mask;
        w.work.header[..4].copy_from_slice(&version.to_le_bytes());
        Some(w)
    }

    /// One independent SHA-256d verification of an ASIC result. Returns its
    /// difficulty, or `None` for stale/invalid work.
    pub fn asic_candidate(&self, w: &MiningWork, nonce: u32, version_bits: u32, minimum: f64) -> Option<f64> {
        if !self.current(w.generation) || version_bits & !w.work.version_mask != 0 {
            return None;
        }
        let mut candidate = w.clone();
        let version = candidate.work.version() | version_bits;
        candidate.work.header[..4].copy_from_slice(&version.to_le_bytes());
        candidate.work.header[76..].copy_from_slice(&nonce.to_le_bytes());
        let hash = sha256d(&candidate.work.header);
        let difficulty = work::hash_difficulty(&hash);
        if !self.current(w.generation) {
            return None;
        }
        if difficulty >= minimum.max(1.0) {
            self.on_candidate(nonce, &hash, &candidate);
        }
        Some(difficulty)
    }

    /// A nonce whose hash passed an engine's filter.
    pub fn on_candidate(&self, nonce: u32, hash: &[u8; 32], w: &MiningWork) {
        let d = work::hash_difficulty(hash);
        let when = if d > self.inner().top[TOP_SHARES - 1].difficulty { self.unix_time() } else { 0 };
        {
            let mut inner = self.inner();
            let s = &mut inner.stats;
            s.session_best_diff = s.session_best_diff.max(d);
            s.period_best_diff = s.period_best_diff.max(d);
            if d > s.best_diff {
                s.best_diff = d;
                inner.best_dirty = true;
            }
            if d >= 1.0 {
                inner.stats.over32bits += 1;
            }
            // Exact bits of the little-endian digest, no float rounding.
            if hash[24..].iter().all(|&b| b == 0) {
                inner.stats.over64bits += 1;
            }
            for b in inner.block_bests.iter_mut() {
                if w.height != 0 && b.height == w.height && d > b.best {
                    b.best = d;
                }
            }
            if d > inner.top[TOP_SHARES - 1].difficulty {
                let mut i = TOP_SHARES - 1;
                while i > 0 && inner.top[i - 1].difficulty < d {
                    inner.top[i] = inner.top[i - 1];
                    i -= 1;
                }
                inner.top[i] = BestShare { difficulty: d, time: when };
            }
        }

        let ww = &w.work;
        let block = work::meets_target(hash, &ww.network_target);
        let share_ok =
            if ww.exact_share_target { work::meets_target(hash, &ww.share_target) } else { d >= ww.share_difficulty };
        if !share_ok && !block {
            return;
        }
        let share = Share {
            job_id: ww.job_id.clone(),
            extranonce2: ww.extranonce2_hex.clone(),
            ntime: ww.ntime,
            nonce,
            difficulty: d,
            pool_difficulty: ww.share_difficulty,
            version_mask: ww.version_mask,
            version: ww.version(),
            hash: *hash,
        };
        let queued = Queued { share, generation: w.generation };
        if block {
            // Serialized with clean jobs and disconnects: a batch finishing on
            // obsolete work must not announce a new block.
            let _board = self.board.lock().unwrap();
            if !self.current(w.generation) {
                return;
            }
            let mut queues = self.queues.lock().unwrap();
            let mut inner = self.inner();
            if inner.candidates.iter().any(|c| &c.hash == hash) {
                return;
            }
            let enqueued = queues.blocks.len() < 2;
            if enqueued {
                queues.blocks.push_back(queued);
                inner.stats.shares_found += 1;
            }
            inner.stats.blocks_found += 1;
            let candidate = BlockCandidate {
                number: inner.stats.blocks_found,
                height: w.height,
                hash: *hash,
                state: if enqueued { BlockState::Queued } else { BlockState::QueueFull },
                previous_session: false,
                coin: self.coin,
                submission_id: 0,
            };
            if inner.candidates.len() == 4 {
                inner.candidates.pop_front();
            }
            inner.candidates.push_back(candidate);
            inner.candidate_revision += 1;
            inner.stats.lifetime_blocks = inner.base.blocks + inner.stats.blocks_found;
            drop(inner);
            drop(queues);
            self.queue_signal.notify_all();
            (self.log)(&format!(
                "[miner] BLOCK CANDIDATE height={} hash={} job={} {}; chain confirmation unknown",
                w.height,
                work::reversed_hex(hash),
                ww.job_id,
                if enqueued { "queued with priority" } else { "NOT QUEUED: candidate queue full" }
            ));
        } else {
            let mut queues = self.queues.lock().unwrap();
            if queues.regular.len() < SHARE_QUEUE {
                queues.regular.push_back(queued);
                drop(queues);
                self.inner().stats.shares_found += 1;
                self.queue_signal.notify_all();
            }
        }
    }

    /// The next share for the pool: block candidates first. Shares of work
    /// that became obsolete meanwhile never reach the next session.
    pub fn next_share(&self, wait: Duration) -> Option<Share> {
        let mut queues = self.queues.lock().unwrap();
        let mut waited = false;
        loop {
            let next = queues.blocks.pop_front().or_else(|| queues.regular.pop_front());
            match next {
                Some(q) if q.generation == self.generation.load(Ordering::SeqCst) && self.board_valid() => {
                    return Some(q.share)
                }
                Some(_) => continue,
                None if waited || wait.is_zero() || self.stopping() => return None,
                None => {
                    queues = self.queue_signal.wait_timeout(queues, wait.min(Duration::from_millis(20))).unwrap().0;
                    waited = true;
                }
            }
        }
    }

    // ------------------------------------------------------- pool feedback

    pub fn pool_connected(&self, connected: bool) {
        let now = self.now();
        let mut inner = self.inner();
        if connected && !inner.stats.pool_connected {
            inner.pool_since = now;
            inner.pool_sessions += 1;
        } else if !connected && inner.stats.pool_connected {
            inner.pool_connected_ms = inner.pool_connected_ms.wrapping_add(now.wrapping_sub(inner.pool_since));
        }
        if !connected {
            // A lost connection takes its pending answers with it.
            for e in inner.share_log.iter_mut().filter(|e| e.state == ShareState::Pending) {
                e.state = ShareState::Unanswered;
            }
            let mut changed = false;
            for c in inner.candidates.iter_mut().filter(|c| c.state == BlockState::Submitted) {
                c.state = BlockState::Unanswered;
                changed = true;
            }
            if changed {
                inner.candidate_revision += 1;
            }
        }
        inner.stats.pool_connected = connected;
    }

    /// (connected ms since boot, sessions that reached mining)
    pub fn pool_uptime(&self) -> (u32, u32) {
        let now = self.now();
        let inner = self.inner();
        let current = if inner.stats.pool_connected { now.wrapping_sub(inner.pool_since) } else { 0 };
        (inner.pool_connected_ms.wrapping_add(current), inner.pool_sessions)
    }

    pub fn pool_difficulty(&self, difficulty: f64) {
        self.inner().stats.pool_diff = difficulty;
    }

    pub fn share_sent(&self, difficulty: f64, backup: bool, submission_id: u32, hash: Option<&[u8; 32]>) {
        let time = self.unix_time();
        let mut inner = self.inner();
        if inner.share_log.len() == SHARE_LOG {
            inner.share_log.pop_front();
        }
        inner.share_log.push_back(ShareEntry {
            time,
            difficulty: difficulty as f32,
            state: ShareState::Pending,
            backup,
            id: submission_id,
        });
        if let Some(hash) = hash {
            let mut changed = false;
            for c in inner.candidates.iter_mut().filter(|c| c.state == BlockState::Queued && &c.hash == hash) {
                c.submission_id = submission_id;
                c.state = BlockState::Submitted;
                changed = true;
            }
            if changed {
                inner.candidate_revision += 1;
            }
        }
    }

    pub fn share_result(&self, accepted: bool, ack: &ShareAck) {
        let mut inner = self.inner();
        let mut changed = false;
        for c in inner.candidates.iter_mut().filter(|c| c.state == BlockState::Submitted && c.submission_id == ack.id) {
            c.state = if accepted { BlockState::ShareAccepted } else { BlockState::ShareRejected };
            changed = true;
        }
        if changed {
            inner.candidate_revision += 1;
        }
        if let Some(e) = inner.share_log.iter_mut().find(|e| e.state == ShareState::Pending && e.id == ack.id) {
            e.state = if accepted { ShareState::Accepted } else { ShareState::Rejected };
        }
        if accepted {
            inner.stats.accepted += 1;
            // Credit the submitted target, never the latest one.
            inner.stats.accepted_work += ack.difficulty;
        } else {
            inner.stats.rejected += 1;
            inner.stats.rejected_reasons[ack.reason as usize] += 1;
        }
    }

    // ------------------------------------------------------- statistics

    pub fn stats(&self) -> MinerStats {
        let mut s = self.inner().stats.clone();
        for (i, h) in s.hashes.iter_mut().enumerate() {
            *h = self.hashes[i].load(Ordering::Relaxed);
        }
        s
    }

    pub fn hashrate(&self) -> f64 {
        self.inner().hashrate
    }
    pub fn hashrate_live(&self) -> f64 {
        self.inner().hashrate_live
    }
    pub fn engine_rate(&self, i: usize) -> f64 {
        self.inner().engine_rate.get(i).copied().unwrap_or(0.0)
    }
    pub fn total_hashes(&self) -> u64 {
        self.inner().total_hashes
    }
    pub fn uptime_seconds(&self) -> u32 {
        (self.inner().session_ms / 1000) as u32
    }
    pub fn share_log(&self) -> Vec<ShareEntry> {
        self.inner().share_log.iter().rev().copied().collect()
    }
    pub fn top_shares(&self) -> Vec<BestShare> {
        self.inner().top.iter().take_while(|t| t.difficulty > 0.0).copied().collect()
    }
    /// Your best share on each recent block height, newest first.
    pub fn block_bests(&self) -> Vec<BlockBest> {
        let inner = self.inner();
        (0..BLOCK_BESTS)
            .map(|i| inner.block_bests[(inner.block_head + BLOCK_BESTS - i) % BLOCK_BESTS])
            .filter(|b| b.height != 0)
            .collect()
    }
    /// The latest candidate, including a saved proof from an earlier boot.
    pub fn last_block_candidate(&self) -> Option<BlockCandidate> {
        let inner = self.inner();
        inner.candidates.back().copied().or(inner.previous_candidate)
    }
    pub fn history(&self) -> (Vec<HistoryPoint>, u32) {
        let inner = self.inner();
        (inner.history.iter().copied().collect(), inner.history_revision)
    }
    pub fn history_revision(&self) -> u32 {
        self.inner().history_revision
    }
    pub fn profiles(&self) -> [Profile; MAX_MINERS] {
        self.inner().profiles
    }
    /// The best share difficulty since the previous call.
    pub fn take_period_best(&self) -> f64 {
        std::mem::take(&mut self.inner().stats.period_best_diff)
    }

    pub fn observe(&self, temperature: f32, effective_power: u8) {
        let mut inner = self.inner();
        if temperature.is_finite() && temperature > -100.0 && temperature < 200.0 {
            inner.temperature_sum += temperature;
            inner.temperature_samples += 1;
        }
        inner.power_sum += effective_power.min(100) as u32;
        inner.observation_samples += 1;
    }

    fn profile(&self, engine: usize, f: impl FnOnce(&mut Profile)) {
        if let Some(p) = self.inner().profiles.get_mut(engine) {
            f(p);
        }
    }

    /// Once a second: rates, lifetime totals, history and periodic saves.
    pub fn tick(&self) {
        let now = self.now();
        let save = {
            let mut inner = self.inner();
            let elapsed = now.wrapping_sub(inner.last_tick_ms);
            inner.session_ms += elapsed as u64;
            let count = inner.stats.miner_count;
            let mut deltas = [0u64; MAX_MINERS];
            for (i, d) in deltas.iter_mut().enumerate().take(count) {
                let h = self.hashes[i].load(Ordering::Relaxed);
                *d = h.wrapping_sub(inner.last_hashes[i]);
                inner.last_hashes[i] = h;
            }
            let delta: u64 = deltas.iter().sum();
            inner.total_hashes += delta;
            if inner.rate_ring.len() == RATE_WINDOW {
                inner.rate_ring.pop_front();
            }
            inner.rate_ring.push_back((elapsed, deltas));
            let ms: u64 = inner.rate_ring.iter().map(|r| r.0 as u64).sum();
            if ms != 0 {
                let mut total = 0.0;
                for i in 0..MAX_MINERS {
                    let sum: u64 = inner.rate_ring.iter().map(|r| r.1[i]).sum();
                    inner.engine_rate[i] = sum as f64 * 1000.0 / ms as f64;
                    total += inner.engine_rate[i];
                }
                inner.hashrate = total;
            }
            let (live_ms, live_sum) = inner
                .rate_ring
                .iter()
                .rev()
                .take(LIVE_WINDOW)
                .fold((0u64, 0u64), |(m, s), r| (m + r.0 as u64, s + r.1.iter().sum::<u64>()));
            if live_ms != 0 {
                inner.hashrate_live = live_sum as f64 * 1000.0 / live_ms as f64;
            }
            inner.last_tick_ms = now;
            inner.stats.lifetime_hashes = inner.base.hashes + inner.total_hashes;
            inner.stats.lifetime_shares = inner.base.shares + inner.stats.accepted;
            inner.stats.lifetime_blocks = inner.base.blocks + inner.stats.blocks_found;
            inner.stats.lifetime_seconds = inner.base.seconds + (inner.session_ms / 1000) as u32;

            if inner.history_ms == 0 {
                inner.history_ms = now.max(1);
                inner.history_hashes = inner.total_hashes;
            } else if now.wrapping_sub(inner.history_ms) >= HISTORY_INTERVAL_S * 1000 {
                let span = now.wrapping_sub(inner.history_ms);
                let point = HistoryPoint {
                    hashrate: ((inner.total_hashes - inner.history_hashes) as f64 * 1000.0 / span as f64) as f32,
                    temperature_deci_c: if inner.temperature_samples != 0 {
                        (inner.temperature_sum * 10.0 / inner.temperature_samples as f32).round() as i16
                    } else {
                        i16::MIN
                    },
                    power_percent: inner.power_sum.checked_div(inner.observation_samples).unwrap_or(0) as u8,
                    coverage_percent: (inner.observation_samples as u64 * 100_000 / span as u64).min(100) as u8,
                };
                inner.temperature_sum = 0.0;
                inner.temperature_samples = 0;
                inner.power_sum = 0;
                inner.observation_samples = 0;
                if inner.history.len() == HISTORY_LEN {
                    inner.history.pop_front();
                }
                inner.history.push_back(point);
                inner.history_revision += 1;
                inner.history_ms = now;
                inner.history_hashes = inner.total_hashes;
            }
            // Flash wear: a block at once, a new best at most once a minute,
            // the totals every 10 minutes.
            (now.wrapping_sub(inner.retry_save_at) as i32) >= 0
                && (inner.stats.blocks_found != inner.saved_blocks
                    || inner.candidate_revision != inner.saved_candidate_revision
                    || (inner.best_dirty && now.wrapping_sub(inner.last_save) > 60_000)
                    || now.wrapping_sub(inner.last_save) > 600_000)
        };
        if save {
            let ok = self.save_stats(true);
            let mut inner = self.inner();
            if ok {
                inner.last_save = now;
                inner.retry_save_at = now;
            } else {
                inner.retry_save_at = now.wrapping_add(30_000);
            }
        }
    }

    // ------------------------------------------------------- power

    /// Refresh the safety lease after sampling temperature, once a second.
    pub fn set_power(&self, percent: u8, battery_saver: bool) {
        self.battery_limited.store(battery_saver, Ordering::Relaxed);
        self.safety.update(self.now(), percent);
    }
    /// Called on every main-loop pass: the loop is alive, the last power stands.
    pub fn renew_safety(&self) {
        self.safety.renew(self.now());
    }
    pub fn set_ota_busy(&self, busy: bool) {
        self.ota_busy.store(busy, Ordering::Relaxed);
    }
    pub fn power_percent(&self) -> u8 {
        if self.ota_busy.load(Ordering::Relaxed) {
            0
        } else {
            self.safety.power(self.now())
        }
    }
    pub fn throttle(&self) -> Throttle {
        match self.power_percent() {
            0 => Throttle::Paused,
            100 => Throttle::Full,
            _ if self.battery_limited.load(Ordering::Relaxed) => Throttle::Half,
            _ => Throttle::Reduced,
        }
    }

    // ------------------------------------------------------- persistence

    /// Reads the saved lifetime totals and the last block candidate.
    pub fn load_stats(&self) {
        let mut store = self.storage.lock().unwrap();
        let a = store.get(STATS_NAMESPACE, "statA").and_then(|b| decode_record(&b));
        let b = store.get(STATS_NAMESPACE, "statB").and_then(|b| decode_record(&b));
        let chosen = match (a, b) {
            (Some(a), Some(b)) => Some(if (a.0.wrapping_sub(b.0) as i32) > 0 { a } else { b }),
            (a, b) => a.or(b),
        };
        let ca = store.get(STATS_NAMESPACE, "blockA").and_then(|b| decode_candidate(&b));
        let cb = store.get(STATS_NAMESPACE, "blockB").and_then(|b| decode_candidate(&b));
        let history = store.get(STATS_NAMESPACE, "hist").and_then(|b| decode_history(&b));
        drop(store);
        let mut inner = self.inner();
        if let Some((seq, s)) = chosen {
            inner.stats_seq = seq;
            inner.base = s;
            inner.top = s.top;
            inner.stats.best_diff = s.best_diff;
            (self.log)(&format!("[stats] restored save #{seq}"));
        }
        let base = inner.base;
        inner.stats.lifetime_hashes = base.hashes;
        inner.stats.lifetime_shares = base.shares;
        inner.stats.lifetime_blocks = base.blocks;
        inner.stats.lifetime_seconds = base.seconds;
        let candidate = match (ca, cb) {
            (Some(a), Some(b)) => Some(if (a.0.wrapping_sub(b.0) as i32) > 0 { a } else { b }),
            (a, b) => a.or(b),
        };
        if let Some((seq, mut c)) = candidate {
            inner.candidate_seq = seq;
            c.state = match c.state {
                BlockState::Queued => BlockState::Stale,
                BlockState::Submitted => BlockState::Unanswered,
                s => s,
            };
            inner.previous_candidate = Some(c);
        }
        inner.restore = history;
        inner.loaded = true;
    }

    /// Merges the history saved before the last restart once the clock is set.
    pub fn history_poll(&self) {
        let now = self.now();
        let mut inner = self.inner();
        let Some((last_at, points)) = inner.restore.take() else { return };
        // The first new point comes after five minutes.
        if now > (HISTORY_INTERVAL_S - 20) * 1000 {
            (self.log)("[stats] hashrate history not restored: clock not set in time");
            return;
        }
        let Some(unix) = self.clock.unix_time().filter(|&t| t > CLOCK_SET) else {
            inner.restore = Some((last_at, points));
            return;
        };
        if !inner.history.is_empty() {
            return;
        }
        let off = unix as i64 - (now / 1000) as i64 - last_at as i64;
        let missing = if off > 0 { (off / HISTORY_INTERVAL_S as i64) as usize } else { 0 };
        if missing >= HISTORY_LEN {
            return;
        }
        let keep = points.len().min(HISTORY_LEN - missing);
        inner.history.extend(points[points.len() - keep..].iter().copied());
        inner.history.extend(std::iter::repeat_n(HistoryPoint::OFF, missing));
        inner.history_revision += 1;
        (self.log)(&format!("[stats] hashrate history restored: {keep} points, {missing} while off"));
    }

    pub fn stats_snapshot(&self) -> StatsSnapshot {
        let inner = self.inner();
        StatsSnapshot {
            best_diff: inner.stats.best_diff,
            hashes: inner.stats.lifetime_hashes,
            shares: inner.stats.lifetime_shares,
            blocks: inner.stats.lifetime_blocks,
            seconds: inner.stats.lifetime_seconds,
            top: inner.top,
        }
    }

    /// `periodic`: the 10-minute save, which writes the history only every 30
    /// minutes; any other call (before a restart) writes it too.
    pub fn save_stats(&self, periodic: bool) -> bool {
        let now = self.now();
        let (snapshot, history, candidate, revision) = {
            let inner = self.inner();
            if !inner.loaded || inner.frozen {
                return true;
            }
            let history = (inner.history_revision != inner.history_saved_revision
                && inner.restore.is_none()
                && (!periodic || now.wrapping_sub(inner.history_saved_ms) >= 30 * 60 * 1000))
                .then(|| {
                    let unix = self.unix_time();
                    let last_at = unix.saturating_sub(now.wrapping_sub(inner.history_ms) / 1000);
                    (unix != 0).then(|| (encode_history(last_at, &inner.history), inner.history_revision))
                })
                .flatten();
            let candidate = (inner.candidate_revision != inner.saved_candidate_revision)
                .then(|| inner.candidates.back().copied())
                .flatten();
            drop(inner);
            (self.stats_snapshot(), history, candidate, self.inner().candidate_revision)
        };
        let mut store = self.storage.lock().unwrap();
        if let Some((bytes, rev)) = history {
            if store.set(STATS_NAMESPACE, "hist", &bytes) {
                let mut inner = self.inner();
                inner.history_saved_ms = now;
                inner.history_saved_revision = rev;
            }
        }
        let seq = self.inner().stats_seq.wrapping_add(1);
        let key = if seq & 1 != 0 { "statB" } else { "statA" };
        let record = encode_record(seq, &snapshot);
        let ok = store.set(STATS_NAMESPACE, key, &record)
            && store.get(STATS_NAMESPACE, key).and_then(|b| decode_record(&b)).is_some_and(|r| r.0 == seq);
        if !ok {
            (self.log)("[stats] save failed; previous record kept");
            return false;
        }
        {
            let mut inner = self.inner();
            inner.stats_seq = seq;
            inner.best_dirty = inner.stats.best_diff > snapshot.best_diff;
            inner.saved_blocks = snapshot.blocks - inner.base.blocks;
        }
        if let Some(c) = candidate {
            let seq = self.inner().candidate_seq.wrapping_add(1);
            let key = if seq & 1 != 0 { "blockB" } else { "blockA" };
            let ok = store.set(STATS_NAMESPACE, key, &encode_candidate(seq, &c))
                && store.get(STATS_NAMESPACE, key).and_then(|b| decode_candidate(&b)).is_some_and(|r| r.0 == seq);
            if !ok {
                (self.log)("[stats] candidate save failed; previous proof retained");
                return false;
            }
            let mut inner = self.inner();
            inner.candidate_seq = seq;
            inner.saved_candidate_revision = revision;
        } else {
            let mut inner = self.inner();
            if inner.candidates.is_empty() {
                inner.saved_candidate_revision = inner.candidate_revision;
            }
        }
        true
    }

    /// Saves `s` as the lifetime statistics, used from the next start on.
    pub fn stats_restore(&self, s: &StatsSnapshot) -> bool {
        let seq = self.inner().stats_seq.wrapping_add(1);
        let key = if seq & 1 != 0 { "statB" } else { "statA" };
        let ok = self.storage.lock().unwrap().set(STATS_NAMESPACE, key, &encode_record(seq, s));
        if ok {
            let mut inner = self.inner();
            inner.stats_seq = seq;
            inner.frozen = true;
        }
        ok
    }
}

// ---------------------------------------------------------------------------
// Records, laid out like the C++ firmware's structs (little-endian, natural
// alignment) so its saved statistics stay readable.

const STATS_MAGIC: u32 = 0x314c_4442; // "BDL1"
const STATS_RECORD: usize = 208;
const CANDIDATE_MAGIC: u32 = 0x3143_4c42; // BLC1: Bitcoin
const MULTI_COIN_CANDIDATE_MAGIC: u32 = 0x3243_4c42; // BLC2
const CANDIDATE_RECORD: usize = 52;
const HISTORY_MAGIC: u32 = 0x4844_4c42;

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn encode_record(seq: u32, s: &StatsSnapshot) -> Vec<u8> {
    let mut r = vec![0u8; STATS_RECORD];
    r[0..4].copy_from_slice(&STATS_MAGIC.to_le_bytes());
    r[4..8].copy_from_slice(&seq.to_le_bytes());
    r[8..16].copy_from_slice(&s.best_diff.to_le_bytes());
    r[16..24].copy_from_slice(&s.hashes.to_le_bytes());
    r[24..28].copy_from_slice(&s.shares.to_le_bytes());
    r[28..32].copy_from_slice(&s.blocks.to_le_bytes());
    r[32..36].copy_from_slice(&s.seconds.to_le_bytes());
    for (i, t) in s.top.iter().enumerate() {
        let at = 40 + 16 * i;
        r[at..at + 8].copy_from_slice(&t.difficulty.to_le_bytes());
        r[at + 8..at + 12].copy_from_slice(&t.time.to_le_bytes());
    }
    let crc = crc32(&r[..200]);
    r[200..204].copy_from_slice(&crc.to_le_bytes());
    r
}

fn decode_record(r: &[u8]) -> Option<(u32, StatsSnapshot)> {
    if r.len() != STATS_RECORD || le32(r, 0) != STATS_MAGIC || le32(r, 200) != crc32(&r[..200]) {
        return None;
    }
    let f64_at = |at: usize| f64::from_le_bytes(r[at..at + 8].try_into().unwrap());
    let mut s = StatsSnapshot {
        best_diff: f64_at(8),
        hashes: u64::from_le_bytes(r[16..24].try_into().unwrap()),
        shares: le32(r, 24),
        blocks: le32(r, 28),
        seconds: le32(r, 32),
        top: Default::default(),
    };
    for (i, t) in s.top.iter_mut().enumerate() {
        *t = BestShare { difficulty: f64_at(40 + 16 * i), time: le32(r, 48 + 16 * i) };
    }
    Some((le32(r, 4), s))
}

fn encode_candidate(seq: u32, c: &BlockCandidate) -> Vec<u8> {
    let mut r = vec![0u8; CANDIDATE_RECORD];
    let magic = if c.coin == Coin::Btc { CANDIDATE_MAGIC } else { MULTI_COIN_CANDIDATE_MAGIC };
    r[0..4].copy_from_slice(&magic.to_le_bytes());
    r[4..8].copy_from_slice(&seq.to_le_bytes());
    r[8..12].copy_from_slice(&c.height.to_le_bytes());
    r[12..44].copy_from_slice(&c.hash);
    r[44] = c.state as u8;
    r[45] = c.coin.id();
    let crc = crc32(&r[..48]);
    r[48..52].copy_from_slice(&crc.to_le_bytes());
    r
}

fn decode_candidate(r: &[u8]) -> Option<(u32, BlockCandidate)> {
    if r.len() != CANDIDATE_RECORD || le32(r, 48) != crc32(&r[..48]) {
        return None;
    }
    let coin = Coin::from_id(r[45] as u32)?;
    let magic = le32(r, 0);
    if !((magic == CANDIDATE_MAGIC && coin == Coin::Btc) || (magic == MULTI_COIN_CANDIDATE_MAGIC && coin != Coin::Btc))
    {
        return None;
    }
    Some((
        le32(r, 4),
        BlockCandidate {
            number: 0,
            height: le32(r, 8),
            hash: r[12..44].try_into().unwrap(),
            state: BlockState::from_u8(r[44])?,
            previous_session: true,
            coin,
            submission_id: 0,
        },
    ))
}

fn encode_history(last_at: u32, points: &VecDeque<HistoryPoint>) -> Vec<u8> {
    let mut r = Vec::with_capacity(16 + 8 * points.len());
    r.extend_from_slice(&HISTORY_MAGIC.to_le_bytes());
    r.extend_from_slice(&last_at.to_le_bytes());
    r.extend_from_slice(&(points.len() as u32).to_le_bytes());
    for p in points {
        r.extend_from_slice(&p.hashrate.to_le_bytes());
        r.extend_from_slice(&p.temperature_deci_c.to_le_bytes());
        r.push(p.power_percent);
        r.push(p.coverage_percent);
    }
    let crc = crc32(&r);
    r.extend_from_slice(&crc.to_le_bytes());
    r
}

fn decode_history(r: &[u8]) -> Option<(u32, Vec<HistoryPoint>)> {
    if r.len() < 16 || le32(r, 0) != HISTORY_MAGIC {
        return None;
    }
    let count = le32(r, 8) as usize;
    if count == 0
        || count > HISTORY_LEN
        || r.len() != 16 + 8 * count
        || le32(r, 12 + 8 * count) != crc32(&r[..12 + 8 * count])
    {
        return None;
    }
    let points = (0..count)
        .map(|i| {
            let p = &r[12 + 8 * i..20 + 8 * i];
            HistoryPoint {
                hashrate: f32::from_le_bytes(p[..4].try_into().unwrap()),
                temperature_deci_c: i16::from_le_bytes([p[4], p[5]]),
                power_percent: p[6],
                coverage_percent: p[7],
            }
        })
        .collect();
    Some((le32(r, 4), points))
}

// ---------------------------------------------------------------------------
// The portable software worker.

/// Hashes on one thread until `Miner::shutdown`. `scan` hashes `count`
/// nonces from `first` and reports candidates; the default is the portable
/// kernel, ESP32 builds may pass a faster one.
pub fn software_worker(miner: &Miner, engine: usize, owns_core: bool) {
    let clock = miner.clock.clone();
    let mut current: Option<(MiningWork, u32)> = None;
    let mut nonce: u32 = 0;
    let mut last_pause = clock.millis();
    while !miner.stopping() {
        if !miner.board_valid() {
            current = None;
            let started = clock.millis();
            clock.sleep(Duration::from_millis(100));
            miner.profile(engine, |p| p.waiting_ms += clock.millis().wrapping_sub(started) as u64);
            continue;
        }
        let power = miner.power_percent();
        if power == 0 {
            let started = clock.millis();
            clock.sleep(Duration::from_millis(200));
            miner.profile(engine, |p| p.paused_ms += clock.millis().wrapping_sub(started) as u64);
            continue;
        }
        let batch_start = clock.millis();
        let started = std::time::Instant::now();
        if current.as_ref().is_none_or(|(_, seq)| *seq != miner.board_seq()) {
            current = miner.fetch_work();
            nonce = 0;
            if current.is_none() {
                continue;
            }
        }
        let (w, _) = current.as_ref().unwrap();
        let limit = w.work.quick_limit.max(BEST_DIFF_FILTER);
        for n in nonce..nonce.wrapping_add(BATCH_SW) {
            if let Some(hash) = w.work.mid.scan(n, limit) {
                miner.on_candidate(n, &hash, w);
            }
        }
        miner.add_hashes(engine, BATCH_SW as u64);
        miner.profile(engine, |p| p.hashing_us += started.elapsed().as_micros() as u64);
        nonce = nonce.wrapping_add(BATCH_SW);
        if nonce == 0 {
            current = None; // 2^32 nonces done
        }
        if power < 100 {
            let rest_started = clock.millis();
            let mut rest = safety::rest_ms(clock.millis().wrapping_sub(batch_start), power);
            let seq = miner.board_seq();
            while rest > 0 {
                let part = rest.min(100);
                clock.sleep(Duration::from_millis(part as u64));
                rest -= part;
                if miner.power_percent() != power || seq != miner.board_seq() || !miner.board_valid() {
                    break;
                }
            }
            last_pause = clock.millis();
            miner.profile(engine, |p| p.rest_ms += clock.millis().wrapping_sub(rest_started) as u64);
        }
        if !owns_core || clock.millis().wrapping_sub(last_pause) >= 10_000 {
            std::thread::yield_now();
            last_pause = clock.millis();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip_in_the_cpp_layout() {
        let mut s = StatsSnapshot {
            best_diff: 2536.5,
            hashes: 1 << 40,
            shares: 7,
            blocks: 1,
            seconds: 3600,
            ..Default::default()
        };
        s.top[0] = BestShare { difficulty: 2536.5, time: 1_800_000_000 };
        let r = encode_record(3, &s);
        assert_eq!(r.len(), 208);
        assert_eq!(decode_record(&r), Some((3, s)));
        let mut bad = r.clone();
        bad[30] ^= 1;
        assert!(decode_record(&bad).is_none());
        let c = BlockCandidate {
            number: 1,
            height: 900_000,
            hash: [7; 32],
            state: BlockState::Submitted,
            previous_session: false,
            coin: Coin::Bch,
            submission_id: 12,
        };
        let (seq, back) = decode_candidate(&encode_candidate(9, &c)).unwrap();
        assert!(seq == 9 && back.height == 900_000 && back.coin == Coin::Bch && back.state == BlockState::Submitted);
        let points: VecDeque<_> = (0..5).map(|i| HistoryPoint { hashrate: i as f32, ..HistoryPoint::OFF }).collect();
        let (at, back) = decode_history(&encode_history(1_800_000_000, &points)).unwrap();
        assert!(at == 1_800_000_000 && back.len() == 5 && back[4].hashrate == 4.0);
    }
}
