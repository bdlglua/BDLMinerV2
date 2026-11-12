//! What the physical display shows: the pages of the C++ firmware's
//! `src/ui.cpp` (`buildModel`), as data for its renderer (`src/ui_view.cpp`,
//! which the ESP32 build runs unchanged). Each page has one big value
//! (`metrics[0]`, its unit in `detail`) and up to three readings below it.

use bdlminer_core::coin::Coin;
use bdlminer_core::display_config::{self, Snapshot};
use bdlminer_core::work::format_si;

use crate::miner::MinerStats;
use crate::settings::Settings;

pub const PAGES: u32 = 8;
pub const TEXT: u32 = 0xF1F5FA;
pub const MUTED: u32 = 0x94A3B8;
pub const GREEN: u32 = 0x42D99B;
pub const RED: u32 = 0xFF6577;
pub const AMBER: u32 = 0xFFBC55;
const THEMES: [u32; 5] = [0xF7931A, 0x38D07A, 0x3B9CF6, 0xA66BF5, 0xEC5AA0];
/// The brightness button's steps, in percent.
pub const BRIGHTNESS_STEPS: [u8; 5] = [100, 60, 30, 10, 0];
/// Hash rate graph: one sample every 10 s, the last 40 minutes.
pub const SAMPLE_MS: u32 = 10_000;
pub const SAMPLES: usize = 240;

/// The accent colour of a theme (settings `theme`).
pub fn accent(theme: u8) -> u32 {
    THEMES[theme as usize % THEMES.len()]
}

