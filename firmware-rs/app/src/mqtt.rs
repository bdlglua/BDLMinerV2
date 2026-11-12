//! MQTT for Home Assistant (setting `mqtt`, the broker's host), as the C++
//! firmware's mqtt.cpp: discovery of the sensors and controls, the state
//! every 30 s on bdlminer/<host name>/state, and the commands of
//! bdlminer/<host name>/set/<key> (mining switch, power, brightness,
//! restart). A small MQTT 3.1.1 client over the platform's TCP connections.

use std::io::{self, ErrorKind};
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use serde_json::{json, Value};

use crate::app::App;
use crate::platform::{Connection, Network, FW_VERSION};

const STATE_MS: u32 = 30_000;
const RETRY_MS: u32 = 30_000;
const KEEP_ALIVE_S: u16 = 60;
const MAX_PACKET: usize = 4096;

/// "off", "connected" or "disconnected", for the dashboard.
#[derive(Default)]
pub struct MqttState(AtomicU8);

impl MqttState {
    pub fn name(&self, app: &App) -> &'static str {
        if app.settings().mqtt_host.is_empty() {
            "off"
        } else if self.0.load(Ordering::Relaxed) == 1 {
            "connected"
        } else {
            "disconnected"
        }
    }
    fn set(&self, up: bool) {
        self.0.store(up as u8, Ordering::Relaxed);
    }
}

fn remaining_length(mut n: usize, out: &mut Vec<u8>) {
    loop {
        let mut b = (n % 128) as u8;
        n /= 128;
        if n > 0 {
            b |= 0x80;
        }
        out.push(b);
        if n == 0 {
            break;
        }
    }
}

