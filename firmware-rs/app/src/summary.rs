//! The daily summary notification (setting `summary`, the local hour; 255 =
//! off), as the C++ firmware's watch.cpp: once a day within the first ten
//! minutes of that hour, what the miner did since the previous summary or
//! since it started. Sent once a day, across restarts.

use crate::app::App;

#[derive(Default)]
pub struct Summary {
    started: bool,
    from_ms: u32,
    hashes0: u64,
    accepted0: u32,
    rejected0: u32,
    blocks0: u32,
    connected0: u32,
    sent_day: u32,
}

/// C's "%.4g".
pub fn g4(x: f64) -> String {
    if !x.is_finite() {
        return format!("{x}");
    }
    if x == 0.0 {
        return "0".into();
    }
    let exp = x.abs().log10().floor() as i32;
    let trim = |s: String| {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            s
        }
    };
    if !(-4..4).contains(&exp) {
        let m = x / 10f64.powi(exp);
        // Rounding may carry the mantissa to 10.
        let (m, exp) = if (m.abs() * 1000.0).round() >= 10_000.0 { (m / 10.0, exp + 1) } else { (m, exp) };
        format!("{}e{}{:02}", trim(format!("{m:.3}")), if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        trim(format!("{:.*}", (3 - exp).max(0) as usize, x))
    }
}

pub fn rate_text(rate: f64) -> String {
    if rate >= 1e9 {
        format!("{:.2} GH/s", rate / 1e9)
    } else if rate >= 1e6 {
        format!("{:.2} MH/s", rate / 1e6)
    } else {
        format!("{:.1} kH/s", rate / 1e3)
    }
}

impl Summary {
    /// Once a second.
    pub fn tick(&mut self, app: &App) {
        let now = app.clock.millis();
        let stats = app.miner.stats();
        if !self.started {
            self.started = true;
            self.reset(app, now);
            // Survives restarts: one summary a day.
            self.sent_day = app
                .storage
                .lock()
                .unwrap()
                .get("summary", "day")
                .and_then(|v| v.try_into().ok())
                .map(u32::from_le_bytes)
                .unwrap_or(0);
        }
        let hour = app.settings().summary_hour;
        let Some(t) = app.clock.local_time() else { return };
        if hour > 23 || t.hour != hour || t.minute >= 10 {
            return;
        }
        let day = t.year * 1000 + t.year_day + 1;
        if day == self.sent_day {
            return;
        }
        self.sent_day = day;
        app.storage.lock().unwrap().set("summary", "day", &day.to_le_bytes());
        let ms = now.wrapping_sub(self.from_ms);
        // Every counter restarts here, sent or not: so does the period's best.
        let best = app.miner.take_period_best();
        // Nothing worth a message just after a start.
        if ms >= 30 * 60 * 1000 {
            let (connected, _) = app.miner.pool_uptime();
            let rate = app.miner.total_hashes().saturating_sub(self.hashes0) as f64 * 1000.0 / ms as f64;
            let blocks = stats.blocks_found.wrapping_sub(self.blocks0);
            let since = if ms >= 23 * 3_600_000 {
                "24 h".to_string()
            } else {
                format!("Since start {} h ago", (ms + 1_800_000) / 3_600_000)
            };
            let msg = format!(
                "{}{since}: {} average, best {} (record {}), {} accepted / {} rejected, pool connected {:.1} %",
                if blocks > 0 { "BLOCK CANDIDATE! " } else { "" },
                rate_text(rate),
                g4(best),
                g4(stats.best_diff),
                stats.accepted.wrapping_sub(self.accepted0),
                stats.rejected.wrapping_sub(self.rejected0),
                100.0 * connected.wrapping_sub(self.connected0) as f64 / ms as f64
            );
            app.log(&format!("[watch] daily summary: {msg}"));
            app.notifier.notify("Daily summary", &msg, 2, "bar_chart");
        }
        self.reset(app, now);
    }

    fn reset(&mut self, app: &App, now: u32) {
        let stats = app.miner.stats();
        self.from_ms = now;
        self.hashes0 = app.miner.total_hashes();
        self.accepted0 = stats.accepted;
        self.rejected0 = stats.rejected;
        self.blocks0 = stats.blocks_found;
        self.connected0 = app.miner.pool_uptime().0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_formats() {
        assert_eq!(g4(1234.6), "1235");
        assert_eq!(g4(0.0123), "0.0123");
        assert_eq!(g4(1.0234e6), "1.023e+06");
        assert_eq!(g4(99995.0), "1e+05");
        assert_eq!(g4(0.0), "0");
        assert_eq!(rate_text(1.25e6), "1.25 MH/s");
        assert_eq!(rate_text(950.0), "0.9 kH/s");
    }
}
