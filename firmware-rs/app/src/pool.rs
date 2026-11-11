//! Network task: keeps a Stratum connection to the pool (V1, or V2 with
//! Noise), feeds jobs to the engines and submits their shares. Fails over to
//! the backup pool and optionally rotates between both.

use std::io::{self, ErrorKind};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bdlminer_core::coin::Coin;
use bdlminer_core::line_buffer::{LineBuffer, Push};
use bdlminer_core::pool_agent::PoolAgent;
use bdlminer_core::stratum::{self, Event};
use bdlminer_core::sv2_key::authority_key;
use bdlminer_core::sv2_noise::{self, Noise};
use bdlminer_core::sv2_protocol::{self as sv2, Frame, MAX_PAYLOAD};

use crate::miner::Miner;
use crate::platform::{Clock, Connection, Network, CLOCK_SET};

const POOL_SILENCE_TIMEOUT_MS: u32 = 5 * 60 * 1000;
const HANDSHAKE_TIMEOUT_MS: u32 = 30_000;
const SHARE_ACK_TIMEOUT_MS: u32 = 60_000;
const MAX_LINE: usize = 16 * 1024;
/// Sessions in a row without authorization and usable work before the backup.
const FAILOVER_AFTER: u32 = 3;
const BACKUP_SESSION_MS: u32 = 30 * 60 * 1000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    /// 0 Stratum V1, 1 Stratum V2 + Noise.
    pub protocol: u8,
    pub authority: String,
}

/// What the network task needs from the settings (read once at start).
#[derive(Clone, Debug)]
pub struct PoolConfig {
    pub coin: Coin,
    pub main: Endpoint,
    pub backup: Option<Endpoint>,
    pub user: String,
    pub password: String,
    pub suggested_difficulty: f64,
    pub user_agent: String,
    pub native_agent: String,
    pub rotate_hours: u8,
    /// ASIC boards negotiate (V1) and require (V2) version rolling.
    pub asic: bool,
    pub hardware: String,
    pub firmware: String,
}

/// What the dashboard shows about the pool connection.
pub struct PoolState {
    status: Mutex<String>,
    on_backup: AtomicBool,
    pending: AtomicU8,
    failovers: AtomicU32,
    agent: Mutex<String>,
    log: Box<dyn Fn(&str) + Send + Sync>,
}

impl PoolState {
    pub fn new(log: impl Fn(&str) + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(PoolState {
            status: Mutex::new("Starting".into()),
            on_backup: AtomicBool::new(false),
            pending: AtomicU8::new(0),
            failovers: AtomicU32::new(0),
            agent: Mutex::new(String::new()),
            log: Box::new(log),
        })
    }
    pub fn set_status(&self, s: &str) {
        *self.status.lock().unwrap() = s.chars().take(47).collect();
        (self.log)(&format!("[pool] {s}"));
    }
    pub fn status(&self) -> String {
        self.status.lock().unwrap().clone()
    }
    pub fn on_backup(&self) -> bool {
        self.on_backup.load(Ordering::Relaxed)
    }
    pub fn pending(&self) -> u8 {
        self.pending.load(Ordering::Relaxed)
    }
    pub fn failovers(&self) -> u32 {
        self.failovers.load(Ordering::Relaxed)
    }
    pub fn user_agent(&self) -> String {
        self.agent.lock().unwrap().clone()
    }
    fn log(&self, s: &str) {
        (self.log)(s);
    }
}

impl PoolConfig {
    pub fn endpoint(&self, backup: bool) -> &Endpoint {
        match (&self.backup, backup) {
            (Some(b), true) => b,
            _ => &self.main,
        }
    }
    pub fn protocol_name(&self, backup: bool) -> &'static str {
        if self.endpoint(backup).protocol == 1 {
            "Stratum V2 (Noise)"
        } else {
            "Stratum V1"
        }
    }
}

fn would_block(e: &io::Error) -> bool {
    matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted)
}

struct Session<'a, N: Network> {
    net: &'a N,
    cfg: &'a PoolConfig,
    miner: &'a Miner,
    state: &'a PoolState,
    clock: &'a dyn Clock,
    backup: bool,
}

#[derive(Default)]
struct Outcome {
    had_work: bool,
    timed_out: bool,
}

