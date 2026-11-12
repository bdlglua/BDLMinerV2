//! Bitcoin network data from the mempool.space public API (price, fees,
//! network hash rate, next difficulty adjustment, mempool, recent blocks)
//! and what the pool's statistics API says about this miner's address, as
//! the C++ firmware's market.cpp. Refreshed by the HTTPS task; the screens
//! and the dashboard read the latest copy.

use std::sync::Mutex;

use crate::app::App;
use crate::json_stream::{Parser, Scalar};
use crate::notify::{BodySink, HttpClient};
use crate::platform::transport_agent;

const API: &str = "https://mempool.space/api";
/// Each refresh costs TLS handshakes.
const REFRESH_MS: u32 = 10 * 60 * 1000;
const RETRY_MS: u32 = 30 * 1000;
const POOL_STATS_MS: u32 = 30 * 60 * 1000;
/// Unreachable API (often a pool outage): sooner.
const POOL_STATS_RETRY_MS: u32 = 10 * 60 * 1000;
/// No API found: search again after 6 h.
const API_SEARCH_MS: u32 = 6 * 60 * 60 * 1000;
const MIN_HEAP: u32 = 54 * 1024;
const BLOCKS: usize = 8;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Block {
    pub height: u32,
    /// Unix time.
    pub time: u32,
    pub txs: u32,
    /// Who mined it, "" when unknown.
    pub pool: String,
}

#[derive(Clone, Debug, Default)]
pub struct MarketData {
    /// millis() of the last successful refresh, 0 = never.
    pub updated_ms: u32,
    /// 1 BTC in the configured currency, 0 = unknown.
    pub price: f64,
    /// Recommended fees, sat/vB.
    pub fee_fast: u32,
    pub fee_half_hour: u32,
    pub fee_hour: u32,
    /// H/s.
    pub network_hashrate: f64,
    /// Estimated difficulty change, percent.
    pub retarget_change: f64,
    /// Percent of the current epoch done.
    pub retarget_progress: f64,
    pub retarget_blocks: u32,
    pub mempool_tx: u32,
    /// Newest first.
    pub blocks: Vec<Block>,
}

/// What the pool says about this miner's address (public-pool and ckpool
/// style APIs).
#[derive(Clone, Debug)]
pub struct PoolStats {
    pub updated_ms: u32,
    /// False: no stats API found for this pool.
    pub supported: bool,
    pub hashrate: f64,
    pub best_ever: f64,
    pub shares: f64,
    pub workers: u32,
    /// The API that answered: "public-pool.io:40557/api/client/".
    pub source: String,
    /// Last refresh: 0 OK, an HTTP status, or < 0 no connection.
    pub error: i32,
}

impl Default for PoolStats {
    fn default() -> Self {
        PoolStats {
            updated_ms: 0,
            supported: true,
            hashrate: 0.0,
            best_ever: 0.0,
            shares: 0.0,
            workers: 0,
            source: String::new(),
            error: 0,
        }
    }
}

/// "856K" (ckpool hash rates) is 856000.
pub fn parse_rate(s: &str) -> Option<f64> {
    let s = s.trim();
    let split = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let value: f64 = s[..split].trim().parse().ok()?;
    let unit = s[split..].trim();
    let scale = match unit.to_ascii_uppercase().as_str() {
        "" => 1.0,
        "K" => 1e3,
        "M" => 1e6,
        "G" => 1e9,
        "T" => 1e12,
        "P" => 1e15,
        "E" => 1e18,
        _ => return None,
    };
    let v = value * scale;
    (v.is_finite() && v >= 0.0).then_some(v)
}

fn pool_number(v: Scalar, previous: f64) -> f64 {
    v.number().filter(|n| n.is_finite() && *n >= 0.0).unwrap_or(previous)
}

