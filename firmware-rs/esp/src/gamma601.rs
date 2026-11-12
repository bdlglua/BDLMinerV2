//! Bitaxe Gamma 601 (feature `gamma601`): one BM1370, its TPS546 regulator
//! and EMC2101 fan controller and thermometer, ported from the C++
//! firmware's src/gamma601.cpp. Volatile PMBus writes only, every one read
//! back; never STORE_USER_ALL nor any NVM command.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use bdlminer_app::miner::MiningWork;
use bdlminer_app::platform::Storage;
use bdlminer_app::App;
use bdlminer_core::bm1370;
use bdlminer_core::fan_control::{fan_percent, fan_pwm, fan_pwm_percent, FanControl};

use crate::asic::Status;
use crate::periph::{delay_ms, now, output, set, I2c, Uart};

const TPS: u8 = 0x24;
const EMC: u8 = 0x4c;
const RESET: i32 = 1;
const NOT_FOUND: &str = "Regulator or sensors not found: plug in the 5V supply";
/// The ASIC diode alone can stop the ASIC: a reading out of bounds must hold
/// for this many readings in a row (0.75 s while mining).
const DIODE_CONFIRM: u8 = 3;
const AUTO_RESTARTS: u8 = 3;
const AUTO_RESTART_WINDOW_MS: u32 = 60 * 60 * 1000;
/// Pool work can disappear for a few seconds: the ASIC keeps running on its
/// last job this long before it is powered off.
const NO_WORK_GRACE_MS: u32 = 60_000;

/// Percent, automatic bit 8, thermal limit byte 2 (as the C++ fanSetting).
static FAN_SETTING: AtomicU32 = AtomicU32::new(100 | (65 << 16));
static REQUESTED_MHZ: AtomicU32 = AtomicU32::new(bm1370::STOCK_MHZ as u32);
static REQUESTED_MV: AtomicU32 = AtomicU32::new(bm1370::STOCK_MV as u32);
/// The board's own temperature (EMC2101 internal sensor), for the thermal
/// policy while the ASIC diode has no reading.
static BOARD_TEMPERATURE: AtomicU32 = AtomicU32::new(0x7fc0_0000); // NaN

struct Shared {
    status: Mutex<Status>,
    last_fault: Mutex<(String, u32)>,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| Shared {
        status: Mutex::new(Status {
            temperature: f32::NAN,
            regulator_temperature: f32::NAN,
            input_voltage: f32::NAN,
            core_voltage: f32::NAN,
            current: f32::NAN,
            requested_mhz: bm1370::STOCK_MHZ,
            hardware_error_percent: -1.0,
            fan_percent: -1,
            requested_fan_percent: 100,
            fan_override: true,
            fan_reason: "Startup or stopped ASIC",
            ..Default::default()
        }),
        last_fault: Mutex::new((String::new(), 0)),
    })
}

fn requested_mhz() -> u16 {
    let mhz = REQUESTED_MHZ.load(Ordering::Relaxed);
    if bm1370::valid_profile(mhz) {
        mhz as u16
    } else {
        bm1370::STOCK_MHZ
    }
}

/// The core voltage chosen in the settings (ESP-Miner's BM1370 values).
fn core_volts() -> f32 {
    let mv = REQUESTED_MV.load(Ordering::Relaxed);
    (if bm1370::valid_voltage(mv) { mv } else { bm1370::STOCK_MV as u32 }) as f32 / 1000.0
}

pub fn set_settings(mhz: u16, mv: u16, fan_auto: bool, fan_percent_setting: u8, max_temp: u8) {
    REQUESTED_MHZ.store(mhz as u32, Ordering::Relaxed);
    REQUESTED_MV.store(mv as u32, Ordering::Relaxed);
    FAN_SETTING.store(
        ((max_temp as u32) << 16) | if fan_auto { 0x100 } else { 0 } | fan_percent(fan_percent_setting as u32) as u32,
        Ordering::Relaxed,
    );
}

#[derive(Clone, Default)]
struct Slot {
    work: Option<MiningWork>,
    sent: u32,
    /// Duplicate UART results, without conflating two rolled versions.
    proofs: [u64; 32],
    proof_head: usize,
    proof_count: usize,
}

struct Driver {
    app: Arc<App>,
    engine: usize,
    i2c: Option<I2c>,
    uart: Uart,
    storage: Arc<Mutex<dyn Storage>>,
    live: Status,
    fan_policy: FanControl,
    applied_fan_duty: u8,
    configured: bool,
    latched: bool,
    powered: bool,
    log_setup: bool,
    voltage_mode: u8,
    diode_strikes: u8,
    last_diode: f32,
    diode_ready: bool,
    power_enabled_at: u32,
    last_health_sample: u32,
    receiver: bm1370::Receiver,
    counter: bm1370::Counter,
    errors: bm1370::ErrorMonitor,
    force_stock: bool,
    error_seen: bool,
    error_value: u32,
    last_error_reply: u32,
    slots: Vec<Slot>,
    next_slot: usize,
    active_generation: u32,
    active_mask: u32,
    last_hash_reply: u32,
    last_hash_progress: u32,
    no_work_since: u32,
    auto_restarts: u8,
    auto_restart_since: u32,
    mining_state: &'static str,
    sampled: u32,
    sent: u32,
    requested: u32,
    setup_at: u32,
    noted_difficulty: f64,
}

impl Driver {
    fn publish(&self) {
        *shared().status.lock().unwrap() = self.live.clone();
    }

