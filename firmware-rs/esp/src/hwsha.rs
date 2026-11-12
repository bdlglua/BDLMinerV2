//! The SHA peripheral and the Xtensa assembly kernels, in Rust: a port of
//! the C++ firmware's src/hw_sha.cpp, with its kernels (src/hw_sha_classic.S,
//! hw_sha_fill.S, sw_sha_prefilter.S) assembled by `global_asm!` (build.rs
//! does what the C preprocessor did for them).
//!
//! - ESP32-S2/S3/C3: the peripheral resumes from a saved midstate, so one
//!   nonce costs two blocks; nonce positions are the nonces themselves.
//! - Original ESP32 (revision 3 and later): the peripheral cannot be given a
//!   midstate, so one nonce costs three blocks, but a cycle-counted assembly
//!   loop keeps it busy all the time (~1 MH/s at 240 MHz). Position p is
//!   nonce `p.swap_bytes()`, only hashes with top-32 bits <= 0xFFFF are
//!   seen, and a second work can be hashed in software in the loop's idle
//!   cycles (engine "fill").

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use bdlminer_app::miner::{Engine, Miner, MiningWork, BATCH_HW, BATCH_SW, BEST_DIFF_FILTER};
use bdlminer_core::sha256::{sha256d, MidState};
use bdlminer_core::work::{from_hex_array, quick_limit_for};

// Software SHA-256d up to the 16-bit early exit (LX6 and LX7).
#[cfg(any(esp32, esp32s3))]
core::arch::global_asm!(include_str!(concat!(env!("OUT_DIR"), "/sw_sha_prefilter.s")), options(raw));

#[cfg(any(esp32, esp32s3))]
extern "C" {
    /// True exactly when the top 16 bits of the double hash are zero.
    fn sw_sha256d_prefilter(mid: *const u32, tail: *const u8, bake: *const u32) -> bool;
}

#[cfg(any(esp32, esp32s3))]
fn prefilter(mid: &MidState, tail: &[u8; 16]) -> bool {
    unsafe { sw_sha256d_prefilter(mid.mid.as_ptr(), tail.as_ptr(), mid.bake.as_ptr()) }
}

#[cfg(not(any(esp32, esp32s3)))]
fn prefilter(_mid: &MidState, _tail: &[u8; 16]) -> bool {
    false
}

const PREFILTER_BUILT: bool = cfg!(any(esp32, esp32s3));

/// Set by `init` when the assembly prefilter passed its self-test.
static USE_ASM: AtomicBool = AtomicBool::new(false);
/// Where the log lines go (the dashboard log).
type Logger = Box<dyn Fn(&str) + Send + Sync>;
static LOG: OnceLock<Logger> = OnceLock::new();

fn log(line: &str) {
    match LOG.get() {
        Some(f) => f(line),
        None => log::info!("{line}"),
    }
}

/// A hit: nonce, hash, and whether it belongs to the fill work.
type Hit<'a> = &'a mut dyn FnMut(u32, &[u8; 32], bool);

fn top32(hash: &[u8; 32]) -> u32 {
    u32::from_le_bytes([hash[28], hash[29], hash[30], hash[31]])
}

// ---------------------------------------------------------------------------
// ESP32-S2, S3 and C3: a resumable engine
// ---------------------------------------------------------------------------

#[cfg(any(esp32s2, esp32s3, esp32c3))]
mod engine {
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

    use super::{log, sha256d, top32, Hit};

    // soc/hwcrypto_reg.h, the same on the three chips (DR_REG_SHA_BASE).
    const BASE: usize = 0x6003_B000;
    const MODE: *mut u32 = BASE as *mut u32;
    const START: *mut u32 = (BASE + 0x10) as *mut u32;
    const CONTINUE: *mut u32 = (BASE + 0x14) as *mut u32;
    const BUSY: *mut u32 = (BASE + 0x18) as *mut u32;
    const H: *mut u32 = (BASE + 0x40) as *mut u32;
    const TEXT: *mut u32 = (BASE + 0x80) as *mut u32;
    /// esp_sha_type SHA2_256 on these chips.
    const SHA2_256: u32 = 2;

    extern "C" {
        // components/mbedtls/port/sha: the engine's lock, shared with mbedtls.
        fn esp_sha_acquire_hardware();
        fn esp_sha_release_hardware();
    }

    #[inline(always)]
    fn wr(reg: *mut u32, value: u32) {
        unsafe { reg.write_volatile(value) }
    }

    #[inline(always)]
    fn rd(reg: *const u32) -> u32 {
        unsafe { reg.read_volatile() }
    }

    #[inline(always)]
    fn at(base: *mut u32, i: usize) -> *mut u32 {
        unsafe { base.add(i) }
    }

    pub struct Job {
        /// The midstate in register format.
        mid: [u32; 8],
        /// The second header block, nonce in tail[3].
        tail: [u32; 16],
    }

    impl Default for Job {
        fn default() -> Self {
            Job { mid: [0; 8], tail: [0; 16] }
        }
    }

    #[derive(Default)]
    pub struct Fill {
        pub hashes: u32,
    }

    pub fn available() -> bool {
        true
    }

    pub fn nonce_at(position: u32) -> u32 {
        position
    }

    // While held, the miner starts no hardware batch (it sees `HELD` and
    // hashes in software) and mbedtls takes the engine through its own lock,
    // waiting at most for the batch in progress. hold() must not take that
    // lock itself: ESP-IDF 5 made the SHA/AES lock non-recursive, so mbedtls,
    // taking it again from the same task during the handshake, would wait
    // forever (the TLS task, then every SHA user after it, such as the
    // update's image check).
    static HELD: AtomicBool = AtomicBool::new(false);
    static HOLDS: AtomicI32 = AtomicI32::new(0); // hold() nests (TLS tasks may overlap)
    static SPARSE: AtomicBool = AtomicBool::new(true); // selected by self_test(), before workers start
    static DIRECT: AtomicBool = AtomicBool::new(true); // correctness and speed checked before workers start
    static OVERLAPPED: AtomicBool = AtomicBool::new(cfg!(esp32s3)); // S3 only: checked and timed at boot

    // ESP-IDF 5 resets the engine each time it is acquired, and its start
    // and continue helpers no longer write the mode register: left at 0
    // after the reset, the engine computes SHA-1. Select SHA-256 after every
    // acquire.
    #[inline(always)]
    fn acquire() {
        unsafe { esp_sha_acquire_hardware() };
        wr(MODE, SHA2_256);
    }

    #[inline(always)]
    fn release_hw() {
        unsafe { esp_sha_release_hardware() };
    }

    pub fn hold() {
        HOLDS.fetch_add(1, Ordering::SeqCst);
        HELD.store(true, Ordering::SeqCst);
    }

    pub fn release() {
        if HOLDS.fetch_sub(1, Ordering::SeqCst) == 1 {
            HELD.store(false, Ordering::SeqCst);
        }
    }

    pub fn idle() {}

    fn wait_idle() {
        while rd(BUSY) != 0 {}
    }

