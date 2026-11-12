//! Private remote monitoring ("My miners" on miner.bdl-pool.com), opt-in
//! (setting `remote`), as the C++ firmware's remote.cpp: every 5 minutes the
//! readings go to the owner's space, named by a private access key or by
//! the payout address. No raw MAC or pool host leaves: the device is a hash.

use serde_json::{json, Value};

use crate::app::App;
use crate::notify::HttpClient;
use crate::platform::{native_user_agent, FW_VERSION};
use bdlminer_core::coin::public_pool_group;
use bdlminer_core::remote_data;

pub const REMOTE_URL: &str = "https://api.bdl-pool.com:36018/api/remote/report";
const EVERY_MS: u32 = 300_000;
const WAIT_MS: u32 = 60_000;
const MIN_HEAP: u32 = 54 * 1024;

pub struct Remote {
    next: u32,
}

impl Default for Remote {
    fn default() -> Self {
        Remote { next: 60_000 }
    }
}

/// The report body (key or address-derived id, never the MAC itself).
pub fn body(app: &App, id: &str) -> Value {
    let s = app.settings();
    let stats = app.miner.stats();
    let rate = app.miner.hashrate();
    let t = app.system.temperature();
    let pool = if stats.pool_connected {
        public_pool_group(&app.pool_config.endpoint(app.pool.on_backup()).host)
    } else {
        "unknown"
    };
    json!({
        "id": id,
        "name": if s.remote_name.is_empty() { app.board.name.to_owned() } else { s.remote_name.clone() },
        "v": FW_VERSION,
        "board": crate::share::stats_board(app.board.env),
        "coin": s.coin().ticker(),
        "boot_id": format!("{:x}", app.boot_id),
        "uptime_s": app.miner.uptime_seconds(),
        "total_hashes": app.miner.total_hashes() as f64,
        "hashrate": if rate.is_finite() && rate > 0.0 { rate } else { 0.0 },
        "temp_c": if t.is_finite() { json!(t) } else { Value::Null },
        "accepted": stats.accepted,
        "rejected": stats.rejected,
        "best": if stats.best_diff.is_finite() && stats.best_diff >= 0.0 { stats.best_diff } else { 0.0 },
        "power_percent": app.miner.power_percent(),
        "pool_connected": stats.pool_connected,
        "pool": pool,
        "firmware_trial": app.firmware_trial.load(std::sync::atomic::Ordering::Relaxed),
    })
}

impl Remote {
    pub fn poll(&mut self, app: &App, http: &dyn HttpClient, link_up: bool) {
        let s = app.settings();
        let now = app.clock.millis();
        // A private key keeps its secret space; otherwise the payout address names it.
        let keyed = remote_data::valid_key(&s.remote_key);
        let address = remote_data::payout_address(&s.btc_address);
        if !s.remote_enabled || (!keyed && address.is_none()) || (now.wrapping_sub(self.next) as i32) < 0 {
            return;
        }
        // Verified TLS needs a set clock.
        if !link_up || app.system.free_heap() < MIN_HEAP || app.clock.unix_time().is_none() {
            self.next = now.wrapping_add(WAIT_MS);
            return;
        }
        self.next = now.wrapping_add(EVERY_MS);
        let (id, authorization) = match (keyed, address) {
            (true, _) => (remote_data::device_id(&s.remote_key, app.mac), format!("Bearer {}", s.remote_key)),
            (false, Some(a)) => (remote_data::address_device_id(&a, app.mac), format!("Address {a}")),
            (false, None) => return,
        };
        let body = body(app, &id).to_string();
        let agent = native_user_agent();
        let headers = [
            ("User-Agent", agent.as_str()),
            ("Content-Type", "application/json"),
            ("Authorization", authorization.as_str()),
        ];
        // Never log the key, address, body, answer or pseudonym.
        match http.post(REMOTE_URL, &headers, body.as_bytes()) {
            Ok(204) => {
                app.log(&format!("[remote] readings sent ({})", if keyed { "private key" } else { "payout address" }))
            }
            Ok(code) => app.log(&format!("[remote] readings not sent ({code})")),
            Err(_) => app.log("[remote] readings not sent (-1)"),
        }
    }
}
