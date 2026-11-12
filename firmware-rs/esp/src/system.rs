//! Clock, randomness and device readings from ESP-IDF.

use std::ffi::CString;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use bdlminer_app::platform::{Clock, SystemInfo, CLOCK_SET};
use esp_idf_svc::sys::*;

pub fn millis() -> u32 {
    (unsafe { esp_timer_get_time() } / 1000) as u32
}

pub fn random(out: &mut [u8]) {
    // Cryptographically secure while WiFi (or the bootloader RNG) is on.
    unsafe { esp_fill_random(out.as_mut_ptr().cast(), out.len()) };
}

pub fn mac() -> u64 {
    let mut mac = [0u8; 8];
    unsafe { esp_efuse_mac_get_default(mac.as_mut_ptr()) };
    u64::from_le_bytes(mac)
}

/// millis() since boot; wall time once SNTP (or a pool job) set the clock.
pub struct EspClock;

impl EspClock {
    pub fn new(timezone: &str) -> Self {
        // POSIX TZ string, as the C++ firmware passes to configTzTime().
        if let (Ok(name), Ok(value)) = (CString::new("TZ"), CString::new(timezone)) {
            unsafe {
                setenv(name.as_ptr(), value.as_ptr(), 1);
                tzset();
            }
        }
        EspClock
    }
}

impl Clock for EspClock {
    fn millis(&self) -> u32 {
        millis()
    }
    fn unix_time(&self) -> Option<u32> {
        let t = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        (t > CLOCK_SET as u64).then_some(t as u32)
    }
    fn local_hour(&self) -> Option<u8> {
        let now = self.unix_time()? as time_t;
        let mut tm: tm = unsafe { std::mem::zeroed() };
        unsafe { localtime_r(&now, &mut tm) };
        Some(tm.tm_hour as u8)
    }
}

pub struct EspSystem {
    pub ip: Mutex<String>,
    pub ssid: Mutex<String>,
    /// A board thermometer that takes precedence over the chip's (the
    /// M5StickC Plus2's IMU).
    pub imu: Option<Box<dyn Fn() -> f32 + Send + Sync>>,
}

impl EspSystem {
    fn reading(&self) -> (f32, &'static str) {
        if let Some(imu) = &self.imu {
            return (imu(), "imu");
        }
        (chip_temperature(), "chip")
    }
}

/// The chip's own temperature sensor (ESP32-S2/S3/C3); the original ESP32
/// has no usable one.
#[cfg(any(esp32s2, esp32s3, esp32c3))]
fn chip_temperature() -> f32 {
    use std::sync::OnceLock;
    static SENSOR: OnceLock<usize> = OnceLock::new();
    let handle = *SENSOR.get_or_init(|| {
        let config = temperature_sensor_config_t { range_min: 20, range_max: 100, ..Default::default() };
        let mut handle: temperature_sensor_handle_t = std::ptr::null_mut();
        unsafe {
            if temperature_sensor_install(&config, &mut handle) != ESP_OK || temperature_sensor_enable(handle) != ESP_OK
            {
                return 0;
            }
        }
        handle as usize
    });
    if handle == 0 {
        return f32::NAN;
    }
    let mut celsius = f32::NAN;
    if unsafe { temperature_sensor_get_celsius(handle as temperature_sensor_handle_t, &mut celsius) } != ESP_OK {
        return f32::NAN;
    }
    celsius
}

#[cfg(not(any(esp32s2, esp32s3, esp32c3)))]
fn chip_temperature() -> f32 {
    f32::NAN
}

impl SystemInfo for EspSystem {
    fn free_heap(&self) -> u32 {
        unsafe { esp_get_free_heap_size() }
    }
    fn max_alloc(&self) -> u32 {
        unsafe { heap_caps_get_largest_free_block(MALLOC_CAP_8BIT) as u32 }
    }
    fn ip(&self) -> String {
        self.ip.lock().unwrap().clone()
    }
    fn ssid(&self) -> String {
        self.ssid.lock().unwrap().clone()
    }
    fn rssi(&self) -> i32 {
        let mut info: wifi_ap_record_t = unsafe { std::mem::zeroed() };
        if unsafe { esp_wifi_sta_get_ap_info(&mut info) } == ESP_OK {
            info.rssi as i32
        } else {
            0
        }
    }
    fn temperature(&self) -> f32 {
        self.reading().0
    }
    fn temperature_source(&self) -> &'static str {
        match self.reading() {
            (t, source) if t.is_finite() => source,
            _ => "unavailable",
        }
    }
    fn boot_reason(&self) -> String {
        let reason = unsafe { esp_reset_reason() };
        #[allow(non_upper_case_globals)]
        match reason {
            esp_reset_reason_t_ESP_RST_POWERON => "power on",
            esp_reset_reason_t_ESP_RST_EXT => "reset pin",
            esp_reset_reason_t_ESP_RST_SW => "software restart",
            esp_reset_reason_t_ESP_RST_PANIC => "crash (panic)",
            esp_reset_reason_t_ESP_RST_INT_WDT => "interrupt watchdog",
            esp_reset_reason_t_ESP_RST_TASK_WDT => "task watchdog",
            esp_reset_reason_t_ESP_RST_WDT => "watchdog",
            esp_reset_reason_t_ESP_RST_DEEPSLEEP => "deep sleep wake-up",
            esp_reset_reason_t_ESP_RST_BROWNOUT => "brownout (power supply too weak)",
            _ => "unknown",
        }
        .into()
    }
}