/// The French text of an English one, for `language` 1 (src/i18n.cpp; plain
/// ASCII: the display fonts have no accented letters).
pub fn tr(en: &'static str, language: u8) -> &'static str {
    if language != 1 {
        return en;
    }
    match en {
        "MINING" => "MINAGE",
        "CLOCK" => "HORLOGE",
        "NETWORK" => "RESEAU",
        "DEVICE" => "APPAREIL",
        "LIVE" => "ACTIF",
        "PAUSED" => "PAUSE",
        "COOLING" => "REFROIDIT",
        "NO SENSOR" => "SANS CAPTEUR",
        "BATTERY" => "BATTERIE",
        "LIMITED" => "LIMITE",
        "LIVE HASHRATE" => "HASHRATE DIRECT",
        "LOCAL TIME" => "HEURE LOCALE",
        "LOCAL IP" => "IP LOCALE",
        "BACKUP POOL" => "POOL DE SECOURS",
        "FEES sat/vB" => "FRAIS sat/vB",
        "AVG H/s" => "MOY H/s",
        "RUN TIME" => "DUREE TOTALE",
        "MINING BLOCK" => "BLOC EN COURS",
        "BEST DIFF" => "MEILL. DIFF",
        "BLOCKS" => "BLOCS",
        "32-BIT" => "32 BITS",
        "TEMP" => "TEMP.",
        "BTC PRICE" => "PRIX BTC",
        "BITCOIN PRICE" => "PRIX DU BITCOIN",
        "NET HASHRATE" => "HASHRATE RESEAU",
        "NEXT RETARGET" => "AJUSTEMENT",
        "BLOCK HEIGHT" => "HAUTEUR DE BLOC",
        "NETWORK DIFF" => "DIFF RESEAU",
        "BEST EVER" => "RECORD",
        "BOARD" => "CARTE",
        "FREE RAM" => "RAM LIBRE",
        "BLOCK" => "BLOC",
        "OFFLINE" => "HORS LIGNE",
        "WiFi setup" => "Config WiFi",
        "BLOCK CANDIDATE" => "BLOC CANDIDAT",
        "Check pool / chain" => "Verifier pool / chaine",
        "Hold A at boot: setup" => "A au demarrage: config",
        "Connecting..." => "Connexion...",
        _ => en,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Metric {
    pub label: String,
    pub value: String,
    pub detail: String,
    pub color: u32,
}

/// One frame for the renderer (ui_view::Model).
#[derive(Clone, Debug, Default)]
pub struct Model {
    pub page: u32,
    pub language: u8,
    pub accent: u32,
    pub status_color: u32,
    pub title: String,
    pub status: String,
    pub battery: String,
    pub footer: String,
    pub metrics: [Metric; 6],
    /// Graph page: (millis, H/s), oldest first.
    pub points: Vec<(u32, f32)>,
    pub graph_revision: u32,
}

/// Everything a page shows, read by the platform once per refresh.
#[derive(Clone, Debug, Default)]
pub struct Inputs {
    pub stats: MinerStats,
    pub hashrate: f64,
    pub temperature: f32,
    /// Why mining is limited (`Watch::limit_reason`), "" when it is not.
    pub limit: &'static str,
    pub power_percent: u8,
    /// Battery level in percent, negative without a gauge.
    pub battery: i32,
    /// Local "HH:MM" and "Thu 23 Oct" (or French), when the clock is set.
    pub clock: Option<(String, String)>,
    pub pool_host: String,
    pub on_backup: bool,
    pub ip: String,
    pub board: String,
    pub firmware: String,
    pub free_heap: u32,
    pub uptime: u32,
    /// Show "+1 SHARE" in the header.
    pub share_flash: bool,
    /// mempool.space readings (`App::market`).
    pub market: crate::market::MarketData,
    /// Unix time, when the clock is set (the age of recent blocks).
    pub unix_time: Option<u32>,
    /// Your best share while each recent block was the one to find.
    pub block_bests: Vec<(u32, f64)>,
}

/// The hash rate graph's samples.
#[derive(Clone, Debug, Default)]
pub struct Samples {
    points: std::collections::VecDeque<(u32, f32)>,
    last: Option<u32>,
    revision: u32,
}

impl Samples {
    /// Records `rate` when 10 s have passed since the last sample.
    pub fn sample(&mut self, now: u32, rate: f64) {
        if self.last.is_some_and(|t| now.wrapping_sub(t) < SAMPLE_MS) {
            return;
        }
        self.last = Some(now);
        if self.points.len() == SAMPLES {
            self.points.pop_front();
        }
        self.points.push_back((now, rate as f32));
        self.revision = self.revision.wrapping_add(1);
    }
}

pub fn diff_text(value: f64) -> String {
    if !value.is_finite() || value <= 0.0 {
        "-".into()
    } else if value < 1.0 {
        format!("{value:.5}")
    } else {
        format_si(value, 2)
    }
}

/// The value and its unit.
pub fn rate_text(rate: f64) -> (String, &'static str) {
    let mut rate = if rate.is_finite() && rate >= 0.0 { rate } else { 0.0 };
    let mut unit = "H/s";
    for (scale, name) in [(1e12, "TH/s"), (1e9, "GH/s"), (1e6, "MH/s"), (1e3, "kH/s")] {
        if rate >= scale {
            rate /= scale;
            unit = name;
            break;
        }
    }
    let text = if rate >= 100.0 {
        format!("{rate:.1}")
    } else if rate >= 10.0 {
        format!("{rate:.2}")
    } else {
        format!("{rate:.3}")
    };
    (text, unit)
}

/// "45s", "12min", "3h05", "2d 4h", "1.5 yr" (src/ui.cpp duration()).
pub fn duration(seconds: f64, language: u8) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "-".into();
    }
    if seconds < 60.0 {
        format!("{:.0}s", seconds.floor())
    } else if seconds < 3600.0 {
        format!("{:.0}min", (seconds / 60.0).floor())
    } else if seconds < 86400.0 {
        format!("{}h{:02}", (seconds / 3600.0) as u32, (seconds / 60.0) as u32 % 60)
    } else if seconds < 31_557_600.0 {
        let days = (seconds / 86400.0).floor();
        let hours = ((seconds / 3600.0) % 24.0).floor();
        format!("{days:.0}{} {hours:.0}h", if language == 1 { "j" } else { "d" })
    } else {
        format!("{} {}", format_si(seconds / 31_557_600.0, 1), if language == 1 { "ans" } else { "yr" })
    }
}

fn price_text(price: f64) -> String {
    if !price.is_finite() || price <= 0.0 {
        return "-".into();
    }
    format!("{price:.0}")
}

pub fn short_duration(seconds: f64, language: u8) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "-".into();
    }
    if seconds < 3600.0 {
        format!("{:.0}min", (seconds / 60.0).floor())
    } else if seconds < 86400.0 {
        format!("{}h", (seconds / 3600.0) as u32)
    } else {
        let days = (seconds / 86400.0).floor();
        let hours = ((seconds / 3600.0) % 24.0).floor();
        format!("{days:.0}{} {hours:.0}h", if language == 1 { "j" } else { "d" })
    }
}