impl<N: Network> Session<'_, N> {
    fn endpoint(&self) -> &Endpoint {
        self.cfg.endpoint(self.backup)
    }

    fn mining_status(&self) -> &'static str {
        if self.backup {
            "Mining (backup pool)"
        } else {
            "Mining"
        }
    }

    fn publish(&self, job: &bdlminer_core::work::Job) {
        self.miner.pool_difficulty(job.difficulty);
        self.miner.publish_job(job, job.clean);
    }

    /// One Stratum V1 session until the connection drops or `max_ms`.
    fn stratum_v1(
        &self,
        conn: &mut N::Conn,
        proto: &mut stratum::Protocol,
        agent: &mut PoolAgent,
        max_ms: u32,
    ) -> Outcome {
        let mut out = Outcome::default();
        let start = self.clock.millis();
        let (mut last_rx, mut last_reply) = (start, start);
        let ep = self.endpoint();
        agent.endpoint(&ep.host, ep.port);
        let identity = agent.select(&self.cfg.native_agent, false, Some(&self.cfg.user_agent)).to_owned();
        *self.state.agent.lock().unwrap() = identity.clone();
        self.state.log(&format!("[pool] Stratum V1 user agent \"{identity}\""));
        let send = |conn: &mut N::Conn, line: &str| conn.write_all(line.as_bytes()).is_ok();
        if self.cfg.asic && !send(conn, &proto.configure_version_rolling()) {
            return out;
        }
        if !send(conn, &proto.subscribe(&identity)) {
            return out;
        }
        let mut input = LineBuffer::new(MAX_LINE);
        let mut chunk = [0u8; 512];
        let mut logged_height = 0;
        loop {
            if !self.net.link_up() || self.miner.stopping() {
                break;
            }
            let mut received = 0;
            while received < 4096 {
                let n = match conn.read(&mut chunk) {
                    Ok(0) => return self.v1_done(proto, out),
                    Ok(n) => n,
                    Err(e) if would_block(&e) => break,
                    Err(_) => return self.v1_done(proto, out),
                };
                received += n;
                last_rx = self.clock.millis();
                for &c in &chunk[..n] {
                    match input.push(c) {
                        Push::More => continue,
                        Push::Oversized => {
                            self.state.log("[pool] invalid or oversized message discarded");
                            continue;
                        }
                        Push::Complete => {}
                    }
                    if input.is_empty() {
                        continue;
                    }
                    let line = input.line().into_owned();
                    input.clear();
                    match proto.handle_line(&line) {
                        Event::Subscribed => {
                            if self.cfg.suggested_difficulty > 0.0
                                && !send(conn, &proto.suggest_difficulty(self.cfg.suggested_difficulty))
                            {
                                return self.v1_done(proto, out);
                            }
                            if !send(conn, &proto.authorize()) {
                                return self.v1_done(proto, out);
                            }
                        }
                        Event::SubscribeFailed => {
                            self.state.set_status("Subscribe refused");
                            self.state.log(&format!(
                                "[pool] subscribe refused: {} (user agent \"{identity}\")",
                                proto.last_error()
                            ));
                            return self.v1_done(proto, out);
                        }
                        Event::Authorized => {
                            self.miner.pool_connected(true);
                            self.state.set_status(self.mining_status());
                            // Pools can send the first notify before authorization.
                            if proto.has_job() {
                                if !self.cfg.coin.job_compatible(proto.job().version, proto.job().version_mask) {
                                    self.miner.invalidate();
                                    self.state.set_status("DGB requires a SHA-256d pool");
                                    return self.v1_done(proto, out);
                                }
                                self.publish(proto.job());
                            }
                        }
                        Event::AuthFailed => {
                            self.state.set_status("Auth failed: check address");
                            self.state.log(&format!("[pool] error: {}", proto.last_error()));
                            return self.v1_done(proto, out);
                        }
                        Event::NewJob => {
                            if !proto.authorized() || !proto.has_job() {
                                continue;
                            }
                            let job = proto.job();
                            if !self.cfg.coin.job_compatible(job.version, job.version_mask) {
                                self.miner.invalidate();
                                self.state.set_status("DGB requires a SHA-256d pool");
                                return self.v1_done(proto, out);
                            }
                            self.publish(job);
                            let height = job.block_height(self.cfg.coin);
                            if job.clean && height != logged_height {
                                logged_height = height;
                                self.state
                                    .log(&format!("[pool] new block {height}, job {} diff={}", job.id, job.difficulty));
                            }
                        }
                        Event::Difficulty => {
                            if !proto.has_job() {
                                self.miner.pool_difficulty(proto.difficulty());
                            }
                            self.state.log(&format!("[pool] difficulty {}", proto.difficulty()));
                        }
                        Event::ShareAccepted => {
                            last_reply = self.clock.millis();
                            self.miner.share_result(true, proto.acknowledgment());
                        }
                        Event::ShareRejected => {
                            last_reply = self.clock.millis();
                            self.miner.share_result(false, proto.acknowledgment());
                            self.state.log(&format!("[pool] share rejected: {}", proto.last_error()));
                        }
                        Event::Error | Event::None => {}
                    }
                }
            }
            let now = self.clock.millis();
            if (!proto.authorized() || !proto.has_job()) && now.wrapping_sub(start) > HANDSHAKE_TIMEOUT_MS {
                self.state.set_status(if proto.authorized() {
                    "Pool work timed out"
                } else {
                    "Pool handshake timed out"
                });
                break;
            }
            if now.wrapping_sub(last_rx) > POOL_SILENCE_TIMEOUT_MS {
                self.state.set_status("Pool silent, reconnecting");
                break;
            }
            if max_ms != 0 && now.wrapping_sub(start) > max_ms {
                self.state.set_status("Switching pool");
                out.timed_out = true;
                break;
            }
            self.state.pending.store(proto.pending_shares() as u8, Ordering::Relaxed);
            if proto.pending_shares() != 0 && now.wrapping_sub(last_reply) > SHARE_ACK_TIMEOUT_MS {
                self.state.set_status("Share acknowledgments timed out");
                break;
            }
            if !proto.authorized() || !proto.has_job() || !proto.can_submit() {
                self.clock.sleep(Duration::from_millis(20));
                continue;
            }
            let Some(share) = self.miner.next_share(Duration::from_millis(20)) else { continue };
            // A clean job voids every older one.
            if proto.job().clean && proto.job().id != share.job_id {
                self.state.log(&format!("[pool] stale share dropped (job {})", share.job_id));
                continue;
            }
            if share.difficulty >= 1.0 {
                self.state.log(&format!(
                    "[pool] submit nonce={:08x} diff={:.4} job={}",
                    share.nonce, share.difficulty, share.job_id
                ));
            }
            if proto.pending_shares() == 0 {
                last_reply = self.clock.millis();
            }
            let Some(line) = proto.submit(&share) else { continue };
            if !send(conn, &line) {
                break;
            }
            self.miner.share_sent(share.difficulty, self.backup, proto.submission_id(), Some(&share.hash));
            self.state.pending.store(proto.pending_shares() as u8, Ordering::Relaxed);
        }
        self.v1_done(proto, out)
    }

    fn v1_done(&self, proto: &stratum::Protocol, mut out: Outcome) -> Outcome {
        out.had_work = proto.authorized()
            && proto.has_job()
            && self.cfg.coin.job_compatible(proto.job().version, proto.job().version_mask);
        out
    }

    fn read_exact(&self, conn: &mut N::Conn, buf: &mut [u8], timeout_ms: u32) -> bool {
        let start = self.clock.millis();
        let mut got = 0;
        while got < buf.len() && self.clock.millis().wrapping_sub(start) < timeout_ms {
            match conn.read(&mut buf[got..]) {
                Ok(0) => return false,
                Ok(n) => got += n,
                Err(e) if would_block(&e) => {}
                Err(_) => return false,
            }
        }
        got == buf.len()
    }

    /// One Stratum V2 session (Noise_NX, extended channel).
    fn stratum_v2(&self, conn: &mut N::Conn, max_ms: u32, identity: &str) -> Outcome {
        let mut out = Outcome::default();
        let Some(authority) = authority_key(&self.endpoint().authority) else {
            self.state.set_status("SV2 authority key missing/invalid");
            return out;
        };
        let Some(now) = self.clock.unix_time().filter(|&t| t >= CLOCK_SET) else {
            self.state.set_status("SV2 waiting for synchronized time");
            return out;
        };
        self.state.set_status("SV2 encrypted handshake");
        let mut noise = Noise::new();
        let mut random = |b: &mut [u8]| self.net.random(b);
        let hello = match noise.start(&authority, &mut random) {
            Ok(h) => h,
            Err(e) => {
                self.state.set_status("SV2 Noise handshake failed");
                self.state.log(&format!("[pool] SV2: {e}"));
                return out;
            }
        };
        let mut response = [0u8; sv2_noise::RESPONSE_LEN];
        if conn.write_all(&hello).is_err() || !self.read_exact(conn, &mut response, 10_000) {
            self.state.set_status("SV2 Noise handshake failed");
            return out;
        }
        let now = self.clock.unix_time().unwrap_or(now);
        if let Err(e) = noise.finish(&response, now) {
            self.state.set_status("SV2 server verification failed");
            self.state.log(&format!("[pool] SV2: {e}"));
            return out;
        }
        self.state.log(&format!("[pool] Stratum V2 Noise: server authority verified, identity \"{identity}\""));
        let mut transport =
            Transport { noise, buffer: vec![0; 22], got: 0, payload: false, started: None, header: (0, 0) };
        let mut proto = sv2::Protocol::new();
        let ep = self.endpoint();
        let Some(setup) =
            proto.setup(&ep.host, ep.port, identity, &self.cfg.hardware, &self.cfg.firmware, self.cfg.asic)
        else {
            return out;
        };
        if !transport.send(conn, &setup) {
            return out;
        }
        let start = self.clock.millis();
        let (mut last_reply, mut last_rx) = (start, start);
        let mut authorized = false;
        let mut logged_height = 0;
        while self.net.link_up() && !self.miner.stopping() {
            let received = transport.poll(conn, self.clock);
            let frame = match received {
                Err(()) => {
                    self.state.set_status("SV2 invalid or unauthenticated frame");
                    return out;
                }
                Ok(Some(f)) => {
                    last_rx = self.clock.millis();
                    Some(f)
                }
                Ok(None) => None,
            };
            if let Some(frame) = frame {
                let event = proto.handle(&frame);
                match event {
                    sv2::Event::Error => {
                        if proto.version_rolling_refused() {
                            out.had_work = false;
                            self.state.set_status("Pool needs version rolling");
                        } else {
                            self.state.set_status("SV2 protocol refused/invalid");
                        }
                        self.state.log(&format!("[pool] SV2: {}", proto.error()));
                        return out;
                    }
                    sv2::Event::Setup => {
                        let mut rate = self.miner.hashrate();
                        if self.cfg.asic && (rate.is_nan() || rate <= 0.0) {
                            rate = 1e12; // channel negotiation hint only
                        }
                        let rate = if rate > 0.0 { rate } else { 100_000.0 };
                        let Some(open) = proto.open(&self.cfg.user, rate as f32) else { return out };
                        if !transport.send(conn, &open) {
                            return out;
                        }
                    }
                    sv2::Event::Opened => {
                        authorized = true;
                        self.miner.pool_connected(true);
                        self.state.set_status(self.mining_status());
                        self.miner.pool_difficulty(proto.difficulty());
                    }
                    sv2::Event::Job | sv2::Event::Target if proto.has_job() => {
                        out.had_work = true;
                        self.publish(proto.job());
                        let height = proto.job().block_height(self.cfg.coin);
                        if proto.job().clean && height != logged_height {
                            logged_height = height;
                            self.state.log(&format!(
                                "[pool] SV2 new block {height}, job {} diff={}",
                                proto.job().id,
                                proto.job().difficulty
                            ));
                        }
                    }
                    sv2::Event::Target => self.miner.pool_difficulty(proto.difficulty()),
                    sv2::Event::Accepted | sv2::Event::Rejected => {
                        last_reply = self.clock.millis();
                        let accepted = event == sv2::Event::Accepted;
                        for ack in proto.acknowledgments() {
                            self.miner.share_result(accepted, ack);
                        }
                        if !accepted {
                            self.state.log(&format!(
                                "[pool] SV2 {} share(s) rejected: {}",
                                proto.acknowledgments().len(),
                                proto.error()
                            ));
                        }
                    }
                    _ => {}
                }
            }
            self.state.pending.store(proto.pending() as u8, Ordering::Relaxed);
            let now = self.clock.millis();
            if (!authorized || !out.had_work) && now.wrapping_sub(start) > HANDSHAKE_TIMEOUT_MS {
                self.state.set_status(if authorized { "SV2 work timed out" } else { "SV2 setup/channel timed out" });
                return out;
            }
            if now.wrapping_sub(last_rx) > POOL_SILENCE_TIMEOUT_MS {
                self.state.set_status("Pool silent, reconnecting");
                return out;
            }
            if proto.pending() != 0 && now.wrapping_sub(last_reply) > SHARE_ACK_TIMEOUT_MS {
                self.state.set_status("Share acknowledgments timed out");
                return out;
            }
            if max_ms != 0 && now.wrapping_sub(start) > max_ms {
                out.timed_out = true;
                self.state.set_status("Switching pool");
                return out;
            }
            if !proto.opened() || proto.pending() >= 32 {
                self.clock.sleep(Duration::from_millis(10));
                continue;
            }
            let Some(share) = self.miner.next_share(Duration::from_millis(20)) else { continue };
            let first = proto.pending() == 0;
            let Some(submit) = proto.submit(&share) else {
                self.state.log(&format!("[pool] SV2 obsolete share dropped (job {})", share.job_id));
                continue;
            };
            if first {
                last_reply = self.clock.millis();
            }
            if !transport.send(conn, &submit) {
                return out;
            }
            self.miner.share_sent(share.difficulty, self.backup, proto.submission_id(), Some(&share.hash));
        }
        out
    }
}

