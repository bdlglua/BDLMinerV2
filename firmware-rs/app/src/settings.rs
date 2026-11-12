//! Persistent user settings, the table that describes every setting to the
//! web page (also the export format), validation of web/backup changes and
//! the two-record flash store.
//!
//! The stored records ("cfgA"/"cfgB" in namespace "bdlminer": header, then
//! the JSON object) are byte-compatible with the C++ firmware, so an update
//! from it keeps the configuration.

use serde_json::{Map, Value};

use bdlminer_core::coin::{self, Coin};
use bdlminer_core::pool_agent::{user_agent, NERD_MINER};
use bdlminer_core::pool_identity::{pool_identity, trim_identity};
use bdlminer_core::{bm1370, display_config, remote_data, sv2_key};

use crate::platform::{native_user_agent, BoardProfile, Storage};

pub const NAMESPACE: &str = "bdlminer";
pub const DEFAULT_POOL_HOST: &str = "public-pool.io";
pub const DEFAULT_POOL_PASSWORD: &str = "x";
pub const DEFAULT_BACKUP_PORT: u16 = 3333;
pub const PORTAL_AP_PASSWORD: &str = "MineYourCoins";
pub const PORTAL_TIMEOUT_S: u32 = 180;
/// POSIX TZ string (Central European Time with daylight saving).
pub const DEFAULT_TIMEZONE: &str = "CET-1CEST,M3.5.0,M10.5.0/3";
pub const DEFAULT_CURRENCY: &str = "USD";
pub const DEFAULT_NTFY_SERVER: &str = "ntfy.sh";
pub const GITHUB_REPO: &str = "bdlglua/BDLMinerV2";
/// nightFrom / nightTo / summary: off.
pub const HOUR_OFF: u8 = 255;
/// Default thermal limit of boards that read the chip's own sensor.
pub const CHIP_SENSOR_MAX_TEMP: u8 = 75;
pub const CURRENCIES: [&str; 7] = ["USD", "EUR", "GBP", "CAD", "CHF", "AUD", "JPY"];
/// What the pages see instead of a set secret.
pub const SECRET_MASK: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    // Pool
    pub mining_coin: u8,
    pub pool_host: String,
    pub pool_port: u16,
    pub backup_host: String,
    pub backup_port: u16,
    pub rotate_hours: u8,
    pub btc_address: String,
    pub pool_password: String,
    pub suggested_difficulty: f64,
    pub user_agent_auto: bool,
    pub user_agent: String,
    pub pool_protocol: u8,
    pub backup_protocol: u8,
    pub update_channel: u8,
    pub pool_authority: String,
    pub backup_authority: String,
    // Display and sound
    pub brightness: u8,
    pub rotation: u8,
    pub auto_rotate: bool,
    pub theme: u8,
    pub language: u8,
    pub cycle_seconds: u8,
    pub custom_screen: bool,
    pub screen_widgets: [u8; 4],
    pub screen_pages: u8,
    pub night_from: u8,
    pub night_to: u8,
    pub beep: bool,
    pub timezone: String,
    pub currency: String,
    // Notifications
    pub ntfy_topic: String,
    pub ntfy_server: String,
    pub notify32: bool,
    pub webhook_url: String,
    pub alert_percent: u8,
    pub alert_minutes: u8,
    pub summary_hour: u8,
    // Gamma 601
    pub asic_frequency: u16,
    pub asic_voltage: u16,
    pub fan_auto: bool,
    pub fan_percent: u8,
    // Safety
    pub max_temp: u8,
    pub mining_power: u8,
    pub reboot_hours: u8,
    pub battery_saver: bool,
    // Network
    pub wifi2_ssid: String,
    pub wifi2_pass: String,
    pub wifi3_ssid: String,
    pub wifi3_pass: String,
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_user: String,
    pub mqtt_pass: String,
    pub peers: String,
    pub web_password: String,
    /// 0 off, 1 check and offer, 2 install by itself.
    pub github_update: u8,
    pub usage_count: bool,
    pub share_hashrate: bool,
    pub share_pool: bool,
    pub remote_enabled: bool,
    pub remote_key: String,
    pub remote_name: String,
}

