//! Whether the latest local block candidate is in the Bitcoin best chain,
//! as mempool.space sees it (the C++ firmware's block_status.cpp). Explorer
//! observations only: independent of the pool's share answers and payouts.

use std::sync::Mutex;

use bdlminer_core::coin::Coin;
use bdlminer_core::work;

use crate::app::App;
use crate::notify::{get_body, HttpClient};
use crate::platform::transport_agent;

const INITIAL_MS: u32 = 15_000;
const POLL_MS: u32 = 120_000;
const MAX_RETRY_MS: u32 = 6 * 3_600_000;
const MIN_HEAP: u32 = 54 * 1024;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChainStatus {
    /// Digest order, matched to the latest local candidate.
    pub hash: [u8; 32],
    pub known: bool,
    pub in_best_chain: bool,
    pub not_found: bool,
    pub height: u32,
    pub confirmations: u32,
    pub checked_ms: u32,
    pub error: i32,
}

struct Inner {
    observed: Option<ChainStatus>,
    notified: bool,
    next_check: u32,
    retry_ms: u32,
}

pub struct BlockStatus {
    inner: Mutex<Inner>,
}

impl Default for BlockStatus {
    fn default() -> Self {
        BlockStatus { inner: Mutex::new(Inner { observed: None, notified: false, next_check: 0, retry_ms: 30_000 }) }
    }
}

/// The tip height mempool.space answers ("900001\n").
pub fn tip_height(text: &str) -> Option<u32> {
    let t = text.trim_end_matches(['\n', '\r', ' ']);
    if t.is_empty() || t.len() > 10 || !t.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse().ok()
}

impl BlockStatus {
    /// The status of the candidate with this hash, if it is the one checked.
    pub fn get(&self, hash: &[u8; 32]) -> Option<ChainStatus> {
        self.inner.lock().unwrap().observed.as_ref().filter(|o| &o.hash == hash).cloned()
    }

    /// Called every few seconds by the HTTPS task. No candidate, no request.
    pub fn poll(&self, app: &App, http: &dyn HttpClient, link_up: bool) {
        // This explorer serves Bitcoin only: an alternate-chain proof must not
        // be reported missing merely because it is absent from BTC.
        let Some(candidate) = app.miner.last_block_candidate().filter(|c| c.coin == Coin::Btc) else { return };
        let now = app.clock.millis();
        let mut result = {
            let mut inner = self.inner.lock().unwrap();
            if inner.observed.as_ref().is_none_or(|o| o.hash != candidate.hash) {
                inner.observed = Some(ChainStatus { hash: candidate.hash, ..Default::default() });
                inner.notified = false;
                inner.retry_ms = 30_000;
                inner.next_check = now.wrapping_add(INITIAL_MS);
                return;
            }
            if (now.wrapping_sub(inner.next_check) as i32) < 0 || !link_up || app.system.free_heap() < MIN_HEAP {
                return;
            }
            inner.observed.clone().unwrap()
        };
        let id = work::reversed_hex(&candidate.hash);
        let headers = [("User-Agent", transport_agent())];
        let get = |path: &str| -> Result<String, i32> {
            match get_body(http, &format!("https://mempool.space/api{path}"), &headers, 512) {
                Ok((200, body)) if !body.is_empty() => String::from_utf8(body).map_err(|_| -3),
                Ok((200, _)) => Err(-3),
                Ok((code, _)) => Err(code as i32),
                Err(e) if e.contains("too large") => Err(413),
                Err(_) => Err(-1),
            }
        };
        let mut code = 0;
        let mut success = false;
        match get(&format!("/block/{id}/status")) {
            Ok(body) => {
                let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                match (v["in_best_chain"].as_bool(), v["height"].as_u64()) {
                    (Some(false), _) => {
                        result.known = true;
                        result.in_best_chain = false;
                        result.confirmations = 0;
                        result.height = 0;
                        success = true;
                    }
                    (Some(true), Some(h))
                        if h <= u32::MAX as u64 && (candidate.height == 0 || h == candidate.height as u64) =>
                    {
                        let height = h as u32;
                        match get("/blocks/tip/height") {
                            Ok(tip) => match tip_height(&tip) {
                                Some(tip) if tip >= height => {
                                    result.known = true;
                                    result.in_best_chain = true;
                                    result.height = height;
                                    result.confirmations = tip - height + 1;
                                    success = true;
                                }
                                _ => code = -4,
                            },
                            Err(c) => code = c,
                        }
                    }
                    _ => code = -4,
                }
            }
            Err(c) => code = c,
        }
        result.error = if success { 0 } else { code };
        result.not_found = !success && code == 404;
        if success {
            result.checked_ms = app.clock.millis().max(1);
        }
        // A new candidate may have arrived meanwhile: never attach this
        // result to it, nor alert about the wrong one.
        if app.miner.last_block_candidate().is_none_or(|c| c.hash != candidate.hash) {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        inner.observed = Some(result.clone());
        if success && result.in_best_chain && !inner.notified {
            inner.notified = true;
            drop(inner);
            app.notifier.notify(
                "Block seen in chain",
                &format!(
                    "mempool.space reports block {} in the Bitcoin best chain ({} confirmations). Pool payout is separate.",
                    result.height, result.confirmations
                ),
                5,
                "link",
            );
            inner = self.inner.lock().unwrap();
        } else if success && !result.in_best_chain && inner.notified {
            inner.notified = false;
            drop(inner);
            app.notifier.notify(
                "Block chain changed",
                "mempool.space no longer reports this block in its best chain. Check the explorer and pool.",
                4,
                "warning",
            );
            inner = self.inner.lock().unwrap();
        }
        let now = app.clock.millis();
        if success {
            inner.retry_ms = 30_000;
            inner.next_check = now.wrapping_add(if result.confirmations >= 6 { 600_000 } else { POLL_MS });
        } else {
            inner.next_check = now.wrapping_add(inner.retry_ms);
            inner.retry_ms = (inner.retry_ms * 2).min(MAX_RETRY_MS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tip() {
        assert_eq!(tip_height("900001\n"), Some(900_001));
        assert_eq!(tip_height(""), None);
        assert_eq!(tip_height("12a"), None);
        assert_eq!(tip_height("99999999999"), None);
    }
}
