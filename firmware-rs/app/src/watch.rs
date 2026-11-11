//! Supervision, once a second from the main loop: thermal policy and power,
//! night mode, alerts and notifications, and the automatic restarts.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use bdlminer_core::safety::ThermalPolicy;

use crate::miner::{Miner, MinerStats};
use crate::notify::Notifier;
use crate::platform::{BoardProfile, Clock};
use crate::settings::Settings;

const NO_POOL_RESTART_MS: u32 = 30 * 60 * 1000;
const OFFLINE_NOTIFY_MS: u32 = 10 * 60 * 1000;
const BEST_NOTIFY_MS: u32 = 10 * 60 * 1000;
const ALERT_WARMUP_MS: u32 = 5 * 60 * 1000;
const NORMAL_POINTS: usize = 6;

/// What the board does for supervision (no-ops by default).
pub trait Indicators: Send + Sync {
    fn beep(&self) {}
    fn flash_led(&self, _on: bool) {}
    fn set_backlight(&self, _percent: u8) {}
    fn block_found(&self, _height: u32) {}
    fn share_accepted(&self) {}
}

pub struct NoIndicators;
impl Indicators for NoIndicators {}

#[derive(Default)]
struct Events {
    started: bool,
    last_over32: u32,
    last_accepted: u32,
    accepted_at: u32,
    last_connected: u32,
    last_best: f64,
    best_at: u32,
    offline_sent: bool,
    boot_sent: bool,
    last_blocks: u32,
    low_since: u32,
    low_sent: bool,
    window_started: bool,
    window_at: u32,
    accepted0: u32,
    rejected0: u32,
    normal: f64,
    normal_revision: Option<u32>,
}

pub struct Watch {
    thermal: Mutex<ThermalPolicy>,
    events: Mutex<Events>,
    night: AtomicBool,
    awake_since: AtomicU32,
    limit_reason: Mutex<&'static str>,
    restart: Mutex<Option<String>>,
}

impl Watch {
    pub fn new(board: &BoardProfile) -> Self {
        let mut thermal = ThermalPolicy::default();
        if board.asic() {
            thermal.steps = false; // each power change restarts the ASIC
            thermal.hold_ms = 120_000; // its sensor is silent while it is off
        }
        Watch {
            thermal: Mutex::new(thermal),
            events: Mutex::new(Events::default()),
            night: AtomicBool::new(false),
            awake_since: AtomicU32::new(0),
            limit_reason: Mutex::new("supervision"),
            restart: Mutex::new(None),
        }
    }

    pub fn night(&self) -> bool {
        self.night.load(Ordering::Relaxed)
    }

    /// A button press suspends night mode for 30 s.
    pub fn wake(&self, now: u32) {
        self.awake_since.store(now.max(1), Ordering::Relaxed);
    }

