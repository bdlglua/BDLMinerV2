//! M5StickC Plus2 (feature `m5stickc-plus2`): the power-hold pin and the
//! IMU's die temperature, next to the battery (the board's thermometer in
//! the C++ firmware too).

use std::sync::Mutex;

use esp_idf_svc::hal::delay::TickType;
use esp_idf_svc::hal::gpio::{Gpio21, Gpio22, Gpio4, PinDriver};
use esp_idf_svc::hal::i2c::{I2cConfig, I2cDriver, I2C0};
use esp_idf_svc::hal::units::Hertz;

/// MPU6886 on the internal I2C bus (SDA 21, SCL 22).
const MPU6886: u8 = 0x68;
const WHO_AM_I: u8 = 0x75;
const PWR_MGMT_1: u8 = 0x6B;
const TEMP_OUT_H: u8 = 0x41;

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
pub fn imu(i2c: I2C0, sda: Gpio21, scl: Gpio22) -> Option<Box<dyn Fn() -> f32 + Send + Sync>> {
    let config = I2cConfig::new().baudrate(Hertz(400_000));
    let mut bus = I2cDriver::new(i2c, sda, scl, &config).ok()?;
    let timeout = TickType::new_millis(50).ticks();
    let mut id = [0u8; 1];
    bus.write_read(MPU6886, &[WHO_AM_I], &mut id, timeout).ok()?;
    if id[0] != 0x19 {
        log::warn!("[board] IMU id {:#04x}, not an MPU6886: no temperature", id[0]);
        return None;
    }
    // Out of sleep, internal clock.
    bus.write(MPU6886, &[PWR_MGMT_1, 0x00], timeout).ok()?;
    let bus = Mutex::new(bus);
    Some(Box::new(move || {
        let mut raw = [0u8; 2];
        match bus.lock().unwrap().write_read(MPU6886, &[TEMP_OUT_H], &mut raw, timeout) {
            Ok(()) => i16::from_be_bytes(raw) as f32 / 326.8 + 25.0,
            Err(_) => f32::NAN,
        }
    }))
}
