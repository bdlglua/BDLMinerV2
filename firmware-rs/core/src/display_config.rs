//! Configurable mining screen widgets and screen cycling.

pub const WIDGETS: u32 = 8;

pub fn valid_widget(n: u32) -> bool {
    (1..=WIDGETS).contains(&n)
}

/// Next enabled page after `current` (page 0, mining, is always enabled).
pub fn next_page(current: u32, count: u32, enabled: u8) -> u32 {
    if count == 0 {
        return 0;
    }
    let enabled = enabled | 1;
    (1..=count).map(|n| (current + n) % count).find(|p| *p < 8 && enabled & (1 << p) != 0).unwrap_or(0)
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub hashrate: f64,
    pub best: f64,
    pub temperature: f32,
    pub frequency: f32,
    pub accepted: u32,
    pub uptime: u32,
    pub fan_rpm: i32,
    pub connected: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Snapshot {
            hashrate: 0.0,
            best: 0.0,
            temperature: f32::NAN,
            frequency: f32::NAN,
            accepted: 0,
            uptime: 0,
            fan_rpm: -1,
            connected: false,
        }
    }
}

pub fn label(widget: u32, french: bool) -> &'static str {
    const EN: [&str; 8] = ["HASHRATE", "SHARES", "TEMP", "BEST DIFF", "UPTIME", "POOL", "FREQUENCY", "FAN"];
    const FR: [&str; 8] = ["HASHRATE", "SHARES", "TEMP", "RECORD", "DUREE", "POOL", "FREQUENCE", "VENTILO"];
    let i = if valid_widget(widget) { widget as usize - 1 } else { 0 };
    if french {
        FR[i]
    } else {
        EN[i]
    }
}

pub fn scaled(mut n: f64, suffix: &str) -> String {
    if !n.is_finite() || n < 0.0 {
        return "--".into();
    }
    const UNITS: &[u8] = b" kMGTPE";
    let mut unit = 0;
    while n >= 1000.0 && unit < 6 {
        n /= 1000.0;
        unit += 1;
    }
    format!("{n:.2} {}{suffix}", UNITS[unit] as char)
}

pub fn value(widget: u32, s: &Snapshot, french: bool) -> String {
    match widget {
        1 => scaled(s.hashrate, "H/s"),
        2 => s.accepted.to_string(),
        3 if s.temperature.is_finite() => format!("{:.1} C", s.temperature),
        4 => scaled(s.best, ""),
        5 => format!("{}h {:02}m", s.uptime / 3600, s.uptime / 60 % 60),
        6 => match (s.connected, french) {
            (true, false) => "ONLINE",
            (true, true) => "EN LIGNE",
            (false, false) => "OFFLINE",
            (false, true) => "HORS LIGNE",
        }
        .into(),
        7 if s.frequency.is_finite() => format!("{:.0} MHz", s.frequency),
        8 if s.fan_rpm >= 0 => format!("{} RPM", s.fan_rpm),
        _ => "--".into(),
    }
}
