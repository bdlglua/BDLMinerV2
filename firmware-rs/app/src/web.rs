//! The local dashboard and JSON API, independent of the HTTP server: each
//! platform turns its requests into a `Request` and sends the `Response`.
//! The pages and scripts are the ones in web/, as the C++ firmware serves.

use std::borrow::Cow;
use std::sync::atomic::Ordering;
use std::sync::OnceLock;

use serde_json::{json, Map, Value};

use bdlminer_core::coin::Coin;
use bdlminer_core::pool_agent::NERD_MINER;
use bdlminer_core::sha256::Sha256;
use bdlminer_core::share_reason::RejectReason;
use bdlminer_core::{address, pool_diagnostics, work};

use crate::app::App;
use crate::miner::{StatsSnapshot, Throttle, HISTORY_INTERVAL_S, TOP_SHARES};
use crate::platform::{native_user_agent, FW_VERSION};
use crate::settings::{self, Kind};
use crate::web_validation as validation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Other,
}

#[derive(Clone, Debug)]
pub struct Request<'a> {
    pub method: Method,
    pub path: &'a str,
    pub query: &'a str,
    pub host: Option<&'a str>,
    pub origin: Option<&'a str>,
    pub authorization: Option<&'a str>,
    pub body: &'a [u8],
}