/// GET `url` as streamed JSON; `on` sees every value. The HTTP status, or
/// a negative error (-1 no connection, -3 not JSON).
pub fn fetch_json(http: &dyn HttpClient, url: &str, on: &mut dyn FnMut(&str, Scalar)) -> Result<(), i32> {
    struct Sink<'a> {
        parser: Parser,
        on: &'a mut dyn FnMut(&str, Scalar),
        bad: bool,
    }
    impl BodySink for Sink<'_> {
        fn data(&mut self, chunk: &[u8]) -> Result<(), String> {
            let r = self.parser.feed(chunk, self.on);
            self.bad |= r.is_err();
            r
        }
    }
    let mut sink = Sink { parser: Parser::new(), on, bad: false };
    let headers = [("User-Agent", transport_agent()), ("Accept", "application/json")];
    match http.get(url, &headers, &mut sink) {
        Ok(200) => {
            let Sink { mut parser, on, .. } = sink;
            parser.finish(on).map_err(|_| -3)
        }
        Ok(code) => Err(code as i32),
        Err(_) if sink.bad => Err(-3),
        Err(_) => Err(-1),
    }
}

/// Pool statistics APIs, tried in turn: ckpool's /users/<address> (also
/// /api/miner?address=), public-pool's /api/client/<address>, on the pool's
/// host, its parent domain and the ports these servers use by default.
/// Appended, never inserted: the index found is saved on each miner.
struct PoolApi {
    secure: bool,
    parent: bool,
    port: u16,
    path: &'static str,
}

const POOL_APIS: [PoolApi; 10] = [
    PoolApi { secure: true, parent: false, port: 443, path: "/users/" },
    PoolApi { secure: true, parent: false, port: 40557, path: "/api/client/" },
    PoolApi { secure: true, parent: false, port: 443, path: "/api/client/" },
    PoolApi { secure: true, parent: true, port: 443, path: "/users/" },
    PoolApi { secure: true, parent: true, port: 40557, path: "/api/client/" },
    PoolApi { secure: true, parent: true, port: 443, path: "/api/client/" },
    // Self-hosted, local network.
    PoolApi { secure: false, parent: false, port: 3334, path: "/api/client/" },
    PoolApi { secure: false, parent: false, port: 80, path: "/users/" },
    PoolApi { secure: true, parent: false, port: 443, path: "/api/miner?address=" },
    PoolApi { secure: true, parent: true, port: 443, path: "/api/miner?address=" },
];

/// "eu.pool.example" is "pool.example"; empty for two-label names and IP addresses.
pub fn parent_domain(host: &str) -> String {
    let Some(dot) = host.find('.') else { return String::new() };
    if !host[dot + 1..].contains('.') || host.bytes().all(|c| c.is_ascii_digit() || c == b'.') {
        return String::new();
    }
    host[dot + 1..].to_owned()
}

/// Reads either API's answer into `p`; false for JSON neither of them sends.
pub fn parse_stats(body: &[u8], p: &mut PoolStats) -> bool {
    let mut public_pool = false;
    let mut ckpool = false;
    let mut worker_rates = 0.0;
    let mut q = p.clone();
    let (mut rate_1h, mut rate_5m) = (None, None);
    let ok = crate::json_stream::parse_all(body, &mut |path, v| match path {
        "bestDifficulty" => {
            public_pool = true;
            q.best_ever = pool_number(v, q.best_ever);
        }
        "workersCount" => {
            public_pool = true;
            q.workers = v.number().unwrap_or(0.0) as u32;
        }
        "accounting.totalAcceptedShares" => q.shares = pool_number(v, q.shares),
        p if p.starts_with("workers[") && p.ends_with("].hashRate") => worker_rates += pool_number(v, 0.0),
        "hashrate1hr" => {
            ckpool = true;
            rate_1h = match v {
                Scalar::Str(s) => parse_rate(s),
                v => v.number(),
            };
        }
        "hashrate5m" => {
            ckpool = true;
            if let Scalar::Str(s) = v {
                rate_5m = parse_rate(s);
            }
        }
        "bestever" => {
            ckpool = true;
            q.best_ever = pool_number(v, q.best_ever);
        }
        "shares" => q.shares = pool_number(v, q.shares),
        "workers" => q.workers = v.number().unwrap_or(0.0) as u32,
        _ => {}
    });
    if ok.is_err() {
        return false;
    }
    if public_pool {
        q.hashrate = worker_rates;
    } else if ckpool {
        q.hashrate = rate_1h.or(rate_5m).unwrap_or(p.hashrate);
    } else {
        return false;
    }
    *p = q;
    true
}