/// Encrypted SV2 frames: a 22-byte header record (6 bytes + tag), then the
/// payload record. Incremental, so a frame split over many TCP packets keeps
/// shares and acknowledgments moving.
struct Transport {
    noise: Noise,
    buffer: Vec<u8>,
    got: usize,
    payload: bool,
    started: Option<u32>,
    header: (u16, u8),
}

impl Transport {
    fn send<C: Connection>(&mut self, conn: &mut C, frame: &Frame) -> bool {
        if frame.payload.is_empty() || frame.payload.len() > MAX_PAYLOAD {
            return false;
        }
        let n = frame.payload.len();
        let header = [frame.extension as u8, (frame.extension >> 8) as u8, frame.kind, n as u8, (n >> 8) as u8, 0];
        let (Ok(h), Ok(p)) = (self.noise.encrypt(&header), self.noise.encrypt(&frame.payload)) else { return false };
        conn.write_all(&h).is_ok() && conn.write_all(&p).is_ok()
    }

    fn reset(&mut self) {
        self.buffer = vec![0; 22];
        self.got = 0;
        self.payload = false;
        self.started = None;
    }

    fn poll<C: Connection>(&mut self, conn: &mut C, clock: &dyn Clock) -> Result<Option<Frame>, ()> {
        if self.started.is_some_and(|s| clock.millis().wrapping_sub(s) > 30_000) {
            return Err(());
        }
        let n = match conn.read(&mut self.buffer[self.got..]) {
            Ok(0) => return Err(()),
            Ok(n) => n,
            Err(e) if would_block(&e) => return Ok(None),
            Err(_) => return Err(()),
        };
        self.started.get_or_insert(clock.millis());
        self.got += n;
        if self.got != self.buffer.len() {
            return Ok(None);
        }
        if !self.payload {
            let header = self.noise.decrypt(&self.buffer).map_err(|_| ())?;
            let extension = u16::from_le_bytes([header[0], header[1]]);
            let length = header[3] as usize | (header[4] as usize) << 8 | (header[5] as usize) << 16;
            if length > MAX_PAYLOAD || extension & 0x7fff != 0 {
                return Err(());
            }
            self.header = (extension, header[2]);
            if length == 0 {
                self.reset();
                return Ok(Some(Frame::new(extension, self.header.1, vec![])));
            }
            self.buffer = vec![0; length + sv2_noise::TAG_LEN];
            self.got = 0;
            self.payload = true;
            return Ok(None);
        }
        let payload = self.noise.decrypt(&self.buffer).map_err(|_| ())?;
        let frame = Frame::new(self.header.0, self.header.1, payload);
        self.reset();
        Ok(Some(frame))
    }
}