    #[inline(always)]
    fn wait_hash_word7() -> u32 {
        let h7 = at(H, 7);
        loop {
            for _ in 0..32 {
                let value = rd(h7);
                if value != 0 {
                    return value;
                }
            }
            // A completed hash can legitimately have a zero most-significant
            // word.
            if rd(BUSY) == 0 {
                return rd(h7);
            }
        }
    }

    fn read_digest() -> [u8; 32] {
        let mut hash = [0u8; 32];
        for i in 0..8 {
            hash[4 * i..4 * i + 4].copy_from_slice(&rd(at(H, i)).to_le_bytes());
        }
        hash
    }

    #[inline(always)]
    fn write_words(dest: *mut u32, src: &[u32]) {
        for (i, &w) in src.iter().enumerate() {
            wr(at(dest, i), w);
        }
    }

    #[inline(always)]
    fn digest_to_text() {
        for i in 0..8 {
            wr(at(TEXT, i), rd(at(H, i)));
        }
    }

    pub fn prepare(job: &mut Job, header: &[u8; 80]) {
        // The engine works on raw message bytes, so the header is copied
        // as-is.
        let mut tail = [0u8; 64];
        tail[..16].copy_from_slice(&header[64..80]);
        tail[16] = 0x80;
        tail[62] = 0x02; // 640 bits, big-endian length
        tail[63] = 0x80;
        for i in 0..16 {
            job.tail[i] = u32::from_le_bytes([tail[4 * i], tail[4 * i + 1], tail[4 * i + 2], tail[4 * i + 3]]);
        }
        acquire();
        for i in 0..16 {
            let w = u32::from_le_bytes([header[4 * i], header[4 * i + 1], header[4 * i + 2], header[4 * i + 3]]);
            wr(at(TEXT, i), w);
        }
        wr(START, 1);
        wait_idle();
        for i in 0..8 {
            job.mid[i] = rd(at(H, i));
        }
        release_hw();
    }

    pub fn fill_available() -> bool {
        false
    }

    pub fn prepare_fill(_fill: &mut Fill, _header: &[u8; 80]) {}

    /// `SPARSE_TEXT`: words 9..14 are zero in both padded blocks and the
    /// peripheral reads TEXT without changing it: written once per batch.
    /// `DIRECT`: register to register, no RAM copy of the first digest.
    #[inline(never)]
    fn scan_with_padding<const SPARSE_TEXT: bool, const DIRECT_COPY: bool>(
        job: &mut Job,
        first: u32,
        count: u32,
        limit: u32,
        hit: Hit,
    ) -> u32 {
        if HELD.load(Ordering::Relaxed) {
            return 0;
        }
        acquire();
        // Reinitialized for every batch because TLS may have used it in
        // between.
        if SPARSE_TEXT {
            for i in 9..15 {
                wr(at(TEXT, i), 0);
            }
        }
        let mut nonce = first;
        for _ in 0..count {
            job.tail[3] = nonce;
            write_words(H, &job.mid);
            write_words(TEXT, &job.tail[..if SPARSE_TEXT { 9 } else { 16 }]);
            if SPARSE_TEXT {
                wr(at(TEXT, 15), job.tail[15]);
            }
            wr(CONTINUE, 1);
            wait_hash_word7();
            if DIRECT_COPY {
                digest_to_text();
            } else {
                let mut second = [0u32; 8];
                for (i, w) in second.iter_mut().enumerate() {
                    *w = rd(at(H, i));
                }
                write_words(TEXT, &second);
            }
            wr(at(TEXT, 8), 0x0000_0080); // padding byte 0x80
            if !SPARSE_TEXT {
                for i in 9..15 {
                    wr(at(TEXT, i), 0);
                }
            }
            wr(at(TEXT, 15), 0x0001_0000); // 256 bits, big-endian length
            wr(START, 1);
            // Bytes 28..31 of the digest, read as a little-endian word, are
            // the most significant 32 bits of the hash as a 256-bit number.
            if wait_hash_word7() <= limit {
                hit(nonce, &read_digest(), false);
            }
            nonce = nonce.wrapping_add(1);
        }
        release_hw();
        count
    }

    /// ESP32-S3: every register access crosses the slow peripheral bus,
    /// which costs more than the block computation itself. The engine
    /// latches TEXT when a block starts (measured on the chip by NerdMiner
    /// Modded, MIT, its PIPELINED_S3_MINING loop: ~740 instead of ~1080
    /// cycles per nonce), so the next nonce's header block is written while
    /// the second hash computes, and only its words that the second block
    /// changed: 0..8 and 15 (9..14 stay 0). After a hit the whole block is
    /// written again, whatever the callback did. self_test() checks every
    /// digest and the exact set of hits, then times it against the other
    /// kernels; any failure falls back to them.
    #[inline(never)]
    fn scan_overlapped(job: &mut Job, first: u32, count: u32, limit: u32, hit: Hit) -> u32 {
        if HELD.load(Ordering::Relaxed) || count == 0 {
            return 0;
        }
        acquire();
        let mut nonce = first;
        job.tail[3] = nonce;
        write_words(TEXT, &job.tail);
        for _ in 0..count {
            write_words(H, &job.mid);
            wr(CONTINUE, 1);
            wait_hash_word7();
            digest_to_text();
            wr(at(TEXT, 8), 0x0000_0080); // padding byte 0x80
            wr(at(TEXT, 15), 0x0001_0000); // 256 bits, big-endian length
            wr(START, 1);
            // While it computes: the next nonce's header block, changed words
            // only.
            job.tail[3] = nonce.wrapping_add(1);
            write_words(TEXT, &job.tail[..9]);
            wr(at(TEXT, 15), job.tail[15]);
            if wait_hash_word7() <= limit {
                hit(nonce, &read_digest(), false);
                write_words(TEXT, &job.tail);
            }
            nonce = nonce.wrapping_add(1);
        }
        release_hw();
        count
    }

    fn scan_plain(job: &mut Job, first: u32, count: u32, limit: u32, hit: Hit) -> u32 {
        let (sparse, direct) = (SPARSE.load(Ordering::Relaxed), DIRECT.load(Ordering::Relaxed));
        if direct && sparse {
            scan_with_padding::<true, true>(job, first, count, limit, hit)
        } else if sparse {
            scan_with_padding::<true, false>(job, first, count, limit, hit)
        } else {
            scan_with_padding::<false, false>(job, first, count, limit, hit)
        }
    }

    pub fn scan(job: &mut Job, first: u32, count: u32, limit: u32, hit: Hit, _fill: Option<&mut Fill>) -> u32 {
        // Specialized outside the nonce loop: no per-nonce mode checks.
        if cfg!(esp32s3) && OVERLAPPED.load(Ordering::Relaxed) {
            return scan_overlapped(job, first, count, limit, hit);
        }
        scan_plain(job, first, count, limit, hit)
    }

    fn now_us() -> i64 {
        unsafe { esp_idf_svc::sys::esp_timer_get_time() }
    }