fn limit_state(inputs: &Inputs) -> Option<&'static str> {
    match inputs.limit {
        "thermal" | "hot" => Some("COOLING"),
        "sensor" => Some("NO SENSOR"),
        "battery" => Some("BATTERY"),
        _ if inputs.power_percent == 0 => Some("PAUSED"),
        _ if inputs.power_percent < 100 => Some("LIMITED"),
        _ => None,
    }
}

/// The model of `page` (0..PAGES), as src/ui.cpp builds it.
pub fn build(page: u32, settings: &Settings, inputs: &Inputs, samples: &Samples) -> Model {
    let lang = settings.language;
    let accent = accent(settings.theme);
    let stats = &inputs.stats;
    let mut model = Model {
        page,
        language: lang,
        accent,
        status: tr(if stats.pool_connected { "LIVE" } else { "OFFLINE" }, lang).into(),
        status_color: if stats.pool_connected { GREEN } else { RED },
        battery: if inputs.battery >= 0 { format!("{}%", inputs.battery) } else { String::new() },
        ..Default::default()
    };
    if let Some(state) = limit_state(inputs) {
        model.status = tr(state, lang).into();
        model.status_color = AMBER;
    }
    if inputs.share_flash {
        model.status = "+1 SHARE".into();
        model.status_color = AMBER;
    }
    let mut set = |i: usize, label: &str, value: String, detail: &str, color: u32| {
        model.metrics[i] = Metric { label: label.into(), value, detail: detail.into(), color };
    };
    let height = || if stats.block_height > 0 { stats.block_height.to_string() } else { "-".into() };
    let coin = settings.coin();
    let title: String;
    match page {
        0 if settings.custom_screen => {
            title = tr("MINING", lang).into();
            let snapshot = Snapshot {
                hashrate: inputs.hashrate,
                best: stats.session_best_diff,
                temperature: inputs.temperature,
                accepted: stats.accepted,
                uptime: inputs.uptime,
                connected: stats.pool_connected,
                ..Default::default()
            };
            for (i, &widget) in settings.screen_widgets.iter().enumerate() {
                let w = widget as u32;
                let value = display_config::value(w, &snapshot, lang == 1);
                set(i, display_config::label(w, lang == 1), value, "", if i == 0 { accent } else { TEXT });
            }
        }
        0 => {
            title = tr("MINING", lang).into();
            let (rate, unit) = rate_text(inputs.hashrate);
            set(0, tr("LIVE HASHRATE", lang), rate, unit, accent);
            set(1, "SHARES", stats.accepted.to_string(), "", if stats.rejected > 0 { AMBER } else { GREEN });
            set(2, tr("BEST DIFF", lang), diff_text(stats.session_best_diff), "", TEXT);
            let t = inputs.temperature;
            let max = settings.max_temp as f32;
            let color = if !t.is_finite() {
                MUTED
            } else if max > 0.0 && t >= max {
                RED
            } else if max > 0.0 && t >= max - 10.0 {
                AMBER
            } else {
                TEXT
            };
            let value = if t.is_finite() { format!("{t:.1} C") } else { "--".into() };
            set(3, tr("TEMP", lang), value, "", color);
        }
        1 => {
            title = tr("CLOCK", lang).into();
            let (clock, date) = inputs.clock.clone().unwrap_or(("--:--".into(), "-".into()));
            set(0, tr("LOCAL TIME", lang), clock, "", TEXT);
            set(1, "DATE", date, "", TEXT);
            if coin == Coin::Btc {
                set(2, tr("BTC PRICE", lang), price_text(inputs.market.price), "", accent);
            } else {
                set(2, "CRYPTO", coin.ticker().into(), "", accent);
            }
            set(3, tr("BLOCK", lang), height(), "", TEXT);
        }
        2 => {
            title = coin.name().into();
            if coin.bitcoin_data() {
                let m = &inputs.market;
                set(0, tr("BITCOIN PRICE", lang), price_text(m.price), &settings.currency, accent);
                let fees = if m.fee_fast > 0 {
                    format!("{}/{}/{}", m.fee_fast, m.fee_half_hour, m.fee_hour)
                } else {
                    "-".into()
                };
                set(1, tr("FEES sat/vB", lang), fees, "", TEXT);
                let (retarget, color) = if m.updated_ms == 0 {
                    ("-".to_string(), MUTED)
                } else {
                    (format!("{:+.2}%", m.retarget_change), if m.retarget_change >= 0.0 { GREEN } else { RED })
                };
                set(2, tr("NEXT RETARGET", lang), retarget, "", color);
                let net = if m.network_hashrate > 0.0 {
                    format!("{}H/s", format_si(m.network_hashrate, 1))
                } else {
                    "-".into()
                };
                set(3, tr("NET HASHRATE", lang), net, "", TEXT);
            } else {
                set(0, tr("BLOCK HEIGHT", lang), height(), "", accent);
                set(1, tr("NETWORK DIFF", lang), diff_text(stats.network_diff), "", TEXT);
                set(2, "POOL", inputs.pool_host.clone(), "", TEXT);
                set(3, tr("LIVE HASHRATE", lang), format_si(inputs.hashrate, 1), "H/s", TEXT);
            }
        }
        3 => {
            title = tr("NETWORK", lang).into();
            set(0, tr("BLOCK HEIGHT", lang), height(), "", accent);
            set(1, tr("NETWORK DIFF", lang), diff_text(stats.network_diff), "", TEXT);
            let (label, color) = if inputs.on_backup { (tr("BACKUP POOL", lang), AMBER) } else { ("POOL", TEXT) };
            set(2, label, inputs.pool_host.clone(), "", color);
            let interval = coin.halving_interval();
            match stats.block_height.checked_div(interval) {
                Some(epoch) => {
                    let left = if stats.block_height > 0 {
                        ((epoch + 1) * interval - stats.block_height).to_string()
                    } else {
                        "-".into()
                    };
                    set(3, "HALVING", left, "", TEXT);
                }
                None => set(3, "CRYPTO", coin.ticker().into(), "", TEXT),
            }
        }
        4 => {
            title = tr("DEVICE", lang).into();
            set(0, tr("LOCAL IP", lang), inputs.ip.clone(), "", TEXT);
            set(1, tr("BOARD", lang), inputs.board.clone(), "", TEXT);
            set(2, "FIRMWARE", format!("v{}", inputs.firmware), "", TEXT);
            set(3, tr("FREE RAM", lang), format!("{} KB", inputs.free_heap / 1024), "", TEXT);
        }
        5 => {
            title = "HASHRATE".into();
            let valid: Vec<f32> = samples.points.iter().map(|p| p.1).filter(|v| v.is_finite() && *v >= 0.0).collect();
            let text = |v: Option<f32>| v.map(|v| format_si(v as f64, 1)).unwrap_or_else(|| "-".into());
            let avg = (!valid.is_empty()).then(|| valid.iter().sum::<f32>() / valid.len() as f32);
            let min = valid.iter().copied().reduce(f32::min);
            let max = valid.iter().copied().reduce(f32::max);
            set(0, tr("AVG H/s", lang), text(avg), "H/s", accent);
            set(1, "MIN", text(min), "", TEXT);
            set(2, "MAX", text(max), "", TEXT);
            model.points = samples.points.iter().copied().collect();
            model.graph_revision = samples.revision;
        }
        6 => {
            title = "RECORDS".into();
            set(0, tr("BEST EVER", lang), diff_text(stats.best_diff), "", accent);
            set(1, tr("32-BIT", lang), stats.over32bits.to_string(), "", TEXT);
            set(2, "SESSION", diff_text(stats.session_best_diff), "", TEXT);
            set(3, tr("RUN TIME", lang), short_duration(stats.lifetime_seconds as f64, lang), "", TEXT);
        }
        _ => {
            title = tr("BLOCKS", lang).into();
            set(0, tr("MINING BLOCK", lang), height(), "", accent);
            for (i, block) in inputs.market.blocks.iter().take(3).enumerate() {
                let mut detail = match inputs.unix_time {
                    Some(now) if block.time != 0 => duration(now.saturating_sub(block.time) as f64, lang),
                    _ => "-".into(),
                };
                for &(h, best) in &inputs.block_bests {
                    if h == block.height {
                        detail = format!("{detail} / {}", diff_text(best));
                    }
                }
                let pool = if block.pool.is_empty() { "?".to_string() } else { block.pool.clone() };
                set(i + 1, &block.height.to_string(), pool, &detail, TEXT);
            }
        }
    }
    model.title = title;
    model
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::BoardProfile;

    fn settings() -> Settings {
        Settings::defaults(&BoardProfile::host())
    }

    #[test]
    fn mining_page_matches_the_cpp_texts() {
        let mut inputs = Inputs { hashrate: 1_234_567.0, temperature: 48.25, power_percent: 100, ..Default::default() };
        inputs.stats.accepted = 3;
        inputs.stats.pool_connected = true;
        inputs.stats.session_best_diff = 0.0123;
        let m = build(0, &settings(), &inputs, &Samples::default());
        assert_eq!(m.title, "MINING");
        assert_eq!((m.status.as_str(), m.status_color), ("LIVE", GREEN));
        assert_eq!((m.metrics[0].value.as_str(), m.metrics[0].detail.as_str()), ("1.235", "MH/s"));
        assert_eq!(m.metrics[1].value, "3");
        assert_eq!(m.metrics[2].value, "0.01230");
        assert_eq!(m.metrics[3].value, "48.2 C");
    }

    #[test]
    fn french_and_limits() {
        let mut s = settings();
        s.language = 1;
        let inputs = Inputs { limit: "thermal", power_percent: 50, temperature: f32::NAN, ..Default::default() };
        let m = build(0, &s, &inputs, &Samples::default());
        assert_eq!((m.title.as_str(), m.status.as_str(), m.status_color), ("MINAGE", "REFROIDIT", AMBER));
        assert_eq!((m.metrics[3].value.as_str(), m.metrics[3].color), ("--", MUTED));
        let paused = Inputs { power_percent: 0, ..Default::default() };
        assert_eq!(build(0, &s, &paused, &Samples::default()).status, "PAUSE");
    }

    #[test]
    fn graph_samples_every_ten_seconds() {
        let mut samples = Samples::default();
        for t in (0..3_000_000u32).step_by(1000) {
            samples.sample(t, (t / 1000) as f64);
        }
        assert_eq!(samples.points.len(), SAMPLES);
        let m = build(5, &settings(), &Inputs { power_percent: 100, ..Default::default() }, &samples);
        assert_eq!(m.points.len(), SAMPLES);
        assert_eq!(m.points.first().unwrap().0, 2_990_000 - 239 * 10_000);
        assert_eq!(m.metrics[1].label, "MIN");
    }

    #[test]
    fn halving_and_rates() {
        let mut inputs = Inputs { power_percent: 100, ..Default::default() };
        inputs.stats.block_height = 840_001;
        let m = build(3, &settings(), &inputs, &Samples::default());
        assert_eq!(m.metrics[3].value, "209999");
        assert_eq!(rate_text(999.0), ("999.0".to_string(), "H/s"));
        assert_eq!(rate_text(1500.0), ("1.500".to_string(), "kH/s"));
        assert_eq!(short_duration(90_061.0, 0), "1d 1h");
    }
}
