//! The ASIC boards (features `gamma601`, `nerdnos`): the C++ firmware's
//! drivers (src/gamma601.cpp, src/nerdnos.cpp, built by components/bdl_asic)
//! mine through the Rust miner, which hands them work and verifies, counts
//! and submits their results as for the C++ firmware's miner.h.

use std::collections::VecDeque;
use std::ffi::{c_char, CStr};
use std::sync::{Arc, Mutex, OnceLock};

use bdlminer_app::miner::MiningWork;
use bdlminer_app::App;

#[repr(C)]
pub struct Status {
    pub temperature: f32,
    pub regulator_temperature: f32,
    pub input_voltage: f32,
    pub core_voltage: f32,
    pub current: f32,
    pub frequency: f32,
    pub rpm: u16,
    pub regulator_status: u16,
    pub running: bool,
    pub state: *const c_char,
    pub requested_mhz: u16,
    pub stock_fallback: bool,
    pub fallback_reason: *const c_char,
    pub hardware_error_percent: f32,
    pub fan_percent: i16,
    pub requested_fan_percent: u8,
    pub fan_automatic: bool,
    pub fan_override: bool,
    pub fan_reason: *const c_char,
    pub results: u32,
}

impl Default for Status {
    fn default() -> Self {
        // Numbers zero, pointers null: what the drivers fill in.
        unsafe { std::mem::zeroed() }
    }
}

impl Status {
    pub fn state(&self) -> String {
        text(self.state)
    }
}