    /// The best of three of `a` (false) and `b` (true), alternating order,
    /// over 4096 nonces: (best a, best b) in microseconds, None on a failed
    /// scan. These short local scans are not pool work and never enter
    /// mining counters.
    fn race(job: &mut Job, run: &mut dyn FnMut(&mut Job, bool) -> u32) -> Option<[i64; 2]> {
        const COUNT: u32 = 4096;
        let mut best = [i64::MAX; 2];
        for round in 0..3 {
            for turn in 0..2 {
                let second = (round + turn) & 1 == 1;
                let start = now_us();
                let done = run(job, second);
                let elapsed = now_us() - start;
                if done != COUNT || elapsed <= 0 {
                    return None;
                }
                best[second as usize] = best[second as usize].min(elapsed);
            }
        }
        Some(best)
    }

    /// A compiler/SDK/chip combination may make the buffered path faster.
    /// Both are compared on the actual peripheral, including locks and busy
    /// waits, at boot only.
    fn prefer_direct(job: &mut Job) -> bool {
        let Some(best) = race(job, &mut |job, direct| {
            let mut discard = |_: u32, _: &[u8; 32], _: bool| {};
            if direct {
                scan_with_padding::<true, true>(job, 0, 4096, 0, &mut discard)
            } else {
                scan_with_padding::<true, false>(job, 0, 4096, 0, &mut discard)
            }
        }) else {
            return false;
        };
        let faster = best[1] * 100 < best[0] * 99; // at least 1% faster
        log(&format!(
            "[miner] resumable SHA transfers: {} (buffered {:.1}, direct {:.1} kH/s, boot scan)",
            if faster { "direct" } else { "buffered" },
            4096.0 * 1000.0 / best[0] as f64,
            4096.0 * 1000.0 / best[1] as f64
        ));
        faster
    }

    /// The overlapped kernel against the best other one, timed like
    /// prefer_direct(): kept only when at least 1% faster.
    fn prefer_overlapped(job: &mut Job) -> bool {
        let Some(best) = race(job, &mut |job, overlapped| {
            let mut discard = |_: u32, _: &[u8; 32], _: bool| {};
            if overlapped {
                scan_overlapped(job, 0, 4096, 0, &mut discard)
            } else {
                scan_plain(job, 0, 4096, 0, &mut discard)
            }
        }) else {
            return false;
        };
        let faster = best[1] * 100 < best[0] * 99;
        log(&format!(
            "[miner] overlapped SHA transfers: {} ({:.1} against {:.1} kH/s, boot scan)",
            if faster { "kept" } else { "not faster" },
            4096.0 * 1000.0 / best[1] as f64,
            4096.0 * 1000.0 / best[0] as f64
        ));
        faster
    }

    fn random_header(seed: &mut u32) -> [u8; 80] {
        let mut header = [0u8; 80];
        for byte in header.iter_mut() {
            *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *byte = (*seed >> 24) as u8;
        }
        header
    }

    fn reference(header: &[u8; 80], nonce: u32) -> [u8; 32] {
        let mut h = *header;
        h[76..80].copy_from_slice(&nonce.to_le_bytes());
        sha256d(&h)
    }

    /// With every nonce a hit, the kernels write whole blocks after each
    /// one. Here about one nonce in eight is a hit: the hashes between them
    /// come from the fast path, and the exact set of hits must match the
    /// software digests.
    fn verify_hit_set() -> bool {
        const COUNT: u32 = 256;
        const LIMIT: u32 = u32::MAX / 8;
        let mut seed = 0x3c5a_7e91u32;
        for first in [0, 0xff80, u32::MAX - 127] {
            // carries and wrap
            let header = random_header(&mut seed);
            let (mut hits, mut correct) = (0u32, true);
            let mut job = Job::default();
            prepare(&mut job, &header);
            let done = scan(
                &mut job,
                first,
                COUNT,
                LIMIT,
                &mut |nonce, digest, _| {
                    if reference(&header, nonce) != *digest {
                        correct = false;
                    }
                    hits += 1;
                },
                None,
            );
            let expected =
                (0..COUNT).filter(|&i| top32(&reference(&header, first.wrapping_add(i))) <= LIMIT).count() as u32;
            if done != COUNT || !correct || hits != expected {
                return false;
            }
        }
        true
    }

    /// A single rare genesis candidate cannot detect every missed or
    /// misordered transfer: complete digests at easy targets, nonce carries
    /// and wrap, across different midstates, before timing either kernel.
    fn verify_resume_transfers() -> bool {
        let mut seed = 0xa5b6_c7d8u32;
        for first in [0, 0xfffc, u32::MAX - 3] {
            let header = random_header(&mut seed);
            let (mut next, mut hits, mut correct) = (first, 0u32, true);
            let mut job = Job::default();
            prepare(&mut job, &header);
            let done = scan(
                &mut job,
                first,
                8,
                u32::MAX,
                &mut |nonce, digest, _| {
                    if nonce != next || reference(&header, nonce) != *digest {
                        correct = false;
                    }
                    next = next.wrapping_add(1);
                    hits += 1;
                },
                None,
            );
            if done != 8 || hits != 8 || !correct {
                return false;
            }
        }
        true
    }

    /// The kernel choice after the genesis check passed (`correct`) or not;
    /// `again` reruns the whole self-test with the choice made so far.
    pub fn select(correct: bool, job: &mut Job, again: &dyn Fn() -> bool) -> bool {
        let correct = correct && verify_resume_transfers() && verify_hit_set();
        if !correct && OVERLAPPED.load(Ordering::Relaxed) {
            OVERLAPPED.store(false, Ordering::Relaxed);
            log("[miner] overlapped SHA transfers failed their self-test; using the other kernels");
            return again();
        }
        if !correct && DIRECT.load(Ordering::Relaxed) {
            DIRECT.store(false, Ordering::Relaxed);
            log("[miner] direct SHA transfers failed their self-test; retrying buffered transfers");
            return again();
        }
        if !correct && SPARSE.load(Ordering::Relaxed) {
            // A chip/SDK which does not retain TEXT padding keeps its working
            // hardware path: retry the known answer with complete block
            // writes.
            SPARSE.store(false, Ordering::Relaxed);
            log("[miner] SHA padding reuse failed its self-test; retrying full block writes");
            return again();
        }
        // Time the others first (their choice is the fallback), then this
        // one.
        if correct && OVERLAPPED.load(Ordering::Relaxed) {
            OVERLAPPED.store(false, Ordering::Relaxed);
            let others = again();
            OVERLAPPED.store(true, Ordering::Relaxed);
            if !others {
                return true; // this one passed every check on its own
            }
            if !prefer_overlapped(job) {
                OVERLAPPED.store(false, Ordering::Relaxed);
            }
            return true; // the other kernels were just validated
        }
        if correct && DIRECT.load(Ordering::Relaxed) && !prefer_direct(job) {
            DIRECT.store(false, Ordering::Relaxed);
            return again(); // validate the selected buffered path as well
        }
        correct
    }

    pub fn pick() -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Original ESP32: cycle-counted assembly loops
// ---------------------------------------------------------------------------

#[cfg(esp32)]
core::arch::global_asm!(include_str!(concat!(env!("OUT_DIR"), "/hw_sha_classic.s")), options(raw));
#[cfg(esp32)]
core::arch::global_asm!(include_str!(concat!(env!("OUT_DIR"), "/hw_sha_fill.s")), options(raw));

#[cfg(esp32)]
mod engine {
    use std::cell::UnsafeCell;
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicUsize, Ordering};

