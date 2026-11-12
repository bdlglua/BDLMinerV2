//! M5StickC Plus2 (feature `m5stickc-plus2`): the power-hold pin and the
//! IMU's die temperature, next to the battery (the board's thermometer in
//! the C++ firmware too).

use std::sync::Mutex;

use esp_idf_svc::hal::gpio::{Gpio4, PinDriver};

// components/bdl_ui/ui_ffi.cpp: the MPU6886 through M5GFX's I2C driver.
// esp-idf-hal's I2cDriver uses ESP-IDF's legacy driver, and ESP-IDF aborts
// at boot ("CONFLICT! driver_ng is not allowed to be used with this old
// driver") when it is linked next to the new one M5GFX uses.
extern "C" {
    fn bdl_imu_begin() -> i32;
    fn bdl_imu_temperature() -> f32;
}

/// Serialises the IMU reads.
static IMU: Mutex<()> = Mutex::new(());

/// On battery the board turns itself off unless GPIO4 stays high: call
/// first thing at boot.
pub fn hold_power(pin: Gpio4) {
    match PinDriver::output(pin) {
        Ok(mut hold) => {
            let _ = hold.set_high();
            // Keep the pin driven for as long as the firmware runs.
            std::mem::forget(hold);
        }
        Err(e) => log::error!("power hold: {e:?}"),
    }
}

/// The IMU's temperature reader, or `None` when it does not answer.
pub fn imu() -> Option<Box<dyn Fn() -> f32 + Send + Sync>> {
    let id = {
        let _bus = IMU.lock().unwrap();
        unsafe { bdl_imu_begin() }
    };
    if id < 0 {
        log::warn!("[board] IMU not answering: no temperature");
        return None;
    }
    if id != 0x19 {
        log::warn!("[board] IMU id {id:#04x}, not an MPU6886: no temperature");
        return None;
    }
    Some(Box::new(|| {
        let _bus = IMU.lock().unwrap();
        unsafe { bdl_imu_temperature() }
    }))
}