impl Request<'_> {
    pub fn arg(&self, name: &str) -> Option<String> {
        self.query.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (k == name).then(|| v.to_owned())
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub headers: Vec<(&'static str, String)>,
    /// The embedded pages and scripts are served from flash as they are:
    /// a copy of a 50 KB script can exceed the largest free block of an
    /// ESP32's heap, and a failed allocation restarts the firmware.
    pub body: Cow<'static, [u8]>,
}

impl Response {
    pub fn new(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Response { status, content_type, headers: Vec::new(), body: Cow::Owned(body.into()) }
    }
    fn text(status: u16, body: &str) -> Self {
        Self::new(status, "text/plain", body)
    }
    fn json(value: &Value) -> Self {
        Self::new(200, "application/json", value.to_string()).no_store()
    }
    fn no_store(mut self) -> Self {
        self.headers.push(("Cache-Control", "no-store".into()));
        self
    }
    fn header(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }
}

// The maintained sources of the pages (web/), embedded as they are.
const STYLE: &[u8] = include_bytes!("../../../web/style.css");
const COMMON: &[u8] = include_bytes!("../../../web/common.js");
const DASHBOARD_JS: &[u8] = include_bytes!("../../../web/dashboard.js");
const SETTINGS_JS: &[u8] = include_bytes!("../../../web/settings.js");
const LOG_JS: &[u8] = include_bytes!("../../../web/log.js");
const ICON_192: &[u8] = include_bytes!("../../../web/icon-192.png");
const ICON_512: &[u8] = include_bytes!("../../../web/icon-512.png");
const MANIFEST: &[u8] = include_bytes!("../../../web/manifest.webmanifest");
const HEADER: &str = include_str!("../../../web/header.html");
const DASHBOARD: &str = include_str!("../../../web/dashboard.html");
const SETTINGS_PAGE: &str = include_str!("../../../web/settings.html");
const LOG_PAGE: &str = include_str!("../../../web/log.html");

/// Cache-busting version of the assets, like tools/make_web.py's: hashed
/// once, piece by piece from flash (the ~170 KB of sources gathered in one
/// buffer exceeded an ESP32's largest free heap block and restarted it on
/// the first page load).
pub fn web_version() -> String {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION
        .get_or_init(|| {
            let sources: [(&str, &[u8]); 6] = [
                ("common.js", COMMON),
                ("dashboard.js", DASHBOARD_JS),
                ("settings.js", SETTINGS_JS),
                ("log.js", LOG_JS),
                ("style.css", STYLE),
                ("manifest.webmanifest", MANIFEST),
            ];
            let mut hash = Sha256::new();
            for (name, data) in sources {
                hash.update(name.as_bytes());
                hash.update(&[0]);
                hash.update(data);
            }
            work::to_hex(&hash.finish()[..8])
        })
        .clone()
}

fn page(app: &App, title: &str, body: &str, script: &str) -> Response {
    let lang = if app.settings().language == 1 { "fr" } else { "en" };
    let v = web_version();
    let html = format!(
        "<!doctype html><html lang=\"{lang}\"><head><title>{title}</title><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"color-scheme\" content=\"dark\">\
<meta name=\"theme-color\" content=\"#0a0d12\"><meta name=\"mobile-web-app-capable\" content=\"yes\"><meta name=\"apple-mobile-web-app-capable\" content=\"yes\"><meta name=\"apple-mobile-web-app-status-bar-style\" content=\"black\">\
<link rel=\"manifest\" href=\"/manifest.webmanifest\"><link rel=\"icon\" type=\"image/png\" href=\"/assets/icon-192.png\"><link rel=\"apple-touch-icon\" href=\"/assets/icon-192.png\">\
<link rel=\"stylesheet\" href=\"/assets/style.css?v={v}\"><script>const LANG='{lang}';</script><script defer src=\"/assets/common.js?v={v}\"></script>\
<script defer src=\"/assets/{script}.js?v={v}\"></script></head><body>{HEADER}{body}</div></div></body></html>"
    );
    Response::new(200, "text/html; charset=utf-8", html).no_store()
}

fn asset(mime: &'static str, data: &'static [u8]) -> Response {
    Response { status: 200, content_type: mime, headers: Vec::new(), body: Cow::Borrowed(data) }
        .header("Cache-Control", "public, max-age=31536000, immutable")
}

const ACCENTS: [&str; 5] = ["#f7931a", "#38d07a", "#3b9cf6", "#a66bf5", "#ec5aa0"];
const REFUSED: &str = "Refused: open the miner's own page (http://its address/) to do this";

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.trim_end_matches('=').bytes() {
        acc = acc << 6 | ALPHABET.iter().position(|&a| a == c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn own_host(app: &App, req: &Request) -> bool {
    app.rebinding.lock().unwrap().allowed(req.host, &app.hostname, app.clock.millis())
}

/// The live stream (/api/events) carries everything the miner's own pages
/// show: only to a request naming the miner itself.
pub fn events_allowed(app: &App, host: Option<&str>) -> bool {
    app.rebinding.lock().unwrap().allowed(host, &app.hostname, app.clock.millis())
}

/// One message of the live stream: the dashboard's readings.
pub fn event_message(app: &App) -> String {
    format!("data: {}\n\n", stats(app, false, true))
}

/// Settings, backups, firmware and the log: from the miner's own pages only,
/// then with the web password when one is set.
fn authorized(app: &App, req: &Request) -> Result<(), Response> {
    if !own_host(app, req) || !validation::origin_allowed(req.origin, req.host) {
        return Err(Response::text(403, REFUSED));
    }
    let password = app.settings().web_password;
    if password.is_empty() {
        return Ok(());
    }
    let expected = format!("admin:{password}");
    let given = req
        .authorization
        .and_then(|a| a.strip_prefix("Basic "))
        .and_then(|b| base64_decode(b.trim()))
        .unwrap_or_default();
    // Constant-time comparison of the credentials.
    let same =
        given.len() == expected.len() && given.iter().zip(expected.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0;
    if same {
        Ok(())
    } else {
        Err(Response::text(401, "Wrong password").header("WWW-Authenticate", "Basic realm=\"BDLMiner\""))
    }
}

fn number(x: f64) -> Value {
    serde_json::Number::from_f64(x).map_or(Value::Null, Value::Number)
}

pub fn pool_diagnostics(app: &App) -> Value {
    let stats = app.miner.stats();
    let status = app.pool.status();
    let input = pool_diagnostics::Input {
        connected: stats.pool_connected,
        work: stats.work_available,
        mask: stats.version_mask,
        required_bits: app.board.required_rolling_bits(),
        difficulty: stats.pool_diff,
        hashrate: app.miner.hashrate(),
        accepted_work: stats.accepted_work,
        seconds: app.miner.uptime_seconds() as f64,
        accepted: stats.accepted,
        status: &status,
    };
    let r = pool_diagnostics::diagnose(&input);
    json!({
        "code": r.issue.code(),
        "compatible": r.compatible,
        "connected": input.connected,
        "work_available": input.work,
        "protocol": app.pool_config.protocol_name(app.pool.on_backup()),
        "status": status,
        "version_rolling_bits": r.rolling_bits,
        "required_rolling_bits": input.required_bits,
        "difficulty": number(input.difficulty),
        "expected_share_seconds": if r.expected_share_seconds.is_finite() { number(r.expected_share_seconds) } else { Value::Null },
        "share_based_hashrate": if r.share_based_hashrate.is_finite() { number(r.share_based_hashrate) } else { Value::Null },
        "accepted_samples": input.accepted,
        "estimate_window_s": input.seconds as u32,
        "checked_ms": app.clock.millis(),
    })
}

/// What the dashboard shows; `history` adds the 24-hour chart. Without
/// `owner`, what names the owner or network is left empty.
pub fn stats(app: &App, history: bool, owner: bool) -> Value {
    let s = app.settings();
    let stats = app.miner.stats();
    let now = app.clock.millis();
    let engines: Vec<Value> = (0..stats.miner_count)
        .map(|i| json!({"name": stats.engine[i], "rate": number(app.miner.engine_rate(i))}))
        .collect();
    let reasons: Map<String, Value> =
        RejectReason::ALL.iter().map(|r| (r.name().to_owned(), stats.rejected_reasons[*r as usize].into())).collect();
    let mut j = json!({
        "board": app.board.name,
        "api_version": 2,
        "firmware": FW_VERSION,
        "boot_id": format!("{:x}", app.boot_id),
        "uptime_s": app.miner.uptime_seconds(),
        "hashrate": number(app.miner.hashrate()),
        "hashrate_live": number(app.miner.hashrate_live()),
        "firmware_env": app.board.env,
        "total_hashes": app.miner.total_hashes() as f64,
        "engines": engines,
        "shares": {"accepted": stats.accepted, "rejected": stats.rejected, "work": number(stats.accepted_work), "rejected_reasons": reasons},
        "pool_diagnostics": pool_diagnostics(app),
        "best": {"session": number(stats.session_best_diff), "ever": number(stats.best_diff)},
        "blocks": stats.blocks_found,
        "over32bits": stats.over32bits,
        "over64bits": stats.over64bits,
        "top": app.miner.top_shares().iter().map(|t| json!({"d": number(t.difficulty), "t": t.time})).collect::<Vec<_>>(),
        "network": {"height": stats.block_height, "difficulty": number(stats.network_diff)},
        "address": if owner { s.btc_address.clone() } else { String::new() },
        "coin": s.coin().ticker(),
        "coin_name": s.coin().name(),
        "algorithm": "SHA-256d",
        "address_type": address::parse(&s.btc_address, s.coin()).kind.name(),
        "lifetime": {"hashes": stats.lifetime_hashes as f64, "shares": stats.lifetime_shares, "blocks": stats.lifetime_blocks, "seconds": stats.lifetime_seconds},
        "history_revision": app.miner.history_revision(),
        "heap": app.system.free_heap(),
        "task_failures": 0,
        "heap_max_block": app.system.max_alloc(),
        "temperature_source": app.system.temperature_source(),
        "power_percent": app.miner.power_percent(),
        "power_limit_percent": s.mining_power,
        "max_temp_c": s.max_temp,
        "limitation_reason": app.watch.limit_reason(&app.miner, app.ota_busy.load(Ordering::Relaxed)),
        "night": app.watch.night(),
        "boot_reason": app.system.boot_reason(),
        "firmware_trial": app.firmware_trial.load(Ordering::Relaxed),
        "accent": ACCENTS[s.theme as usize % 5],
        "ip": if owner { app.system.ip() } else { String::new() },
        "ssid": if owner { app.system.ssid() } else { String::new() },
        "rssi": app.system.rssi(),
        "lang": if s.language == 1 { "fr" } else { "en" },
    });
    let o = j.as_object_mut().unwrap();
    if owner {
        if let Some(crash) = app.crashes.json() {
            o.insert("crash".into(), crash);
        }
    }
    if let Some(c) = app.miner.last_block_candidate() {
        let chain = if c.coin == Coin::Btc { app.block_status.get(&c.hash) } else { None };
        let valid = chain.as_ref().filter(|s| s.known && s.error == 0);
        let mut b = json!({
            "height": c.height,
            "coin": c.coin.ticker(),
            "hash": work::reversed_hex(&c.hash),
            "status": c.state.name(),
            "previous_session": c.previous_session,
            "chain_confirmed": valid.map_or(Value::Null, |s| s.in_best_chain.into()),
            "chain_source": if c.coin == Coin::Btc { "mempool.space" } else { "" },
            "confirmations": valid.map_or(0, |s| s.confirmations),
            "chain_checked_s": match &chain {
                Some(s) if s.checked_ms != 0 => (app.clock.millis().wrapping_sub(s.checked_ms) / 1000) as i64,
                _ => -1,
            },
        });
        if let Some(s) = &chain {
            if s.not_found {
                b["chain_not_found"] = true.into();
            }
            if s.error != 0 {
                b["chain_error"] = s.error.into();
            }
        }
        o.insert("block_candidate".into(), b);
    }
    let (connected_ms, sessions) = app.miner.pool_uptime();
    let backup = app.pool.on_backup();
    let ep = app.pool_config.endpoint(backup);
    o.insert(
        "pool".into(),
        json!({
            "host": ep.host,
            "port": ep.port,
            "backup": backup,
            "connected": stats.pool_connected,
            "status": app.pool.status(),
            "difficulty": number(stats.pool_diff),
            "pending_shares": app.pool.pending(),
            "user_agent": app.pool.user_agent(),
            "protocol": app.pool_config.protocol_name(backup),
            "user_agent_auto": s.user_agent_auto,
            "connected_percent": if now != 0 { (1000.0 * connected_ms as f64 / now as f64).round() / 10.0 } else { 0.0 },
            "sessions": sessions,
            "failovers": app.pool.failovers(),
        }),
    );
    let log = app.miner.share_log();
    if !log.is_empty() {
        let entries: Vec<Value> = log
            .iter()
            .map(|e| {
                let mut v = json!({"t": e.time, "d": e.difficulty, "s": e.state.name()});
                if e.backup {
                    v["b"] = true.into();
                }
                v
            })
            .collect();
        o.insert("shares_log".into(), entries.into());
    }
    let now = app.clock.millis();
    let age = |updated: u32| if updated == 0 { -1 } else { (now.wrapping_sub(updated) / 1000) as i64 };
    let pv = app.market.pool_stats(app);
    let mut view = json!({"updated_s": age(pv.updated_ms), "supported": pv.supported, "hashrate": number(pv.hashrate),
                          "best": number(pv.best_ever), "shares": number(pv.shares), "workers": pv.workers});
    if !pv.source.is_empty() {
        view["source"] = pv.source.into();
    }
    if pv.error != 0 {
        view["error"] = pv.error.into();
    }
    o.insert("pool_view".into(), view);
    if history {
        let (points, _) = app.miner.history();
        let rates: Vec<Value> = points
            .iter()
            .map(|p| if p.coverage_percent != 0 || p.hashrate != 0.0 { number(p.hashrate as f64) } else { Value::Null })
            .collect();
        o.insert("history".into(), json!({"interval_s": HISTORY_INTERVAL_S, "hashrate": rates}));
    }
    let t = app.system.temperature();
    o.insert("temp_c".into(), if t.is_finite() { (t.round() as i64).into() } else { Value::Null });
    let throttle = match app.miner.throttle() {
        Throttle::Paused => "paused",
        Throttle::Half => "half",
        Throttle::Reduced => "reduced",
        Throttle::Full => "full",
    };
    o.insert("throttle".into(), throttle.into());
    let ns = app.notifier.status();
    let mut notify = json!({"enabled": ns.enabled});
    if ns.id != 0 {
        notify["title"] = ns.title.into();
        notify["ntfy"] = ns.ntfy.into();
        notify["webhook"] = ns.webhook.into();
    }
    let mut integrations = json!({"mqtt": app.mqtt.name(app), "notify": notify});
    if owner && !s.mqtt_host.is_empty() {
        integrations["mqtt_host"] = format!("{}:{}", s.mqtt_host, s.mqtt_port).into();
    }
    o.insert("integrations".into(), integrations);
    let peers: Vec<Value> = if owner {
        s.peers.split(',').map(str::trim).filter(|p| !p.is_empty()).map(|p| p.into()).collect()
    } else {
        Vec::new()
    };
    o.insert("peers".into(), peers.into());
    let m = app.market.get(app);
    let now = app.clock.millis();
    o.insert(
        "market".into(),
        json!({"updated_s": if m.updated_ms == 0 { -1 } else { (now.wrapping_sub(m.updated_ms) / 1000) as i64 },
               "currency": s.currency, "price": number(m.price),
               "fees": [m.fee_fast, m.fee_half_hour, m.fee_hour], "hashrate": number(m.network_hashrate),
               "mempool_tx": m.mempool_tx,
               "retarget": {"change": number(m.retarget_change), "progress": number(m.retarget_progress),
                            "blocks": m.retarget_blocks}}),
    );
    let bests = app.miner.block_bests();
    let mut blocks = Vec::new();
    if stats.block_height != 0 {
        let mut b = json!({"h": stats.block_height, "mining": true});
        if let Some(best) = bests.iter().find(|b| b.height == stats.block_height) {
            b["best"] = number(best.best);
        }
        blocks.push(b);
    }
    for block in &m.blocks {
        let mut b = json!({"h": block.height, "t": block.time, "pool": block.pool, "tx": block.txs});
        if let Some(best) = bests.iter().find(|b| b.height == block.height) {
            b["best"] = number(best.best);
        }
        blocks.push(b);
    }
    o.insert("blocks_recent".into(), blocks.into());
    let channel = if s.update_channel == 1 { "beta" } else { "stable" };
    let gu = app.updater.state(app.clock.millis());
    let release_url = if gu.version.is_empty() {
        format!("https://github.com/{}/releases", settings::GITHUB_REPO)
    } else {
        format!("https://github.com/{}/releases/tag/rust-v{}", settings::GITHUB_REPO, gu.version)
    };
    let notes = if gu.installing { format!("{} %", gu.progress) } else { gu.error.clone() };
    o.insert(
        "github_update".into(),
        json!({"available": gu.available, "mode": s.github_update, "channel": channel, "checked": gu.checked,
               "checking": gu.checking, "request_id": gu.install_request, "version": gu.version,
               "installing": gu.installing, "progress": gu.progress, "retry_in_s": gu.retry_seconds,
               "supported": app.system.ota_supported(), "release_url": release_url,
               "error": gu.error, "notes": notes}),
    );
    j
}

/// Written straight into the response: a `Value` tree of 4 x 288 points
/// took about 30 KB of an ESP32's heap at once, the text about 6 KB.
fn history(app: &App) -> Response {
    use std::fmt::Write;
    let (points, revision) = app.miner.history();
    let mut body = String::with_capacity(96 + points.len() * 22);
    let _ =
        write!(body, "{{\"boot_id\":\"{:x}\",\"interval_s\":{HISTORY_INTERVAL_S},\"revision\":{revision}", app.boot_id);
    let mut field = |name: &str, value: &dyn Fn(&crate::miner::HistoryPoint) -> Option<String>| {
        let _ = write!(body, ",\"{name}\":[");
        for (i, p) in points.iter().enumerate() {
            if i > 0 {
                body.push(',');
            }
            body.push_str(value(p).as_deref().unwrap_or("null"));
        }
        body.push(']');
    };
    field("hashrate", &|p| {
        (p.coverage_percent != 0 || p.hashrate != 0.0).then(|| (p.hashrate.round() as i64).to_string())
    });
    field("temperature_c", &|p| {
        (p.temperature_deci_c != i16::MIN)
            .then(|| number((p.temperature_deci_c as f64 / 10.0 * 10.0).round() / 10.0).to_string())
    });
    field("power_percent", &|p| (p.coverage_percent != 0).then(|| p.power_percent.to_string()));
    field("coverage_percent", &|p| Some(p.coverage_percent.to_string()));
    body.push('}');
    Response::new(200, "application/json", body).no_store()
}

fn diagnostics(app: &App) -> Response {
    let stats = app.miner.stats();
    let profiles = app.miner.profiles();
    let engines: Vec<Value> = (0..stats.miner_count)
        .filter(|&i| stats.engine[i] != "fill")
        .map(|i| {
            let p = profiles[i];
            json!({"index": i, "name": stats.engine[i], "hashing_ms": p.hashing_us / 1000, "rest_ms": p.rest_ms,
                   "waiting_ms": p.waiting_ms, "paused_ms": p.paused_ms})
        })
        .collect();
    Response::json(&json!({
        "uptime_s": app.miner.uptime_seconds(),
        "firmware": FW_VERSION,
        "boot_id": format!("{:x}", app.boot_id),
        "tls": {"handshakes": 0, "failures": 0, "total_ms": 0, "last_ms": 0},
        "engines": engines,
    }))
}

/// Written piece by piece: a `Value` tree of the whole answer (a hundred
/// settings with their schema) took tens of KB of an ESP32's heap at once,
/// next to the HTTPS tasks, and the M5StickC Plus2 restarted when the
/// settings page opened.
fn get_settings(app: &App) -> Response {
    let board = &app.board;
    let mut capabilities = json!({
        "ota": app.system.ota_supported(),
        "firmware_env": board.env,
        "display_pages": board.display_pages,
        "thermal_steps": !board.asic(),
        "user_agents": [NERD_MINER, native_user_agent()],
    });
    if board.gamma() {
        capabilities["fan_control"] = true.into();
    }
    if board.asic() {
        capabilities["asic"] = true.into();
    }
    let portal = json!({"name": crate::app::portal_ap_name(app.mac), "password": app.portal_password()});
    let mut out = String::with_capacity(8 * 1024);
    out.push_str("{\"capabilities\":");
    out.push_str(&capabilities.to_string());
    out.push_str(",\"portal\":");
    out.push_str(&portal.to_string());
    out.push_str(",\"schema\":[");
    for (i, info) in settings::table(board).filter(|s| s.key != "uaauto").enumerate() {
        let limits = match info.kind {
            Kind::Number(_) => {
                let l = settings::limits_for(info, board);
                json!({"min": l.minimum, "max": l.maximum})
            }
            Kind::Decimal => json!({"min": 0}),
            _ => json!({}),
        };
        if i > 0 {
            out.push(',');
        }
        out.push_str(&json!([info.key, info.kind.name(), info.group, info.label, limits]).to_string());
    }
    out.push_str("],\"values\":");
    {
        let mut current = app.settings();
        current.brightness = app.brightness();
        out.push_str(&Value::Object(settings::values(&current, board, false)).to_string());
    }
    let live: Vec<&str> = settings::table(board).map(|s| s.key).filter(|k| settings::is_live(k)).collect();
    out.push_str(",\"live\":");
    out.push_str(&json!(live).to_string());
    out.push('}');
    Response::new(200, "application/json", out).no_store()
}

fn parse_object(body: &[u8]) -> Option<Map<String, Value>> {
    match serde_json::from_slice(body) {
        Ok(Value::Object(m)) => Some(m),
        _ => None,
    }
}

fn post_settings(app: &App, req: &Request) -> Response {
    if app.restart_pending() || app.ota_busy.load(Ordering::Relaxed) {
        return Response::text(409, "Firmware update or restart pending; try again afterwards");
    }
    let Some(mut values) = parse_object(req.body) else { return Response::text(400, "Not a settings object") };
    settings::keep_secrets(&mut values, &app.board);
    let mut current = app.settings();
    current.brightness = app.brightness();
    if let Some(e) = settings::error(&values, &current, &app.board) {
        return Response::text(400, e);
    }
    let mut pending = current.clone();
    if settings::apply(&mut pending, &values, &app.board) == 0 {
        return Response::text(400, "No settings to save");
    }
    settings::sanitize(&mut pending, &app.board);
    if pending.pool_host.is_empty() || pending.btc_address.is_empty() {
        return Response::text(400, "Pool host and payout address cannot be empty");
    }
    let change = settings::compare(&pending, &current, &app.board);
    let restart = change.count != 0 && !change.live;
    if change.count != 0 && !app.save_settings(&pending, restart) {
        return Response::text(500, "Settings could not be saved. Previous configuration kept; try again.");
    }
    if restart {
        app.log(&format!("[web] {} settings saved, restarting", change.count));
        app.request_restart("settings changed from the web page");
    } else if change.count != 0 {
        app.apply_live(&pending);
        if pending.brightness != current.brightness {
            app.set_brightness(pending.brightness);
        }
        app.log(&format!("[web] {} settings applied without restarting", change.count));
    }
    Response::new(200, "application/json", format!("{{\"restart\":{restart},\"changed\":{}}}", change.count))
}

fn get_backup(app: &App) -> Response {
    let s = app.settings();
    let snapshot = app.miner.stats_snapshot();
    let top: Vec<Value> = snapshot
        .top
        .iter()
        .take_while(|t| t.difficulty > 0.0)
        .map(|t| json!({"d": number(t.difficulty), "t": t.time}))
        .collect();
    let body = json!({
        "bdlminer_backup": 1,
        "board": app.board.name,
        "firmware": FW_VERSION,
        "date": app.clock.unix_time().unwrap_or(0),
        // Without a web password anyone on the network could download it.
        "settings": settings::values(&s, &app.board, !s.web_password.is_empty()),
        "stats": {"best": number(snapshot.best_diff), "hashes": snapshot.hashes, "shares": snapshot.shares,
                  "blocks": snapshot.blocks, "seconds": snapshot.seconds, "top": top},
    });
    Response::json(&body)
        .header("Content-Disposition", format!("attachment; filename=\"{}-backup.json\"", app.hostname))
}

fn post_backup(app: &App, req: &Request) -> Response {
    if app.restart_pending() || app.ota_busy.load(Ordering::Relaxed) {
        return Response::text(409, "Firmware update or restart pending; try again afterwards");
    }
    let Some(doc) = parse_object(req.body).map(Value::Object) else { return Response::text(400, "Not a backup file") };
    if let Some(e) = validation::backup_error(&doc, TOP_SHARES) {
        return Response::text(400, e);
    }
    let full = doc.get("bdlminer_backup").is_some();
    let mut values =
        if full { doc["settings"].as_object().cloned().unwrap_or_default() } else { doc.as_object().cloned().unwrap() };
    settings::keep_secrets(&mut values, &app.board);
    settings::migrate_backup_coin(&mut values, full);
    settings::migrate_backup_agent(&mut values);
    let mut current = app.settings();
    current.brightness = app.brightness();
    if let Some(e) = settings::error(&values, &current, &app.board) {
        return Response::text(400, e);
    }
    let mut pending = current;
    let count = settings::apply(&mut pending, &values, &app.board);
    if count != 0 {
        settings::sanitize(&mut pending, &app.board);
        if pending.pool_host.is_empty() || pending.btc_address.is_empty() {
            return Response::text(400, "Pool host and payout address cannot be empty");
        }
        if !app.save_settings(&pending, true) {
            return Response::text(500, "Settings could not be saved. Previous configuration kept; try again.");
        }
    }
    let with_stats = full && doc.get("stats").is_some_and(Value::is_object);
    if with_stats {
        let st = &doc["stats"];
        let mut s = StatsSnapshot {
            best_diff: st["best"].as_f64().unwrap_or(0.0),
            hashes: st["hashes"].as_u64().unwrap_or(0),
            shares: st["shares"].as_u64().unwrap_or(0) as u32,
            blocks: st["blocks"].as_u64().unwrap_or(0) as u32,
            seconds: st["seconds"].as_u64().unwrap_or(0) as u32,
            top: Default::default(),
        };
        let mut top: Vec<_> = st["top"]
            .as_array()
            .map(|a| {
                a.iter()
                    .take(TOP_SHARES)
                    .map(|o| crate::miner::BestShare {
                        difficulty: o["d"].as_f64().unwrap_or(0.0),
                        time: o["t"].as_u64().unwrap_or(0) as u32,
                    })
                    .collect()
            })
            .unwrap_or_default();
        top.sort_by(|a, b| b.difficulty.total_cmp(&a.difficulty));
        s.top[..top.len()].copy_from_slice(&top);
        if !app.miner.stats_restore(&s) {
            return Response::text(
                500,
                "Settings saved, but statistics could not be restored. Retry before restarting.",
            );
        }
    }
    if count == 0 && !with_stats {
        return Response::text(400, "Nothing to restore in this file");
    }
    app.log(&format!(
        "[web] backup restored: {count} settings{}, restarting",
        if with_stats { " and statistics" } else { "" }
    ));
    app.request_restart("backup restored from the web page");
    Response::text(
        200,
        &format!(
            "Restored {count} settings{}, restarting\u{2026} reload in 15 s",
            if with_stats { " and the statistics" } else { "" }
        ),
    )
}

fn metrics(app: &App) -> Response {
    let stats = app.miner.stats();
    let mut out = String::with_capacity(1536);
    let mut metric = |name: &str, kind: Option<&str>, value: f64, labels: &str| {
        if let Some(kind) = kind {
            out += &format!("# TYPE {name} {kind}\n");
        }
        out += &format!("{name}{labels} {value}\n");
    };
    let g = Some("gauge");
    let c = Some("counter");
    metric("bdlminer_hashrate_hps", g, app.miner.hashrate(), "");
    metric("bdlminer_hashes_total", c, app.miner.total_hashes() as f64, "");
    metric("bdlminer_shares_accepted_total", c, stats.accepted as f64, "");
    metric("bdlminer_shares_rejected_total", c, stats.rejected as f64, "");
    metric("bdlminer_blocks_found_total", c, stats.blocks_found as f64, "");
    metric("bdlminer_block_candidates_total", c, stats.blocks_found as f64, "");
    metric("bdlminer_best_difficulty", g, stats.session_best_diff, "");
    metric("bdlminer_best_difficulty_ever", g, stats.best_diff, "");
    metric("bdlminer_shares_32bit_total", c, stats.over32bits as f64, "");
    metric("bdlminer_shares_64bit_total", c, stats.over64bits as f64, "");
    metric("bdlminer_lifetime_hashes_total", c, stats.lifetime_hashes as f64, "");
    metric("bdlminer_lifetime_shares_total", c, stats.lifetime_shares as f64, "");
    metric("bdlminer_lifetime_seconds_total", c, stats.lifetime_seconds as f64, "");
    metric("bdlminer_pool_connected", g, stats.pool_connected as u8 as f64, "");
    metric("bdlminer_pool_backup", g, app.pool.on_backup() as u8 as f64, "");
    metric("bdlminer_pool_difficulty", g, stats.pool_diff, "");
    metric("bdlminer_block_height", g, stats.block_height as f64, "");
    metric("bdlminer_network_difficulty", g, stats.network_diff, "");
    metric("bdlminer_uptime_seconds", g, app.miner.uptime_seconds() as f64, "");
    let t = app.system.temperature();
    if t.is_finite() {
        metric("bdlminer_temperature_celsius", g, t as f64, "");
    }
    metric("bdlminer_throttle", g, app.miner.throttle() as u8 as f64, "");
    metric("bdlminer_power_percent", g, app.miner.power_percent() as f64, "");
    metric("bdlminer_wifi_rssi_dbm", g, app.system.rssi() as f64, "");
    metric("bdlminer_free_heap_bytes", g, app.system.free_heap() as f64, "");
    for i in 0..stats.miner_count {
        let labels = format!("{{engine=\"{}\",index=\"{i}\"}}", stats.engine[i]);
        metric("bdlminer_engine_hashrate_hps", if i == 0 { g } else { None }, app.miner.engine_rate(i), &labels);
    }
    metric("bdlminer_info", g, 1.0, &format!("{{board=\"{}\",firmware=\"{FW_VERSION}\"}}", app.board.name));
    Response::new(200, "text/plain; version=0.0.4", out).no_store()
}

/// Routes one request.
pub fn handle(app: &App, req: &Request) -> Response {
    use Method::*;
    let v = |r: Result<(), Response>| r.err();
    match (req.method, req.path) {
        (Get, "/assets/style.css") => asset("text/css; charset=utf-8", STYLE),
        (Get, "/assets/common.js") => asset("text/javascript; charset=utf-8", COMMON),
        (Get, "/assets/dashboard.js") => asset("text/javascript; charset=utf-8", DASHBOARD_JS),
        (Get, "/assets/settings.js") => asset("text/javascript; charset=utf-8", SETTINGS_JS),
        (Get, "/assets/log.js") => asset("text/javascript; charset=utf-8", LOG_JS),
        (Get, "/assets/icon-192.png") | (Get, "/favicon.ico") => asset("image/png", ICON_192),
        (Get, "/assets/icon-512.png") => asset("image/png", ICON_512),
        (Get, "/manifest.webmanifest") => asset("application/manifest+json", MANIFEST),
        (Get, "/") => page(app, "BDLMiner V2", DASHBOARD, "dashboard"),
        (Get, "/settings") => page(app, "BDLMiner V2 settings", SETTINGS_PAGE, "settings"),
        (Get, "/log") => page(app, "BDLMiner V2 log", LOG_PAGE, "log"),
        (Get, "/api/stats") => {
            // Other miners' dashboards on the local network get everything;
            // any other site gets the readings only.
            let local = req.origin.is_none_or(|o| o.is_empty() || validation::origin_local(Some(o)));
            let owner = own_host(app, req) && local;
            let mut r = Response::json(&stats(app, req.arg("history").as_deref() != Some("0"), owner));
            if let Some(origin) = req.origin.filter(|o| !o.is_empty()) {
                r = r.header("Access-Control-Allow-Origin", if local { origin.to_owned() } else { "*".into() });
                if local {
                    r = r.header("Vary", "Origin");
                }
            }
            r
        }
        // Served by the ESP32's server itself (a long-lived connection);
        // elsewhere the pages poll instead.
        (Get, "/api/events") => Response::text(503, "Too many live dashboards; polling instead"),
        (Get, "/api/history") => history(app),
        (Get, "/api/diagnostics") => diagnostics(app),
        (Get, "/api/pool/diagnostics") => Response::json(&pool_diagnostics(app)),
        (Get, "/api/log") => {
            v(authorized(app, req)).unwrap_or_else(|| Response::new(200, "text/plain", app.log.text()).no_store())
        }
        (Get, "/api/settings") => v(authorized(app, req)).unwrap_or_else(|| get_settings(app)),
        (Post, "/api/settings") => v(authorized(app, req)).unwrap_or_else(|| post_settings(app, req)),
        (Get, "/api/backup") => v(authorized(app, req)).unwrap_or_else(|| get_backup(app)),
        (Post, "/api/backup") => v(authorized(app, req)).unwrap_or_else(|| post_backup(app, req)),
        (Post, "/api/notify-test") => v(authorized(app, req)).unwrap_or_else(|| {
            if !app.notifier.status().enabled {
                Response::text(409, "Set an ntfy topic or a webhook URL, save, then try again")
            } else {
                match app.notifier.test() {
                    0 => Response::text(503, "Notifications are busy; try again shortly"),
                    id => Response::new(202, "application/json", format!("{{\"id\":{id}}}")),
                }
            }
        }),
        (Get, "/api/notify-test") => match req.arg("id").map(|a| a.parse::<u32>()) {
            Some(Err(_)) => Response::text(400, "Invalid notification test ID"),
            id => Response::json(&app.notifier.result(id.and_then(Result::ok).unwrap_or(0))),
        },
        (Post, "/api/restart") => v(authorized(app, req)).unwrap_or_else(|| {
            app.request_restart("restart requested from the web page");
            Response::text(200, "Restarting\u{2026}")
        }),
        // Only looks for a newer release, so no password: the installation
        // asks for it. One requested check per 30 s at most, so that nobody
        // on the network can keep the miner busy with TLS handshakes.
        (Post, "/api/github-check") => github_check(app, req),
        (Post, "/api/github-update") => v(authorized(app, req)).unwrap_or_else(|| {
            if app.updater.request_install(app) {
                Response::text(200, "Installing, the miner restarts by itself")
            } else {
                Response::text(409, "No update to install")
            }
        }),
        // The firmware upload streams into the other slot: the server reads
        // it itself (`update_allowed`, then `multipart`).
        (Post, "/update") => Response::text(400, "No firmware file uploaded"),
        // The dump itself is streamed by the server (`owner_allowed`).
        (Get, "/api/coredump") => v(authorized(app, req)).unwrap_or_else(|| Response::text(404, "No crash report")),
        (Post, "/api/coredump/erase") => v(authorized(app, req)).unwrap_or_else(|| {
            if app.crashes.erase(app) {
                Response::text(200, "Crash report erased")
            } else {
                Response::text(500, "The crash report could not be erased")
            }
        }),
        (Get, "/metrics") => metrics(app),
        _ => Response::text(404, "Not found"),
    }
}

fn github_check(app: &App, req: &Request) -> Response {
    static LAST: std::sync::Mutex<Option<u32>> = std::sync::Mutex::new(None);
    if !own_host(app, req) || !validation::origin_allowed(req.origin, req.host) {
        return Response::text(403, REFUSED);
    }
    if app.settings().github_update == 0 {
        return Response::text(409, "GitHub updates are disabled in settings");
    }
    let now = app.clock.millis();
    let mut last = LAST.lock().unwrap();
    if last.is_some_and(|t| now.wrapping_sub(t) < 30_000) {
        return Response::text(202, "Check requested");
    }
    if app.updater.request_check(app) {
        *last = Some(now);
        return Response::text(202, "Check requested");
    }
    // A check already queued or running answers this request as well.
    if app.updater.state(now).installing || app.ota_busy.load(Ordering::Relaxed) {
        Response::text(409, "An update is being installed")
    } else {
        Response::text(202, "Check requested")
    }
}

/// Settings, backups, firmware, the crash report and the log: from the
/// miner's own pages, with the password.
pub fn owner_allowed(app: &App, req: &Request) -> Result<(), Response> {
    authorized(app, req)
}

/// Whether a firmware upload (POST /update) may start: from the miner's own
/// page, with the password, nothing else pending.
pub fn update_allowed(app: &App, req: &Request) -> Result<(), Response> {
    authorized(app, req)?;
    if app.restart_pending() || app.ota_busy.load(Ordering::Relaxed) {
        return Err(Response::text(409, "Another firmware update or restart is already pending"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::base64_decode;

    #[test]
    fn decodes_basic_credentials() {
        assert_eq!(base64_decode("YWRtaW46c2VjcmV0").unwrap(), b"admin:secret");
        assert_eq!(base64_decode("YQ==").unwrap(), b"a");
        assert!(base64_decode("*").is_none());
    }
}
