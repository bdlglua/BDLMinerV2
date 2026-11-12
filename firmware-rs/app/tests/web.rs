//! The dashboard API as the pages and other miners use it: routes, the
//! Host/Origin/password checks, live and restarting settings changes,
//! backups and metrics.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use bdlminer_app::app::{App, Platform};
use bdlminer_app::platform::{BoardProfile, Clock, MemoryStorage, Storage, SystemInfo};
use bdlminer_app::settings::{self, Settings};
use bdlminer_app::web::{handle, Method, Request, Response};
use serde_json::{json, Value};

const ADDRESS: &str = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";

struct TestClock(AtomicU32);
impl Clock for TestClock {
    fn millis(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
    fn unix_time(&self) -> Option<u32> {
        Some(1_800_000_000)
    }
    fn local_hour(&self) -> Option<u8> {
        Some(12)
    }
}

struct TestSystem;
impl SystemInfo for TestSystem {
    fn ip(&self) -> String {
        "192.168.1.20".into()
    }
    fn ssid(&self) -> String {
        "home".into()
    }
}

fn app_with(configure: impl FnOnce(&mut Settings)) -> Arc<App> {
    let board = BoardProfile::host();
    let mut storage = MemoryStorage::default();
    let mut s = Settings::defaults(&board);
    s.btc_address = ADDRESS.into();
    configure(&mut s);
    settings::save(&mut storage, &s, 0, &board).unwrap();
    app_on(Arc::new(Mutex::new(storage)))
}

/// An app on existing storage, as after a restart.
fn app_on(storage: Arc<Mutex<dyn Storage>>) -> Arc<App> {
    let app = App::new(Platform {
        board: BoardProfile::host(),
        clock: Arc::new(TestClock(AtomicU32::new(5000))),
        storage,
        system: Arc::new(TestSystem),
        console: Box::new(|_| {}),
        mac: 0x1a2b_0000_0000,
        boot_id: 0xabc,
        random: Arc::new(|b: &mut [u8]| b.fill(7)),
    });
    app.miner.load_stats();
    app
}

fn request<'a>(method: Method, path: &'a str, body: &'a [u8]) -> Request<'a> {
    Request { method, path, query: "", host: Some("192.168.1.20"), origin: None, authorization: None, body }
}

fn json_of(r: &Response) -> Value {
    serde_json::from_slice(&r.body).unwrap()
}

#[test]
fn pages_assets_and_unknown_routes() {
    let app = app_with(|_| {});
    let page = handle(&app, &request(Method::Get, "/", b""));
    assert_eq!(page.status, 200);
    let html = String::from_utf8(page.body.into_owned()).unwrap();
    assert!(html.starts_with("<!doctype html><html lang=\"en\">") && html.contains("/assets/dashboard.js?v="));
    for path in [
        "/assets/style.css",
        "/assets/common.js",
        "/assets/settings.js",
        "/assets/icon-192.png",
        "/manifest.webmanifest",
    ] {
        let r = handle(&app, &request(Method::Get, path, b""));
        assert!(r.status == 200 && !r.body.is_empty(), "{path}");
    }
    assert_eq!(handle(&app, &request(Method::Get, "/nothing", b"")).status, 404);
    assert_eq!(handle(&app, &request(Method::Post, "/", b"")).status, 404);
    assert_eq!(handle(&app, &request(Method::Get, "/api/events", b"")).status, 503);
}

#[test]
fn stats_hide_the_owner_from_other_sites() {
    let app = app_with(|_| {});
    let own = handle(&app, &request(Method::Get, "/api/stats", b""));
    let s = json_of(&own);
    for key in
        ["hashrate", "shares", "pool", "pool_diagnostics", "lifetime", "history", "market", "github_update", "engines"]
    {
        assert!(s.get(key).is_some(), "{key}");
    }
    assert_eq!(s["api_version"], 2);
    assert_eq!(s["address"], ADDRESS);
    assert_eq!(s["ip"], "192.168.1.20");
    assert_eq!(s["coin"], "BTC");
    assert_eq!(s["address_type"], "P2WPKH");
    assert_eq!(s["boot_id"], "abc");

    let mut foreign = request(Method::Get, "/api/stats", b"");
    foreign.origin = Some("http://example.com");
    let r = handle(&app, &foreign);
    let s = json_of(&r);
    assert!(s["address"] == "" && s["ip"] == "" && s["ssid"] == "");
    assert!(r.headers.contains(&("Access-Control-Allow-Origin", "*".into())));

    let mut peer = request(Method::Get, "/api/stats", b"");
    peer.origin = Some("http://192.168.1.41");
    let r = handle(&app, &peer);
    assert_eq!(json_of(&r)["address"], ADDRESS);
    assert!(r.headers.contains(&("Access-Control-Allow-Origin", "http://192.168.1.41".into())));

    let no_history = Request { query: "history=0", ..request(Method::Get, "/api/stats", b"") };
    assert!(json_of(&handle(&app, &no_history)).get("history").is_none());
}

#[test]
fn settings_need_the_own_page_and_the_password() {
    let app = app_with(|_| {});
    let mut rebinding = request(Method::Get, "/api/settings", b"");
    rebinding.host = Some("evil.example.com");
    assert_eq!(handle(&app, &rebinding).status, 403);
    let mut cross = request(Method::Post, "/api/settings", br#"{"bright":10}"#);
    cross.origin = Some("http://evil.example.com");
    assert_eq!(handle(&app, &cross).status, 403);
    let r = handle(&app, &request(Method::Get, "/api/settings", b""));
    assert_eq!(r.status, 200);
    let s = json_of(&r);
    assert_eq!(s["schema"].as_array().unwrap().len(), settings::table(&app.board).count() - 1); // no "uaauto"
    assert_eq!(s["values"]["addr"], ADDRESS);
    assert_eq!(s["portal"]["name"], "BDLMinerV2-1A2B");
    assert_eq!(s["portal"]["password"].as_str().unwrap().len(), 10);

    let locked = app_with(|s| s.web_password = "s3cret".into());
    let r = handle(&locked, &request(Method::Get, "/api/settings", b""));
    assert_eq!(r.status, 401);
    assert!(r.headers.iter().any(|(n, _)| *n == "WWW-Authenticate"));
    let wrong = Request { authorization: Some("Basic YWRtaW46d3Jvbmc="), ..request(Method::Get, "/api/log", b"") };
    assert_eq!(handle(&locked, &wrong).status, 401);
    // admin:s3cret
    let right = Request { authorization: Some("Basic YWRtaW46czNjcmV0"), ..request(Method::Get, "/api/settings", b"") };
    let r = handle(&locked, &right);
    assert_eq!(r.status, 200);
    assert_eq!(json_of(&r)["values"]["webpass"], settings::SECRET_MASK);
}

#[test]
fn live_and_restarting_changes() {
    let app = app_with(|_| {});
    let r = handle(&app, &request(Method::Post, "/api/settings", br#"{"bright":30,"power":60}"#));
    assert_eq!((r.status, json_of(&r)), (200, json!({"restart": false, "changed": 2})));
    assert!(app.brightness() == 30 && app.settings().mining_power == 60 && !app.restart_pending());

    let r = handle(&app, &request(Method::Post, "/api/settings", br#"{"bright":300}"#));
    assert_eq!(
        (r.status, String::from_utf8(r.body.into_owned()).unwrap().as_str()),
        (400, "Setting outside its allowed range")
    );
    assert_eq!(handle(&app, &request(Method::Post, "/api/settings", b"[1]")).status, 400);
    assert_eq!(handle(&app, &request(Method::Post, "/api/settings", b"{}")).status, 400);

    let r = handle(&app, &request(Method::Post, "/api/settings", br#"{"pool":"solo.ckpool.org:3333"}"#));
    assert_eq!(json_of(&r), json!({"restart": true, "changed": 2})); // host and port
    assert!(app.restart_pending());
    // Nothing else is saved over the staged configuration before the restart.
    assert_eq!(handle(&app, &request(Method::Post, "/api/settings", br#"{"bright":40}"#)).status, 409);
}

#[test]
fn backups_round_trip() {
    let app = app_with(|s| s.web_password = "pw".into());
    let auth = Some("Basic YWRtaW46cHc="); // admin:pw
    let r = handle(&app, &Request { authorization: auth, ..request(Method::Get, "/api/backup", b"") });
    assert_eq!(r.status, 200);
    let mut backup = json_of(&r);
    assert_eq!(backup["bdlminer_backup"], 1);
    assert_eq!(backup["settings"]["webpass"], "pw"); // protected by the password
    backup["settings"]["bright"] = 55.into();
    backup["stats"]["best"] = 12.5.into();
    backup["stats"]["top"] = json!([{"d": 3.0, "t": 0}, {"d": 12.5, "t": 1_800_000_000u32}]);
    let body = backup.to_string();
    let target = app_with(|s| s.web_password = "pw".into());
    let r = handle(&target, &Request { authorization: auth, ..request(Method::Post, "/api/backup", body.as_bytes()) });
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
    assert!(target.restart_pending());
    let mut store = target.storage.lock().unwrap();
    let mut saved = Settings::defaults(&target.board);
    settings::load(&mut *store, &mut saved, &target.board).unwrap();
    assert_eq!(saved.brightness, 55);
    drop(store);
    // The restored statistics are read at the next start, best first.
    let next = app_on(target.storage.clone());
    let top = next.miner.top_shares();
    assert!(top[0].difficulty == 12.5 && top[1].difficulty == 3.0);

    let bad = json!({"bdlminer_backup": 2, "settings": {}, "stats": {}}).to_string();
    let r = handle(&target, &Request { authorization: auth, ..request(Method::Post, "/api/backup", bad.as_bytes()) });
    assert_eq!(r.status, 409); // a restart is pending
    let fresh = app_with(|_| {});
    let r = handle(&fresh, &request(Method::Post, "/api/backup", bad.as_bytes()));
    assert_eq!(
        (r.status, String::from_utf8(r.body.into_owned()).unwrap().as_str()),
        (400, "Unsupported backup version")
    );
}

#[test]
fn metrics_and_diagnostics() {
    let app = app_with(|_| {});
    app.miner.register_engine("sw");
    let text = String::from_utf8(handle(&app, &request(Method::Get, "/metrics", b"")).body.into_owned()).unwrap();
    assert!(text.contains("# TYPE bdlminer_hashrate_hps gauge\nbdlminer_hashrate_hps 0\n"));
    assert!(text.contains("bdlminer_engine_hashrate_hps{engine=\"sw\",index=\"0\"} 0"));
    assert!(text.contains("bdlminer_info{board=\"Host\",firmware=\""));
    let d = json_of(&handle(&app, &request(Method::Get, "/api/pool/diagnostics", b"")));
    assert_eq!(d["code"], "disconnected");
    let d = json_of(&handle(&app, &request(Method::Get, "/api/diagnostics", b"")));
    assert_eq!(d["engines"][0]["name"], "sw");
    let h = json_of(&handle(&app, &request(Method::Get, "/api/history", b"")));
    assert!(h["interval_s"] == 300 && h["hashrate"].as_array().unwrap().is_empty());
    assert_eq!(handle(&app, &request(Method::Post, "/api/notify-test", b"")).status, 409); // nothing configured
    assert_eq!(handle(&app, &Request { query: "id=x", ..request(Method::Get, "/api/notify-test", b"") }).status, 400);
}

/// The history as the dashboard reads it, written without a JSON tree: one
/// entry per five minutes, nulls where nothing was measured.
#[test]
fn history_points() {
    let board = BoardProfile::host();
    let mut storage = MemoryStorage::default();
    let mut s = Settings::defaults(&board);
    s.btc_address = ADDRESS.into();
    settings::save(&mut storage, &s, 0, &board).unwrap();
    let clock = Arc::new(TestClock(AtomicU32::new(5000)));
    let app = App::new(Platform {
        board,
        clock: clock.clone(),
        storage: Arc::new(Mutex::new(storage)),
        system: Arc::new(TestSystem),
        console: Box::new(|_| {}),
        mac: 0x1a2b_0000_0000,
        boot_id: 0xabc,
        random: Arc::new(|b: &mut [u8]| b.fill(7)),
    });
    for step in 0..=3u32 {
        clock.0.store(5000 + step * 300_000, Ordering::Relaxed);
        app.miner.tick();
    }
    let r = handle(&app, &request(Method::Get, "/api/history", b""));
    assert_eq!(r.content_type, "application/json");
    let h = json_of(&r);
    assert_eq!(h["boot_id"], "abc");
    assert_eq!(h["interval_s"], 300);
    assert_eq!(h["revision"], 3);
    for key in ["hashrate", "temperature_c", "power_percent", "coverage_percent"] {
        assert_eq!(h[key].as_array().map(Vec::len), Some(3), "{key}");
    }
    // Nothing hashed or measured: no rate, temperature or power.
    assert_eq!(h["hashrate"][0], Value::Null);
    assert_eq!(h["temperature_c"][0], Value::Null);
    assert_eq!(h["coverage_percent"][0], 0);
}