    fn log(&self, line: &str) {
        self.app.log(line);
    }

    fn power_percent(&self) -> u8 {
        self.app.miner.power_percent()
    }

    fn ready(&self) -> bool {
        self.app.miner.board_valid()
    }

    fn target_frequency(&self) -> f32 {
        let mhz = if self.force_stock { bm1370::STOCK_MHZ } else { requested_mhz() };
        50f32.max(mhz as f32 * self.power_percent() as f32 / 100.0)
    }

    // ---------------------------------------------------------------- I2C

    fn read(&self, address: u8, reg: u8, out: &mut [u8]) -> bool {
        self.i2c.as_ref().is_some_and(|i2c| i2c.read(address, reg, out))
    }

    fn write8(&self, address: u8, reg: u8, value: u8) -> bool {
        self.i2c.as_ref().is_some_and(|i2c| i2c.write(address, &[reg, value]))
    }

    fn write16(&self, reg: u8, value: u16) -> bool {
        self.i2c.as_ref().is_some_and(|i2c| i2c.write(TPS, &[reg, value as u8, (value >> 8) as u8]))
    }

    fn read16(&self, address: u8, reg: u8) -> Option<u16> {
        let mut bytes = [0u8; 2];
        self.read(address, reg, &mut bytes).then(|| u16::from_le_bytes(bytes))
    }

    /// Every setting is read back; one that does not hold stops the setup,
    /// and the log says which.
    fn unset(&self, address: u8, reg: u8, why: &str) -> bool {
        if self.log_setup {
            self.log(&format!("[gamma] device 0x{address:02x} register 0x{reg:02x}: {why}"));
        }
        false
    }

    fn verify8(&self, address: u8, reg: u8, value: u8) -> bool {
        let mut actual = [0u8; 1];
        if !self.write8(address, reg, value) || !self.read(address, reg, &mut actual) {
            return self.unset(address, reg, "no answer");
        }
        if actual[0] != value {
            return self.unset(address, reg, "reads back another value");
        }
        true
    }

    fn set_fan_duty(&mut self, duty: u8, force: bool) -> bool {
        if !force && duty == self.applied_fan_duty {
            return true;
        }
        if !self.verify8(EMC, 0x4c, duty) {
            self.applied_fan_duty = 255;
            self.live.fan_percent = -1;
            return false;
        }
        self.applied_fan_duty = duty;
        self.live.fan_percent = fan_pwm_percent(duty) as i16;
        true
    }

    fn verify16(&self, reg: u8, value: u16) -> bool {
        let Some(actual) = self.write16(reg, value).then(|| self.read16(TPS, reg)).flatten() else {
            return self.unset(TPS, reg, "no answer");
        };
        if actual != value {
            return self.unset(TPS, reg, "reads back another value");
        }
        true
    }

    /// A number may come back in the regulator's own format (another
    /// exponent), or rounded to its resolution: compared as values.
    fn linear_word(&self, reg: u8, word: u16, value: f32, tolerance: Option<f32>) -> bool {
        let Some(actual) = self.write16(reg, word).then(|| self.read16(TPS, reg)).flatten() else {
            return self.unset(TPS, reg, "no answer");
        };
        let tolerance = tolerance.unwrap_or(0.01f32.max(value.abs() * 0.03));
        if (bm1370::linear11(actual) - value).abs() > tolerance {
            return self.unset(TPS, reg, "reads back another value");
        }
        true
    }

    fn linear(&self, reg: u8, value: f32, tolerance: Option<f32>) -> bool {
        self.linear_word(reg, bm1370::encode_linear11(value), value, tolerance)
    }

    /// Temperatures and timings: whole numbers, written as ESP-Miner does.
    fn linear_int(&self, reg: u8, value: u32) -> bool {
        self.linear_word(reg, bm1370::encode_linear11_int(value), value as f32, None)
    }

    fn exponent(&self) -> i32 {
        let e = (self.voltage_mode & 31) as i32;
        if e & 16 != 0 {
            e - 32
        } else {
            e
        }
    }

    fn voltage(&self, reg: u8, value: f32, tolerance: f32) -> bool {
        let Some(word) = bm1370::voltage_word(value, self.voltage_mode) else {
            return self.unset(TPS, reg, "unsupported VOUT_MODE");
        };
        let Some(actual) = self.write16(reg, word).then(|| self.read16(TPS, reg)).flatten() else {
            return self.unset(TPS, reg, "no answer");
        };
        if ((actual as f32 - word as f32) * 2f32.powi(self.exponent())).abs() > tolerance {
            return self.unset(TPS, reg, "reads back another value");
        }
        true
    }

    /// The margin and OV/UV limits, as a fraction of VOUT_COMMAND: relative
    /// by default (VOUT_MODE bit 7, as on the Bitaxe), else in volts. The
    /// hardware rounds them to steps of 1 to 10%: compared within 6%.
    fn limit(&self, reg: u8, fraction: f32) -> bool {
        if self.voltage_mode & 0x80 != 0 {
            self.voltage(reg, fraction, 0.06)
        } else {
            self.voltage(reg, fraction * core_volts(), 0.06 * core_volts())
        }
    }

    /// PIN_DETECT_OVERRIDE (EEh), written as ESP-Miner does (FFFFh); some of
    /// its bits are reserved: a different read-back is logged, not a failure.
    fn pin_detect_override(&self) -> bool {
        let Some(actual) = self.write16(0xee, 0xffff).then(|| self.read16(TPS, 0xee)).flatten() else {
            return self.unset(TPS, 0xee, "no answer");
        };
        if actual != 0xffff && self.log_setup {
            self.log(&format!("[gamma] PIN_DETECT_OVERRIDE reads 0x{actual:04x}"));
        }
        true
    }