fn text(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

extern "C" {
    #[cfg(feature = "gamma601")]
    fn bdl_gamma_settings(mhz: u16, mv: u16, fan_auto: bool, fan_percent: u8, max_temp: u8);
    #[cfg(feature = "gamma601")]
    fn bdl_gamma_hold_reset();
    #[cfg(feature = "gamma601")]
    fn bdl_gamma_begin();
    #[cfg(feature = "gamma601")]
    fn bdl_gamma_start() -> bool;
    #[cfg(feature = "gamma601")]
    fn bdl_gamma_fan(automatic: bool, percent: u8, thermal_limit: u8);
    #[cfg(feature = "gamma601")]
    fn bdl_gamma_status(out: *mut Status);
    #[cfg(feature = "gamma601")]
    fn bdl_gamma_temperature() -> f32;
    #[cfg(feature = "nerdnos")]
    fn bdl_nerdnos_begin();
    #[cfg(feature = "nerdnos")]
    fn bdl_nerdnos_start() -> bool;
    #[cfg(feature = "nerdnos")]
    fn bdl_nerdnos_status(out: *mut Status);
    #[cfg(feature = "nerdnos")]
    fn bdl_nerdnos_temperature() -> f32;
}

/// The miner the drivers report to, and its "asic" engine.
static APP: OnceLock<Arc<App>> = OnceLock::new();
static ENGINE: OnceLock<usize> = OnceLock::new();
/// The works handed out, to find the one a result belongs to (the drivers
/// keep only their header): as many as their job slots, and some more.
static WORKS: Mutex<VecDeque<MiningWork>> = Mutex::new(VecDeque::new());
const KEPT_WORKS: usize = 160;

/// First thing at boot: the Gamma's ASIC held in reset (GPIO1 low).
pub fn hold_reset() {
    #[cfg(feature = "gamma601")]
    unsafe {
        bdl_gamma_hold_reset()
    };
}

/// The driver's settings (Gamma: frequency, core voltage, fan, limit).
pub fn apply_settings(app: &App) {
    #[cfg(feature = "gamma601")]
    {
        let s = app.settings();
        unsafe {
            bdl_gamma_settings(s.asic_frequency, s.asic_voltage, s.fan_auto, s.fan_percent, s.max_temp);
            bdl_gamma_fan(s.fan_auto, s.fan_percent, s.max_temp);
        }
    }
    #[cfg(not(feature = "gamma601"))]
    let _ = app;
}

/// The board's regulator, sensors and ASIC, still off (after the screen:
/// the Gamma's shares its I2C bus).
pub fn begin(app: &App) {
    apply_settings(app);
    #[cfg(feature = "gamma601")]
    unsafe {
        bdl_gamma_begin()
    };
    #[cfg(feature = "nerdnos")]
    unsafe {
        bdl_nerdnos_begin()
    };
}

/// Starts the ASIC task; it powers the chip once the pool has work.
pub fn start(app: Arc<App>) -> bool {
    let Some(engine) = app.miner.register_engine("asic") else { return false };
    let _ = ENGINE.set(engine);
    let _ = APP.set(app);
    #[cfg(feature = "gamma601")]
    return unsafe { bdl_gamma_start() };
    #[cfg(feature = "nerdnos")]
    return unsafe { bdl_nerdnos_start() };
}

pub fn status() -> Status {
    let mut s = Status::default();
    #[cfg(feature = "gamma601")]
    unsafe {
        bdl_gamma_status(&mut s)
    };
    #[cfg(feature = "nerdnos")]
    unsafe {
        bdl_nerdnos_status(&mut s)
    };
    s
}

/// The board's thermometer for the thermal policy, sampled once a second
/// (the NerdNOS driver stops its ASIC when its NTC stops reading).
pub fn temperature() -> f32 {
    #[cfg(feature = "gamma601")]
    return unsafe { bdl_gamma_temperature() };
    #[cfg(feature = "nerdnos")]
    return unsafe { bdl_nerdnos_temperature() };
}

#[no_mangle]
extern "C" fn bdl_asic_rs_log(line: *const c_char) {
    let line = text(line);
    match APP.get() {
        Some(app) => app.log(&line),
        None => println!("{line}"),
    }
}

#[no_mangle]
extern "C" fn bdl_asic_rs_power_percent() -> u8 {
    APP.get().map_or(0, |app| app.miner.power_percent())
}

#[no_mangle]
extern "C" fn bdl_asic_rs_ready() -> bool {
    APP.get().is_some_and(|app| app.miner.board_valid())
}

#[no_mangle]
extern "C" fn bdl_asic_rs_work(
    header: *mut u8,
    version_mask: *mut u32,
    share_difficulty: *mut f64,
    generation: *mut u32,
    height: *mut u32,
) -> bool {
    let Some(app) = APP.get() else { return false };
    let Some(w) = app.miner.asic_work() else { return false };
    unsafe {
        std::ptr::copy_nonoverlapping(w.work.header.as_ptr(), header, 80);
        *version_mask = w.work.version_mask;
        *share_difficulty = w.work.share_difficulty;
        *generation = w.generation;
        *height = w.height;
    }
    let mut works = WORKS.lock().unwrap();
    if works.len() >= KEPT_WORKS {
        works.pop_front();
    }
    works.push_back(w);
    true
}

#[no_mangle]
extern "C" fn bdl_asic_rs_current(generation: u32) -> bool {
    APP.get().is_some_and(|app| app.miner.current(generation))
}

#[no_mangle]
extern "C" fn bdl_asic_rs_candidate(
    header: *const u8,
    generation: u32,
    nonce: u32,
    version_bits: u32,
    minimum: f64,
) -> f64 {
    let Some(app) = APP.get() else { return -1.0 };
    let header = unsafe { std::slice::from_raw_parts(header, 80) };
    let work = {
        let works = WORKS.lock().unwrap();
        works.iter().rev().find(|w| w.generation == generation && w.work.header[..76] == header[..76]).cloned()
    };
    match work {
        Some(w) => app.miner.asic_candidate(&w, nonce, version_bits, minimum).unwrap_or(-1.0),
        None => -1.0,
    }
}

#[no_mangle]
extern "C" fn bdl_asic_rs_hashes(hashes: u64) {
    if let (Some(app), Some(&engine)) = (APP.get(), ENGINE.get()) {
        app.miner.add_hashes(engine, hashes);
    }
}