impl Settings {
    pub fn defaults(board: &BoardProfile) -> Self {
        let asic = board.asic();
        Settings {
            mining_coin: 0,
            pool_host: DEFAULT_POOL_HOST.into(),
            pool_port: if asic { 3333 } else { 21496 },
            backup_host: String::new(),
            backup_port: DEFAULT_BACKUP_PORT,
            rotate_hours: 0,
            btc_address: String::new(),
            pool_password: DEFAULT_POOL_PASSWORD.into(),
            suggested_difficulty: default_difficulty(board),
            user_agent_auto: false,
            user_agent: native_user_agent(),
            pool_protocol: 0,
            backup_protocol: 0,
            update_channel: 0,
            pool_authority: String::new(),
            backup_authority: String::new(),
            brightness: 80,
            rotation: 1,
            auto_rotate: false,
            theme: 0,
            language: 0,
            cycle_seconds: 0,
            custom_screen: false,
            screen_widgets: [1, 2, 4, 3],
            screen_pages: 255,
            night_from: HOUR_OFF,
            night_to: HOUR_OFF,
            beep: true,
            timezone: DEFAULT_TIMEZONE.into(),
            currency: DEFAULT_CURRENCY.into(),
            ntfy_topic: String::new(),
            ntfy_server: DEFAULT_NTFY_SERVER.into(),
            notify32: false,
            webhook_url: String::new(),
            alert_percent: 50,
            alert_minutes: 15,
            summary_hour: 20,
            asic_frequency: bm1370::STOCK_MHZ,
            asic_voltage: bm1370::STOCK_MV,
            fan_auto: false,
            fan_percent: 100,
            max_temp: match board.kind {
                _ if !board.temperature_sensor => 0,
                crate::platform::BoardKind::NerdNos => 75,
                _ if asic || board.m5 => 65,
                _ => CHIP_SENSOR_MAX_TEMP,
            },
            mining_power: 100,
            reboot_hours: 0,
            battery_saver: false,
            wifi2_ssid: String::new(),
            wifi2_pass: String::new(),
            wifi3_ssid: String::new(),
            wifi3_pass: String::new(),
            mqtt_host: String::new(),
            mqtt_port: 1883,
            mqtt_user: String::new(),
            mqtt_pass: String::new(),
            peers: String::new(),
            web_password: String::new(),
            github_update: if asic { 1 } else { 2 },
            usage_count: true,
            share_hashrate: true,
            share_pool: true,
            remote_enabled: false,
            remote_key: String::new(),
            remote_name: String::new(),
        }
    }

    pub fn coin(&self) -> Coin {
        Coin::from_id(self.mining_coin as u32).unwrap_or_default()
    }

    pub fn normalize_agent(&mut self) {
        self.user_agent = user_agent::normalize(&self.user_agent, &native_user_agent()).to_owned();
        self.user_agent_auto = false;
    }
}