    fn clear_faults(&self) -> bool {
        self.i2c.as_ref().is_some_and(|i2c| i2c.write(TPS, &[0x03]))
    }

    // ----------------------------------------------------------- faults

    fn invalidate(&mut self) {
        for slot in &mut self.slots {
            slot.work = None;
        }
        self.receiver.clear();
        self.counter.reset();
        self.errors.reset();
        self.error_seen = false;
        self.next_slot = 0;
    }

    fn save_fault(&self, reason: &str) {
        let l = &self.live;
        let text = format!(
            "{reason} (regulator status 0x{:04x}, input {:.2} V, core {:.3} V, {:.1} A, ASIC {:.1} C, regulator {:.1} C, fan {} rpm)",
            l.regulator_status, l.input_voltage, l.core_voltage, l.current, l.temperature, l.regulator_temperature, l.rpm
        );
        let text: String = text.chars().take(191).collect();
        let unix =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as u32).unwrap_or(0);
        let when = if unix > 1_700_000_000 { unix } else { 0 };
        *shared().last_fault.lock().unwrap() = (text.clone(), when);
        let mut store = self.storage.lock().unwrap();
        store.set("gamma", "fault", text.as_bytes());
        store.set("gamma", "faultt", &when.to_le_bytes());
    }

    fn load_fault(&self) {
        let mut store = self.storage.lock().unwrap();
        let Some(text) = store.get("gamma", "fault").filter(|t| !t.is_empty() && t.len() < 192) else { return };
        let when = store.get("gamma", "faultt").and_then(|b| b.try_into().ok()).map(u32::from_le_bytes).unwrap_or(0);
        *shared().last_fault.lock().unwrap() = (String::from_utf8_lossy(&text).into_owned(), when);
    }

    fn off(&mut self, reason: &'static str, fault: bool) {
        // Assert reset first: even a broken I2C bus must stop the ASIC
        // immediately.
        set(RESET, false);
        let disabled = self.write8(TPS, 0x01, 0);
        let fan = self.set_fan_duty(63, true);
        // Retain thermal hysteresis through pauses.
        self.fan_policy.update(false, false, 100, f32::NAN, f32::NAN, now(), 65);
        self.live.fan_override = true;
        self.live.fan_reason = "Startup or stopped ASIC";
        self.powered = false;
        self.diode_ready = false;
        self.live.running = false;
        self.live.frequency = 0.0;
        self.no_work_since = 0;
        if fault || !disabled || !fan {
            self.latched = true;
        }
        self.live.state = if !disabled || !fan { "I2C shutdown fault; restart required" } else { reason };
        if self.latched {
            self.save_fault(self.live.state);
        }
        self.invalidate();
        self.publish();
    }

    /// The 525 MHz fallback is kept across restarts while the requested
    /// profile is the one that failed, at the same core voltage.
    fn save_fallback(&self, mhz: u16, reason: &str) {
        let mut saved = [0u8; 64];
        saved[..2].copy_from_slice(&mhz.to_le_bytes());
        saved[2..4].copy_from_slice(&(REQUESTED_MV.load(Ordering::Relaxed) as u16).to_le_bytes());
        let bytes = reason.as_bytes();
        let n = bytes.len().min(59);
        saved[4..4 + n].copy_from_slice(&bytes[..n]);
        self.storage.lock().unwrap().set("gamma", "fallback", &saved);
    }

    fn load_fallback(&mut self, mhz: u16) {
        let saved = self.storage.lock().unwrap().get("gamma", "fallback").filter(|s| s.len() == 64);
        let same = saved.as_ref().is_some_and(|s| {
            u16::from_le_bytes([s[0], s[1]]) == mhz
                && u16::from_le_bytes([s[2], s[3]]) as u32 == REQUESTED_MV.load(Ordering::Relaxed)
        });
        if saved.is_some() && !same {
            self.storage.lock().unwrap().remove("gamma", "fallback"); // another frequency or voltage since
        }
        if !same || mhz <= bm1370::STOCK_MHZ {
            return;
        }
        let saved = saved.unwrap();
        let end = saved[4..].iter().position(|&b| b == 0).map_or(saved.len(), |p| p + 4);
        let reason = String::from_utf8_lossy(&saved[4..end]).into_owned();
        self.force_stock = true;
        self.live.stock_fallback = true;
        self.live.fallback_reason = reason.clone();
        self.log(&format!(
            "[gamma] {mhz} MHz at {} mV failed before ({reason}): mining at 525 MHz; choose another frequency or voltage to clear it",
            u16::from_le_bytes([saved[2], saved[3]])
        ));
    }

    /// A BM1370 that stops answering, or whose hash counter stops, is reset
    /// and started again; only a chip that keeps failing stops until the
    /// board restarts.
    fn auto_restart(&mut self, t: u32) -> bool {
        if self.auto_restarts == 0 || t.wrapping_sub(self.auto_restart_since) >= AUTO_RESTART_WINDOW_MS {
            self.auto_restarts = 0;
            self.auto_restart_since = t;
        }
        if self.auto_restarts >= AUTO_RESTARTS {
            return false;
        }
        self.auto_restarts += 1;
        true
    }

    fn instability(&mut self, reason: &'static str) {
        if !self.force_stock && self.live.frequency > bm1370::STOCK_MHZ as f32 {
            self.log(&format!("[gamma] {reason} at {:.2} MHz; falling back to 525 MHz", self.live.frequency));
            self.force_stock = true;
            self.live.stock_fallback = true;
            self.live.fallback_reason = reason.into();
            self.save_fallback(self.live.requested_mhz, reason);
            self.off("Restarting at 525 MHz (fallback)", false);
        } else if self.auto_restart(now()) {
            self.save_fault(reason);
            self.log(&format!(
                "[gamma] {reason} at {:.2} MHz: ASIC restarted automatically ({} of {AUTO_RESTARTS} this hour)",
                self.live.frequency, self.auto_restarts
            ));
            self.off("Restarting the ASIC", false);
        } else {
            self.off(reason, true);
        }
    }

    fn init_failed(&mut self) {
        let reason = "BM1370 initialization failed";
        if !self.auto_restart(now()) {
            self.off(reason, true);
            return;
        }
        self.save_fault(reason);
        self.log(&format!(
            "[gamma] {reason}: ASIC started again automatically ({} of {AUTO_RESTARTS} this hour)",
            self.auto_restarts
        ));
        self.off("Restarting the ASIC", false);
        delay_ms(500);
    }

    fn register_result(&mut self, r: &bm1370::Response) {
        if !self.powered || r.address != 0 {
            return;
        }
        let t = now();
        if r.reg == 0x4c {
            self.error_value = r.value;
            self.last_error_reply = t;
            self.error_seen = true;
            return;
        }
        if r.reg != 0x8c {
            return;
        }
        let delta = self.counter.update(r.value, t);
        // Hashing the last job while the pool is away is no mining.
        if self.ready() {
            self.app.miner.add_hashes(self.engine, delta);
        }
        self.last_hash_reply = t;
        if delta != 0 {
            self.last_hash_progress = t;
        }
        if self.error_seen && t.wrapping_sub(self.last_error_reply) <= 2000 {
            let unstable = self.errors.update(self.error_value, r.value, t);
            self.live.hardware_error_percent = self.errors.percent;
            self.publish();
            if unstable && self.live.frequency > bm1370::STOCK_MHZ as f32 {
                self.instability("High ASIC hardware error rate");
            }
        } else {
            self.errors.reset();
            self.live.hardware_error_percent = -1.0;
        }
    }

    fn check_watchdogs(&mut self, t: u32) {
        if !self.powered || !self.live.running {
            return;
        }
        if t.wrapping_sub(self.last_hash_reply) > 3000 {
            self.instability("ASIC not responding; restart required");
        } else if t.wrapping_sub(self.last_hash_progress) > 5000 {
            self.instability("ASIC counter stalled; restart required");
        } else if self.live.frequency > bm1370::STOCK_MHZ as f32 && t.wrapping_sub(self.last_error_reply) > 5000 {
            self.instability("ASIC error telemetry unavailable");
        }
    }

    fn send(&self, packet: &[u8]) -> bool {
        self.uart.write(packet)
    }

    fn reg(&self, address: u8, value: u32, all: bool) -> bool {
        self.send(&bm1370::reg(address, value, all))
    }

    fn request(&self, address: u8, broadcast: bool) -> bool {
        self.send(&bm1370::command(if broadcast { 0x52 } else { 0x42 }, &[0, address]))
    }

    fn readings(&self) -> bm1370::Health {
        let l = &self.live;
        bm1370::Health {
            temperature: l.temperature,
            regulator_temperature: l.regulator_temperature,
            input_voltage: l.input_voltage,
            core_voltage: l.core_voltage,
            current: l.current,
            rpm: l.rpm,
            status: l.regulator_status,
            core_target: core_volts(),
        }
    }

    fn update_fan(&mut self, diode: f32) -> bool {
        let command = FAN_SETTING.load(Ordering::Relaxed);
        self.live.fan_automatic = command & 0x100 != 0;
        self.live.requested_fan_percent = command as u8;
        // The raw diode reading: even an unconfirmed anomaly demands full
        // cooling while the three-reading fault filter decides.
        let decision = self.fan_policy.update(
            self.powered && self.live.running,
            self.live.fan_automatic,
            self.live.requested_fan_percent as u32,
            diode,
            self.live.regulator_temperature,
            now(),
            (command >> 16) & 255,
        );
        self.live.fan_override = decision.forced;
        self.live.fan_reason = decision.reason;
        if !self.set_fan_duty(fan_pwm(decision.percent as u32), false) {
            self.off("Fan control I2C fault; restart required", true);
            return false;
        }
        true
    }

    fn collect(&mut self, require_diode: bool) -> bool {
        self.last_health_sample = now();
        let mut b = [0u8; 1];
        let mut byte = |s: &Self, address: u8, reg: u8| s.read(address, reg, &mut b).then_some(b[0]);
        let values = (|| {
            Some((
                byte(self, EMC, 0x00)?,
                byte(self, EMC, 0x01)?,
                byte(self, EMC, 0x10)?,
                byte(self, EMC, 0x46)?,
                byte(self, EMC, 0x47)?,
                self.read16(TPS, 0x88)?,
                self.read16(TPS, 0x8b)?,
                self.read16(TPS, 0x8c)?,
                self.read16(TPS, 0x8d)?,
                self.read16(TPS, 0x79)?,
            ))
        })();
        let Some((internal, msb, lsb, tach_low, tach_high, vin, vout, current, vr, status)) = values else {
            self.log("[gamma] sensors not answering on I2C");
            self.live.temperature = f32::NAN;
            self.off("Sensor I2C fault; restart required", true);
            return false;
        };
        self.live.regulator_status = status;
        // Two's complement; below 0 C is not a reading here.
        let board = if internal < 0x80 { internal as f32 } else { f32::NAN };
        BOARD_TEMPERATURE.store(board.to_bits(), Ordering::Relaxed);
        // The ASIC's analog mux is only trustworthy while its core is powered
        // and its routing register has been initialized.
        let diode = if self.powered && self.diode_ready { bm1370::diode_temperature(msb, lsb) } else { f32::NAN };
        let suspect = if diode.is_finite() { diode >= bm1370::ASIC_MAX_C } else { require_diode };
        let unconfirmed = suspect && {
            self.diode_strikes += 1;
            self.diode_strikes < DIODE_CONFIRM
        };
        if unconfirmed {
            self.log(&format!(
                "[gamma] ASIC temperature reading {diode:.1} C not confirmed yet ({} of {DIODE_CONFIRM})",
                self.diode_strikes
            ));
            self.live.temperature = self.last_diode;
        } else {
            if !suspect {
                self.diode_strikes = 0;
                self.last_diode = diode;
            }
            self.live.temperature = diode;
        }
        self.live.regulator_temperature = bm1370::linear11(vr);
        self.live.input_voltage = bm1370::linear11(vin);
        self.live.core_voltage = vout as f32 * 2f32.powi(self.exponent());
        self.live.current = bm1370::linear11(current);
        self.live.rpm = bm1370::fan_rpm(tach_low as u16 | (tach_high as u16) << 8);
        if let Some(fault) = self.readings().fault(self.powered, require_diode && !unconfirmed) {
            let l = &self.live;
            self.log(&format!(
                "[gamma] {fault}: regulator status 0x{:04x}, input {:.2} V, core {:.3} V, {:.1} A, ASIC {:.1} C, regulator {:.1} C, fan {} rpm",
                l.regulator_status, l.input_voltage, l.core_voltage, l.current, l.temperature, l.regulator_temperature, l.rpm
            ));
            self.off(fault, true);
            return false;
        }
        if !self.update_fan(diode) {
            return false;
        }
        self.publish();
        true
    }

    /// All startup waits are short and keep the safety checks running.
    fn wait_safe(&mut self, ms: u32, require_diode: bool) -> bool {
        let since = now();
        loop {
            let power = self.power_percent();
            if power < 10 || !self.ready() {
                self.off(
                    if power != 0 && power < 10 {
                        "Gamma power limit must be 10-100% or paused"
                    } else {
                        "Paused / waiting for pool"
                    },
                    false,
                );
                return false;
            }
            // TPS546 power-good settling; monitoring goes on during PLL ramps.
            let t = now();
            if self.powered
                && t.wrapping_sub(self.power_enabled_at) >= 250
                && t.wrapping_sub(self.last_health_sample) >= 100
                && !self.collect(require_diode)
            {
                return false;
            }
            delay_ms(5);
            if now().wrapping_sub(since) >= ms {
                return true;
            }
        }
    }

    fn init_asic(&mut self, first: &MiningWork) -> bool {
        self.active_generation = first.generation;
        self.active_mask = first.work.version_mask;
        self.live.requested_mhz = requested_mhz();
        self.live.hardware_error_percent = -1.0;
        self.errors.reset();
        self.error_seen = false;
        if !self.collect(false) {
            return false;
        }
        // On USB alone there is no 5 V for the ASIC and its fan: wait for it.
        if let Some(wait) = self.readings().not_ready() {
            self.live.state = wait;
            self.publish();
            return false;
        }
        if self.power_percent() < 10 || !self.ready() {
            return false;
        }
        // An undervoltage latched while the supply was missing must not stop
        // this start.
        if !self.clear_faults() || !self.write8(TPS, 0x01, 0x80) {
            self.off("ASIC power enable failed", true);
            return false;
        }
        self.powered = true;
        self.diode_ready = false;
        self.power_enabled_at = now();
        self.last_health_sample = self.power_enabled_at;
        self.live.state = "Initializing BM1370";
        self.publish();
        if !self.wait_safe(500, false) {
            return false;
        }
        set(RESET, true);
        if !self.wait_safe(100, false) {
            return false;
        }
        if !self.uart.begin(115_200, 4096) {
            return false;
        }
        self.uart.drain();
        self.receiver.clear();
        for _ in 0..3 {
            if !self.send(&bm1370::version_mask(self.active_mask)) {
                return false;
            }
        }
        if !self.request(0, true) {
            return false;
        }
        let since = now();
        let mut chips = 0;
        loop {
            while let Some(byte) = self.uart.read() {
                if let Some(r) = self.receiver.push(byte) {
                    if !r.job && r.reg == 0 {
                        if r.value >> 16 != 0x1370 {
                            return false;
                        }
                        chips += 1;
                    }
                }
            }
            if !self.wait_safe(5, false) {
                return false;
            }
            if now().wrapping_sub(since) >= 1000 {
                break;
            }
        }
        if chips != 1 {
            return false;
        }
        if !self.send(&bm1370::version_mask(self.active_mask))
            || !self.reg(0xa8, 0x0007_0000, true)
            || !self.reg(0x18, 0xf000_c100, true)
            || !self.send(&bm1370::command(0x53, &[0, 0]))
            || !self.send(&bm1370::command(0x40, &[0, 0]))
        {
            return false;
        }
        const SETUP: [(u8, u32, bool); 15] = [
            (0x3c, 0x8000_8b00, true),
            (0x3c, 0x8000_8010, true),
            (0x14, 0x0000_00ff, true),
            (0x54, 3, true),
            (0x58, 0x0001_1111, true),
            (0x68, 0x5aa5_5aa5, true),
            (0xa8, 0x0007_01f0, false),
            (0x18, 0xf000_c100, false),
            (0x3c, 0x8000_8b00, false),
            (0x3c, 0x8000_8010, false),
            (0x3c, 0x8000_82aa, false),
            (0xb9, 0x0000_4480, true),
            (0x54, 2, true),
            (0xb9, 0x0000_4480, true),
            (0x3c, 0x8000_8dee, true),
        ];
        for (address, value, all) in SETUP {
            if !self.reg(address, value, all) || !self.wait_safe(10, false) {
                return false;
            }
        }
        // Minimum supported ASIC difficulty 16: no valid pool share lost to
        // a higher hardware filter.
        if !self.send(&bm1370::ticket_mask(4)) || !self.reg(0x28, 0x1130_0200, true) {
            return false;
        }
        self.uart.flush();
        self.uart.set_baud(1_000_000);
        // Let the external-diode conversion settle after configuring the mux.
        if !self.wait_safe(100, false) {
            return false;
        }
        self.diode_ready = true;
        if !self.collect(true) {
            return false;
        }
        // ESP-Miner's 6.25 MHz / 100 ms steps, with live safety checks.
        let mut target = self.target_frequency();
        let mut f = 50.0f32;
        while f < target {
            let Some((packet, actual)) = bm1370::frequency(f) else { return false };
            self.live.frequency = actual;
            if !self.send(&packet) || !self.wait_safe(100, true) {
                return false;
            }
            target = self.target_frequency();
            f += 6.25;
        }
        let Some((packet, actual)) = bm1370::frequency(target) else { return false };
        self.live.frequency = actual;
        if !self.send(&packet) {
            return false;
        }
        let hcn = ((4_294_967_296.0f64 / 128.0 * 25.0 / self.live.frequency as f64 * 0.5) - 268.0) as u32;
        if !self.reg(0x10, hcn, true) || !self.collect(true) {
            return false;
        }
        self.counter.reset();
        let t = now();
        self.last_hash_reply = t;
        self.last_hash_progress = t;
        self.last_error_reply = t;
        self.live.running = true;
        self.live.state = if self.force_stock { "Mining (525 MHz fallback profile)" } else { "Mining (experimental)" };
        self.log(&format!(
            "[gamma] BM1370 {:.2} MHz at {} mV, requested profile {} MHz{}",
            self.live.frequency,
            (core_volts() * 1000.0).round() as u32,
            self.live.requested_mhz,
            if self.force_stock { " (stock fallback)" } else { "" }
        ));
        self.publish();
        true
    }

    /// The regulator and sensor setup, volatile and verified; false if
    /// anything does not answer or hold (a regulator without its 5 V supply
    /// does not answer).
    fn configure(&mut self) -> bool {
        // The regulator is kept off throughout configuration and WiFi
        // provisioning.
        let disabled = self.write8(TPS, 0x01, 0);
        let mut id = [0u8; 1];
        if !self.read(EMC, 0xfd, &mut id) || (id[0] != 0x16 && id[0] != 0x28) {
            if self.log_setup {
                self.log("[gamma] EMC2101 fan/temperature sensor not found");
            }
            self.log_setup = false;
            return false;
        }
        let emc = self.verify8(EMC, 0x4a, 0x23)
            && self.set_fan_duty(63, true)
            && self.verify8(EMC, 0x03, 0x04)
            && self.verify8(EMC, 0x17, 0x24)
            && self.verify8(EMC, 0x18, 0);
        let mut device = [0u8; 7];
        let mut mode = [0u8; 1];
        let prefix = [6u8, 0x54, 0x49, 0x54, 0x6d, 0x24];
        if !emc
            || !disabled
            || !self.read(TPS, 0xad, &mut device)
            || device[..6] != prefix
            || (device[6] != 0x41 && device[6] != 0x62)
            || !self.read(TPS, 0x20, &mut mode)
        {
            if self.log_setup && emc {
                self.log("[gamma] TPS546 regulator not found (is the 5V supply plugged in?)");
            }
            self.log_setup = false;
            return false;
        }
        self.voltage_mode = mode[0];
        // Volatile PMBus writes only, the values of ESP-Miner.
        let core = core_volts();
        let tps = self.verify8(TPS, 0x02, 0x1b)
            && self.verify16(0xec, 0)
            && self.verify8(TPS, 0xe4, 0x10)
            && self.verify8(TPS, 0x04, 0)
            && self.linear_int(0x33, 650)
            && self.linear(0x35, 4.8, None)
            && self.linear(0x36, 4.5, None)
            && self.linear(0x55, 6.5, None)
            && self.verify8(TPS, 0x56, 0xb7)
            && self.linear(0x29, 0.25, None)
            && self.voltage(0x24, 2.0, 0.004)
            && self.voltage(0x2b, 1.0, 0.004)
            && self.voltage(0x21, core, 0.004)
            && self.limit(0x40, 1.25)
            && self.limit(0x42, 1.16)
            && self.limit(0x25, 1.1)
            && self.limit(0x26, 0.9)
            && self.limit(0x43, 0.9)
            && self.limit(0x44, 0.75)
            // Current limits: the hardware rounds them up to 2 A steps.
            && self.linear(0x4a, 25.0, Some(2.0))
            && self.linear(0x46, 30.0, Some(2.0))
            && self.verify8(TPS, 0x47, 0xc0)
            && self.linear_int(0x51, 105)
            && self.linear_int(0x4f, 145)
            && self.verify8(TPS, 0x50, 0xff)
            && self.linear_int(0x60, 0)
            && self.linear_int(0x61, 3)
            && self.linear_int(0x62, 0)
            && self.verify8(TPS, 0x63, 0x3b)
            // TOFF_FALL (65h) accepts 0.5 to 31.75 ms only: left as it is.
            && self.linear_int(0x64, 0)
            && self.pin_detect_override()
            && self.clear_faults();
        if tps {
            self.log("[gamma] regulator and sensors configured");
        }
        self.log_setup = tps;
        tps
    }

    fn low_difficulty_note(&mut self, difficulty: f64) {
        if difficulty < 16.0 && difficulty != self.noted_difficulty {
            self.log(&format!(
                "[gamma] pool difficulty {difficulty} is below 16: only shares from 16 up are sent, the pool shows a lower hashrate"
            ));
        }
        self.noted_difficulty = difficulty;
    }

    /// Waiting for work while powered: true while the grace period lasts.
    fn keep_running_without_work(&mut self, t: u32) -> bool {
        if !self.powered {
            return false;
        }
        if self.no_work_since == 0 {
            self.no_work_since = t.max(1);
            self.live.state = "Waiting for pool";
            self.publish();
            self.log(&format!("[gamma] no pool work: ASIC kept running for up to {} s", NO_WORK_GRACE_MS / 1000));
        }
        if t.wrapping_sub(self.no_work_since) < NO_WORK_GRACE_MS {
            return true;
        }
        self.off("Paused / waiting for pool", false);
        false
    }

    fn work_available(&mut self) {
        if self.no_work_since == 0 {
            return;
        }
        self.no_work_since = 0;
        if self.powered && self.live.running {
            self.live.state = self.mining_state;
            self.publish();
        }
    }

    /// One pass of the worker loop.
    fn step(&mut self) {
        let mut t = now();
        if self.latched {
            self.no_work_since = 0;
            delay_ms(100);
            return;
        }
        // A regulator without its 5 V does not answer: try again until it does.
        if !self.configured {
            if t.wrapping_sub(self.setup_at) >= 5000 {
                self.setup_at = t;
                self.configured = self.configure();
                self.live.state = if self.configured { "Ready / waiting for pool" } else { NOT_FOUND };
                self.publish();
            }
            delay_ms(100);
            return;
        }
        let power = self.power_percent();
        if power > 0 && power < 10 {
            if self.powered {
                self.off("Gamma power limit must be 10-100% or paused", false);
            }
            self.live.state = "Gamma power limit must be 10-100% or paused";
            self.publish();
            delay_ms(10);
            return;
        }
        // Keep sampling while paused so thermal hysteresis can observe cooldown.
        if t.wrapping_sub(self.sampled) >= 250 {
            self.sampled = t;
            if !self.collect(self.powered) {
                self.no_work_since = 0;
                return;
            }
        }
        if self.power_percent() == 0 {
            self.no_work_since = 0;
            if self.powered {
                self.off("Paused / waiting for pool", false);
            }
            delay_ms(10);
            return;
        }
        let ready = self.ready();
        if !ready && !self.keep_running_without_work(t) {
            self.no_work_since = 0;
            delay_ms(10);
            return;
        }
        if ready && (!self.powered || t.wrapping_sub(self.sent) >= 100) {
            match self.app.miner.asic_work() {
                // The job board was busy (the network task publishing a job):
                // not a reason to stop.
                None => {
                    if !self.powered {
                        delay_ms(10);
                        return;
                    }
                }
                Some(next) => {
                    self.work_available();
                    // Without enough negotiated rolling bits the chip exhausts
                    // its nonce space: refuse that pool.
                    if next.work.version_mask.count_ones() < 8 {
                        if self.powered {
                            self.off("Pool needs version rolling", false);
                        }
                        self.live.state = "Pool needs version rolling";
                        self.publish();
                        delay_ms(100);
                        return;
                    }
                    self.low_difficulty_note(next.work.share_difficulty);
                    if self.powered && self.active_mask != next.work.version_mask {
                        self.off("Changing version mask", false);
                    }
                    // A clean job (a new block): the ASIC keeps running on the
                    // new work; results of earlier jobs are dropped.
                    if self.powered && next.generation != self.active_generation {
                        self.active_generation = next.generation;
                        for slot in &mut self.slots {
                            slot.work = None;
                        }
                    }
                    if !self.powered {
                        if !self.init_asic(&next) {
                            if !self.latched && self.powered {
                                self.init_failed();
                            } else if !self.powered {
                                delay_ms(500); // waiting for the supply, the fan or the pool
                            }
                            return;
                        }
                        self.mining_state = self.live.state;
                        t = now();
                        self.sampled = t;
                    }
                    let target = self.target_frequency();
                    if (self.live.frequency - target).abs() > 3.0 {
                        // Power changes reset the chip.
                        self.off("Applying power limit", false);
                        return;
                    }
                    let packet = bm1370::job(self.next_slot as u8, &next.work.header);
                    self.slots[self.next_slot] = Slot { work: Some(next), sent: t, ..Default::default() };
                    if !self.send(&packet) {
                        self.off("ASIC UART write failed", true);
                        return;
                    }
                    self.next_slot = (self.next_slot + 1) % bm1370::SLOTS;
                    self.sent = t;
                }
            }
        }
        let mut budget = 2048;
        while self.powered && budget > 0 {
            let Some(byte) = self.uart.read() else { break };
            budget -= 1;
            let Some(r) = self.receiver.push(byte) else { continue };
            if !r.job {
                self.register_result(&r);
                continue;
            }
            let slot = &mut self.slots[r.slot as usize % bm1370::SLOTS];
            let Some(work) = slot.work.clone() else { continue };
            if t.wrapping_sub(slot.sent) > 1500 || !self.app.miner.current(work.generation) {
                continue;
            }
            let proof = (r.version_bits as u64) << 32 | r.nonce as u64;
            if slot.proofs[..slot.proof_count].contains(&proof) {
                continue;
            }
            slot.proofs[slot.proof_head] = proof;
            slot.proof_head = (slot.proof_head + 1) % slot.proofs.len();
            if slot.proof_count < slot.proofs.len() {
                slot.proof_count += 1;
            }
            self.app.miner.asic_candidate(&work, r.nonce, r.version_bits, 1.0);
        }
        if !self.powered {
            return;
        }
        if t.wrapping_sub(self.requested) >= 1000 {
            self.requested = t;
            if !self.request(0x4c, false) || !self.request(0x8c, false) {
                self.off("ASIC counter request failed", true);
                return;
            }
        }
        self.check_watchdogs(now());
        delay_ms(2);
    }

    fn run(mut self) -> ! {
        delay_ms(2000); // fan spin-up, ASIC held in reset and unpowered
        self.sampled = now();
        self.setup_at = self.sampled;
        loop {
            self.step();
        }
    }
}