#[derive(Default)]
struct PoolSearch {
    host: String,
    index: Option<usize>,
    search_after: u32,
    shares_at_search: u32,
    retry_on_share: bool,
}

struct Inner {
    next_market: u32,
    next_pool: u32,
    retry: u32,
    endpoint_at: [Option<u32>; 6],
    last_backup: bool,
    search: [PoolSearch; 2],
    wait_logged: bool,
}

pub struct Market {
    data: Mutex<MarketData>,
    pool: Mutex<(PoolStats, bool)>,
    inner: Mutex<Inner>,
}

impl Default for Market {
    fn default() -> Self {
        Market {
            data: Mutex::new(MarketData::default()),
            pool: Mutex::new((PoolStats::default(), false)),
            inner: Mutex::new(Inner {
                next_market: 0,
                // Pool stats once mining for a minute.
                next_pool: 60_000,
                retry: RETRY_MS,
                endpoint_at: [None; 6],
                last_backup: false,
                search: [
                    PoolSearch { retry_on_share: true, ..Default::default() },
                    PoolSearch { retry_on_share: true, ..Default::default() },
                ],
                wait_logged: false,
            }),
        }
    }
}

impl Market {
    /// The latest data (empty when the coin has no Bitcoin data).
    pub fn get(&self, app: &App) -> MarketData {
        if !app.settings().coin().bitcoin_data() {
            return MarketData::default();
        }
        self.data.lock().unwrap().clone()
    }

    /// What the pool in use says, if anything yet.
    pub fn pool_stats(&self, app: &App) -> PoolStats {
        if !app.settings().coin().bitcoin_data() {
            return PoolStats { supported: false, ..Default::default() };
        }
        let (p, backup) = self.pool.lock().unwrap().clone();
        if backup == app.pool.on_backup() {
            p
        } else {
            PoolStats::default()
        }
    }

    /// Called every few seconds by the HTTPS task.
    pub fn poll(&self, app: &App, http: &dyn HttpClient, link_up: bool) {
        let now = app.clock.millis();
        let backup = app.pool.on_backup();
        // Certificates are checked against the clock: no HTTPS before it is set.
        let online = link_up && app.clock.unix_time().is_some();
        let bitcoin = app.settings().coin().bitcoin_data();
        let (market_due, pool_due) = {
            let mut inner = self.inner.lock().unwrap();
            if backup != inner.last_backup {
                inner.last_backup = backup;
                // Do not wait up to half an hour after failover.
                inner.next_pool = now;
            }
            if !online && !inner.wait_logged && now > 90_000 {
                inner.wait_logged = true;
                app.log(if link_up {
                    "[market] waiting for the clock (NTP has not answered yet)"
                } else {
                    "[market] waiting for WiFi"
                });
            }
            (
                bitcoin && online && now.wrapping_sub(inner.next_market) as i32 >= 0,
                bitcoin && online && app.miner.stats().pool_connected && now.wrapping_sub(inner.next_pool) as i32 >= 0,
            )
        };
        if market_due {
            let ok = self.refresh(app, http);
            let mut inner = self.inner.lock().unwrap();
            let delay = if ok { REFRESH_MS } else { inner.retry };
            inner.next_market = app.clock.millis().wrapping_add(delay);
            inner.retry = if ok { RETRY_MS } else { (inner.retry * 2).min(REFRESH_MS) };
        }
        if pool_due {
            let ok = self.refresh_pool(app, http);
            self.inner.lock().unwrap().next_pool =
                app.clock.millis().wrapping_add(if ok { POOL_STATS_MS } else { POOL_STATS_RETRY_MS });
        }
    }