pub fn default_difficulty(board: &BoardProfile) -> f64 {
    if board.asic() {
        1024.0
    } else {
        0.0002
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Secret,
    /// Unsigned number of 8 or 16 bits.
    Number(u8),
    Decimal,
    Bool,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Secret => "secret",
            Kind::Number(_) => "number",
            Kind::Decimal => "decimal",
            Kind::Bool => "bool",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SettingInfo {
    /// NVS key and JSON name.
    pub key: &'static str,
    pub kind: Kind,
    pub group: &'static str,
    pub label: &'static str,
    gamma_only: bool,
}

const fn s(key: &'static str, kind: Kind, group: &'static str, label: &'static str) -> SettingInfo {
    SettingInfo { key, kind, group, label, gamma_only: false }
}
const fn g(key: &'static str, kind: Kind, group: &'static str, label: &'static str) -> SettingInfo {
    SettingInfo { key, kind, group, label, gamma_only: true }
}
const N8: Kind = Kind::Number(8);
const N16: Kind = Kind::Number(16);

/// Every setting, in the order the C++ table lists them.
const TABLE: &[SettingInfo] = &[
    s("coin", N8, "Pool", "Cryptocurrency"),
    s("pool", Kind::Text, "Pool", "Pool host"),
    s("port", N16, "Pool", "Pool port"),
    s("addr", Kind::Text, "Pool", "Payout address or pool account (optionally .worker)"),
    s("pass", Kind::Text, "Pool", "Pool password"),
    s("diff", Kind::Decimal, "Pool", "Suggested share difficulty"),
    s("proto", N8, "Pool", "Main pool protocol"),
    s("sv2key", Kind::Text, "Pool", "Main pool Stratum V2 authority key"),
    s("uaauto", Kind::Bool, "Pool", "User agent mode"),
    s("ua", Kind::Text, "Pool", "User agent"),
    s("pool2", Kind::Text, "Pool", "Backup pool host (empty = none)"),
    s("port2", N16, "Pool", "Backup pool port"),
    s("proto2", N8, "Pool", "Backup pool protocol"),
    s("sv2key2", Kind::Text, "Pool", "Backup pool Stratum V2 authority key"),
    s("rotateh", N8, "Pool", "Alternate main / backup pool every N hours (0 = only on failure)"),
    s("bright", N8, "Display", "Brightness (%)"),
    s("rot", N8, "Display", "Rotation (0-3)"),
    s("autorot", Kind::Bool, "Display", "Rotate with the accelerometer (M5 devices)"),
    s("lang", N8, "Display", "Language: 0 English, 1 French"),
    s("theme", N8, "Display", "Colour theme (0 orange, 1 green, 2 blue, 3 purple, 4 pink)"),
    s("cycle", N8, "Display", "Next screen every N seconds (0 = off)"),
    s("customscreen", Kind::Bool, "Display", "Custom mining screen"),
    s("screen0", N8, "Display", "First screen metric"),
    s("screen1", N8, "Display", "Second screen metric"),
    s("screen2", N8, "Display", "Third screen metric"),
    s("screen3", N8, "Display", "Fourth screen metric"),
    s("pages", N8, "Display", "Enabled screens"),
    s("nightfrom", N8, "Display", "Night mode from hour (0-23, 255 = off)"),
    s("nightto", N8, "Display", "Night mode until hour (0-23)"),
    s("beep", Kind::Bool, "Display", "Beep on a 32-bit share"),
    s("tz", Kind::Text, "Display", "Timezone (POSIX TZ, e.g. CET-1CEST,M3.5.0,M10.5.0/3 or UTC0)"),
    s("fiat", Kind::Text, "Display", "Price currency (USD, EUR, GBP, CAD, CHF, AUD, JPY)"),
    s("ntfy", Kind::Text, "Notifications", "ntfy topic (empty = off)"),
    s("ntfysrv", Kind::Text, "Notifications", "ntfy server"),
    s("notify32", Kind::Bool, "Notifications", "Also notify every 32-bit share"),
    s(
        "hook",
        Kind::Secret,
        "Notifications",
        "Webhook URL: JSON POST for Discord, Telegram... (empty = off, holds its token)",
    ),
    s("alertpct", N8, "Notifications", "Alert when hashrate stays below this % of normal (0 = off)"),
    s("alertmin", N8, "Notifications", "... for this many minutes"),
    s("summary", N8, "Notifications", "Daily summary at this hour (0-23, 255 = off)"),
    s("maxtemp", N8, "Safety", "Pause mining above this temperature in C (0 = off)"),
    g("asicmhz", N16, "Safety", "Gamma ASIC frequency (MHz)"),
    g("asicmv", N16, "Safety", "Gamma ASIC core voltage (mV)"),
    s("power", N8, "Safety", "Mining power on USB and battery (0-100%, 0 = paused)"),
    g("fanauto", Kind::Bool, "Safety", "Fan control"),
    g("fanpct", N8, "Safety", "Manual fan speed (%)"),
    s("rebooth", N8, "Safety", "Restart after N hours without an accepted share (0 = off)"),
    s("saver", Kind::Bool, "Safety", "Battery saver: dim and mine at half speed on battery"),
    s("wifi2", Kind::Text, "Network", "Fallback WiFi 1 name"),
    s("wpass2", Kind::Secret, "Network", "Fallback WiFi 1 password"),
    s("wifi3", Kind::Text, "Network", "Fallback WiFi 2 name"),
    s("wpass3", Kind::Secret, "Network", "Fallback WiFi 2 password"),
    s("mqtt", Kind::Text, "Network", "MQTT broker for Home Assistant (empty = off)"),
    s("mqttport", N16, "Network", "MQTT port"),
    s("mqttuser", Kind::Text, "Network", "MQTT user"),
    s("mqttpass", Kind::Secret, "Network", "MQTT password"),
    s("peers", Kind::Text, "Network", "Other miners on the dashboard (comma separated IPs or names)"),
    s(
        "webpass",
        Kind::Secret,
        "Network",
        "Web password (optional: protects settings and firmware update; empty = open)",
    ),
    s("update", N8, "Network", "Updates from GitHub: 0 off, 1 check and offer, 2 install automatically"),
    s("channel", N8, "Network", "Update channel"),
    s(
        "count",
        Kind::Bool,
        "Network",
        "Anonymous usage count: once a day, download a 2-byte file of this version's release (nothing is sent)",
    ),
    s(
        "sharehr",
        Kind::Bool,
        "Network",
        "Share my hashrate anonymously: every 5 minutes, send the version, the board, the cryptocurrency and the hashrate",
    ),
    s("sharepool", Kind::Bool, "Network", "Share my pool anonymously: optional public pool group, only with hashrate sharing"),
    s("remote", Kind::Bool, "Network", "Private remote monitoring (optional, by payout address)"),
    s(
        "remotekey",
        Kind::Secret,
        "Network",
        "Optional private remote access key (64 lowercase hexadecimal characters)",
    ),
    s("rname", Kind::Text, "Network", "Name in my remote dashboard"),
];

/// The settings of this board, in table order.
pub fn table(board: &BoardProfile) -> impl Iterator<Item = &'static SettingInfo> + '_ {
    TABLE.iter().filter(move |s| !s.gamma_only || board.gamma())
}

/// One setting's storage, for generic code.
pub enum Field<'a> {
    Text(&'a mut String),
    U8(&'a mut u8),
    U16(&'a mut u16),
    F64(&'a mut f64),
    Bool(&'a mut bool),
}

impl Settings {
    pub fn field(&mut self, key: &str) -> Option<Field<'_>> {
        use Field::*;
        Some(match key {
            "coin" => U8(&mut self.mining_coin),
            "pool" => Text(&mut self.pool_host),
            "port" => U16(&mut self.pool_port),
            "addr" => Text(&mut self.btc_address),
            "pass" => Text(&mut self.pool_password),
            "diff" => F64(&mut self.suggested_difficulty),
            "proto" => U8(&mut self.pool_protocol),
            "sv2key" => Text(&mut self.pool_authority),
            "uaauto" => Bool(&mut self.user_agent_auto),
            "ua" => Text(&mut self.user_agent),
            "pool2" => Text(&mut self.backup_host),
            "port2" => U16(&mut self.backup_port),
            "proto2" => U8(&mut self.backup_protocol),
            "sv2key2" => Text(&mut self.backup_authority),
            "rotateh" => U8(&mut self.rotate_hours),
            "bright" => U8(&mut self.brightness),
            "rot" => U8(&mut self.rotation),
            "autorot" => Bool(&mut self.auto_rotate),
            "lang" => U8(&mut self.language),
            "theme" => U8(&mut self.theme),
            "cycle" => U8(&mut self.cycle_seconds),
            "customscreen" => Bool(&mut self.custom_screen),
            "screen0" => U8(&mut self.screen_widgets[0]),
            "screen1" => U8(&mut self.screen_widgets[1]),
            "screen2" => U8(&mut self.screen_widgets[2]),
            "screen3" => U8(&mut self.screen_widgets[3]),
            "pages" => U8(&mut self.screen_pages),
            "nightfrom" => U8(&mut self.night_from),
            "nightto" => U8(&mut self.night_to),
            "beep" => Bool(&mut self.beep),
            "tz" => Text(&mut self.timezone),
            "fiat" => Text(&mut self.currency),
            "ntfy" => Text(&mut self.ntfy_topic),
            "ntfysrv" => Text(&mut self.ntfy_server),
            "notify32" => Bool(&mut self.notify32),
            "hook" => Text(&mut self.webhook_url),
            "alertpct" => U8(&mut self.alert_percent),
            "alertmin" => U8(&mut self.alert_minutes),
            "summary" => U8(&mut self.summary_hour),
            "maxtemp" => U8(&mut self.max_temp),
            "asicmhz" => U16(&mut self.asic_frequency),
            "asicmv" => U16(&mut self.asic_voltage),
            "power" => U8(&mut self.mining_power),
            "fanauto" => Bool(&mut self.fan_auto),
            "fanpct" => U8(&mut self.fan_percent),
            "rebooth" => U8(&mut self.reboot_hours),
            "saver" => Bool(&mut self.battery_saver),
            "wifi2" => Text(&mut self.wifi2_ssid),
            "wpass2" => Text(&mut self.wifi2_pass),
            "wifi3" => Text(&mut self.wifi3_ssid),
            "wpass3" => Text(&mut self.wifi3_pass),
            "mqtt" => Text(&mut self.mqtt_host),
            "mqttport" => U16(&mut self.mqtt_port),
            "mqttuser" => Text(&mut self.mqtt_user),
            "mqttpass" => Text(&mut self.mqtt_pass),
            "peers" => Text(&mut self.peers),
            "webpass" => Text(&mut self.web_password),
            "update" => U8(&mut self.github_update),
            "channel" => U8(&mut self.update_channel),
            "count" => Bool(&mut self.usage_count),
            "sharehr" => Bool(&mut self.share_hashrate),
            "sharepool" => Bool(&mut self.share_pool),
            "remote" => Bool(&mut self.remote_enabled),
            "remotekey" => Text(&mut self.remote_key),
            "rname" => Text(&mut self.remote_name),
            _ => return None,
        })
    }

    /// A setting's value as JSON.
    pub fn value(&self, key: &str) -> Value {
        // `field` needs `&mut`: read through a clone of the one value.
        let mut copy = self.clone();
        match copy.field(key) {
            Some(Field::Text(s)) => Value::String(std::mem::take(s)),
            Some(Field::U8(v)) => (*v).into(),
            Some(Field::U16(v)) => (*v).into(),
            Some(Field::F64(v)) => serde_json::Number::from_f64(*v).map_or(Value::Null, Value::Number),
            Some(Field::Bool(v)) => (*v).into(),
            None => Value::Null,
        }
    }
}

fn trim(s: &mut String) {
    let t = s.trim_matches(|c: char| c.is_ascii_whitespace() || c == '\0');
    if t.len() != s.len() {
        *s = t.to_owned();
    }
}

/// "stratum+tcp://pool.example.com:3333/" -> host "pool.example.com" and,
/// with `port`, port 3333 (without it a ":port" stays in the host).
pub fn clean_host(host: &mut String, port: Option<&mut u16>) {
    trim(host);
    if let Some(i) = host.find("://") {
        host.drain(..i + 3);
    }
    if let Some(slash) = host.find('/') {
        host.truncate(slash);
    }
    if let (Some(port), Some(colon)) = (port, host.rfind(':')) {
        if colon > 0 && host.find(':') == Some(colon) {
            // not an IPv6 address
            if let Ok(p) = host[colon + 1..].trim().parse::<u32>() {
                if (1..65536).contains(&p) {
                    *port = p as u16;
                }
            } else if let Some(p) = leading_number(&host[colon + 1..]) {
                if (1..65536).contains(&p) {
                    *port = p as u16;
                }
            }
            host.truncate(colon);
        }
    }
}

/// Arduino's String::toInt(): leading digits, 0 when none.
fn leading_number(s: &str) -> Option<u32> {
    let digits: String = s.trim_start().chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn pool_url(host: &str, protocol: &mut u8, authority: &mut String) {
    let id = pool_identity(host, *protocol, authority);
    *protocol = id.protocol;
    *authority = id.authority;
}

fn normalize_currency(c: &str) -> String {
    let c = c.trim().to_ascii_uppercase();
    if CURRENCIES.contains(&c.as_str()) {
        c
    } else {
        DEFAULT_CURRENCY.into()
    }
}

/// Trims texts and brings out-of-range values back to defaults.
pub fn sanitize(v: &mut Settings, board: &BoardProfile) {
    if !remote_data::can_report(&v.remote_key, &v.btc_address) {
        v.remote_enabled = false;
    }
    if v.remote_name.len() > 48 {
        let mut end = 48;
        while !v.remote_name.is_char_boundary(end) {
            end -= 1;
        }
        v.remote_name.truncate(end);
    }
    if Coin::from_id(v.mining_coin as u32).is_none() {
        v.mining_coin = 0;
    }
    for info in table(board).filter(|s| s.kind == Kind::Text) {
        if let Some(Field::Text(s)) = v.field(info.key) {
            trim(s);
        }
    }
    let (host, mut protocol, mut authority) = (v.pool_host.clone(), v.pool_protocol, v.pool_authority.clone());
    pool_url(&host, &mut protocol, &mut authority);
    (v.pool_protocol, v.pool_authority) = (protocol, authority);
    let (host, mut protocol, mut authority) = (v.backup_host.clone(), v.backup_protocol, v.backup_authority.clone());
    pool_url(&host, &mut protocol, &mut authority);
    (v.backup_protocol, v.backup_authority) = (protocol, authority);
    clean_host(&mut v.pool_host, Some(&mut v.pool_port));
    clean_host(&mut v.backup_host, Some(&mut v.backup_port));
    clean_host(&mut v.ntfy_server, None);
    clean_host(&mut v.mqtt_host, Some(&mut v.mqtt_port));
    let defaults = Settings::defaults(board);
    if v.pool_port == 0 {
        v.pool_port = defaults.pool_port;
    }
    if v.backup_port == 0 {
        v.backup_port = DEFAULT_BACKUP_PORT;
    }
    if !(v.suggested_difficulty.is_finite() && v.suggested_difficulty > 0.0) {
        v.suggested_difficulty = default_difficulty(board);
    }
    v.normalize_agent();
    if v.pool_protocol > 1 {
        v.pool_protocol = 0;
    }
    if v.backup_protocol > 1 {
        v.backup_protocol = 0;
    }
    if v.timezone.is_empty() {
        v.timezone = DEFAULT_TIMEZONE.into();
    }
    if v.ntfy_server.is_empty() {
        v.ntfy_server = DEFAULT_NTFY_SERVER.into();
    }
    v.currency = normalize_currency(&v.currency);
    v.brightness = v.brightness.min(100);
    v.rotation &= 3;
    if v.language > 1 {
        v.language = 0;
    }
    if v.github_update > 2 {
        v.github_update = 2;
    }
    if v.update_channel > 1 {
        v.update_channel = 0;
    }
    for (i, d) in [1u8, 2, 4, 3].into_iter().enumerate() {
        if !display_config::valid_widget(v.screen_widgets[i] as u32) {
            v.screen_widgets[i] = d;
        }
    }
    v.screen_pages |= 1;
    if v.max_temp != 0 && v.max_temp < 30 {
        v.max_temp = 30; // resumes 5 C below
    }
    if board.gamma() {
        if !bm1370::valid_profile(v.asic_frequency as u32) {
            v.asic_frequency = bm1370::STOCK_MHZ;
        }
        if !bm1370::valid_voltage(v.asic_voltage as u32) {
            v.asic_voltage = bm1370::STOCK_MV;
        }
        v.max_temp = v.max_temp.min(bm1370::MAX_USER_TEMP_C);
        v.fan_percent = v.fan_percent.clamp(30, 100);
    }
    v.mining_power = v.mining_power.min(100);
    v.alert_percent = v.alert_percent.min(100);
    if v.alert_minutes == 0 {
        v.alert_minutes = 15;
    }
    if v.summary_hour > 23 && v.summary_hour != HOUR_OFF {
        v.summary_hour = 20;
    }
    if v.night_from > 23 {
        v.night_from = HOUR_OFF;
    }
    if v.night_to > 23 {
        v.night_to = HOUR_OFF;
    }
}

// ---------------------------------------------------------------------------
// Two complete records: an unsuccessful write never replaces the last valid
// configuration.

pub const MAGIC: u32 = 0x3247_4643;
/// New-profile records must also be rejected by firmware predating `coin`.
pub const NEW_COIN_MAGIC: u32 = 0x3347_4643;
pub const MAX_PAYLOAD: usize = 8192;
const HEADER: usize = 16;

/// CRC-32 (zlib; ESP-IDF's `esp_rom_crc32_le(0, ...)`).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

pub fn encode(v: &Settings, board: &BoardProfile) -> Option<String> {
    Coin::from_id(v.mining_coin as u32)?;
    let mut doc = Map::new();
    for info in table(board) {
        doc.insert(info.key.into(), v.value(info.key));
    }
    let text = Value::Object(doc).to_string();
    (!text.is_empty() && text.len() <= MAX_PAYLOAD).then_some(text)
}

/// Stores a JSON value into a setting, checking its type and range.
fn store_value(v: &mut Settings, info: &SettingInfo, value: &Value) -> bool {
    match (v.field(info.key), info.kind) {
        (Some(Field::Text(s)), _) => match value.as_str() {
            Some(t) => {
                *s = t.to_owned();
                true
            }
            None => false,
        },
        (Some(Field::U8(n)), _) => match value.as_u64().filter(|&x| x <= 255) {
            Some(x) => {
                *n = x as u8;
                true
            }
            None => false,
        },
        (Some(Field::U16(n)), _) => match value.as_u64().filter(|&x| x <= 65535) {
            Some(x) => {
                *n = x as u16;
                true
            }
            None => false,
        },
        (Some(Field::F64(d)), _) => match value.as_f64() {
            Some(x) => {
                *d = x;
                true
            }
            None => false,
        },
        (Some(Field::Bool(b)), _) => match value.as_bool() {
            Some(x) => {
                *b = x;
                true
            }
            None => false,
        },
        (None, _) => false,
    }
}

pub fn decode(payload: &[u8], v: &mut Settings, board: &BoardProfile) -> bool {
    let Ok(Value::Object(doc)) = serde_json::from_slice::<Value>(payload) else { return false };
    for info in table(board) {
        // Settings added by a newer firmware keep their defaults.
        let Some(value) = doc.get(info.key) else { continue };
        if !store_value(v, info, value) {
            return false;
        }
    }
    if Coin::from_id(v.mining_coin as u32).is_none() {
        return false;
    }
    v.normalize_agent();
    true
}

fn read_slot(store: &mut dyn Storage, key: &str, v: &mut Settings, board: &BoardProfile) -> Option<u32> {
    let bytes = store.get(NAMESPACE, key)?;
    if bytes.len() < HEADER || bytes.len() > HEADER + MAX_PAYLOAD {
        return None;
    }
    let word = |i: usize| u32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap());
    let (magic, sequence, length, crc) = (word(0), word(1), word(2), word(3));
    if (magic != MAGIC && magic != NEW_COIN_MAGIC) || length as usize != bytes.len() - HEADER {
        return None;
    }
    let mut check = bytes.clone();
    check[12..16].fill(0);
    if crc32(&check) != crc {
        return None;
    }
    let mut candidate = v.clone();
    if !decode(&bytes[HEADER..], &mut candidate, board) || (magic == NEW_COIN_MAGIC) != (candidate.mining_coin > 1) {
        return None;
    }
    *v = candidate;
    Some(sequence)
}

/// The newest valid record, and its sequence number.
pub fn load(store: &mut dyn Storage, v: &mut Settings, board: &BoardProfile) -> Option<u32> {
    let (mut a, mut b) = (v.clone(), v.clone());
    let sa = read_slot(store, "cfgA", &mut a, board);
    let sb = read_slot(store, "cfgB", &mut b, board);
    let pick_a = match (sa, sb) {
        (None, None) => return None,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (Some(x), Some(y)) => (x.wrapping_sub(y) as i32) > 0,
    };
    *v = if pick_a { a } else { b };
    if pick_a {
        sa
    } else {
        sb
    }
}

/// Writes the next record and reads it back; returns its sequence number.
pub fn save(store: &mut dyn Storage, v: &Settings, sequence: u32, board: &BoardProfile) -> Option<u32> {
    let payload = encode(v, board)?;
    let next = sequence.wrapping_add(1);
    let mut bytes = Vec::with_capacity(HEADER + payload.len());
    let magic = if v.mining_coin > 1 { NEW_COIN_MAGIC } else { MAGIC };
    for w in [magic, next, payload.len() as u32, 0] {
        bytes.extend_from_slice(&w.to_le_bytes());
    }
    bytes.extend_from_slice(payload.as_bytes());
    let crc = crc32(&bytes);
    bytes[12..16].copy_from_slice(&crc.to_le_bytes());
    let key = if next & 1 != 0 { "cfgB" } else { "cfgA" };
    if !store.set(NAMESPACE, key, &bytes) {
        return None;
    }
    let mut verified = v.clone();
    (read_slot(store, key, &mut verified, board) == Some(next)).then_some(next)
}

// ---------------------------------------------------------------------------
// Web and backup changes: validate the whole request, then apply it to a
// staged copy.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub minimum: i64,
    pub maximum: i64,
}

pub fn limits(info: &SettingInfo) -> Limits {
    let mut l = Limits { minimum: 0, maximum: if info.kind == Kind::Number(8) { 255 } else { 65535 } };
    match info.key {
        "port" | "port2" | "mqttport" => l.minimum = 1,
        "bright" | "power" | "alertpct" => l.maximum = 100,
        "alertmin" => l = Limits { minimum: 1, maximum: 240 },
        "rot" => l.maximum = 3,
        "lang" => l.maximum = 1,
        "coin" => l.maximum = coin::COUNT as i64 - 1,
        "theme" => l.maximum = 4,
        "proto" | "proto2" => l.maximum = 1,
        "update" => l.maximum = 2,
        "channel" => l.maximum = 1,
        "screen0" | "screen1" | "screen2" | "screen3" => {
            l = Limits { minimum: 1, maximum: display_config::WIDGETS as i64 }
        }
        "pages" => l.minimum = 1,
        "asicmhz" => l = Limits { minimum: bm1370::STOCK_MHZ as i64, maximum: bm1370::MAX_MHZ as i64 },
        "asicmv" => l = Limits { minimum: bm1370::MIN_MV as i64, maximum: bm1370::MAX_MV as i64 },
        "fanpct" => l = Limits { minimum: 30, maximum: 100 },
        _ => {}
    }
    l
}

/// Limits of a setting on this board. On a Gamma the ASIC diode's hard stop
/// latches until a restart: the user limit must pause before it.
pub fn limits_for(info: &SettingInfo, board: &BoardProfile) -> Limits {
    let mut l = limits(info);
    if info.key == "maxtemp" && board.gamma() {
        l.maximum = bm1370::MAX_USER_TEMP_C as i64;
    }
    l
}

fn text_of<'a>(values: &'a Map<String, Value>, key: &str, current: &'a str) -> &'a str {
    values.get(key).and_then(Value::as_str).unwrap_or(current)
}

/// Old complete backups predate coin selection and describe a BTC setup.
pub fn migrate_backup_coin(values: &mut Map<String, Value>, full: bool) {
    if !values.contains_key("coin") && (full || (values.contains_key("pool") && values.contains_key("addr"))) {
        values.insert("coin".into(), 0.into());
    }
}

pub fn migrate_backup_agent(values: &mut Map<String, Value>) {
    if let Some(ua) = values.get("ua").and_then(Value::as_str) {
        let normalized = user_agent::normalize(ua, &native_user_agent()).to_owned();
        values.insert("ua".into(), normalized.into());
    }
    if values.get("uaauto").is_some_and(Value::is_boolean) {
        values.insert("uaauto".into(), false.into());
    }
}

/// Drops the secrets sent back masked: they stay as they are.
pub fn keep_secrets(values: &mut Map<String, Value>, board: &BoardProfile) {
    for info in table(board).filter(|s| s.kind == Kind::Secret) {
        if values.get(info.key).and_then(Value::as_str) == Some(SECRET_MASK) {
            values.remove(info.key);
        }
    }
}

/// Validates a complete request before staging or persisting any field.
pub fn error(values: &Map<String, Value>, current: &Settings, board: &BoardProfile) -> Option<&'static str> {
    for info in table(board) {
        let Some(v) = values.get(info.key) else { continue };
        match info.kind {
            Kind::Text | Kind::Secret => {
                let Some(text) = v.as_str() else { return Some("Invalid text setting") };
                match info.key {
                    "ua" if text.len() > 64 => return Some("User agent must be at most 64 characters"),
                    "remotekey" if !text.is_empty() && !remote_data::valid_key(text) => {
                        return Some("The remote access key must contain 64 lowercase hexadecimal characters")
                    }
                    "rname" if text.len() > 48 => return Some("The remote miner name must be at most 48 bytes"),
                    "hook" => {
                        let url = text.trim_start();
                        if !url.is_empty() && !url.starts_with("http://") && !url.starts_with("https://") {
                            return Some("The webhook URL must start with http:// or https://");
                        }
                    }
                    "pool" | "addr" if text.trim().is_empty() => {
                        return Some("Pool host and payout address cannot be empty")
                    }
                    _ => {}
                }
            }
            Kind::Number(_) => {
                let l = limits_for(info, board);
                let Some(n) = v.as_i64().filter(|n| (l.minimum..=l.maximum).contains(n)) else {
                    return Some("Setting outside its allowed range");
                };
                match info.key {
                    "pages" if n & 1 == 0 => return Some("The mining screen must remain enabled"),
                    "asicmhz" if !bm1370::valid_profile(n as u32) => {
                        return Some("Gamma frequency must be 525-800 MHz in 25 MHz steps, or 690 MHz")
                    }
                    "asicmv" if !bm1370::valid_voltage(n as u32) => {
                        return Some("Gamma core voltage must be 1000, 1060, 1100, 1150, 1200, 1250 or 1300 mV")
                    }
                    "maxtemp" if n != 0 && n < 30 => return Some("Temperature must be 0 or at least 30 C"),
                    "nightfrom" | "nightto" if n > 23 && n != 255 => return Some("Night hours must be 0-23 or 255"),
                    "summary" if n > 23 && n != 255 => return Some("The summary hour must be 0-23 or 255"),
                    _ => {}
                }
            }
            Kind::Decimal => {
                if !v.as_f64().is_some_and(|d| d.is_finite() && d > 0.0) {
                    return Some("Difficulty must be a finite positive number");
                }
            }
            Kind::Bool => {
                if !v.is_boolean() {
                    return Some("Invalid checkbox setting");
                }
            }
        }
    }
    let remote = values.get("remote").and_then(Value::as_bool).unwrap_or(current.remote_enabled);
    let user = text_of(values, "addr", &current.btc_address);
    if remote && !remote_data::can_report(text_of(values, "remotekey", &current.remote_key), user) {
        return Some("Set a payout address before enabling remote monitoring");
    }
    let selected = values
        .get("coin")
        .and_then(Value::as_u64)
        .and_then(|c| Coin::from_id(c as u32))
        .unwrap_or_else(|| current.coin());
    // Stratum users are plain text: a phone's typographic apostrophe or an
    // accent is a mistyped address, never a pool account.
    if !trim_identity(user).chars().all(|c| c.is_ascii_graphic()) {
        return Some("The payout address can only hold plain letters, digits and symbols");
    }
    if !selected.address_compatible(&trim_identity(user)) {
        return Some("Use an address for the selected cryptocurrency");
    }
    if !user_agent::valid(text_of(values, "ua", &current.user_agent), &native_user_agent()) {
        return Some("Choose one of the available user agents");
    }
    for main in [true, false] {
        let (proto_key, key_key, host_key) =
            if main { ("proto", "sv2key", "pool") } else { ("proto2", "sv2key2", "pool2") };
        let protocol = values.get(proto_key).and_then(Value::as_u64).map(|p| p as u8).unwrap_or(if main {
            current.pool_protocol
        } else {
            current.backup_protocol
        });
        let key = text_of(values, key_key, if main { &current.pool_authority } else { &current.backup_authority });
        let host = text_of(values, host_key, if main { &current.pool_host } else { &current.backup_host });
        let identity = pool_identity(host, protocol, key);
        if main || !trim_identity(host).is_empty() {
            if let Some(e) = selected.pool_error(host, identity.protocol) {
                return Some(e);
            }
        }
        if identity.protocol == 1 && sv2_key::authority_key(&identity.authority).is_none() {
            return Some("A valid Stratum V2 authority key is required");
        }
    }
    None
}

/// Applies the known keys of `values` (already validated) to `pending`;
/// returns how many were present.
pub fn apply(pending: &mut Settings, values: &Map<String, Value>, board: &BoardProfile) -> usize {
    let mut changed = 0;
    for info in table(board) {
        let Some(v) = values.get(info.key).filter(|v| !v.is_null()) else { continue };
        match pending.field(info.key) {
            Some(Field::Text(s)) => *s = v.as_str().unwrap_or_default().to_owned(),
            Some(Field::U8(n)) => *n = v.as_i64().unwrap_or(0).clamp(0, 255) as u8,
            Some(Field::U16(n)) => *n = v.as_i64().unwrap_or(0).clamp(0, 65535) as u16,
            Some(Field::F64(d)) => *d = v.as_f64().unwrap_or(0.0),
            Some(Field::Bool(b)) => *b = v.as_bool().unwrap_or(false),
            None => continue,
        }
        changed += 1;
    }
    pending.normalize_agent();
    changed
}

/// Settings applied while the miner runs; anything else needs a restart.
pub fn is_live(key: &str) -> bool {
    const LIVE: [&str; 31] = [
        "bright",
        "rot",
        "autorot",
        "lang",
        "theme",
        "cycle",
        "nightfrom",
        "nightto",
        "beep",
        "notify32",
        "alertpct",
        "alertmin",
        "summary",
        "maxtemp",
        "power",
        "rebooth",
        "saver",
        "update",
        "count",
        "sharehr",
        "sharepool",
        "fanauto",
        "fanpct",
        "customscreen",
        "screen0",
        "screen1",
        "screen2",
        "screen3",
        "pages",
        "channel",
        "remote",
    ];
    LIVE.contains(&key)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub count: usize,
    pub live: bool,
}

/// How `pending` differs from `current`.
pub fn compare(pending: &Settings, current: &Settings, board: &BoardProfile) -> Change {
    let mut change = Change { count: 0, live: true };
    for info in table(board) {
        if pending.value(info.key) != current.value(info.key) {
            change.count += 1;
            if !is_live(info.key) {
                change.live = false;
            }
        }
    }
    change
}

/// Copies the live settings that changed (numbers and switches only).
pub fn copy_live(live: &mut Settings, pending: &Settings, board: &BoardProfile) {
    for info in table(board).filter(|s| is_live(s.key) && !matches!(s.kind, Kind::Text | Kind::Secret)) {
        let value = pending.value(info.key);
        store_value(live, info, &value);
    }
}

/// Every setting's value; secrets only with `secrets`.
pub fn values(v: &Settings, board: &BoardProfile, secrets: bool) -> Map<String, Value> {
    let mut out = Map::new();
    for info in table(board) {
        let mut value = v.value(info.key);
        if info.kind == Kind::Secret && !secrets && value.as_str().is_some_and(|s| !s.is_empty()) {
            value = SECRET_MASK.into();
        }
        out.insert(info.key.into(), value);
    }
    out
}

/// Whether the saved agent is one of the two choices.
pub fn legacy_manual_agent(agent: &str) -> bool {
    !agent.is_empty() && agent != NERD_MINER && !agent.starts_with("BDLMinerV1/") && !agent.starts_with("BDLMinerV2/")
}