    /// Why mining is limited: "full", "user", "hot", "sensor"...
    pub fn limit_reason(&self, miner: &Miner, ota_busy: bool) -> &'static str {
        if ota_busy {
            return "ota";
        }
        let reason = *self.limit_reason.lock().unwrap();
        if miner.power_percent() == 0 && !matches!(reason, "hot" | "sensor" | "user_pause") {
            return "supervision";
        }
        reason
    }

    /// A restart the supervision decided, with its reason.
    pub fn take_restart(&self) -> Option<String> {
        self.restart.lock().unwrap().take()
    }

    fn night_now(s: &Settings, hour: Option<u8>) -> bool {
        if s.night_from > 23 || s.night_to > 23 || s.night_from == s.night_to {
            return false;
        }
        let Some(h) = hour else { return false };
        if s.night_from < s.night_to {
            h >= s.night_from && h < s.night_to
        } else {
            h >= s.night_from || h < s.night_to
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &self,
        s: &Settings,
        clock: &dyn Clock,
        miner: &Miner,
        temperature: f32,
        on_battery: bool,
        brightness: u8,
        pool_status: &str,
        boot_reason: &str,
        notifier: &Notifier,
        ui: &dyn Indicators,
        log: &dyn Fn(&str),
    ) {
        let now = clock.millis();
        let night = Self::night_now(s, clock.local_hour());
        self.night.store(night, Ordering::Relaxed);

        // Thermal policy and power.
        let saver = s.battery_saver && on_battery;
        let (power, hot, was_hot, fault, was_fault) = {
            let mut t = self.thermal.lock().unwrap();
            let (was_hot, was_fault) = (t.hot, t.sensor_fault);
            let power = t.power(temperature, s.max_temp, s.mining_power, saver, now);
            (power, t.hot, was_hot, t.sensor_fault, was_fault)
        };
        let reason = if s.mining_power == 0 {
            "user_pause"
        } else if fault {
            "sensor"
        } else if hot {
            "hot"
        } else if s.max_temp != 0
            && temperature.is_finite()
            && temperature >= s.max_temp as f32 - 10.0
            && power < s.mining_power
        {
            "thermal"
        } else if saver && s.mining_power > 50 {
            "battery"
        } else if s.mining_power < 100 {
            "user"
        } else {
            "full"
        };
        *self.limit_reason.lock().unwrap() = reason;
        if hot != was_hot {
            if hot {
                let msg = format!("{temperature:.0} C, mining paused until {} C", s.max_temp.saturating_sub(5));
                log(&format!("[watch] too hot: {msg}"));
                notifier.notify("Miner too hot", &msg, 4, "thermometer");
            } else {
                log(&format!("[watch] cooled down to {temperature:.0} C, mining again"));
                notifier.notify("Miner cooled down", "Mining again", 2, "thermometer");
            }
        }
        if fault && !was_fault {
            log(&format!(
                "[watch] temperature unavailable: mining {}",
                if power != 0 { "limited to 25%" } else { "paused" }
            ));
            notifier.notify(
                "Temperature unavailable",
                if power != 0 {
                    "No usable sensor: mining limited to 25%"
                } else {
                    "Mining paused until a valid temperature returns"
                },
                4,
                "thermometer",
            );
        }
        miner.set_power(power, saver);
        miner.observe(temperature, miner.power_percent());

        let awake_since = self.awake_since.load(Ordering::Relaxed);
        let awake = awake_since != 0 && now.wrapping_sub(awake_since) < 30_000;
        let backlight = if night && !awake {
            0
        } else if saver {
            brightness.min(20)
        } else {
            brightness
        };
        ui.set_backlight(backlight);

        self.events(s, now, miner, pool_status, boot_reason, notifier, ui, log, night);
    }

    #[allow(clippy::too_many_arguments)]
    fn events(
        &self,
        s: &Settings,
        now: u32,
        miner: &Miner,
        pool_status: &str,
        boot_reason: &str,
        notifier: &Notifier,
        ui: &dyn Indicators,
        log: &dyn Fn(&str),
        night: bool,
    ) {
        let stats = miner.stats();
        let mut e = self.events.lock().unwrap();
        if !e.started {
            e.started = true;
            e.last_over32 = stats.over32bits;
            e.last_accepted = stats.accepted;
            e.accepted_at = now;
            e.last_connected = now;
            e.last_best = stats.best_diff;
        }
        if let Some(c) = miner.last_block_candidate().filter(|c| !c.previous_session && c.number != e.last_blocks) {
            e.last_blocks = c.number;
            let msg = format!(
                "{} block candidate at height {}. Check pool submission and chain confirmation.",
                c.coin.ticker(),
                c.height
            );
            notifier.notify("BLOCK CANDIDATE", &msg, 5, "tada");
            log(&format!("[miner] block candidate alert at height {}; chain confirmation unknown", c.height));
            self.wake(now);
            ui.block_found(c.height);
            if s.beep {
                ui.beep();
            }
        }
        let mut led = false;
        if stats.over32bits != e.last_over32 {
            e.last_over32 = stats.over32bits;
            log(&format!("[miner] 32-bit share found ({} so far)", e.last_over32));
            if s.beep && !night {
                ui.beep();
            }
            led = !night;
            if s.notify32 {
                notifier.notify("32-bit share", &format!("{} so far", e.last_over32), 2, "sparkles");
            }
        }
        if stats.best_diff > e.last_best
            && stats.best_diff >= 1.0
            && (e.best_at == 0 || now.wrapping_sub(e.best_at) > BEST_NOTIFY_MS)
        {
            e.last_best = stats.best_diff;
            e.best_at = now;
            notifier.notify("New record", &format!("New best difficulty ever: {:.4}", stats.best_diff), 3, "trophy");
        }
        if stats.accepted != e.last_accepted {
            e.last_accepted = stats.accepted;
            e.accepted_at = now;
            ui.share_accepted();
        }
        if miner.power_percent() == 0 {
            e.accepted_at = now;
        }
        ui.flash_led(led);
        if stats.pool_connected {
            e.last_connected = now;
            if e.offline_sent {
                e.offline_sent = false;
                notifier.notify("Miner back online", pool_status, 2, "white_check_mark");
            }
            if !e.boot_sent {
                e.boot_sent = true;
                if boot_reason != "power on" && boot_reason != "reset pin" && !boot_reason.contains("web page") {
                    notifier.notify("Miner restarted", &format!("Last reset: {boot_reason}"), 3, "warning");
                }
            }
        } else if !e.offline_sent && now.wrapping_sub(e.last_connected) > OFFLINE_NOTIFY_MS {
            e.offline_sent = true;
            notifier.notify("Miner offline", &format!("Not mining for 10 minutes: {pool_status}"), 4, "warning");
        }
        self.alerts(&mut e, s, now, miner, &stats, notifier, log);
        if now.wrapping_sub(e.last_connected) > NO_POOL_RESTART_MS {
            *self.restart.lock().unwrap() = Some("no pool connection for 30 minutes".into());
        }
        if s.reboot_hours != 0 && now.wrapping_sub(e.accepted_at) > s.reboot_hours as u32 * 3_600_000 {
            *self.restart.lock().unwrap() = Some(format!("no accepted share for {} hours", s.reboot_hours));
        }
    }

    /// Median of the full-power, well-covered history points.
    fn normal_hashrate(e: &mut Events, miner: &Miner) -> f64 {
        let revision = miner.history_revision();
        if e.normal_revision == Some(revision) {
            return e.normal;
        }
        e.normal_revision = Some(revision);
        let (points, _) = miner.history();
        let mut rates: Vec<f32> = points
            .iter()
            .filter(|p| p.power_percent >= 100 && p.coverage_percent >= 90 && p.hashrate > 0.0)
            .map(|p| p.hashrate)
            .collect();
        if rates.len() < NORMAL_POINTS {
            e.normal = 0.0;
            return 0.0;
        }
        let mid = rates.len() / 2;
        rates.select_nth_unstable_by(mid, f32::total_cmp);
        e.normal = rates[mid] as f64;
        e.normal
    }

    #[allow(clippy::too_many_arguments)]
    fn alerts(
        &self,
        e: &mut Events,
        s: &Settings,
        now: u32,
        miner: &Miner,
        stats: &MinerStats,
        notifier: &Notifier,
        log: &dyn Fn(&str),
    ) {
        let span = s.alert_minutes as u32 * 60_000;
        let normal = if s.alert_percent != 0 { Self::normal_hashrate(e, miner) } else { 0.0 };
        let comparable = normal > 0.0
            && now >= ALERT_WARMUP_MS
            && stats.pool_connected
            && s.mining_power >= 100
            && miner.power_percent() >= 100;
        let rate = miner.hashrate();
        if !comparable {
            e.low_since = 0;
        } else if rate < normal * s.alert_percent as f64 / 100.0 {
            if e.low_since == 0 {
                e.low_since = now.max(1);
            }
            if !e.low_sent && now.wrapping_sub(e.low_since) >= span {
                e.low_sent = true;
                let msg = format!("{rate:.0} H/s for {} minutes, normally {normal:.0} H/s", s.alert_minutes);
                log(&format!("[watch] hashrate low: {msg}"));
                notifier.notify("Hashrate low", &msg, 4, "chart_with_downwards_trend");
            }
        } else {
            e.low_since = 0;
            if e.low_sent {
                e.low_sent = false;
                notifier.notify(
                    "Hashrate back to normal",
                    &format!("{rate:.0} H/s, normally {normal:.0} H/s"),
                    2,
                    "chart_with_upwards_trend",
                );
            }
        }
        if !e.window_started || now.wrapping_sub(e.window_at) >= span {
            let rejected = stats.rejected - e.rejected0;
            let accepted = stats.accepted - e.accepted0;
            if e.window_started && rejected >= 5 && rejected * 10 >= rejected + accepted {
                let msg =
                    format!("{rejected} of {} shares rejected in {} minutes", rejected + accepted, s.alert_minutes);
                log(&format!("[watch] {msg}"));
                notifier.notify("Shares rejected", &msg, 4, "x");
            }
            e.window_started = true;
            e.window_at = now;
            e.accepted0 = stats.accepted;
            e.rejected0 = stats.rejected;
        }
    }
}
