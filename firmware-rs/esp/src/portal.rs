//! The setup portal: an access point "BDLMinerV2-XXXX" with a small form at
//! http://192.168.4.1 for the WiFi network, the pool and the payout address.
//! Saved, the miner joins the network and restarts. Restarts 3 minutes after
//! the last sign of use without a configuration, as WiFiManager does in the
//! C++ firmware.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bdlminer_app::settings;
use bdlminer_app::App;
use esp_idf_svc::http::server::{Configuration, EspHttpServer};
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;
use esp_idf_svc::wifi::{AccessPointConfiguration, AuthMethod, BlockingWifi, Configuration as Wifi, EspWifi};
use serde_json::{Map, Value};

use crate::system::millis;
use crate::{captive, wifi};

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(v) => {
                        out.push(v);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

/// The networks in range as buttons that fill in the WiFi network field.
fn network_list(networks: &[captive::Network]) -> String {
    if networks.is_empty() {
        return "<p>No network found. <a href='/'>Scan again</a></p>".into();
    }
    let mut html = String::from("<p>Networks in range (<a href='/'>scan again</a>):</p>");
    for n in networks {
        let quality = (2 * (i32::from(n.rssi) + 100)).clamp(0, 100);
        let name = escape(&n.ssid);
        html.push_str(&format!(
            "<button type='button' class='w' data-s='{name}' onclick='pick(this)'>{name}<span>{}{quality}%</span></button>",
            if n.secured { "&#128274; " } else { "" }
        ));
    }
    html
}

fn form(app: &App, message: &str, networks: &[captive::Network]) -> String {
    let s = app.settings();
    let field = |name: &str, label: &str, value: &str, kind: &str| {
        format!("<label>{label}<input name='{name}' type='{kind}' value='{}'></label>", escape(value))
    };
    format!(
        "<!doctype html><html><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'>\
<title>BDLMiner V2 setup</title><style>body{{font-family:system-ui;background:#0a0d12;color:#e8e8e8;max-width:28rem;margin:auto;padding:1rem}}\
label{{display:block;margin:.6rem 0}}input{{display:block;width:100%;padding:.5rem;margin-top:.2rem;box-sizing:border-box}}\
button{{padding:.7rem 1.2rem;background:#f7931a;border:0;font-weight:700}}p.m{{color:#f7931a}}a{{color:#f7931a}}\
button.w{{display:flex;justify-content:space-between;width:100%;margin:.3rem 0;padding:.6rem;background:#1b2230;color:#e8e8e8;font-weight:400;text-align:left}}</style>\
<script>function pick(b){{var f=document.forms[0];f.ssid.value=b.dataset.s;f.wpass.focus()}}</script></head><body>\
<h1>BDLMiner V2</h1><p class='m'>{}</p>{}<form method='post' action='/save'>{}{}\
<label>Cryptocurrency (SHA-256d: 0 BTC, 1 BCH, 2 BSV, 3 XEC, 4 DGB, 5 PPC)<input name='coin' type='number' min='0' max='5' value='{}'></label>\
{}{}{}{}<label>Protocol: 0 Stratum V1, 1 Stratum V2 (Noise)<input name='proto' type='number' min='0' max='1' value='{}'></label>{}\
<button type='submit'>Save and connect</button></form></body></html>",
        escape(message),
        network_list(networks),
        field("ssid", "WiFi network", &wifi::saved_station().map(|w| w.0).unwrap_or_default(), "text"),
        field("wpass", "WiFi password", "", "password"),
        s.mining_coin,
        field("pool", "Pool host", &s.pool_host, "text"),
        field("port", "Pool port", &s.pool_port.to_string(), "number"),
        field("addr", "Payout address or pool account (optionally .worker)", &s.btc_address, "text"),
        field("pass", "Pool password", &s.pool_password, "text"),
        s.pool_protocol,
        field("sv2key", "Stratum V2 authority key (required for V2)", &s.pool_authority, "text"),
    )
}

/// A network to try, with the settings to save once it is joined.
struct Request {
    ssid: String,
    password: String,
    settings: settings::Settings,
}

/// Validates the form (fast: the reply must leave before the radio moves).
fn validate(app: &App, body: &str) -> Result<Request, String> {
    let fields: Vec<(String, String)> =
        body.split('&').filter_map(|kv| kv.split_once('=').map(|(k, v)| (url_decode(k), url_decode(v)))).collect();
    let get = |k: &str| fields.iter().find(|(name, _)| name == k).map(|(_, v)| v.clone()).unwrap_or_default();
    let mut values = Map::new();
    for key in ["pool", "addr", "pass", "sv2key"] {
        values.insert(key.into(), Value::String(get(key).trim().to_owned()));
    }
    for key in ["coin", "port", "proto"] {
        let n: u64 = get(key).trim().parse().map_err(|_| format!("Invalid number: {key}"))?;
        values.insert(key.into(), n.into());
    }
    let current = app.settings();
    if let Some(e) = settings::error(&values, &current, &app.board) {
        return Err(e.into());
    }
    let mut pending = current;
    settings::apply(&mut pending, &values, &app.board);
    settings::sanitize(&mut pending, &app.board);
    let (ssid, password) = (get("ssid").trim().to_owned(), get("wpass"));
    if ssid.is_empty() || ssid.len() > 32 {
        return Err("Choose a WiFi network (1 to 32 characters)".into());
    }
    // WPA2 passwords have 8 to 64 characters; none for an open network.
    if !password.is_empty() && !(8..=64).contains(&password.len()) {
        return Err("A WiFi password has 8 to 64 characters (none for an open network)".into());
    }
    Ok(Request { ssid, password, settings: pending })
}

/// Where the last "Save and connect" stands.
#[derive(Clone)]
enum Attempt {
    Idle,
    Joining(String),
    Failed(String),
    Joined,
}

/// Joins the network for this boot only, then saves it and the settings:
/// a mistyped password never replaces the network that worked.
fn attempt(app: &App, request: Request, saved: Option<&(String, String)>) -> Attempt {
    if !wifi::join(&request.ssid, &request.password, 20_000, false) {
        wifi::restore_station(saved);
        return Attempt::Failed(format!("Could not join \"{}\": check the network name and password", request.ssid));
    }
    if !wifi::save_station(&request.ssid, &request.password) || !app.save_settings(&request.settings, true) {
        wifi::restore_station(saved);
        return Attempt::Failed("Settings could not be saved".into());
    }
    Attempt::Joined
}

/// While the miner tries the network: the radio may move to that network's
/// channel and the phone lose the access point for a moment, so the page
/// asks again until it gets the outcome.
fn status_page(ssid: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'>\
<meta http-equiv='refresh' content='3;url=/status'><title>BDLMiner V2 setup</title>\
<style>body{{font-family:system-ui;background:#0a0d12;color:#e8e8e8;max-width:28rem;margin:auto;padding:1rem}}</style></head><body>\
<h1>BDLMiner V2</h1><p>Connecting to \"{}\"... This page updates by itself. If your phone left the miner's \
network, join it again.</p></body></html>",
        escape(ssid)
    )
}