fn string(s: &str, out: &mut Vec<u8>) {
    out.extend_from_slice(&(s.len() as u16).to_be_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn packet(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut p = vec![kind];
    remaining_length(body.len(), &mut p);
    p.extend_from_slice(body);
    p
}

/// CONNECT with a retained "offline" will.
pub fn connect_packet(client_id: &str, user: &str, pass: &str, will_topic: &str) -> Vec<u8> {
    let mut b = Vec::new();
    string("MQTT", &mut b);
    b.push(4); // 3.1.1
    let mut flags = 0x02 | 0x04 | 0x20; // clean session, will, will retain
    if !user.is_empty() {
        flags |= 0x80 | 0x40;
    }
    b.push(flags);
    b.extend_from_slice(&KEEP_ALIVE_S.to_be_bytes());
    string(client_id, &mut b);
    string(will_topic, &mut b);
    string("offline", &mut b);
    if !user.is_empty() {
        string(user, &mut b);
        string(pass, &mut b);
    }
    packet(0x10, &b)
}

pub fn publish_packet(topic: &str, payload: &[u8], retain: bool) -> Vec<u8> {
    let mut b = Vec::new();
    string(topic, &mut b);
    b.extend_from_slice(payload);
    packet(0x30 | retain as u8, &b)
}

pub fn subscribe_packet(id: u16, filter: &str) -> Vec<u8> {
    let mut b = id.to_be_bytes().to_vec();
    string(filter, &mut b);
    b.push(0); // QoS 0
    packet(0x82, &b)
}

/// Complete packets out of the bytes received: (first byte, body).
#[derive(Default)]
pub struct Reader {
    buf: Vec<u8>,
}

impl Reader {
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    pub fn next_packet(&mut self) -> Result<Option<(u8, Vec<u8>)>, String> {
        let mut length = 0usize;
        let mut shift = 0;
        let mut i = 1;
        loop {
            let Some(&b) = self.buf.get(i) else { return Ok(None) };
            length |= ((b & 0x7f) as usize) << shift;
            i += 1;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 21 {
                return Err("bad MQTT length".into());
            }
        }
        if length > MAX_PACKET {
            return Err("MQTT packet too large".into());
        }
        if self.buf.len() < i + length {
            return Ok(None);
        }
        let kind = self.buf[0];
        let body = self.buf[i..i + length].to_vec();
        self.buf.drain(..i + length);
        Ok(Some((kind, body)))
    }
}

/// The topic and payload of a PUBLISH body (QoS 0 or 1).
pub fn parse_publish(kind: u8, body: &[u8]) -> Option<(String, Vec<u8>)> {
    let n = u16::from_be_bytes([*body.first()?, *body.get(1)?]) as usize;
    let topic = std::str::from_utf8(body.get(2..2 + n)?).ok()?.to_owned();
    let skip = if (kind >> 1) & 3 > 0 { 2 } else { 0 };
    Some((topic, body.get(2 + n + skip..)?.to_vec()))
}

/// MQTT payloads are text: only complete percentages may change power or light.
pub fn parse_percent(text: &str) -> Option<u8> {
    if text.is_empty() || text.len() > 3 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<u8>().ok().filter(|&v| v <= 100)
}

fn device(app: &App, id: &str) -> Value {
    json!({
        "identifiers": [id],
        "name": format!("BDLMiner {id}"),
        "model": app.board.name,
        "manufacturer": "BDLMiner",
        "sw_version": FW_VERSION,
        "configuration_url": format!("http://{}/", app.system.ip()),
    })
}

/// Home Assistant discovery messages: (topic, payload), retained.
pub fn discovery(app: &App, base: &str) -> Vec<(String, String)> {
    let id = &app.hostname;
    let mut out = Vec::new();
    for (component, key, name, maximum) in [
        ("switch", "mining", "Mining", 0),
        ("number", "power", "Mining power", 100),
        ("number", "brightness", "Screen brightness", 100),
        ("button", "restart", "Restart", 0),
    ] {
        let mut d = json!({
            "name": name,
            "unique_id": format!("{id}_{key}"),
            "command_topic": format!("{base}/set/{key}"),
            "availability_topic": format!("{base}/status"),
        });
        if component != "button" {
            d["state_topic"] = format!("{base}/state").into();
            let field = if key == "power" { "power_limit" } else { key };
            d["value_template"] = format!("{{{{ value_json.{field} }}}}").into();
        }
        if component == "number" {
            d["min"] = 0.into();
            d["max"] = maximum.into();
            d["step"] = 5.into();
            d["unit_of_measurement"] = "%".into();
            d["mode"] = "slider".into();
        }
        if component == "button" {
            d["device_class"] = "restart".into();
        }
        d["device"] = device(app, id);
        out.push((format!("homeassistant/{component}/{id}/{key}/config"), d.to_string()));
    }
    for (key, name, unit, class, state_class) in [
        ("hashrate", "Hashrate", Some("kH/s"), None, Some("measurement")),
        ("temperature", "Temperature", Some("\u{b0}C"), Some("temperature"), Some("measurement")),
        ("accepted", "Accepted shares", None, None, Some("total_increasing")),
        ("rejected", "Rejected shares", None, None, Some("total_increasing")),
        ("best_session", "Best difficulty (session)", None, None, Some("measurement")),
        ("best_ever", "Best difficulty (ever)", None, None, Some("measurement")),
        ("shares32", "32-bit shares", None, None, Some("total_increasing")),
        ("shares64", "64-bit shares", None, None, Some("total_increasing")),
        ("blocks", "Block candidates", None, None, Some("total_increasing")),
        ("uptime", "Uptime", Some("s"), Some("duration"), Some("measurement")),
        ("status", "Pool status", None, None, None),
    ] {
        let mut d = json!({
            "name": name,
            "unique_id": format!("{id}_{key}"),
            "state_topic": format!("{base}/state"),
            // A missing or null value gives an unknown state instead of a template warning.
            "value_template": format!("{{{{ value_json.{key} | default(none) }}}}"),
            "availability_topic": format!("{base}/status"),
        });
        if let Some(u) = unit {
            d["unit_of_measurement"] = u.into();
        }
        if let Some(c) = class {
            d["device_class"] = c.into();
        }
        if let Some(c) = state_class {
            d["state_class"] = c.into();
        }
        d["device"] = device(app, id);
        out.push((format!("homeassistant/sensor/{id}/{key}/config"), d.to_string()));
    }
    out
}

pub fn state(app: &App) -> String {
    let stats = app.miner.stats();
    let s = app.settings();
    let t = app.system.temperature();
    json!({
        "hashrate": (app.miner.hashrate() / 10.0).round() / 100.0,
        "power_percent": app.miner.power_percent(),
        "temperature": if t.is_finite() { json!(((t * 10.0).round() / 10.0) as f64) } else { Value::Null },
        "accepted": stats.accepted,
        "rejected": stats.rejected,
        "best_session": stats.session_best_diff,
        "best_ever": stats.best_diff,
        "shares32": stats.over32bits,
        "shares64": stats.over64bits,
        "blocks": stats.blocks_found,
        "uptime": app.miner.uptime_seconds(),
        "status": app.pool.status(),
        "power_limit": s.mining_power,
        "mining": if s.mining_power > 0 { "ON" } else { "OFF" },
        "brightness": app.brightness(),
    })
    .to_string()
}

/// Applies a command of <base>/set/<key>; false when not understood.
pub fn command(app: &App, key: &str, value: &str, resume_power: &mut u8) -> bool {
    let value = value.trim();
    let power = match key {
        "mining" if value.eq_ignore_ascii_case("ON") => Some(if *resume_power > 0 { *resume_power } else { 100 }),
        "mining" if value.eq_ignore_ascii_case("OFF") => Some(0),
        "power" => match parse_percent(value) {
            Some(p) => Some(p),
            None => return false,
        },
        "brightness" => match parse_percent(value) {
            Some(b) => {
                app.set_brightness(b);
                app.settings_changed.store(true, Ordering::Relaxed);
                None
            }
            None => return false,
        },
        "restart" if value.eq_ignore_ascii_case("PRESS") => {
            app.request_restart("restart requested over MQTT");
            None
        }
        _ => return false,
    };
    if let Some(power) = power {
        let mut copy = app.settings();
        if power != copy.mining_power {
            if copy.mining_power > 0 {
                *resume_power = copy.mining_power;
            }
            copy.mining_power = power;
            copy.brightness = app.brightness();
            app.apply_live(&copy);
            app.save_settings(&copy, false);
            app.log(&format!("[mqtt] mining power {power}%"));
        }
    }
    app.log(&format!("[mqtt] command {key} = {value}"));
    true
}

fn send<C: Connection>(conn: &mut C, data: &[u8]) -> io::Result<()> {
    conn.write_all(data)
}

/// One session with the broker, until it fails or the settings change.
fn session<N: Network>(net: &N, app: &App, state: &MqttState, host: &str, port: u16) -> Result<(), String> {
    let s = app.settings();
    let base = format!("bdlminer/{}", app.hostname);
    let will = format!("{base}/status");
    let mut conn = net.connect(host, port, Duration::from_secs(10)).map_err(|e| e.to_string())?;
    send(&mut conn, &connect_packet(&app.hostname, &s.mqtt_user, &s.mqtt_pass, &will)).map_err(|e| e.to_string())?;
    let mut reader = Reader::default();
    let mut buf = [0u8; 512];
    let started = app.clock.millis();
    // CONNACK.
    loop {
        if let Some((kind, body)) = reader.next_packet()? {
            if kind >> 4 != 2 || body.len() < 2 {
                return Err("unexpected answer".into());
            }
            if body[1] != 0 {
                return Err(format!("refused (code {})", body[1]));
            }
            break;
        }
        if app.clock.millis().wrapping_sub(started) > 10_000 {
            return Err("no answer".into());
        }
        match conn.read(&mut buf) {
            Ok(0) => return Err("closed".into()),
            Ok(n) => reader.push(&buf[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    app.log(&format!("[mqtt] connected to {host}:{port}"));
    state.set(true);
    let io = |e: io::Error| e.to_string();
    send(&mut conn, &subscribe_packet(1, &format!("{base}/set/#"))).map_err(io)?;
    send(&mut conn, &publish_packet(&will, b"online", true)).map_err(io)?;
    for (topic, payload) in discovery(app, &base) {
        send(&mut conn, &publish_packet(&topic, payload.as_bytes(), true)).map_err(io)?;
    }
    let mut last_state: Option<u32> = None;
    let mut last_sent = app.clock.millis();
    let mut resume_power = if s.mining_power > 0 { s.mining_power } else { 100 };
    let prefix = format!("{base}/set/");
    loop {
        let now_settings = app.settings();
        if now_settings.mqtt_host != host || now_settings.mqtt_port != port {
            return Ok(());
        }
        let now = app.clock.millis();
        if last_state.is_none_or(|t| now.wrapping_sub(t) >= STATE_MS) {
            last_state = Some(now);
            send(&mut conn, &publish_packet(&format!("{base}/state"), state_payload(app).as_bytes(), false))
                .map_err(io)?;
            last_sent = now;
        }
        if now.wrapping_sub(last_sent) >= (KEEP_ALIVE_S as u32 * 1000) / 2 {
            send(&mut conn, &[0xc0, 0]).map_err(io)?;
            last_sent = now;
        }
        match conn.read(&mut buf) {
            Ok(0) => return Err("closed by the broker".into()),
            Ok(n) => reader.push(&buf[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => return Err(e.to_string()),
        }
        while let Some((kind, body)) = reader.next_packet()? {
            if kind >> 4 == 3 {
                if let Some((topic, payload)) = parse_publish(kind, &body) {
                    if let Some(key) = topic.strip_prefix(&prefix) {
                        command(app, key, &String::from_utf8_lossy(&payload), &mut resume_power);
                    }
                }
            }
        }
    }
}

fn state_payload(app: &App) -> String {
    state(app)
}

/// The MQTT task: connects when a broker is set, again every 30 s after a failure.
pub fn run<N: Network>(net: &N, app: &App, state: &MqttState) {
    let mut last_try: Option<u32> = None;
    loop {
        let s = app.settings();
        if s.mqtt_host.is_empty() || !net.link_up() {
            state.set(false);
            app.clock.sleep(Duration::from_secs(10));
            continue;
        }
        let now = app.clock.millis();
        if last_try.is_some_and(|t| now.wrapping_sub(t) < RETRY_MS) {
            app.clock.sleep(Duration::from_secs(1));
            continue;
        }
        last_try = Some(now);
        if let Err(e) = session(net, app, state, &s.mqtt_host, s.mqtt_port) {
            app.log(&format!("[mqtt] cannot connect to {}:{} ({e})", s.mqtt_host, s.mqtt_port));
        }
        state.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets() {
        let c = connect_packet("bdlminer-abc", "", "", "bdlminer/x/status");
        assert_eq!(c[0], 0x10);
        assert_eq!(&c[2..8], b"\0\x04MQTT");
        assert_eq!(c[9], 0x26);
        let p = publish_packet("a/b", b"on", true);
        assert_eq!(p, [0x31, 7, 0, 3, b'a', b'/', b'b', b'o', b'n']);
        let mut r = Reader::default();
        let big = publish_packet("t", &[7u8; 300], false);
        r.push(&big[..100]);
        assert_eq!(r.next_packet().unwrap(), None);
        r.push(&big[100..]);
        let (kind, body) = r.next_packet().unwrap().unwrap();
        let (topic, payload) = parse_publish(kind, &body).unwrap();
        assert_eq!((topic.as_str(), payload.len()), ("t", 300));
        assert_eq!(subscribe_packet(1, "x/#")[0], 0x82);
    }

    #[test]
    fn percent() {
        assert_eq!(parse_percent("50"), Some(50));
        assert_eq!(parse_percent("100"), Some(100));
        assert_eq!(parse_percent("101"), None);
        assert_eq!(parse_percent("50oops"), None);
        assert_eq!(parse_percent("1.5"), None);
        assert_eq!(parse_percent(""), None);
    }
}