    /// One refresh of every endpoint due. Fields that fail keep their value.
    fn refresh(&self, app: &App, http: &dyn HttpClient) -> bool {
        if app.system.free_heap() < MIN_HEAP {
            app.log(&format!("[market] skipped, low memory ({} bytes free)", app.system.free_heap()));
            return false;
        }
        let t0 = app.clock.millis();
        let currency = app.settings().currency;
        let mut d = self.data.lock().unwrap().clone();
        let (mut ok, mut attempted) = (0, 0);
        for endpoint in 0..6 {
            let due = self.inner.lock().unwrap().endpoint_at[endpoint]
                .is_none_or(|at| app.clock.millis().wrapping_sub(at) >= REFRESH_MS);
            if !due {
                continue;
            }
            attempted += 1;
            let path = [
                "/v1/prices",
                "/v1/fees/recommended",
                "/v1/mining/hashrate/3d",
                "/v1/difficulty-adjustment",
                "/mempool",
                "/v1/blocks",
            ][endpoint];
            let mut blocks: Vec<Block> = Vec::new();
            let mut next = d.clone();
            let result = fetch_json(http, &format!("{API}{path}"), &mut |p, v| match (endpoint, p) {
                (0, p) if p == currency => next.price = v.number().unwrap_or(0.0),
                (1, "fastestFee") => next.fee_fast = v.number().unwrap_or(0.0) as u32,
                (1, "halfHourFee") => next.fee_half_hour = v.number().unwrap_or(0.0) as u32,
                (1, "hourFee") => next.fee_hour = v.number().unwrap_or(0.0) as u32,
                (2, "currentHashrate") => next.network_hashrate = v.number().unwrap_or(0.0),
                (3, "progressPercent") => next.retarget_progress = v.number().unwrap_or(0.0),
                (3, "difficultyChange") => next.retarget_change = v.number().unwrap_or(0.0),
                (3, "remainingBlocks") => next.retarget_blocks = v.number().unwrap_or(0.0) as u32,
                (4, "count") => next.mempool_tx = v.number().unwrap_or(0.0) as u32,
                (5, p) => {
                    let Some(rest) = p.strip_prefix('[') else { return };
                    let Some((index, field)) = rest.split_once("].") else { return };
                    let Ok(i) = index.parse::<usize>() else { return };
                    if i >= BLOCKS {
                        return;
                    }
                    while blocks.len() <= i {
                        blocks.push(Block::default());
                    }
                    let b = &mut blocks[i];
                    match (field, v) {
                        ("height", v) => b.height = v.number().unwrap_or(0.0) as u32,
                        ("timestamp", v) => b.time = v.number().unwrap_or(0.0) as u32,
                        ("tx_count", v) => b.txs = v.number().unwrap_or(0.0) as u32,
                        ("extras.pool.name", Scalar::Str(s)) => b.pool = s.chars().take(19).collect(),
                        _ => {}
                    }
                }
                _ => {}
            });
            match result {
                Ok(()) => {
                    if endpoint == 5 {
                        next.blocks = blocks;
                    }
                    d = next;
                    ok += 1;
                    self.inner.lock().unwrap().endpoint_at[endpoint] = Some(app.clock.millis());
                }
                Err(code) => app.log(&format!(
                    "[market] {path}: {}",
                    match code {
                        -1 => "no connection".to_string(),
                        -3 => "not JSON".to_string(),
                        c => format!("HTTP {c}"),
                    }
                )),
            }
        }
        if ok > 0 {
            d.updated_ms = app.clock.millis().max(1);
        }
        app.log(&format!(
            "[market] {ok}/{attempted} ok in {} ms | 1 BTC = {:.0} {currency} | fees {}/{}/{} | net {:.3e} H/s | retarget {:+.2}% in {}",
            app.clock.millis().wrapping_sub(t0),
            d.price,
            d.fee_fast,
            d.fee_half_hour,
            d.fee_hour,
            d.network_hashrate,
            d.retarget_change,
            d.retarget_blocks
        ));
        *self.data.lock().unwrap() = d;
        ok == attempted
    }

