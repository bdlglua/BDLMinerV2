//! The setup portal: an access point "BDLMinerV2-XXXX" with a small form at
//! http://192.168.4.1 for the WiFi network, the pool and the payout address.
//! Saved, the miner joins the network and restarts. Restarts after 3 minutes
//! without a configuration, as the C++ firmware does.

use std::sync::Arc;
use std::time::Duration;

use bdlminer_app::settings;
use bdlminer_app::App;
use esp_idf_svc::http::server::{Configuration, EspHttpServer};
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AuthMethod, BlockingWifi, ClientConfiguration, Configuration as Wifi, EspWifi,
};
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

/// Validates and saves the form; joins the network. Ok: restart now.
fn save(app: &App, body: &str) -> Result<(), String> {
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
    let (ssid, password) = (get("ssid"), get("wpass"));
    if !wifi::join(ssid.trim(), &password, 20_000, true) {
        return Err(format!("Could not join \"{}\": check the network name and password", ssid.trim()));
    }
    if !app.save_settings(&pending, true) {
        return Err("Settings could not be saved".into());
    }
    Ok(())
}

/// Gives the access point 192.168.4.1 (esp-idf-svc's default is
/// 192.168.71.1), the address the C++ firmware's portal uses and the one the
/// log and the screen announce.
fn portal_address() -> Result<(), esp_idf_svc::sys::EspError> {
    use esp_idf_svc::sys::{
        esp, esp_ip4_addr_t, esp_netif_dhcps_start, esp_netif_dhcps_stop, esp_netif_get_handle_from_ifkey,
        esp_netif_ip_info_t, esp_netif_set_ip_info, EspError, ESP_ERR_INVALID_STATE,
    };
    let address = |a: [u8; 4]| esp_ip4_addr_t { addr: u32::from_le_bytes(a) };
    let info = esp_netif_ip_info_t {
        ip: address([192, 168, 4, 1]),
        netmask: address([255, 255, 255, 0]),
        gw: address([192, 168, 4, 1]),
    };
    unsafe {
        let netif = esp_netif_get_handle_from_ifkey(c"WIFI_AP_DEF".as_ptr());
        if netif.is_null() {
            return Err(EspError::from_infallible::<ESP_ERR_INVALID_STATE>());
        }
        // The DHCP server must be stopped while the address changes; a
        // server that was not running is fine.
        let _ = esp_netif_dhcps_stop(netif);
        esp!(esp_netif_set_ip_info(netif, &info))?;
        esp!(esp_netif_dhcps_start(netif))
    }
}

/// Runs the portal until it is configured or times out; then restarts.
pub fn run(app: Arc<App>, wifi: &mut BlockingWifi<EspWifi<'static>>) -> ! {
    let name = bdlminer_app::app::portal_ap_name(app.mac);
    let password = app.portal_password();
    app.log(&format!("[wifi] setup portal: join \"{name}\", then open http://192.168.4.1"));
    println!("[wifi] setup portal: join \"{name}\" (password {password}), then open http://192.168.4.1");
    let ap = AccessPointConfiguration {
        ssid: name.as_str().try_into().unwrap_or_default(),
        password: password.as_str().try_into().unwrap_or_default(),
        auth_method: AuthMethod::WPA2Personal,
        channel: 1,
        max_connections: 4,
        ..Default::default()
    };
    let started = wifi.set_configuration(&Wifi::Mixed(ClientConfiguration::default(), ap)).and_then(|_| wifi.start());
    if let Err(e) = started {
        app.log(&format!("[wifi] portal could not start: {e}"));
    }
    if let Err(e) = portal_address() {
        app.log(&format!("[wifi] portal address 192.168.4.1 not set: {e}"));
    }
    // Every name leads to the portal: phones open it as soon as they join.
    captive::start_dns([192, 168, 4, 1]);
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    // Wildcards: "/*" (registered last) catches every other page.
    let config = Configuration { stack_size: 10 * 1024, uri_match_wildcard: true, ..Default::default() };
    let server = EspHttpServer::new(&config).and_then(|mut server| {
        let page = app.clone();
        server.fn_handler("/", Method::Get, move |req| -> anyhow::Result<()> {
            req.into_response(200, None, &[("Content-Type", "text/html; charset=utf-8")])?.write_all(
                form(&page, "Choose your WiFi network, pool and payout address.", &captive::networks()).as_bytes(),
            )?;
            Ok(())
        })?;
        let (saver, flag) = (app.clone(), done.clone());
        server.fn_handler("/save", Method::Post, move |mut req| -> anyhow::Result<()> {
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
            let html = match save(&saver, &text) {
                Ok(()) => {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    "<!doctype html><meta charset='utf-8'><p>Saved. The miner restarts and joins your network.</p>"
                        .to_owned()
                }
                Err(e) => form(&saver, &e, &captive::networks()),
            };
            req.into_response(200, None, &[("Content-Type", "text/html; charset=utf-8")])?
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
    let start = millis();
    while !done.load(std::sync::atomic::Ordering::Relaxed)
        && millis().wrapping_sub(start) < settings::PORTAL_TIMEOUT_S * 1000
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    if done.load(std::sync::atomic::Ordering::Relaxed) {
        app.log("[cfg] settings saved");
        std::thread::sleep(Duration::from_millis(1500));
    } else {
        app.log("[wifi] not configured, restarting");
    }
    unsafe { esp_idf_svc::sys::esp_restart() }
}