static DRIVER: Mutex<Option<Driver>> = Mutex::new(None);

/// First thing at boot: the ASIC held in reset (GPIO1 low).
pub fn hold_reset() {
    output(RESET, false);
}

/// The regulator and sensors, configured with the ASIC off, after the
/// screen (the SSD1306 shares the I2C bus, SDA 47 / SCL 48).
pub fn begin(app: Arc<App>) {
    let storage = app.storage.clone();
    output(RESET, false);
    let mut driver = Driver {
        app,
        engine: 0,
        i2c: I2c::new(0, 47, 48, &[TPS, EMC]),
        uart: Uart::new(1, 17, 18),
        storage,
        live: shared().status.lock().unwrap().clone(),
        fan_policy: FanControl::default(),
        applied_fan_duty: 255,
        configured: false,
        latched: false,
        powered: false,
        log_setup: true,
        voltage_mode: 0,
        diode_strikes: 0,
        last_diode: f32::NAN,
        diode_ready: false,
        power_enabled_at: 0,
        last_health_sample: 0,
        receiver: bm1370::Receiver::default(),
        counter: bm1370::Counter::default(),
        errors: bm1370::ErrorMonitor::default(),
        force_stock: false,
        error_seen: false,
        error_value: 0,
        last_error_reply: 0,
        slots: vec![Slot::default(); bm1370::SLOTS],
        next_slot: 0,
        active_generation: 0,
        active_mask: 0,
        last_hash_reply: 0,
        last_hash_progress: 0,
        no_work_since: 0,
        auto_restarts: 0,
        auto_restart_since: 0,
        mining_state: "Mining (experimental)",
        sampled: 0,
        sent: 0,
        requested: 0,
        setup_at: 0,
        noted_difficulty: 0.0,
    };
    if driver.i2c.is_none() {
        driver.log("[gamma] I2C bus could not start");
    }
    driver.load_fault();
    let fault = shared().last_fault.lock().unwrap().0.clone();
    if !fault.is_empty() {
        driver.log(&format!("[gamma] last fault before this start: {fault}"));
    }
    driver.configured = driver.configure();
    driver.live.state = if driver.configured { "Ready / waiting for pool" } else { NOT_FOUND };
    driver.publish();
    // A first reading at once: the thermal policy has a temperature from the
    // start.
    if driver.configured {
        driver.collect(false);
    }
    *DRIVER.lock().unwrap() = Some(driver);
}

pub fn start(engine: usize) -> bool {
    let Some(mut driver) = DRIVER.lock().unwrap().take() else { return false };
    driver.engine = engine;
    let command = FAN_SETTING.load(Ordering::Relaxed);
    driver.live.fan_automatic = command & 0x100 != 0;
    driver.live.requested_fan_percent = command as u8;
    driver.live.requested_mhz = requested_mhz();
    driver.load_fallback(driver.live.requested_mhz);
    driver.publish();
    // Started even when the setup failed: the task tries it again.
    crate::spawn_pinned(b"bm1370\0", esp_idf_svc::hal::cpu::Core::Core1, 3, 8 * 1024, move || driver.run())
}

pub fn status() -> Status {
    shared().status.lock().unwrap().clone()
}

/// The ASIC diode when it reads; else the board's sensor, so that a pause
/// never looks like a lost sensor.
pub fn temperature() -> f32 {
    let asic = shared().status.lock().unwrap().temperature;
    if asic.is_finite() {
        asic
    } else {
        f32::from_bits(BOARD_TEMPERATURE.load(Ordering::Relaxed))
    }
}

pub fn last_fault() -> (String, u32) {
    shared().last_fault.lock().unwrap().clone()
}
