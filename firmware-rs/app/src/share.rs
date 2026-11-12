//! Anonymous hashrate sharing (settings `sharehr`, on by default), as the C++
//! firmware's share.cpp: every 5 minutes the firmware version, the board, the
//! selected coin and the hashrate are posted to the BDLMiner statistics
//! collector, which adds up the reports of every miner (the public statistics
//! page). Pool sharing (`sharepool`, on by default) adds a fixed public pool
//! group. No Bitcoin address, worker, configured hostname, port, name or
//! identifier is sent.

use crate::app::App;
use crate::notify::HttpClient;
use crate::platform::FW_VERSION;
use bdlminer_core::coin::{public_pool_group, Coin};

pub const STATS_URL: &str = "https://api.bdl-pool.com:36018/report";

/// The first report waits for a settled hashrate once mining has started; then
/// one every 5 minutes. A failed report is not retried sooner: the next one comes.
const FIRST_MS: u32 = 3 * 60 * 1000;
pub const EVERY_SECONDS: u32 = 300;
const EVERY_MS: u32 = EVERY_SECONDS * 1000;
/// No WiFi, memory, clock or hashrate yet.
const WAIT_MS: u32 = 60 * 1000;
/// What a TLS handshake needs (tls.h kTlsMinFree / kTlsMinBlock).
const TLS_MIN_FREE: u32 = 54 * 1024;
const TLS_MIN_BLOCK: u32 = 24 * 1024;

/// The PlatformIO environment of the C++ firmware for the same board, which
/// the collector knows: "m5stickc-plus2-rust" is "m5stickc-plus2", a chip
/// without a screen "<chip>-headless".
pub fn stats_board(env: &str) -> String {
    let base = env.strip_suffix("-rust").unwrap_or(env);
    match base {
        "esp32" | "esp32s2" | "esp32s3" | "esp32c3" => format!("{base}-headless"),
        _ => base.into(),
    }
}

/// The JSON body: {"v":"2.6.1","board":"m5stickc-plus2","hashrate":1.02e6,
/// "every":300,"coin":"BTC"} with the hashrate rounded to 3 significant digits
/// and the pool reduced to its public group when given.
pub fn body(hashrate: f64, board: &str, coin: Coin, pool_host: Option<&str>) -> String {
    let rate = if hashrate.is_finite() && hashrate > 0.0 { hashrate } else { 0.0 };
    // Environment names are fixed strings of the firmware; anything else is left out.
    let board: String = board.chars().filter(|c| c.is_ascii_alphanumeric() || "-_.".contains(*c)).collect();
    let mut out = format!(
        "{{\"v\":\"{FW_VERSION}\",\"board\":\"{board}\",\"hashrate\":{rate:.2e},\"every\":{EVERY_SECONDS},\"coin\":\"{}\"",
        coin.ticker()
    );
    if let Some(host) = pool_host {
        out.push_str(&format!(",\"pool\":\"{}\"", public_pool_group(host)));
    }
    out.push('}');
    out
}

/// When the next report is due (milliseconds since boot).
pub struct Share {
    next: u32,
}

impl Default for Share {
    fn default() -> Self {
        Share { next: FIRST_MS }
    }
}

impl Share {
    /// Sends the report when it is due. Called every few seconds.
    pub fn poll(&mut self, app: &App, http: &dyn HttpClient, link_up: bool) {
        let s = app.settings();
        let now = app.clock.millis();
        if !s.share_hashrate || (now.wrapping_sub(self.next) as i32) < 0 {
            return;
        }
        let rate = app.miner.hashrate();
        // A paused miner adds nothing: it is not counted either.
        let memory = app.system.free_heap() >= TLS_MIN_FREE && app.system.max_alloc() >= TLS_MIN_BLOCK;
        if !link_up || !memory || app.clock.unix_time().is_none() || rate.is_nan() || rate <= 0.0 {
            self.next = now.wrapping_add(WAIT_MS);
            return;
        }
        self.next = now.wrapping_add(EVERY_MS);
        let host = (s.share_pool && app.miner.stats().pool_connected)
            .then(|| app.pool_config.endpoint(app.pool.on_backup()).host.clone());
        let body = body(rate, &stats_board(app.board.env), s.coin(), host.as_deref());
        // Transport compatibility marker; the displayed/Stratum identity is BDLMinerV2.
        let agent = format!("BDLMinerV1/{FW_VERSION}");
        let headers = [("User-Agent", agent.as_str()), ("Content-Type", "application/json")];
        match http.post(STATS_URL, &headers, body.as_bytes()) {
            Ok(code) if (200..300).contains(&code) => app.log(&format!("[share] hashrate shared ({rate:.3e} H/s)")),
            Ok(code) => app.log(&format!("[share] hashrate not shared: HTTP {code}")),
            Err(e) => app.log(&format!("[share] hashrate not shared: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_body() {
        assert_eq!(
            body(1.0234e6, "m5stickc-plus2", Coin::Btc, None),
            format!("{{\"v\":\"{FW_VERSION}\",\"board\":\"m5stickc-plus2\",\"hashrate\":1.02e6,\"every\":300,\"coin\":\"BTC\"}}")
        );
        let b = body(f64::NAN, "x\"y", Coin::Btc, Some("eu.public-pool.io"));
        assert!(b.contains("\"hashrate\":0.00e0") && b.contains("\"board\":\"xy\""));
        assert!(b.ends_with(",\"pool\":\"public-pool.io\"}"));
        let parsed: serde_json::Value = serde_json::from_str(&b).unwrap();
        assert_eq!(parsed["every"], 300);
    }

    #[test]
    fn board_names() {
        assert_eq!(stats_board("m5stickc-plus2-rust"), "m5stickc-plus2");
        assert_eq!(stats_board("esp32c3-rust"), "esp32c3-headless");
        assert_eq!(stats_board("bitaxe-gamma-601-rust"), "bitaxe-gamma-601");
        assert_eq!(stats_board("host"), "host");
    }
}