    use bdlminer_core::fill::{Buffers, RECORD_BYTES, RECORD_WORDS, STEPS};
    use bdlminer_core::safety::fixed_filter_supports;
    use bdlminer_core::sha256::{MidState, IV, K};

    use super::{log, top32, Hit};

    /// What the loops in hw_sha_classic.S work from (CTX_* offsets there);
    /// the fields from `fill_p` on are only used by hw_sha_fill.S (CTX in
    /// tools/hw_fill/gen_hw_fill.py). Addresses are 32-bit on the ESP32.
    #[repr(C)]
    #[derive(Default)]
    struct Ctx {
        /// Header words as the engine takes them (big-endian).
        hdr: [u32; 20],
        /// In: first position; out: the next one.
        nonce: u32,
        /// In: positions to try; out: positions not tried.
        budget: u32,
        /// Read every nonce: clearing it stops the loop.
        run: u32,
        /// 0x80000000.
        pad: u32,
        /// Only used by the original's benchmark builds.
        chk: u32,
        /// Software hash: current record.
        fill_p: u32,
        /// Software hash: position of the nonce in progress.
        q: u32,
        /// Software candidate: position, and set when there is one.
        hit_q: u32,
        hit_f: u32,
        /// Last record of each hash.
        end1: u32,
        end2: u32,
        /// First record of each hash, record 61 of the first.
        b1: u32,
        b2: u32,
        b1_61: u32,
        iv7: u32,
        c18: u32,
        mid: [u32; 8],
    }

    const _: () = {
        use core::mem::offset_of;
        // Layout known to hw_sha_classic.S.
        assert!(offset_of!(Ctx, nonce) == 80 && offset_of!(Ctx, budget) == 84 && offset_of!(Ctx, run) == 88);
        assert!(offset_of!(Ctx, pad) == 92 && offset_of!(Ctx, chk) == 96);
        // Layout known to hw_sha_fill.S.
        assert!(offset_of!(Ctx, fill_p) == 100 && offset_of!(Ctx, q) == 104 && offset_of!(Ctx, hit_q) == 108);
        assert!(offset_of!(Ctx, hit_f) == 112 && offset_of!(Ctx, end1) == 116 && offset_of!(Ctx, b1) == 124);
        assert!(offset_of!(Ctx, b1_61) == 132 && offset_of!(Ctx, iv7) == 136 && offset_of!(Ctx, c18) == 140);
        assert!(offset_of!(Ctx, mid) == 144);
    };

    /// Returns 1 on a candidate (low 16 bits of the last digest word zero,
    /// i.e. the top 16 bits of the hash) at position ctx.nonce - 1, 0 when
    /// the budget is spent or ctx.run was cleared. The nonce word is written
    /// to the engine as-is, which is why positions are byte-swapped nonces.
    type Loop = unsafe extern "C" fn(text: *mut u32, ctx: *mut Ctx) -> u32;

    extern "C" {
        fn hwsha_classic_fast(text: *mut u32, ctx: *mut Ctx) -> u32;
        fn hwsha_classic_margin(text: *mut u32, ctx: *mut Ctx) -> u32;
        fn hwsha_classic_wide(text: *mut u32, ctx: *mut Ctx) -> u32;
        /// The fast loop with a software hash in its idle cycles.
        fn hwsha_classic_fill(text: *mut u32, ctx: *mut Ctx) -> u32;

        // components/mbedtls/port/sha/parallel_engine: one lock per engine.
        fn esp_sha_try_lock_engine(engine: u32) -> bool;
        fn esp_sha_lock_engine(engine: u32);
        fn esp_sha_unlock_engine(engine: u32);
        // The xtensa component's HAL.
        fn xthal_get_ccount() -> u32;
        fn esp_rom_get_cpu_ticks_per_us() -> u32;
        // esp_chip_info.h (not in the bindings).
        fn esp_chip_info(out: *mut ChipInfo);
    }

    /// esp_chip_info_t.
    #[repr(C)]
    #[allow(dead_code)]
    struct ChipInfo {
        model: u32,
        features: u32,
        /// Major * 100 + minor.
        revision: u16,
        cores: u8,
    }

    /// Fastest first (247, 257 and 317 cycles per nonce); the slower ones
    /// only add timing margin, for chips on which the measured schedule
    /// fails.
    const LOOPS: [(Loop, &str); 3] = [
        (hwsha_classic_fast, "fast"),
        (hwsha_classic_margin, "fast, +2 cycle margins"),
        (hwsha_classic_wide, "wide margins"),
    ];
    /// The selected loop; LOOPS.len() = none usable.
    static CURRENT: AtomicUsize = AtomicUsize::new(LOOPS.len());

    /// soc/hwcrypto_reg.h: SHA_TEXT_BASE (DR_REG_SHA_BASE).
    fn text() -> *mut u32 {
        0x3FF0_3000 as *mut u32
    }

    /// Header words 0x9E3779B9 * (i + 1) and these, the only candidates
    /// among the 262144 positions from 0x1000 (checked on a PC with the
    /// software kernel). One wrong hash that happens to be (or to hide) a
    /// candidate fails the test.
    const EXPECTED: [u32; 6] = [0xa94c, 0xe496, 0x1f476, 0x302f0, 0x35af9, 0x3cf5c];

    fn kat() -> ([u32; 20], [u8; 80]) {
        let mut words = [0u32; 20];
        let mut header = [0u8; 80];
        for i in 0..20 {
            let w = if i < 19 { 0x9E37_79B9u32.wrapping_mul(i as u32 + 1) } else { 0 };
            words[i] = w;
            header[4 * i..4 * i + 4].copy_from_slice(&w.to_be_bytes());
        }
        (words, header)
    }

    fn known_answer(f: Loop) -> bool {
        let mut c =
            Ctx { hdr: kat().0, nonce: 0x1000, budget: 262_144, run: 1, pad: 0x8000_0000, ..Default::default() };
        let mut found = 0;
        while c.budget > 0 && unsafe { f(text(), &mut c) } != 0 {
            if found == EXPECTED.len() || c.nonce.wrapping_sub(1 + 0x1000) != EXPECTED[found] {
                return false;
            }
            found += 1;
        }
        found == EXPECTED.len() && c.budget == 0
    }

    // The ESP32's SHA-1, SHA-256 and SHA-384/512 engines are locked one by
    // one but share the TEXT registers the loops write: all three are taken
    // while a loop runs, so mbedtls (TLS) hashes in software meanwhile
    // instead of writing TEXT under the loop (which made every TLS handshake
    // fail). Always in this order: no deadlock with code that takes them one
    // at a time. esp_sha_type on the ESP32: SHA1 0, SHA2_256 1, SHA2_384 2.
    const ENGINES: [u32; 3] = [0, 1, 2];

    fn try_lock_engines() -> bool {
        for (i, &e) in ENGINES.iter().enumerate() {
            if !unsafe { esp_sha_try_lock_engine(e) } {
                for &held in ENGINES[..i].iter().rev() {
                    unsafe { esp_sha_unlock_engine(held) };
                }
                return false;
            }
        }
        true
    }

    fn lock_engines() {
        for e in ENGINES {
            unsafe { esp_sha_lock_engine(e) };
        }
    }

