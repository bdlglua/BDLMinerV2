//! The ESP32 SHA peripheral and the Xtensa assembly kernels of the C++
//! firmware (src/hw_sha.cpp, hw_sha_classic.S, hw_sha_fill.S,
//! sw_sha_prefilter.S), built unchanged by components/bdl_hwsha and called
//! through its C functions (components/bdl_hwsha/include/bdl_hwsha.h).
//!
//! - ESP32-S2/S3/C3: the peripheral resumes from a saved midstate; nonce
//!   positions are the nonces themselves.
//! - Original ESP32 (revision 3 and later): a cycle-counted assembly loop
//!   keeps the engine busy; position p is nonce `p.swap_bytes()`, only hashes
//!   with top-32 bits <= 0xFFFF are seen, and a second work can be hashed in
//!   software in the loop's idle cycles (engine "fill").

use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use bdlminer_app::miner::{Engine, Miner, MiningWork, BATCH_HW, BATCH_SW, BEST_DIFF_FILTER};
use bdlminer_core::sha256::MidState;

type HitFn = unsafe extern "C" fn(nonce: u32, hash: *const u8, ctx: *mut c_void);
type LogFn = unsafe extern "C" fn(line: *const c_char);

extern "C" {
    fn bdl_hwsha_set_log(f: LogFn);
    fn bdl_hwsha_available() -> bool;
    fn bdl_hwsha_self_test() -> bool;
    fn bdl_hwsha_fill_available() -> bool;
    fn bdl_hwsha_nonce_at(position: u32) -> u32;
    fn bdl_hwsha_job_size() -> usize;
    fn bdl_hwsha_fill_size() -> usize;
    fn bdl_hwsha_job_init(job: *mut c_void);
    fn bdl_hwsha_fill_init(fill: *mut c_void);
    fn bdl_hwsha_prepare(job: *mut c_void, header: *const u8);
    fn bdl_hwsha_prepare_fill(fill: *mut c_void, header: *const u8, ctx: *mut c_void);
    fn bdl_hwsha_fill_take_hashes(fill: *mut c_void) -> u32;
    fn bdl_hwsha_scan(
        job: *mut c_void,
        first: u32,
        count: u32,
        limit: u32,
        hit: HitFn,
        ctx: *mut c_void,
        fill: *mut c_void,
    ) -> u32;
    fn bdl_hwsha_hold();
    fn bdl_hwsha_release();
    fn bdl_hwsha_idle();
    fn bdl_sw_prefilter_available() -> bool;
    fn bdl_sw_prefilter(mid: *const u32, tail: *const u8, bake: *const u32) -> bool;
}

/// Set by `init` when the assembly prefilter passed its self-test.
static USE_ASM: AtomicBool = AtomicBool::new(false);
/// Where the C++ log lines go (the dashboard log).
type Logger = Box<dyn Fn(&str) + Send + Sync>;
static LOG: OnceLock<Logger> = OnceLock::new();

fn log(line: &str) {
    match LOG.get() {
        Some(f) => f(line),
        None => log::info!("{line}"),
    }
}

