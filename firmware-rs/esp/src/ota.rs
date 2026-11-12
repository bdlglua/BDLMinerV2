//! Firmware updates: writing the other app slot, and the trial of a new
//! firmware (the C++ firmware's trial.cpp). The bootloader starts a freshly
//! installed firmware in a pending state; it is confirmed after a few minutes
//! connected to the pool and mining. A crash or hang during the trial
//! restarts the board and the bootloader returns to the previous firmware by
//! itself; a trial that never proves itself rolls back on purpose.

use std::ffi::{c_void, CStr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use bdlminer_app::update::{Firmware, FirmwareWriter};
use bdlminer_app::App;
use esp_idf_svc::sys::*;

use crate::wifi;

/// Accepted shares cannot be the proof: at difficulty 1 a 1 MH/s miner
/// waits about 70 minutes for one.
const CONFIRM_MS: u32 = 3 * 60 * 1000;
const TRIAL_MS: u32 = 20 * 60 * 1000;
/// A firmware may also break WiFi.
const OFFLINE_TRIAL_MS: u32 = 40 * 60 * 1000;
/// Failed trials before a version is refused.
const TRIES: u8 = 2;
/// Trials ended by a reset or power cut that are not held against it.
const MAX_CUTS: u8 = 3;
/// esp_ota_begin(): erase the slot as the image is written, not all at once.
const SEQUENTIAL_WRITES: usize = 0xffff_fffe;
const NS: &str = "trial";

static PENDING: AtomicBool = AtomicBool::new(false);

fn check(code: esp_err_t, what: &str) -> Result<(), String> {
    if code == ESP_OK {
        return Ok(());
    }
    let name = unsafe { CStr::from_ptr(esp_err_to_name(code)) };
    Err(format!("{what}: {}", name.to_string_lossy()))
}

/// The other slot, being written.
struct Writer {
    handle: esp_ota_handle_t,
    partition: *const esp_partition_t,
    open: bool,
}

// The handle and partition table entry are plain values owned by this writer.
unsafe impl Send for Writer {}

impl Writer {
    fn begin() -> Result<Self, String> {
        let partition = unsafe { esp_ota_get_next_update_partition(std::ptr::null()) };
        if !supported() {
            return Err("this board has no room for a second firmware: update by USB".into());
        }
        let mut handle: esp_ota_handle_t = 0;
        check(unsafe { esp_ota_begin(partition, SEQUENTIAL_WRITES, &mut handle) }, "cannot start the update")?;
        Ok(Writer { handle, partition, open: true })
    }
}

impl FirmwareWriter for Writer {
    fn write(&mut self, data: &[u8]) -> Result<(), String> {
        check(unsafe { esp_ota_write(self.handle, data.as_ptr() as *const c_void, data.len()) }, "write failed")
    }
    fn finish(mut self: Box<Self>) -> Result<(), String> {
        self.open = false;
        // Checks the image (header, segments, hash).
        check(unsafe { esp_ota_end(self.handle) }, "invalid firmware image")?;
        check(unsafe { esp_ota_set_boot_partition(self.partition) }, "cannot select the new firmware")
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        if self.open {
            unsafe { esp_ota_abort(self.handle) };
        }
    }
}

pub struct EspFirmware {
    pub app: Arc<App>,
}

impl Firmware for EspFirmware {
    fn supported(&self) -> bool {
        supported()
    }
    fn begin(&self) -> Result<Box<dyn FirmwareWriter>, String> {
        Ok(Box::new(Writer::begin()?))
    }
    fn installed(&self, version: &str) {
        confirm(&self.app, "a newer firmware is installed");
        expect(&self.app, version);
    }
}

/// A second app slot exists. With a single one, ESP-IDF names the running
/// slot as the next.
pub fn supported() -> bool {
    let next = unsafe { esp_ota_get_next_update_partition(std::ptr::null()) };
    !next.is_null() && next != unsafe { esp_ota_get_running_partition() }
}

/// Starts writing the other slot (the dashboard's upload).
pub fn begin() -> Result<Box<dyn FirmwareWriter>, String> {
    Ok(Box::new(Writer::begin()?))
}

fn get_text(app: &App, key: &str) -> String {
    app.storage.lock().unwrap().get(NS, key).map(|v| String::from_utf8_lossy(&v).into_owned()).unwrap_or_default()
}

fn get_u8(app: &App, key: &str) -> u8 {
    app.storage.lock().unwrap().get(NS, key).and_then(|v| v.first().copied()).unwrap_or(0)
}

/// At boot: whether this firmware is on trial, and what a rollback (the
/// other slot marked invalid) says about the version that was tried.
pub fn begin_trial(app: &App) {
    let running = unsafe { esp_ota_get_running_partition() };
    let mut state: esp_ota_img_states_t = 0;
    if !running.is_null()
        && unsafe { esp_ota_get_state_partition(running, &mut state) } == ESP_OK
        && state == esp_ota_img_states_t_ESP_OTA_IMG_PENDING_VERIFY
    {
        PENDING.store(true, Ordering::Relaxed);
        app.firmware_trial.store(true, Ordering::Relaxed);
        app.log(&format!(
            "[update] firmware {} on trial: confirmed after {} minutes of mining, else the previous one returns",
            bdlminer_app::platform::FW_VERSION,
            CONFIRM_MS / 60000
        ));
    }
    let expected = get_text(app, "ver");
    let counted = get_text(app, "told");
    let same = get_text(app, "failver") == expected;
    let mut fails = if same { get_u8(app, "fails") } else { 0 };
    let mut cuts = if same { get_u8(app, "cuts") } else { 0 };
    let last_was_cut = same && get_u8(app, "cut") != 0;
    if unsafe { esp_ota_get_last_invalid_partition() }.is_null()
        || expected.is_empty()
        || expected == bdlminer_app::platform::FW_VERSION
    {
        return;
    }
    let fresh = counted != expected;
    // A power cut, the reset button or a USB serial reset says nothing about
    // the firmware, unless it keeps happening.
    let mut cut = last_was_cut;
    if fresh {
        let reason = unsafe { esp_reset_reason() };
        cut = matches!(
            reason,
            esp_reset_reason_t_ESP_RST_POWERON | esp_reset_reason_t_ESP_RST_EXT | esp_reset_reason_t_ESP_RST_BROWNOUT
        ) && cuts < MAX_CUTS;
        if cut {
            cuts += 1;
        } else {
            fails += 1;
        }
    }
    let why = if cut {
        let why = format!("firmware {expected} was interrupted by a reset or power cut during its trial");
        app.log(&format!("[update] {why}; it is installed again"));
        why
    } else {
        let why = format!("firmware {expected} failed its trial and was rolled back");
        if fails >= TRIES {
            app.updater.refuse(&expected, &why);
            app.log(&format!("[update] {why} {fails} times; it is not installed automatically again"));
        } else {
            app.log(&format!("[update] {why}; it gets one more try"));
        }
        why
    };
    if !fresh {
        return;
    }
    if !cut {
        app.notifier.notify("Update rolled back", &why, 4, "warning");
    }
    let mut store = app.storage.lock().unwrap();
    store.set(NS, "told", expected.as_bytes());
    store.set(NS, "failver", expected.as_bytes());
    store.set(NS, "fails", &[fails]);
    store.set(NS, "cuts", &[cuts]);
    store.set(NS, "cut", &[cut as u8]);
}

/// Once a second: confirms a firmware that proved itself, rolls back one
/// that did not in time.
pub fn poll_trial(app: &App) {
    if !PENDING.load(Ordering::Relaxed) {
        return;
    }
    let now = app.clock.millis();
    // Hashing is not required while mining is paused on purpose.
    let hashing = app.miner.hashrate() > 0.0 || app.settings().mining_power == 0;
    if now >= CONFIRM_MS && app.miner.stats().pool_connected && hashing {
        confirm(app, "connected to the pool and mining");
        return;
    }
    if now >= if wifi::link_up() { TRIAL_MS } else { OFFLINE_TRIAL_MS } {
        app.log(&format!(
            "[update] firmware {} did not prove itself in {} minutes: back to the previous one",
            bdlminer_app::platform::FW_VERSION,
            now / 60000
        ));
        app.prepare_restart("firmware trial failed, previous firmware restored");
        std::thread::sleep(std::time::Duration::from_millis(300));
        // Returns only if there is nothing to go back to.
        unsafe { esp_ota_mark_app_invalid_rollback_and_reboot() };
        PENDING.store(false, Ordering::Relaxed);
    }
}

pub fn confirm(app: &App, why: &str) {
    if !PENDING.swap(false, Ordering::Relaxed) {
        return;
    }
    if unsafe { esp_ota_mark_app_valid_cancel_rollback() } == ESP_OK {
        app.firmware_trial.store(false, Ordering::Relaxed);
        app.log(&format!("[update] firmware {} confirmed ({why})", bdlminer_app::platform::FW_VERSION));
        let mut store = app.storage.lock().unwrap();
        for key in ["ver", "failver", "fails", "cuts", "cut"] {
            store.remove(NS, key);
        }
    } else {
        // A failed flash write did not confirm the bootloader state.
        PENDING.store(true, Ordering::Relaxed);
        app.log(&format!("[update] firmware {} could not be confirmed", bdlminer_app::platform::FW_VERSION));
    }
}

/// The version about to start: a rollback is then counted against it.
pub fn expect(app: &App, version: &str) {
    let mut store = app.storage.lock().unwrap();
    store.set(NS, "ver", version.as_bytes());
    // A new attempt: its rollback, if any, counts again.
    store.remove(NS, "told");
}