    fn unlock_engines() {
        for e in ENGINES {
            unsafe { esp_sha_unlock_engine(e) };
        }
    }

    fn reset_peripheral() {
        unsafe { esp_idf_svc::sys::periph_module_reset(esp_idf_svc::sys::periph_module_t_PERIPH_SHA_MODULE) };
    }

    // ------------------------------------------------------------------
    // Idle-cycle software hashing (hw_sha_fill.S)
    // ------------------------------------------------------------------

    /// The generated code's buffers: one Fill in use at a time (the mining
    /// task's), as in the C++ firmware.
    struct FillBuffers(UnsafeCell<Buffers>);
    unsafe impl Sync for FillBuffers {}
    static FILL_BUF: FillBuffers = FillBuffers(UnsafeCell::new(Buffers {
        b1: [0; bdlminer_core::fill::B1_RECORDS * RECORD_WORDS],
        b2: [0; bdlminer_core::fill::B2_RECORDS * RECORD_WORDS],
        mid: [0; 8],
        c18: 0,
    }));
    static FILL_OK: AtomicBool = AtomicBool::new(false);

    fn buf() -> &'static mut Buffers {
        unsafe { &mut *FILL_BUF.0.get() }
    }

    fn addr<T>(p: *const T) -> u32 {
        p as usize as u32
    }

    /// The second work hashed in the idle cycles of the hardware loop.
    #[derive(Default)]
    pub struct Fill {
        /// To confirm its candidates.
        check: MidState,
        /// State of the generated code between scans.
        p: u32,
        q: u32,
        /// Nonces done; the caller adds them up and resets it.
        pub hashes: u32,
        ready: bool,
    }

    /// Points the generated code at the buffers and resumes `fill`.
    fn fill_load(c: &mut Ctx, f: &Fill) {
        let b = buf();
        c.fill_p = f.p;
        c.q = f.q;
        c.hit_f = 0;
        c.end1 = addr(b.b1.as_ptr().wrapping_add(STEPS * RECORD_WORDS));
        c.end2 = addr(b.b2.as_ptr().wrapping_add(STEPS * RECORD_WORDS));
        c.b1 = addr(b.b1.as_ptr());
        c.b2 = addr(b.b2.as_ptr());
        c.b1_61 = addr(b.b1.as_ptr().wrapping_add(61 * RECORD_WORDS));
        c.iv7 = IV[7];
        c.c18 = b.c18;
        c.mid = b.mid;
    }

    fn fill_start(f: &mut Fill, header: &[u8; 80], first: u32) {
        let b = buf();
        b.prepare(header);
        f.check = MidState::new(header);
        // As after a second hash: the first iteration starts nonce `first`.
        f.p = addr(b.b2.as_ptr().wrapping_add(STEPS * RECORD_WORDS));
        f.q = first.wrapping_sub(1);
        f.ready = true;
    }

    fn kat_ctx() -> Ctx {
        Ctx { hdr: kat().0, run: 1, pad: 0x8000_0000, ..Default::default() }
    }

    /// The hardware part of hw_sha_fill.S is the fast loop instruction for
    /// instruction, so the known answer must pass with the software hash
    /// running, which must meanwhile find the one candidate of its range.
    fn fill_self_test() -> bool {
        let header = kat().1;
        let mut c = kat_ctx();
        let mut f = Fill::default();
        fill_start(&mut f, &header, 0x1000 + 0xa94c - 3);
        fill_load(&mut c, &f);
        c.nonce = 0x1000;
        c.budget = 262_144;
        let (mut found, mut fill_hits, mut wrong_hw, mut first_hit) = (0usize, 0, 0, 0u32);
        let q0 = c.q;
        while c.budget > 0 {
            let hw = unsafe { hwsha_classic_fill(text(), &mut c) } != 0;
            if c.hit_f != 0 {
                if fill_hits == 0 {
                    first_hit = c.hit_q;
                }
                fill_hits += 1;
                c.hit_f = 0;
            }
            if !hw {
                break;
            }
            if found >= 6 || c.nonce.wrapping_sub(1 + 0x1000) != EXPECTED[found] {
                wrong_hw += 1;
            }
            found += 1;
        }
        let ok = wrong_hw == 0 && found == 6 && c.budget == 0 && fill_hits == 1 && first_hit == 0x1000 + 0xa94c;
        if !ok {
            log(&format!(
                "[fill] self-test: hardware {found} candidates ({wrong_hw} wrong, want 6), budget left {}; software {} nonces, {fill_hits} candidates (first {first_hit:08x}, want 0000b94c)",
                c.budget,
                c.q.wrapping_sub(q0)
            ));
        }
        ok
    }

    /// After a failed self-test: runs the software hash for exactly one
    /// nonce (64 hardware nonces) and compares every round with a reference
    /// computed here.
    fn fill_diagnose() {
        fn rounds(st: &[u32; 8], w: &[u32; 64], a_out: &mut [u32; 65], e_out: &mut [u32; 65]) {
            let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *st;
            for i in 0..=64 {
                a_out[i] = a;
                e_out[i] = e;
                if i == 64 {
                    break;
                }
                let t1 = h
                    .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
                    .wrapping_add(g ^ (e & (f ^ g)))
                    .wrapping_add(K[i])
                    .wrapping_add(w[i]);
                let t2 =
                    (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22)).wrapping_add((a & b) | (c & (a | b)));
                h = g;
                g = f;
                f = e;
                e = d.wrapping_add(t1);
                d = c;
                c = b;
                b = a;
                a = t1.wrapping_add(t2);
            }
        }
        fn expand(w: &mut [u32; 64]) {
            for i in 16..64 {
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                w[i] = s1.wrapping_add(w[i - 7]).wrapping_add(s0).wrapping_add(w[i - 16]);
            }
        }

        let header = kat().1;
        let mut c = kat_ctx();
        let q = 0x1000 + 0xa94c; // a candidate
        let mut fl = Fill::default();
        fill_start(&mut fl, &header, q);
        fill_load(&mut c, &fl);
        c.nonce = 0x4000_0000;
        c.budget = 64; // transition B, 31 iterations, transition A, 31 iterations
        while c.budget > 0 && unsafe { hwsha_classic_fill(text(), &mut c) } != 0 {}
        log(&format!(
            "[fill] after 64 iterations: q {:08x} (want {q:08x}), P {} records from the end of hash 2 (want 0)",
            c.q,
            (c.fill_p as i32 - c.end2 as i32) / RECORD_BYTES as i32
        ));

        let ms = MidState::new(&header);
        let mut w = [0u32; 64];
        w[..5].copy_from_slice(&[ms.w[0], ms.w[1], ms.w[2], q, 0x8000_0000]);
        w[15] = 640;
        expand(&mut w);
        let (mut a1, mut e1) = ([0u32; 65], [0u32; 65]);
        rounds(&ms.mid, &w, &mut a1, &mut e1);
        let b1 = &buf().b1;
        let bad = (3..64).find(|&k| b1[k * 5 + 2] != w[k]);
        log(&match bad {
            None => "[fill] hash 1 message words: all right".to_string(),
            Some(k) => format!("[fill] hash 1 message words: first wrong at {k}"),
        });
        match (4..=64).find(|&k| b1[k * 5] != a1[k] || b1[k * 5 + 1] != e1[k]) {
            Some(k) => log(&format!(
                "[fill] hash 1 state: first wrong before round {k}: a {:08x}/{:08x} e {:08x}/{:08x}",
                b1[k * 5],
                a1[k],
                b1[k * 5 + 1],
                e1[k]
            )),
            None => log("[fill] hash 1 state: all right"),
        }

        let h1 = [a1[64], a1[63], a1[62], a1[61], e1[64], e1[63], e1[62], e1[61]];
        let mut w2 = [0u32; 64];
        for k in 0..8 {
            w2[k] = ms.mid[k].wrapping_add(h1[k]);
        }
        w2[8] = 0x8000_0000;
        w2[15] = 256;
        expand(&mut w2);
        let (mut a2, mut e2) = ([0u32; 65], [0u32; 65]);
        rounds(&IV, &w2, &mut a2, &mut e2);
        let b2 = &buf().b2; // record k at k + 3
        let bad = (0..62).find(|&k| b2[(k + 3) * 5 + 2] != w2[k]);
        log(&match bad {
            None => "[fill] hash 2 message words: all right".to_string(),
            Some(k) => format!("[fill] hash 2 message words: first wrong at {k}"),
        });
        match (1..=62).find(|&k| b2[(k + 3) * 5] != a2[k] || b2[(k + 3) * 5 + 1] != e2[k]) {
            Some(k) => log(&format!(
                "[fill] hash 2 state: first wrong before round {k}: a {:08x}/{:08x} e {:08x}/{:08x}",
                b2[(k + 3) * 5],
                a2[k],
                b2[(k + 3) * 5 + 1],
                e2[k]
            )),
            None => log(&format!(
                "[fill] hash 2 state: all right (candidate: {})",
                if e2[61].wrapping_add(IV[7]) & 0xFFFF == 0 { "yes" } else { "no" }
            )),
        }
    }

    /// Cycles for `n` hardware nonces with `f`, the best of two runs, and
    /// the software nonces done meanwhile.
    fn time_loop(f: Loop, n: u32) -> (u32, u32) {
        let mut best = u32::MAX;
        let mut fill_nonces = 0;
        for _ in 0..2 {
            let header = kat().1;
            let mut c = kat_ctx();
            let mut fl = Fill::default();
            fill_start(&mut fl, &header, 0x8000_0000);
            fill_load(&mut c, &fl);
            c.nonce = 0x4000_0000; // a candidate only returns early: the loop goes on
            c.budget = n;
            let t0 = unsafe { xthal_get_ccount() };
            while c.budget > 0 && unsafe { f(text(), &mut c) } != 0 {}
            let t = unsafe { xthal_get_ccount() }.wrapping_sub(t0);
            best = best.min(t);
            fill_nonces = c.q.wrapping_sub(0x8000_0000u32.wrapping_sub(1));
        }
        (best, fill_nonces)
    }

    /// Uses idle-cycle hashing only when it is correct and makes the total
    /// faster.
    fn pick_fill() {
        if !fill_self_test() && !fill_self_test() {
            log("[miner] idle-cycle software hashing failed its self-test, disabled");
            fill_diagnose();
            return;
        }
        let n = 131_072u32;
        let (plain, _) = time_loop(hwsha_classic_fast, n);
        let (mixed, fill_nonces) = time_loop(hwsha_classic_fill, n);
        let mhz = unsafe { esp_rom_get_cpu_ticks_per_us() } as f64;
        let hw_plain = n as f64 * mhz * 1e3 / plain as f64; // kH/s
        let hw_mixed = n as f64 * mhz * 1e3 / mixed as f64;
        let extra = fill_nonces as f64 * mhz * 1e3 / mixed as f64;
        let ok = hw_mixed + extra > hw_plain * 1.002;
        FILL_OK.store(ok, Ordering::Relaxed);
        log(&format!(
            "[miner] idle-cycle software hashing: {} (hardware {hw_plain:.1} -> {hw_mixed:.1} kH/s, +{extra:.1} kH/s software)",
            if ok { "on" } else { "off, no gain" }
        ));
    }

    /// Picks the fastest loop this chip runs correctly.
    pub fn pick() -> bool {
        // Before revision 3, a raw DPORT read on one core (the loops read the
        // digest that way) can corrupt APB reads on the other one: WiFi and
        // UART die within seconds, which no self-test can see.
        let mut chip = ChipInfo { model: 0, features: 0, revision: 0, cores: 0 };
        unsafe { esp_chip_info(&mut chip) };
        let revision = chip.revision as u32; // major * 100 + minor
        if revision / 100 < 3 {
            log(&format!(
                "[miner] ESP32 revision {}.{}: the hardware SHA loop needs revision 3",
                revision / 100,
                revision % 100
            ));
            return false;
        }
        lock_engines();
        reset_peripheral();
        let mut pick = 0;
        // Best of three: one stray misread must not cost the fast loop for
        // good.
        while pick < LOOPS.len() && !(0..3).any(|_| known_answer(LOOPS[pick].0)) {
            log(&format!("[miner] hardware SHA loop '{}' failed its self-test", LOOPS[pick].1));
            pick += 1;
        }
        CURRENT.store(pick, Ordering::Relaxed);
        if pick < LOOPS.len() {
            log(&format!("[miner] hardware SHA loop: {}", LOOPS[pick].1));
        }
        if pick == 0 {
            pick_fill(); // built on the fast loop only
        }
        unlock_engines();
        pick < LOOPS.len()
    }

    /// Every candidate is recomputed in software, so the share of candidates
    /// that software does not confirm measures how many hashes the loop gets
    /// wrong. It is 0 when healthy; otherwise fall back to the next, slower
    /// loop, and to software alone after the last one.
    fn health(confirmed: bool) {
        static HITS: AtomicU32 = AtomicU32::new(0);
        static WRONG: AtomicU32 = AtomicU32::new(0);
        let hits = HITS.fetch_add(1, Ordering::Relaxed) + 1;
        let wrong = WRONG.fetch_add(!confirmed as u32, Ordering::Relaxed) + !confirmed as u32;
        if hits < 256 {
            return;
        }
        if wrong >= 8 {
            let next = CURRENT.load(Ordering::Relaxed) + 1;
            log(&format!(
                "[miner] {wrong} of {hits} hardware SHA candidates were wrong, switching to: {}",
                LOOPS.get(next).map_or("software", |l| l.1)
            ));
            CURRENT.store(next, Ordering::Relaxed);
        }
        HITS.store(0, Ordering::Relaxed);
        WRONG.store(0, Ordering::Relaxed);
    }

    pub fn available() -> bool {
        true
    }

    pub fn nonce_at(position: u32) -> u32 {
        position.swap_bytes()
    }

    // The loops hold all three SHA engines (see lock_engines()), so mbedtls
    // hashes in software and a TLS handshake runs alongside them. But scan()
    // used to free them after every batch, and mbedtls, retrying at each
    // digest, would sooner or later get one in that gap and keep it for the
    // whole handshake (its transcript hash): the hardware loop then stopped
    // for 1.5 s, and the WiFi task, waiting on the TLS task meanwhile, lent
    // it priority 23, which froze everything else on core 0 (screen, web
    // server). Between hold() and release(), scan() keeps the engines from
    // one batch to the next.
    static HOLDS: AtomicI32 = AtomicI32::new(0); // TLS tasks and an update download may overlap
    static MINER_HOLDS: AtomicBool = AtomicBool::new(false); // only touched by the mining task

    pub fn hold() {
        HOLDS.fetch_add(1, Ordering::SeqCst);
    }

    pub fn release() {
        HOLDS.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn idle() {
        if MINER_HOLDS.swap(false, Ordering::SeqCst) {
            unlock_engines();
        }
    }

    pub fn fill_available() -> bool {
        FILL_OK.load(Ordering::Relaxed)
    }

    pub fn prepare_fill(f: &mut Fill, header: &[u8; 80]) {
        fill_start(f, header, 0);
    }

    pub struct Job {
        /// The header as big-endian engine words.
        words: [u32; 20],
        /// Software midstate to confirm candidates.
        check: MidState,
    }

    impl Default for Job {
        fn default() -> Self {
            Job { words: [0; 20], check: MidState::default() }
        }
    }

    pub fn prepare(job: &mut Job, header: &[u8; 80]) {
        for i in 0..20 {
            job.words[i] = u32::from_be_bytes([header[4 * i], header[4 * i + 1], header[4 * i + 2], header[4 * i + 3]]);
        }
        job.check = MidState::new(header);
    }

    /// Hands a software candidate of the idle-cycle hash over, once
    /// confirmed.
    fn fill_hit(c: &mut Ctx, f: &Fill, limit: u32, hit: &mut dyn FnMut(u32, &[u8; 32], bool)) {
        if c.hit_f == 0 {
            return;
        }
        c.hit_f = 0;
        let nonce = nonce_at(c.hit_q);
        let Some(hash) = f.check.scan(nonce, 0x0000_FFFF) else {
            FILL_OK.store(false, Ordering::Relaxed);
            log("[miner] idle-cycle software hashing reported a wrong candidate, disabled");
            return;
        };
        if top32(&hash) <= limit {
            hit(nonce, &hash, true);
        }
    }

    pub fn scan(job: &mut Job, first: u32, count: u32, limit: u32, hit: Hit, fill: Option<&mut Fill>) -> u32 {
        if !fixed_filter_supports(limit) {
            idle();
            return 0;
        }
        let current = CURRENT.load(Ordering::Relaxed);
        if current >= LOOPS.len() {
            // health() gave up on every loop: software mines from now on, so
            // the engines this task may still hold go back to TLS.
            idle();
            return 0;
        }
        // An open TLS connection can hold the engine for seconds (mbedtls
        // then keeps it for the whole digest); never wait for it.
        if !MINER_HOLDS.load(Ordering::Relaxed) && !try_lock_engines() {
            return 0;
        }
        MINER_HOLDS.store(true, Ordering::Relaxed);
        // Start from a clean peripheral, whatever used it last.
        reset_peripheral();

        let mut c = Ctx { hdr: job.words, nonce: first, budget: count, run: 1, pad: 0x8000_0000, ..Default::default() };
        let fill = fill.filter(|f| f.ready && FILL_OK.load(Ordering::Relaxed) && current == 0);
        let f: Loop = if fill.is_some() { hwsha_classic_fill } else { LOOPS[current].0 };
        let mut q0 = 0;
        if let Some(fl) = &fill {
            fill_load(&mut c, fl);
            q0 = c.q;
        }
        while c.budget > 0 && unsafe { f(text(), &mut c) } != 0 {
            if let Some(fl) = &fill {
                fill_hit(&mut c, fl, limit, hit);
            }
            // The loop only looked at 16 bits of the digest: recompute the
            // hash.
            let nonce = nonce_at(c.nonce.wrapping_sub(1));
            let confirmed = job.check.scan(nonce, 0x0000_FFFF);
            health(confirmed.is_some());
            if let Some(hash) = confirmed {
                if top32(&hash) <= limit {
                    hit(nonce, &hash, false);
                }
            }
        }
        if let Some(fl) = fill {
            fill_hit(&mut c, fl, limit, hit);
            fl.p = c.fill_p;
            fl.q = c.q;
            fl.hashes = fl.hashes.wrapping_add(c.q.wrapping_sub(q0));
        }
        if HOLDS.load(Ordering::SeqCst) == 0 {
            idle();
        }
        count - c.budget
    }

    pub fn select(correct: bool, _job: &mut Job, _again: &dyn Fn() -> bool) -> bool {
        correct
    }
}

