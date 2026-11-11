//! Thermal policy, Gamma fan policy and the expiring permission to mine.
//! The expiry is checked by each mining task, even while the main loop is
//! blocked (by a TLS handshake for instance).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Mining stops when the main loop has not renewed its permission for this
/// long. A TLS handshake can hold the loop for 2-2.5 s.
pub const SAFETY_TIMEOUT_MS: u32 = 10_000;
pub const FIXED_FILTER: u32 = 0xffff;

pub fn fixed_filter_supports(limit: u32) -> bool {
    limit <= FIXED_FILTER
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TemperatureSource {
    #[default]
    Unknown,
    Imu,
    Chip,
}

impl TemperatureSource {
    pub fn name(self) -> &'static str {
        match self {
            TemperatureSource::Unknown => "unavailable",
            TemperatureSource::Imu => "imu",
            TemperatureSource::Chip => "chip",
        }
    }
}

/// Keeps one physical sensor for the session: falling back from a hot IMU to
/// a cooler chip sensor must not pretend the board cooled down.
#[derive(Clone, Debug, Default)]
pub struct Thermometer {
    pub source: TemperatureSource,
}

impl Thermometer {
    pub fn sample(&mut self, imu: f32, chip: f32) -> f32 {
        if self.source == TemperatureSource::Unknown {
            if imu.is_finite() {
                self.source = TemperatureSource::Imu;
            } else if chip.is_finite() {
                self.source = TemperatureSource::Chip;
            }
        }
        let t = if self.source == TemperatureSource::Imu { imu } else { chip };
        if t.is_finite() {
            t
        } else {
            f32::NAN
        }
    }
}

#[derive(Clone, Debug)]
pub struct ThermalPolicy {
    pub hot: bool,
    pub sensor_seen: bool,
    pub sensor_fault: bool,
    /// Software boards: 50 % / 25 % steps below the limit. ASIC boards: none
    /// (each power change restarts the ASIC) and, once hot, a pause of at
    /// least `hold_ms`.
    pub steps: bool,
    pub hold_ms: u32,
    pub hot_at: u32,
}

impl Default for ThermalPolicy {
    fn default() -> Self {
        ThermalPolicy { hot: false, sensor_seen: false, sensor_fault: false, steps: true, hold_ms: 0, hot_at: 0 }
    }
}

impl ThermalPolicy {
    pub fn power(&mut self, temperature: f32, limit: u8, requested: u8, battery_saver: bool, now: u32) -> u8 {
        let mut result = requested.min(100);
        if battery_saver && result > 50 {
            result = 50;
        }
        self.sensor_fault = limit != 0 && !temperature.is_finite();
        if limit == 0 {
            self.hot = false;
        } else if self.sensor_fault {
            // A missing sensor permits only a conservative duty cycle; losing a
            // previously working sensor stops mining.
            if self.hot || self.sensor_seen {
                return 0;
            }
            result = result.min(25);
        } else {
            self.sensor_seen = true;
            let limit_f = limit as f32;
            if temperature >= limit_f {
                if !self.hot {
                    self.hot_at = now;
                }
                self.hot = true;
            } else if temperature <= limit_f - 5.0
                && (self.hold_ms == 0 || now.wrapping_sub(self.hot_at) >= self.hold_ms)
            {
                self.hot = false;
            }
            if self.hot {
                return 0;
            }
            if !self.steps {
                return result;
            }
            if temperature >= limit_f - 5.0 && result > 25 {
                result = 25;
            } else if temperature >= limit_f - 10.0 && result > 50 {
                result = 50;
            }
        }
        result
    }
}

/// The permission to mine, renewed by the main loop.
#[derive(Debug, Default)]
pub struct SafetyLease {
    sampled_at: AtomicU32,
    requested: AtomicU32,
    ready: AtomicBool,
}

