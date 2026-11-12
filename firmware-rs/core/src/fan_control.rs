//! Bitaxe Gamma fan policy (src/core/fan_control.h): full cooling always
//! takes priority over the user's settings.

pub const FAN_MIN_PERCENT: u8 = 30;

pub fn fan_percent(value: u32) -> u8 {
    value.clamp(FAN_MIN_PERCENT as u32, 100) as u8
}

/// EMC2101 direct setting, 0..63, rounded up so that the minimum is never
/// undershot.
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
        let full_at = if thermal_limit >= 30 { 60u32.min(thermal_limit - 5) as f32 } else { 60.0 };
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
            // Default curve: 30% at <=40 C, full at 60 C (regulator: 60..80 C).
            // A lower user thermal limit moves the ASIC curve down with it.
            let distance = (asic_c - full_at).max(regulator_c - 80.0);
            target = fan_percent(30f32.max((100.0 + distance * 3.5).ceil()) as u32);
        }
        // Raise cooling immediately. Automatic reductions wait at least 5 s
        // after the previous change; manual changes apply on the next sample.
        if !automatic || target >= self.current || now.wrapping_sub(self.changed_at) >= 5000 {
            if self.current != target {
                self.changed_at = now;
            }
            self.current = target;
        }
        FanDecision { percent: self.current, forced: false, reason: if automatic { "Automatic" } else { "Manual" } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pwm_rounding_never_undershoots() {
        assert_eq!(fan_pwm(30), 19);
        assert_eq!(fan_pwm(100), 63);
        assert_eq!(fan_pwm(0), 19);
        assert_eq!(fan_pwm_percent(63), 100);
        assert!(fan_pwm_percent(fan_pwm(30)) >= 30);
    }

    #[test]
    fn policy_matches_the_cpp_one() {
        let mut fan = FanControl::default();
        assert_eq!(fan.update(false, true, 50, 40.0, 40.0, 0, 65).reason, "Startup or stopped ASIC");
        assert!(fan.update(true, true, 50, f32::NAN, 40.0, 10, 65).forced);
        // Automatic at 40 C: the curve's floor.
        assert_eq!(fan.update(true, true, 50, 40.0, 40.0, 20, 65).percent, 100);
        assert_eq!(fan.update(true, true, 50, 40.0, 40.0, 6000, 65).percent, 30);
        // Hot: full, with hysteresis down to 55 C.
        let hot = fan.update(true, true, 50, 60.0, 40.0, 7000, 65);
        assert!(hot.forced && hot.percent == 100);
        assert!(fan.update(true, true, 50, 57.0, 40.0, 8000, 65).forced);
        assert!(!fan.update(true, true, 50, 54.0, 40.0, 9000, 65).forced);
        // Manual: the requested percent, at least 30.
        let mut manual = FanControl::default();
        assert_eq!(manual.update(true, false, 10, 40.0, 40.0, 0, 65).percent, 30);
    }
}