// ---------------------------------------------------------------------------
// No usable SHA peripheral
// ---------------------------------------------------------------------------

#[cfg(not(any(esp32, esp32s2, esp32s3, esp32c3)))]
mod engine {
    use super::Hit;

    #[derive(Default)]
    pub struct Job;
    #[derive(Default)]
    pub struct Fill {
        pub hashes: u32,
    }
    pub fn available() -> bool {
        false
    }
    pub fn nonce_at(position: u32) -> u32 {
        position
    }
    pub fn hold() {}
    pub fn release() {}
    pub fn idle() {}
    pub fn prepare(_: &mut Job, _: &[u8; 80]) {}
    pub fn fill_available() -> bool {
        false
    }
    pub fn prepare_fill(_: &mut Fill, _: &[u8; 80]) {}
    pub fn scan(_: &mut Job, _: u32, _: u32, _: u32, _: Hit, _: Option<&mut Fill>) -> u32 {
        0
    }
    pub fn pick() -> bool {
        false
    }
    pub fn select(correct: bool, _: &mut Job, _: &dyn Fn() -> bool) -> bool {
        correct
    }
}

/// Checks the peripheral against known answers: the genesis block, then
/// (S2/S3/C3) complete digests and hit sets, each kernel timed. The hardware
/// path is only used if this passes.
fn self_test() -> bool {
    if !engine::available() || !engine::pick() {
        return false;
    }
    genesis_test()
}