unsafe extern "C" fn log_line(line: *const c_char) {
    if !line.is_null() {
        log(&CStr::from_ptr(line).to_string_lossy());
    }
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
/// miner starts. `logger` receives the log lines, the C++ code's included.
pub fn init(logger: impl Fn(&str) + Send + Sync + 'static) -> Capabilities {
    let _ = LOG.set(Box::new(logger));
    unsafe { bdl_hwsha_set_log(log_line) };
    let supported = unsafe { bdl_hwsha_available() };
    let hardware = supported && unsafe { bdl_hwsha_self_test() };
    // The classic ESP32 self-test can leave the engines reserved between
    // batches: release them before the miners start.
    unsafe { bdl_hwsha_idle() };
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
    let fill = hardware && unsafe { bdl_hwsha_fill_available() };
    let asm = unsafe { bdl_sw_prefilter_available() } && prefilter_self_test();
    if unsafe { bdl_sw_prefilter_available() } {
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
        unsafe { bdl_hwsha_hold() };
        Hold
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        unsafe { bdl_hwsha_release() };
    }
}

/// Header words 0x9E3779B9 * (i + 1): the known answer of hw_sha.cpp.
fn known_answer_header() -> [u8; 80] {
    let mut header = [0u8; 80];
    for i in 0..19 {
        let w = 0x9E37_79B9u32.wrapping_mul(i as u32 + 1);
        header[4 * i..4 * i + 4].copy_from_slice(&w.to_be_bytes());
    }
    header
}

/// Compares the assembly with `MidState::scan` on the 6 known candidates of
/// hwsha's known-answer test and the 512 nonces after each (as the C++
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
            let fast = unsafe { bdl_sw_prefilter(ms.mid.as_ptr(), tail.as_ptr(), ms.bake.as_ptr()) };
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
    let nonce_at = |p: u32| if hardware_order { unsafe { bdl_hwsha_nonce_at(p) } } else { p };
    if USE_ASM.load(Ordering::Relaxed) && limit <= BEST_DIFF_FILTER {
        let mut tail = [0u8; 16];
        tail.copy_from_slice(&w.work.header[64..80]);
        for p in first..first.wrapping_add(count) {
            let n = nonce_at(p);
            tail[12..16].copy_from_slice(&n.to_le_bytes());
            if unsafe { bdl_sw_prefilter(mid.mid.as_ptr(), tail.as_ptr(), mid.bake.as_ptr()) } {
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

/// What the C++ hit callback hands back to Rust.
struct HitCtx {
    miner: *const Miner,
    work: *const MiningWork,
}

unsafe extern "C" fn on_hit(nonce: u32, hash: *const u8, ctx: *mut c_void) {
    let ctx = &*(ctx as *const HitCtx);
    let hash = &*(hash as *const [u8; 32]);
    (*ctx.miner).on_candidate(nonce, hash, &*ctx.work);
}

/// A C++ object in a buffer of the size the shim reports, 8-byte aligned.
struct Buffer(Vec<u64>);

impl Buffer {
    fn new(size: usize, init: unsafe extern "C" fn(*mut c_void)) -> Self {
        let mut b = Buffer(vec![0u64; size.div_ceil(8)]);
        unsafe { init(b.ptr()) };
        b
    }
    fn ptr(&mut self) -> *mut c_void {
        self.0.as_mut_ptr().cast()
    }
}

/// The second work hashed in the idle cycles of the classic ESP32's loop.
struct Fill {
    buffer: Buffer,
    /// Boxed: the C++ side keeps a pointer to it (Fill::ctx).
    ctx: Box<HitCtx>,
    work: Box<Option<MiningWork>>,
    /// Stats slot of engine "fill".
    engine: usize,
}

/// The hardware miner: the peripheral, with the software kernel (in the
/// engine's nonce order) while it is busy elsewhere (TLS) or for pool
/// targets the classic ESP32's fixed filter cannot serve.
pub struct HardwareEngine {
    job: Buffer,
    fill: Option<Fill>,
}

impl HardwareEngine {
    /// `fill_engine`: the stats slot for idle-cycle hashing, when
    /// `Capabilities::fill`.
    pub fn new(fill_engine: Option<usize>) -> Self {
        let job = Buffer::new(unsafe { bdl_hwsha_job_size() }, bdl_hwsha_job_init);
        let fill = fill_engine.map(|engine| Fill {
            buffer: Buffer::new(unsafe { bdl_hwsha_fill_size() }, bdl_hwsha_fill_init),
            ctx: Box::new(HitCtx { miner: std::ptr::null(), work: std::ptr::null() }),
            work: Box::new(None),
            engine,
        });
        HardwareEngine { job, fill }
    }
}

impl Engine for HardwareEngine {
    fn start(&mut self, miner: &Miner, w: &MiningWork) {
        unsafe { bdl_hwsha_prepare(self.job.ptr(), w.work.header.as_ptr()) };
        if let Some(fill) = &mut self.fill {
            *fill.work = miner.fetch_work().map(|(w2, _)| w2);
            match &*fill.work {
                Some(w2) => {
                    fill.ctx.work = w2;
                    let ctx = &mut *fill.ctx as *mut HitCtx as *mut c_void;
                    unsafe { bdl_hwsha_prepare_fill(fill.buffer.ptr(), w2.work.header.as_ptr(), ctx) };
                }
                // Not ready: the loop runs without it.
                None => unsafe { bdl_hwsha_fill_init(fill.buffer.ptr()) },
            }
        }
    }

    fn batch(&mut self, miner: &Miner, w: &MiningWork, first: u32, left: u32) -> u32 {
        let limit = w.work.quick_limit.max(BEST_DIFF_FILTER);
        // Never run past the end of the nonce space (after software batches
        // `first` is only a multiple of BATCH_SW).
        let count = if left != 0 && left < BATCH_HW { left } else { BATCH_HW };
        let ctx = HitCtx { miner, work: w };
        let fill = match &mut self.fill {
            Some(f) => {
                f.ctx.miner = miner;
                f.buffer.ptr()
            }
            None => std::ptr::null_mut(),
        };
        let done = unsafe {
            bdl_hwsha_scan(self.job.ptr(), first, count, limit, on_hit, &ctx as *const HitCtx as *mut c_void, fill)
        };
        if let Some(f) = &mut self.fill {
            let hashes = unsafe { bdl_hwsha_fill_take_hashes(f.buffer.ptr()) };
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
        unsafe { bdl_hwsha_idle() };
    }
}