/// The network task: runs until `Miner::shutdown`.
pub fn run<N: Network>(net: &N, cfg: &PoolConfig, miner: &Miner, state: &PoolState, clock: &dyn Clock) {
    let mut proto = stratum::Protocol::new();
    let mut agents = [PoolAgent::default(), PoolAgent::default()];
    let mut backoff_ms: u32 = 2000;
    let mut failures = 0;
    while !miner.stopping() {
        let backup = state.on_backup();
        let ep = cfg.endpoint(backup).clone();
        // Also guard portal/imported settings: a mismatched backup must never
        // switch mining to another chain.
        let configuration_error = cfg.coin.pool_error(&ep.host, ep.protocol).or_else(|| {
            (!cfg.coin.address_compatible(&cfg.user)).then_some("Use an address for the selected cryptocurrency")
        });
        if let Some(e) = configuration_error {
            state.set_status(e);
            sleep_while(clock, miner, 30_000);
            continue;
        }
        if !net.link_up() {
            state.set_status("WiFi lost, reconnecting");
            let (mut since, lost) = (clock.millis(), clock.millis());
            while !net.link_up() && !miner.stopping() {
                if clock.millis().wrapping_sub(since) > 30_000 {
                    // After a minute, also the fallback networks.
                    if clock.millis().wrapping_sub(lost) <= 60_000 || !net.try_fallbacks() {
                        net.reconnect();
                    }
                    since = clock.millis();
                }
                clock.sleep(Duration::from_millis(500));
            }
        }
        // With a rotation period both pools get that long in turn; otherwise
        // the backup only gets BACKUP_SESSION_MS before the main is retried.
        let rotate = cfg.rotate_hours != 0 && cfg.backup.is_some();
        let max_ms = if rotate {
            cfg.rotate_hours as u32 * 3_600_000
        } else if backup {
            BACKUP_SESSION_MS
        } else {
            0
        };
        state.set_status(&format!("Connecting {}", ep.host));
        let session = Session { net, cfg, miner, state, clock, backup };
        let mut outcome = Outcome::default();
        match net.connect(&ep.host, ep.port, Duration::from_secs(10)) {
            Ok(mut conn) => {
                proto.reset();
                proto.set_credentials(&cfg.user, &cfg.password);
                state.set_status("Subscribing");
                let t0 = clock.millis();
                let agent = &mut agents[backup as usize];
                if ep.protocol == 1 {
                    agent.endpoint(&ep.host, ep.port);
                    let identity = agent.select(&cfg.native_agent, false, Some(&cfg.user_agent)).to_owned();
                    *state.agent.lock().unwrap() = identity.clone();
                    outcome = session.stratum_v2(&mut conn, max_ms, &identity);
                } else {
                    outcome = session.stratum_v1(&mut conn, &mut proto, agent, max_ms);
                }
                if clock.millis().wrapping_sub(t0) > 60_000 {
                    backoff_ms = 2000; // the session was healthy
                }
            }
            Err(_) => state.set_status("Pool unreachable"),
        }
        miner.pool_connected(false);
        state.pending.store(0, Ordering::Relaxed);
        miner.invalidate();
        if state.status().starts_with("Mining") {
            state.set_status("Disconnected");
        }
        failures = if outcome.had_work { 0 } else { failures + 1 };
        if rotate && outcome.timed_out {
            state.on_backup.store(!backup, Ordering::Relaxed);
            if !backup {
                state.failovers.fetch_add(1, Ordering::Relaxed);
            }
            failures = 0;
            backoff_ms = 2000;
            state.log(&format!("[pool] rotation: switching to {}", cfg.endpoint(!backup).host));
        } else if backup {
            state.on_backup.store(false, Ordering::Relaxed); // back to the main pool
            failures = 0;
        } else if failures >= FAILOVER_AFTER {
            if let Some(b) = &cfg.backup {
                state.on_backup.store(true, Ordering::Relaxed);
                state.failovers.fetch_add(1, Ordering::Relaxed);
                failures = 0;
                backoff_ms = 2000;
                state.log(&format!(
                    "[pool] main pool failed {FAILOVER_AFTER} times, switching to backup {}:{}",
                    b.host, b.port
                ));
            }
        }
        sleep_while(clock, miner, backoff_ms);
        backoff_ms = (backoff_ms * 2).min(60_000);
    }
}

fn sleep_while(clock: &dyn Clock, miner: &Miner, ms: u32) {
    let start = clock.millis();
    while clock.millis().wrapping_sub(start) < ms && !miner.stopping() {
        clock.sleep(Duration::from_millis(100.min(ms as u64)));
    }
}