fn genesis_test() -> bool {
    const GENESIS: &str = "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c";
    const NONCE: u32 = 2_083_236_893;
    let header: [u8; 80] = from_hex_array(GENESIS).expect("genesis header");
    let mut job = engine::Job::default();
    engine::prepare(&mut job, &header);
    let (mut hits, mut found, mut hash) = (0, 0u32, [0u8; 32]);
    // nonce_at() is its own inverse: this is the genesis nonce's position.
    let pos = engine::nonce_at(NONCE);
    engine::scan(
        &mut job,
        pos.wrapping_sub(1000),
        2000,
        quick_limit_for(1.0),
        &mut |nonce, h, _| {
            found = nonce;
            hash = *h;
            hits += 1;
        },
        None,
    );
    let correct = hits == 1 && found == NONCE && hash == sha256d(&header);
    engine::select(correct, &mut job, &genesis_test)
}

/// What `init` found.
pub struct Capabilities {
    /// The peripheral passed its self-test: a hardware miner can start.
    pub hardware: bool,
    /// Original ESP32: idle-cycle hashing of a second work is correct and
    /// faster.
    pub fill: bool,
}

/// Self-tests the peripheral and the assembly. Call once at boot, before any
/// miner starts. `logger` receives the log lines.
pub fn init(logger: impl Fn(&str) + Send + Sync + 'static) -> Capabilities {
    let _ = LOG.set(Box::new(logger));
    let supported = engine::available();
    let hardware = supported && self_test();
    // The classic ESP32 self-test can leave the engines reserved between
    // batches: release them before the miners start.
    engine::idle();
    log(&format!(
        "[miner] hardware SHA: {}",
        if !supported {
            "not supported"
        } else if hardware {
            "self-test OK"
        } else {
            "self-test FAILED, disabled"
        }
    ));
    let fill = hardware && engine::fill_available();
    let asm = PREFILTER_BUILT && prefilter_self_test();
    if PREFILTER_BUILT {
        let kernel = if asm { "assembly (self-test OK)" } else { "Rust (assembly self-test FAILED)" };
        log(&format!("[miner] software SHA: {kernel}"));
    }
    USE_ASM.store(asm, Ordering::Relaxed);
    Capabilities { hardware, fill }
}

