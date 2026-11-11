//! What the firmware needs from the board and its operating system. The
//! ESP32 build implements these with ESP-IDF, the host build with std.

use std::io;
use std::time::Duration;

/// Milliseconds since boot (wrapping like Arduino's `millis()`) and wall time.
pub trait Clock: Send + Sync {
    fn millis(&self) -> u32;
    /// Unix time, when the clock was set (NTP or a pool job).
    fn unix_time(&self) -> Option<u32>;
    /// Local hour 0..23 in the configured time zone, when the clock was set.
    fn local_hour(&self) -> Option<u8>;
    fn sleep(&self, d: Duration) {
        std::thread::sleep(d);
    }
}

/// A source of cryptographically secure random bytes.
pub type RandomFn = std::sync::Arc<dyn Fn(&mut [u8]) + Send + Sync>;

/// Unix times before this are an unset clock.
pub const CLOCK_SET: u32 = 1_700_000_000;

/// Key-value flash storage (NVS on the ESP32), by namespace.
pub trait Storage: Send {
    fn get(&mut self, namespace: &str, key: &str) -> Option<Vec<u8>>;
    fn set(&mut self, namespace: &str, key: &str, value: &[u8]) -> bool;
    fn remove(&mut self, namespace: &str, key: &str) -> bool;
    fn clear(&mut self, namespace: &str) -> bool;
}

/// A byte stream to a pool. `read` returns `Ok(0)` when the peer closed the
/// connection and an error of kind `WouldBlock` or `TimedOut` when no data
/// arrived within the poll interval.
pub trait Connection: Send {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    fn write_all(&mut self, data: &[u8]) -> io::Result<()>;
}

/// Outgoing connections and the state of the link (WiFi).
pub trait Network: Send + Sync {
    type Conn: Connection;
    fn connect(&self, host: &str, port: u16, timeout: Duration) -> io::Result<Self::Conn>;
    fn link_up(&self) -> bool {
        true
    }
    /// Asks the link to reconnect (the WiFi driver also does it by itself).
    fn reconnect(&self) {}
    /// Tries the fallback networks of the settings once; true if joined.
    fn try_fallbacks(&self) -> bool {
        false
    }
    /// Cryptographically secure random bytes.
    fn random(&self, out: &mut [u8]);
}

/// Values the dashboard shows about the device itself.
pub trait SystemInfo: Send + Sync {
    fn free_heap(&self) -> u32 {
        0
    }
    fn max_alloc(&self) -> u32 {
        0
    }
    fn ip(&self) -> String {
        String::new()
    }
    fn ssid(&self) -> String {
        String::new()
    }
    fn rssi(&self) -> i32 {
        0
    }
    fn temperature(&self) -> f32 {
        f32::NAN
    }
    fn temperature_source(&self) -> &'static str {
        "unavailable"
    }
    /// 1 on battery, 0 on external power, -1 unknown or no battery.
    fn on_battery(&self) -> i32 {
        -1
    }
    fn ota_supported(&self) -> bool {
        false
    }
    fn boot_reason(&self) -> String {
        "unknown".into()
    }
}

/// A board as the firmware sees it at run time (the C++ build's #defines).
#[derive(Clone, Debug)]
pub struct BoardProfile {
    /// BOARD_NAME, e.g. "M5StickC Plus2".
    pub name: &'static str,
    /// The PlatformIO environment, which names release images.
    pub env: &'static str,
    pub kind: BoardKind,
    /// M5 devices may read their IMU (next to the battery).
    pub m5: bool,
    /// Screens the settings page may enable (0: no display).
    pub display_pages: u8,
    /// Whether the board has a temperature sensor (the default thermal
    /// limit is off without one).
    pub temperature_sensor: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardKind {
    /// ESP32 software/hardware SHA miner.
    Esp32,
    /// Bitaxe Gamma 601 (BM1370).
    Gamma601,
    /// NerdNOS (BM1397).
    NerdNos,
}

impl BoardProfile {
    pub fn asic(&self) -> bool {
        self.kind != BoardKind::Esp32
    }
    pub fn gamma(&self) -> bool {
        self.kind == BoardKind::Gamma601
    }
    /// What a Stratum V2 SetupConnection names.
    pub fn hardware(&self) -> &'static str {
        match self.kind {
            BoardKind::Gamma601 => "Bitaxe Gamma 601 / BM1370",
            BoardKind::NerdNos => "NerdNOS / BM1397",
            BoardKind::Esp32 => "ESP32",
        }
    }
    /// Version-rolling bits the board needs from the pool.
    pub fn required_rolling_bits(&self) -> u32 {
        match self.kind {
            BoardKind::Gamma601 => 8,
            BoardKind::NerdNos => 2,
            BoardKind::Esp32 => 0,
        }
    }
    pub fn host() -> Self {
        BoardProfile {
            name: "Host",
            env: "host",
            kind: BoardKind::Esp32,
            m5: false,
            display_pages: 0,
            temperature_sensor: false,
        }
    }
}

/// Firmware version, as released.
pub const FW_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn native_user_agent() -> String {
    format!("BDLMinerV2/{FW_VERSION}")
}

/// An in-memory `Storage` for tests and the host build.
#[derive(Default, Debug, Clone)]
pub struct MemoryStorage {
    pub entries: std::collections::BTreeMap<(String, String), Vec<u8>>,
}

impl Storage for MemoryStorage {
    fn get(&mut self, namespace: &str, key: &str) -> Option<Vec<u8>> {
        self.entries.get(&(namespace.into(), key.into())).cloned()
    }
    fn set(&mut self, namespace: &str, key: &str, value: &[u8]) -> bool {
        self.entries.insert((namespace.into(), key.into()), value.to_vec());
        true
    }
    fn remove(&mut self, namespace: &str, key: &str) -> bool {
        self.entries.remove(&(namespace.into(), key.into())).is_some()
    }
    fn clear(&mut self, namespace: &str) -> bool {
        self.entries.retain(|(ns, _), _| ns != namespace);
        true
    }
}