impl SafetyLease {
    pub fn update(&self, now: u32, power: u8) {
        self.requested.store(power.min(100) as u32, Ordering::Relaxed);
        self.sampled_at.store(now, Ordering::Relaxed);
        self.ready.store(true, Ordering::Release);
    }
    /// The loop is alive: keep the last decided power.
    pub fn renew(&self, now: u32) {
        if self.ready.load(Ordering::Acquire) {
            self.sampled_at.store(now, Ordering::Relaxed);
        }
    }
    pub fn power(&self, now: u32) -> u8 {
        if !self.ready.load(Ordering::Acquire)
            || now.wrapping_sub(self.sampled_at.load(Ordering::Relaxed)) > SAFETY_TIMEOUT_MS
        {
            return 0;
        }
        self.requested.load(Ordering::Relaxed) as u8
    }
}

/// Rest after `busy_ms` of hashing so that hashing takes `power` percent.
pub fn rest_ms(busy_ms: u32, power: u8) -> u32 {
    if power == 0 || power >= 100 {
        return 0;
    }
    let busy = if busy_ms != 0 { busy_ms } else { 1 } as u64;
    (busy * (100 - power as u64)).div_ceil(power as u64) as u32
}

pub const FAN_MIN_PERCENT: u8 = 30;

pub fn fan_percent(value: u32) -> u8 {
    value.clamp(FAN_MIN_PERCENT as u32, 100) as u8
}
/// EMC2101 direct setting 0..63, rounded up so the minimum is never undershot.
pub fn fan_pwm(percent: u32) -> u8 {
    (fan_percent(percent) as u32 * 63).div_ceil(100) as u8
}
pub fn fan_pwm_percent(pwm: u8) -> u8 {
    ((pwm as u32 * 100 + 31) / 63) as u8
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FanDecision {
    pub percent: u8,
    pub forced: bool,
    pub reason: &'static str,
}

/// Gamma fan policy: full cooling always takes priority over user settings.
#[derive(Clone, Debug)]
pub struct FanControl {
    hot: bool,
    current: u8,
    changed_at: u32,
}

impl Default for FanControl {
    fn default() -> Self {
        FanControl { hot: false, current: 100, changed_at: 0 }
    }
}

impl FanControl {
    fn full(&mut self, reason: &'static str, now: u32) -> FanDecision {
        self.current = 100;
        self.changed_at = now;
        FanDecision { percent: 100, forced: true, reason }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        running: bool,
        automatic: bool,
        requested: u32,
        asic_c: f32,
        regulator_c: f32,
        now: u32,
        thermal_limit: u32,
    ) -> FanDecision {
        if !running {
            return self.full("Startup or stopped ASIC", now);
        }
        if !asic_c.is_finite() || asic_c < 0.0 || !regulator_c.is_finite() || regulator_c < -20.0 {
            return self.full("Temperature sensor unavailable", now);
        }
        let full_at = if thermal_limit >= 30 { 60.min(thermal_limit - 5) as f32 } else { 60.0 };
        if asic_c >= full_at || regulator_c >= 80.0 {
            self.hot = true;
        } else if asic_c <= full_at - 5.0 && regulator_c <= 75.0 {
            self.hot = false;
        }
        if self.hot {
            return self.full("Temperature safety", now);
        }
        let mut target = fan_percent(requested);
        if automatic {
            // 30 % at <= 40 C, full at 60 C (regulator: 60..80 C).
            let distance = (asic_c - full_at).max(regulator_c - 80.0);
            target = fan_percent((100.0 + distance * 3.5).ceil().max(30.0) as u32);
        }
        // Raise cooling immediately; automatic reductions wait 5 s.
        if !automatic || target >= self.current || now.wrapping_sub(self.changed_at) >= 5000 {
            if self.current != target {
                self.changed_at = now;
            }
            self.current = target;
        }
        FanDecision { percent: self.current, forced: false, reason: if automatic { "Automatic" } else { "Manual" } }
    }
}
