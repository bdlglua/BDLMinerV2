//! The ASIC boards (features `gamma601`, `nerdnos`): pure Rust drivers
//! (gamma601.rs, nerdnos.rs, ported from the C++ firmware's src/gamma601.cpp
//! and src/nerdnos.cpp) mine through the Rust miner, which hands them work
//! and verifies, counts and submits their results.

use std::sync::Arc;

use bdlminer_app::App;

/// What the dashboard and the screen show of the ASIC and its board.
#[derive(Clone, Debug)]
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
    pub state: &'static str,
    pub requested_mhz: u16,
    pub stock_fallback: bool,
    pub fallback_reason: String,
    pub hardware_error_percent: f32,
    pub fan_percent: i16,
    pub requested_fan_percent: u8,
    pub fan_automatic: bool,
    pub fan_override: bool,
    pub fan_reason: &'static str,
    pub results: u32,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            temperature: 0.0,
            regulator_temperature: 0.0,
            input_voltage: 0.0,
            core_voltage: 0.0,
            current: 0.0,
            frequency: 0.0,
            rpm: 0,
            regulator_status: 0,
            running: false,
            state: "",
            requested_mhz: 0,
            stock_fallback: false,
            fallback_reason: String::new(),
            hardware_error_percent: 0.0,
            fan_percent: 0,
            requested_fan_percent: 0,
            fan_automatic: false,
            fan_override: false,
            fan_reason: "",
            results: 0,
        }
    }
}

impl Status {
    pub fn state(&self) -> String {
        self.state.to_string()
    }

    pub fn set_state(&mut self, state: &'static str) {
        self.state = state;
    }
}

/// First thing at boot: the ASIC held in reset.
pub fn hold_reset() {
    #[cfg(feature = "gamma601")]
    crate::gamma601::hold_reset();
}

/// The driver's settings (Gamma: frequency, core voltage, fan, limit).
pub fn apply_settings(app: &App) {
    #[cfg(feature = "gamma601")]
    {
        let s = app.settings();
        crate::gamma601::set_settings(s.asic_frequency, s.asic_voltage, s.fan_auto, s.fan_percent, s.max_temp);
    }
    #[cfg(not(feature = "gamma601"))]
    let _ = app;
}

/// The board's regulator, sensors and ASIC, still off (after the screen:
/// the Gamma's shares its I2C bus).
pub fn begin(app: &Arc<App>) {
    apply_settings(app);
    #[cfg(feature = "gamma601")]
    crate::gamma601::begin(app.clone());
    #[cfg(feature = "nerdnos")]
    crate::nerdnos::begin();
}

/// Starts the ASIC task; it powers the chip once the pool has work.
pub fn start(app: Arc<App>) -> bool {
    let Some(engine) = app.miner.register_engine("asic") else { return false };
    #[cfg(feature = "gamma601")]
    {
        let _ = app;
        return crate::gamma601::start(engine);
    }
    #[cfg(feature = "nerdnos")]
    return crate::nerdnos::start(app, engine);
}

pub fn status() -> Status {
    #[cfg(feature = "gamma601")]
    return crate::gamma601::status();
    #[cfg(feature = "nerdnos")]
    return crate::nerdnos::status();
}

/// The board's thermometer for the thermal policy, sampled once a second
/// (the NerdNOS driver stops its ASIC when its NTC stops reading).
pub fn temperature() -> f32 {
    #[cfg(feature = "gamma601")]
    return crate::gamma601::temperature();
    #[cfg(feature = "nerdnos")]
    return crate::nerdnos::temperature();
}
