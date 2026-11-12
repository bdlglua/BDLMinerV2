//! M5StickC Plus2 (feature `m5stickc-plus2`): the power-hold pin and the
//! IMU's die temperature, next to the battery (the board's thermometer in
//! the C++ firmware too).

use std::sync::Mutex;

use esp_idf_svc::hal::gpio::Gpio4;

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

/// The board turns itself off unless GPIO4 goes high soon after the power
/// button or the USB cable wakes it: call first thing at boot. The level is
/// set before the pin becomes an output, and the pin is not reset first
/// (`PinDriver::output` would), so the hold never dips.
pub fn hold_power(_pin: Gpio4) {
    use esp_idf_svc::sys::{gpio_config, gpio_config_t, gpio_mode_t_GPIO_MODE_OUTPUT, gpio_set_level};
    let config = gpio_config_t {
        pin_bit_mask: 1 << 4,
        mode: gpio_mode_t_GPIO_MODE_OUTPUT,
        // No pull-up or pull-down, no interrupt.
        ..unsafe { std::mem::zeroed() }
    };
    // The output register first: the pin drives high from the moment it
    // becomes an output.
    let ok = unsafe { gpio_set_level(4, 1) == 0 && gpio_config(&config) == 0 && gpio_set_level(4, 1) == 0 };
    if !ok {
        log::error!("power hold: GPIO4 could not be driven high");
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