    fn query(&self, http: &dyn HttpClient, i: usize, host: &str, address: &str, p: &mut PoolStats) -> i32 {
        let api = &POOL_APIS[i];
        let target = if api.parent { parent_domain(host) } else { host.to_owned() };
        if target.is_empty() {
            return -1;
        }
        let url = format!("{}://{target}:{}{}{address}", if api.secure { "https" } else { "http" }, api.port, api.path);
        let mut body = crate::notify::Collect { body: Vec::new(), limit: 16 * 1024 };
        let headers = [("User-Agent", transport_agent()), ("Accept", "application/json")];
        let code = match http.get(&url, &headers, &mut body) {
            Ok(200) if parse_stats(&body.body, p) => 200,
            Ok(200) => -5,
            Ok(c) => c as i32,
            Err(_) => -1,
        };
        if code == 200 {
            p.source = format!("{}{target}:{}{}", if api.secure { "" } else { "http://" }, api.port, api.path);
        }
        code
    }

    /// Refreshes the stats of the pool in use; false to try again sooner.
    fn refresh_pool(&self, app: &App, http: &dyn HttpClient) -> bool {
        if app.system.free_heap() < MIN_HEAP {
            return false;
        }
        let backup = app.pool.on_backup();
        let slot = backup as usize;
        let host = app.pool_config.endpoint(backup).host.to_ascii_lowercase();
        let user = &app.pool_config.user;
        let address = user.split('.').next().unwrap_or("").to_owned();
        let mut p = {
            let (p, b) = self.pool.lock().unwrap().clone();
            if b == backup {
                p
            } else {
                PoolStats::default()
            }
        };
        let key = if backup { "backup" } else { "main" };
        {
            let mut inner = self.inner.lock().unwrap();
            let s = &mut inner.search[slot];
            if s.host != host {
                s.host = host.clone();
                s.index = load_api(app, key, &host);
                s.search_after = 0;
            }
        }
        let mut code = -1;
        let index = self.inner.lock().unwrap().search[slot].index;
        if let Some(i) = index {
            code = self.query(http, i, &host, &address, &mut p);
            if code == 404 || code == -5 {
                app.log(&format!("[market] pool stats API of {host} stopped answering ({code})"));
                self.inner.lock().unwrap().search[slot].index = None;
                app.storage.lock().unwrap().remove("poolapi", key);
            }
        }
        if self.inner.lock().unwrap().search[slot].index.is_none() {
            let accepted = app.miner.stats().accepted;
            let now = app.clock.millis();
            let wait = {
                let inner = self.inner.lock().unwrap();
                let s = &inner.search[slot];
                let new_share = s.retry_on_share && accepted != s.shares_at_search;
                s.search_after != 0 && (now.wrapping_sub(s.search_after) as i32) < 0 && !new_share
            };
            if wait {
                p.supported = false;
                *self.pool.lock().unwrap() = (p, backup);
                return true;
            }
            let mut address_unknown = false;
            #[allow(clippy::needless_range_loop)]
            for i in 0..POOL_APIS.len() {
                if code == 200 {
                    break;
                }
                if app.system.free_heap() < MIN_HEAP {
                    return false;
                }
                code = self.query(http, i, &host, &address, &mut p);
                address_unknown |= code == 404;
                if code == 200 {
                    self.inner.lock().unwrap().search[slot].index = Some(i);
                    app.storage.lock().unwrap().set("poolapi", key, format!("{host}|{i}").as_bytes());
                    app.log(&format!("[market] pool stats API found: {}", p.source));
                } else if code != -1
                    || !(if POOL_APIS[i].parent { parent_domain(&host) } else { host.clone() }).is_empty()
                {
                    let api = &POOL_APIS[i];
                    let target = if api.parent { parent_domain(&host) } else { host.clone() };
                    if !target.is_empty() {
                        app.log(&format!(
                            "[market] pool stats probe {}{target}:{}{}: {}",
                            if api.secure { "" } else { "http://" },
                            api.port,
                            api.path,
                            match code {
                                -5 => "JSON of an unknown API".to_string(),
                                -1 => "no connection".to_string(),
                                c => format!("HTTP {c}"),
                            }
                        ));
                    }
                }
            }
            if code != 200 {
                {
                    let mut inner = self.inner.lock().unwrap();
                    let s = &mut inner.search[slot];
                    s.shares_at_search = accepted;
                    s.retry_on_share = address_unknown;
                    s.search_after = app.clock.millis().wrapping_add(API_SEARCH_MS).max(1);
                }
                p.supported = false;
                *self.pool.lock().unwrap() = (p, backup);
                app.log(&format!(
                    "[market] no pool stats API found for {host}; searched again {}",
                    if address_unknown { "after its next accepted share, or in 6 h" } else { "in 6 h" }
                ));
                return true;
            }
        }
        p.supported = true;
        p.error = if code == 200 { 0 } else { code };
        if code == 200 {
            p.updated_ms = app.clock.millis().max(1);
            app.log(&format!(
                "[market] pool sees {:.3e} H/s, best {:.4e}, {} worker(s)",
                p.hashrate, p.best_ever, p.workers
            ));
        } else {
            app.log(&format!(
                "[market] pool stats from {}: error {code}",
                if p.source.is_empty() { &host } else { &p.source }
            ));
        }
        *self.pool.lock().unwrap() = (p, backup);
        code == 200
    }
}