const SAVED_PAGE: &str = "<!doctype html><meta charset='utf-8'><meta name='viewport' \
content='width=device-width,initial-scale=1'><p>Saved. The miner restarts and joins your network.</p>";

/// Runs the portal until it is configured or times out; then restarts.
pub fn run(app: Arc<App>, wifi: &mut BlockingWifi<EspWifi<'static>>) -> ! {
    let name = bdlminer_app::app::portal_ap_name(app.mac);
    let password = app.portal_password();
    app.log(&format!("[wifi] setup portal: join \"{name}\", then open http://192.168.4.1"));
    // On the serial console only, not in the log the dashboard shows.
    println!("[wifi] setup portal password: {password}");
    let ap = AccessPointConfiguration {
        ssid: name.as_str().try_into().unwrap_or_default(),
        password: password.as_str().try_into().unwrap_or_default(),
        auth_method: AuthMethod::WPA2Personal,
        channel: 1,
        max_connections: 4,
        ..Default::default()
    };
    // The station keeps the saved network (an empty configuration would
    // erase it): the portal also opens when that network is only down.
    let started = wifi.set_configuration(&Wifi::Mixed(wifi::station_config(), ap)).and_then(|_| wifi.start());
    if let Err(e) = started {
        app.log(&format!("[wifi] portal could not start: {e}"));
    }
    // Every name leads to the portal: phones open it as soon as they join.
    captive::start_dns(wifi::PORTAL_ADDRESS.octets());
    // The network saved before the portal opened: put back after a failed
    // attempt.
    let saved = Arc::new(wifi::saved_station());
    let state = Arc::new(Mutex::new(Attempt::Idle));
    // Restarts 3 minutes after the last sign of someone using the portal
    // (a request, a phone on the access point), as WiFiManager does.
    let active = Arc::new(AtomicU32::new(millis()));
    // Wildcards: "/*" (registered last) catches every other page.
    // Phones open several connections at once (connectivity checks, the
    // page): 6 sockets, as many as lwIP's 10 leave (3 for the server, 1 for
    // the DNS).
    let config =
        Configuration { stack_size: 10 * 1024, uri_match_wildcard: true, max_open_sockets: 6, ..Default::default() };
    let server = EspHttpServer::new(&config).and_then(|mut server| {
        let (page, seen) = (app.clone(), active.clone());
        server.fn_handler("/", Method::Get, move |req| -> anyhow::Result<()> {
            seen.store(millis(), Ordering::Relaxed);
            req.into_response(200, None, &[("Content-Type", "text/html; charset=utf-8")])?.write_all(
                form(&page, "Choose your WiFi network, pool and payout address.", &captive::networks()).as_bytes(),
            )?;
            Ok(())
        })?;
        let (saver, now, seen, before) = (app.clone(), state.clone(), active.clone(), saved.clone());
        server.fn_handler("/save", Method::Post, move |mut req| -> anyhow::Result<()> {
            seen.store(millis(), Ordering::Relaxed);
            let mut body = vec![0u8; 2048];
            let mut got = 0;
            while got < body.len() {
                let n = req.read(&mut body[got..])?;
                if n == 0 {
                    break;
                }
                got += n;
            }
            let text = String::from_utf8_lossy(&body[..got]).into_owned();
            let html = match &*now.lock().unwrap() {
                Attempt::Joining(ssid) => Some(status_page(ssid)),
                Attempt::Joined => Some(SAVED_PAGE.to_owned()),
                _ => None,
            };
            let html = match html {
                Some(html) => html,
                None => match validate(&saver, &text) {
                    Err(e) => form(&saver, &e, &captive::networks()),
                    Ok(request) => {
                        let page = status_page(&request.ssid);
                        *now.lock().unwrap() = Attempt::Joining(request.ssid.clone());
                        let (app, state, saved) = (saver.clone(), now.clone(), before.clone());
                        // After the reply: joining can move the radio away.
                        let started = std::thread::Builder::new().stack_size(8 * 1024).spawn(move || {
                            std::thread::sleep(Duration::from_millis(500));
                            let outcome = attempt(&app, request, (*saved).as_ref());
                            *state.lock().unwrap() = outcome;
                        });
                        if started.is_err() {
                            *now.lock().unwrap() = Attempt::Failed("Could not start the connection".into());
                        }
                        page
                    }
                },
            };
            req.into_response(200, None, &[("Content-Type", "text/html; charset=utf-8")])?
                .write_all(html.as_bytes())?;
            Ok(())
        })?;
        let (page, now, seen) = (app.clone(), state.clone(), active.clone());
        server.fn_handler("/status", Method::Get, move |req| -> anyhow::Result<()> {
            seen.store(millis(), Ordering::Relaxed);
            let current = now.lock().unwrap().clone();
            let html = match current {
                Attempt::Joining(ssid) => status_page(&ssid),
                Attempt::Joined => SAVED_PAGE.to_owned(),
                Attempt::Failed(e) => {
                    *now.lock().unwrap() = Attempt::Idle;
                    form(&page, &e, &captive::networks())
                }
                Attempt::Idle => {
                    form(&page, "Choose your WiFi network, pool and payout address.", &captive::networks())
                }
            };
            req.into_response(
                200,
                None,
                &[("Content-Type", "text/html; charset=utf-8"), ("Cache-Control", "no-store")],
            )?
            .write_all(html.as_bytes())?;
            Ok(())
        })?;
        // Registered last: the connectivity checks of phones and computers
        // (generate_204, hotspot-detect.html, connecttest.txt...) and any
        // other page are sent to the form, which makes them show it.
        server.fn_handler("/*", Method::Get, |req| -> anyhow::Result<()> {
            req.into_response(302, None, &[("Location", "http://192.168.4.1/"), ("Cache-Control", "no-store")])?
                .write_all(b"")?;
            Ok(())
        })?;
        Ok(server)
    });
    let _server = server.map_err(|e| app.log(&format!("[wifi] portal server failed: {e}")));
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let now = millis();
        match &*state.lock().unwrap() {
            Attempt::Joined => break,
            Attempt::Joining(_) => active.store(now, Ordering::Relaxed),
            _ => {}
        }
        if wifi::portal_clients() > 0 {
            active.store(now, Ordering::Relaxed);
        }
        if now.wrapping_sub(active.load(Ordering::Relaxed)) >= settings::PORTAL_TIMEOUT_S * 1000 {
            app.log("[wifi] not configured, restarting");
            unsafe { esp_idf_svc::sys::esp_restart() }
        }
    }
    app.log("[cfg] settings saved");
    // Time for the phone to fetch the "Saved" page.
    std::thread::sleep(Duration::from_millis(3000));
    unsafe { esp_idf_svc::sys::esp_restart() }
}
