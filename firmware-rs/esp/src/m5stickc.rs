//! M5StickC Plus2 (feature `m5stickc-plus2`): the power-hold pin and the
//! IMU's die temperature, next to the battery (the board's thermometer in
//! the C++ firmware too).

use std::sync::Mutex;

use esp_idf_svc::hal::gpio::Gpio4;

// The MPU6886 on the internal bus (SDA 21, SCL 22), through ESP-IDF's new I2C
// master driver (src/periph.rs): ESP-IDF aborts at boot ("CONFLICT!
// driver_ng is not allowed to be used with this old driver") when its legacy
// driver (esp-idf-hal's I2cDriver) is linked next to the new one.
const IMU_ADDRESS: u8 = 0x68;

/// Serialises the IMU reads.
static IMU: Mutex<Option<crate::periph::I2c>> = Mutex::new(None);

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
    let Some(i2c) = crate::periph::I2c::new(0, 21, 22, &[IMU_ADDRESS]) else {
        log::warn!("[board] IMU bus not starting: no temperature");
        return None;
    };
    let mut id = [0u8; 1];
    if !i2c.read(IMU_ADDRESS, 0x75, &mut id) {
        // WHO_AM_I
        log::warn!("[board] IMU not answering: no temperature");
        return None;
    }
    if id[0] != 0x19 {
        log::warn!("[board] IMU id {:#04x}, not an MPU6886: no temperature", id[0]);
        return None;
    }
    // Out of sleep, internal clock.
    if !i2c.write(IMU_ADDRESS, &[0x6B, 0x00]) {
        log::warn!("[board] IMU not waking up: no temperature");
        return None;
    }
    *IMU.lock().unwrap() = Some(i2c);
    Some(Box::new(|| {
        let imu = IMU.lock().unwrap();
        let mut raw = [0u8; 2];
        match imu.as_ref() {
            // TEMP_OUT_H, TEMP_OUT_L
            Some(i2c) if i2c.read(IMU_ADDRESS, 0x41, &mut raw) => i16::from_be_bytes(raw) as f32 / 326.8 + 25.0,
            _ => f32::NAN,
        }
    }))
}

/// The battery behind its 1:2 divider on GPIO38 (as M5Unified reads it).
static BATTERY: Mutex<Option<(crate::periph::Adc, f32)>> = Mutex::new(None);

/// The battery voltage in millivolts, smoothed; `None` without a reading.
pub fn battery_mv() -> Option<u32> {
    let mut battery = BATTERY.lock().unwrap();
    if battery.is_none() {
        *battery = crate::periph::Adc::new(38).map(|adc| (adc, 0.0));
    }
    let (adc, smoothed) = battery.as_mut()?;
    let mv = (0..8).map(|_| adc.millivolts()).sum::<u32>() as f32 / 8.0 * 2.0;
    if mv < 1000.0 {
        return None;
    }
    // A running average: the voltage dips while the radio transmits.
    *smoothed = if *smoothed == 0.0 { mv } else { *smoothed * 0.8 + mv * 0.2 };
    Some(*smoothed as u32)
}

/// Battery level in percent (M5Unified's scale for this board), -1 unknown.
pub fn battery_level() -> i32 {
    battery_mv().map_or(-1, |mv| ((mv as i32 - 3300) * 100 / 850).clamp(0, 100))
}

/// 1 on battery, 0 on USB, -1 unknown. There is no power chip: below
/// ~4.05 V the battery is not being held up by USB (the C++ firmware's rule).
pub fn on_battery() -> i32 {
    battery_mv().map_or(-1, |mv| i32::from(mv < 4050))
}

/// The red LED (GPIO19, shared with the IR emitter, lit when high).
pub fn led(on: bool) {
    use esp_idf_svc::sys::{gpio_config, gpio_config_t, gpio_mode_t_GPIO_MODE_OUTPUT, gpio_set_level};
    static READY: std::sync::Once = std::sync::Once::new();
    READY.call_once(|| unsafe {
        gpio_set_level(19, 0);
        gpio_config(&gpio_config_t { pin_bit_mask: 1 << 19, mode: gpio_mode_t_GPIO_MODE_OUTPUT, ..std::mem::zeroed() });
    });
    unsafe { gpio_set_level(19, u32::from(on)) };
}

/// Gravity along the screen's width at rotation 0, in g (MPU6886, ±2 g).
pub fn accel_x() -> Option<f32> {
    let imu = IMU.lock().unwrap();
    let mut raw = [0u8; 2];
    // ACCEL_XOUT_H, ACCEL_XOUT_L
    (imu.as_ref()?.read(IMU_ADDRESS, 0x3B, &mut raw)).then(|| i16::from_be_bytes(raw) as f32 / 16384.0)
}