/// The API found for a pool is kept across restarts ("host|index").
fn load_api(app: &App, key: &str, host: &str) -> Option<usize> {
    let saved = app.storage.lock().unwrap().get("poolapi", key)?;
    let saved = String::from_utf8(saved).ok()?;
    let (h, i) = saved.rsplit_once('|')?;
    let i: usize = i.parse().ok()?;
    (h == host && i < POOL_APIS.len()).then_some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_answers() {
        let mut p = PoolStats::default();
        assert!(parse_stats(
            br#"{"bestDifficulty":"12345.6","workersCount":2,"workers":[{"hashRate":"1000"},{"hashRate":500}],"accounting":{"totalAcceptedShares":42}}"#,
            &mut p
        ));
        assert_eq!((p.best_ever, p.workers, p.hashrate, p.shares), (12345.6, 2, 1500.0, 42.0));
        let mut c = PoolStats::default();
        assert!(parse_stats(
            br#"{"hashrate1hr":"856K","hashrate5m":"1.2M","bestever":99,"shares":7,"workers":1}"#,
            &mut c
        ));
        assert_eq!((c.hashrate, c.best_ever, c.shares, c.workers), (856_000.0, 99.0, 7.0, 1));
        let mut none = PoolStats::default();
        assert!(!parse_stats(br#"{"error":"not found"}"#, &mut none));
        assert!(!parse_stats(b"<html>", &mut none));
    }

    #[test]
    fn helpers() {
        assert_eq!(parse_rate("856K"), Some(856_000.0));
        assert_eq!(parse_rate("1.5 g"), Some(1.5e9));
        assert_eq!(parse_rate("x"), None);
        assert_eq!(parent_domain("eu.pool.example"), "pool.example");
        assert_eq!(parent_domain("pool.example"), "");
        assert_eq!(parent_domain("192.168.1.10"), "");
    }
}