/// Around TLS handshakes and HTTPS requests. S2/S3/C3: keeps the mining loop
/// off the peripheral (it hashes in software meanwhile). Original ESP32: the
/// miner keeps the engines between batches, so mbedtls hashes in software.
pub struct Hold;

impl Hold {
    pub fn new() -> Self {
        engine::hold();
        Hold
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        engine::release();
    }
}

/// Header words 0x9E3779B9 * (i + 1): the hardware known answer's header.
fn known_answer_header() -> [u8; 80] {
    let mut header = [0u8; 80];
    for i in 0..19 {
        let w = 0x9E37_79B9u32.wrapping_mul(i as u32 + 1);
        header[4 * i..4 * i + 4].copy_from_slice(&w.to_be_bytes());
    }
    header
}

/// Compares the assembly with `MidState::scan` on the 6 known candidates of
/// the hardware known-answer test and the 512 nonces after each (as the C++
/// firmware's prefilterSelfTest()).
fn prefilter_self_test() -> bool {
    const CANDIDATES: [u32; 6] = [0xa94c, 0xe496, 0x1f476, 0x302f0, 0x35af9, 0x3cf5c];
    let header = known_answer_header();
    let ms = MidState::new(&header);
    let mut tail = [0u8; 16];
    tail.copy_from_slice(&header[64..80]);
    let mut hits = 0;
    for c in CANDIDATES {
        let first = (0x1000u32 + c).swap_bytes();
        for n in first..first.wrapping_add(513) {
            tail[12..16].copy_from_slice(&n.to_le_bytes());
            let fast = prefilter(&ms, &tail);
            if fast != ms.scan(n, 0x0000_FFFF).is_some() {
                return false;
            }
            hits += fast as u32;
        }
    }
    hits >= 6
}

/// The software kernel over positions `first..first + count`: the assembly
/// prefilter for the fixed 16-bit filter, the portable kernel for easier
/// targets. A hardware miner keeps to the engine's nonce order
/// (`hardware_order`) so that no nonce is hashed twice.
fn software_batch(miner: &Miner, w: &MiningWork, first: u32, count: u32, limit: u32, hardware_order: bool) {
    let mid = &w.work.mid;
    let nonce_at = |p: u32| if hardware_order { engine::nonce_at(p) } else { p };
    if USE_ASM.load(Ordering::Relaxed) && limit <= BEST_DIFF_FILTER {
        let mut tail = [0u8; 16];
        tail.copy_from_slice(&w.work.header[64..80]);
        for p in first..first.wrapping_add(count) {
            let n = nonce_at(p);
            tail[12..16].copy_from_slice(&n.to_le_bytes());
            if prefilter(mid, &tail) {
                if let Some(hash) = mid.scan(n, limit) {
                    miner.on_candidate(n, &hash, w);
                }
            }
        }
        return;
    }
    for p in first..first.wrapping_add(count) {
        let n = nonce_at(p);
        if let Some(hash) = mid.scan(n, limit) {
            miner.on_candidate(n, &hash, w);
        }
    }
}

/// A software miner with the assembly prefilter when it passed its self-test.
pub struct SoftwareEngine;

impl Engine for SoftwareEngine {
    fn batch(&mut self, miner: &Miner, w: &MiningWork, first: u32, _left: u32) -> u32 {
        software_batch(miner, w, first, BATCH_SW, w.work.quick_limit.max(BEST_DIFF_FILTER), false);
        BATCH_SW
    }
}

/// The second work hashed in the idle cycles of the classic ESP32's loop.
struct Fill {
    fill: engine::Fill,
    work: Option<MiningWork>,
    /// Stats slot of engine "fill".
    engine: usize,
}

/// The hardware miner: the peripheral, with the software kernel (in the
/// engine's nonce order) while it is busy elsewhere (TLS) or for pool
/// targets the classic ESP32's fixed filter cannot serve.
pub struct HardwareEngine {
    job: Box<engine::Job>,
    fill: Option<Fill>,
}

impl HardwareEngine {
    /// `fill_engine`: the stats slot for idle-cycle hashing, when
    /// `Capabilities::fill`.
    pub fn new(fill_engine: Option<usize>) -> Self {
        let fill = fill_engine.map(|engine| Fill { fill: engine::Fill::default(), work: None, engine });
        HardwareEngine { job: Box::default(), fill }
    }
}

impl Engine for HardwareEngine {
    fn start(&mut self, miner: &Miner, w: &MiningWork) {
        engine::prepare(&mut self.job, &w.work.header);
        if let Some(fill) = &mut self.fill {
            fill.work = miner.fetch_work().map(|(w2, _)| w2);
            fill.fill = engine::Fill::default();
            // Not ready without a second work: the loop runs without it.
            if let Some(w2) = &fill.work {
                engine::prepare_fill(&mut fill.fill, &w2.work.header);
            }
        }
    }

    fn batch(&mut self, miner: &Miner, w: &MiningWork, first: u32, left: u32) -> u32 {
        let limit = w.work.quick_limit.max(BEST_DIFF_FILTER);
        // Never run past the end of the nonce space (after software batches
        // `first` is only a multiple of BATCH_SW).
        let count = if left != 0 && left < BATCH_HW { left } else { BATCH_HW };
        let (fill, fill_work) = match &mut self.fill {
            Some(f) => (Some(&mut f.fill), f.work.as_ref()),
            None => (None, None),
        };
        let done = engine::scan(
            &mut self.job,
            first,
            count,
            limit,
            &mut |nonce, hash, from_fill| match (from_fill, fill_work) {
                (false, _) => miner.on_candidate(nonce, hash, w),
                (true, Some(w2)) => miner.on_candidate(nonce, hash, w2),
                (true, None) => {}
            },
            fill,
        );
        if let Some(f) = &mut self.fill {
            let hashes = std::mem::take(&mut f.fill.hashes);
            if hashes > 0 {
                miner.add_hashes(f.engine, hashes as u64);
            }
        }
        if done > 0 {
            return done;
        }
        software_batch(miner, w, first, BATCH_SW, limit, true);
        BATCH_SW
    }

    fn idle(&mut self) {
        engine::idle();
    }
}
