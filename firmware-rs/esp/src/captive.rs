//! What WiFiManager gave the C++ firmware's portal: a DNS server that sends
//! every name to the portal, so a phone joining the access point opens the
//! setup page by itself, and the list of the networks in range.

use std::net::UdpSocket;
use std::sync::Mutex;
use std::time::Duration;

use esp_idf_svc::sys::*;

use crate::system::millis;

/// A network in range: SSID, signal (dBm) and whether it has a password.
#[derive(Clone)]
pub struct Network {
    pub ssid: String,
    pub rssi: i8,
    pub secured: bool,
}

/// Scans (a few seconds), strongest first, one entry per SSID.
fn scan_now() -> Vec<Network> {
    let mut found: Vec<Network> = Vec::new();
    unsafe {
        if esp_wifi_scan_start(std::ptr::null(), true) != ESP_OK {
            return found;
        }
        let mut count: u16 = 0;
        esp_wifi_scan_get_ap_num(&mut count);
        let mut records: Vec<wifi_ap_record_t> = vec![std::mem::zeroed(); count.min(32) as usize];
        let mut got = records.len() as u16;
        if esp_wifi_scan_get_ap_records(&mut got, records.as_mut_ptr()) != ESP_OK {
            return found;
        }
        records.truncate(got as usize);
        for r in &records {
            let end = r.ssid.iter().position(|&c| c == 0).unwrap_or(r.ssid.len());
            let ssid = String::from_utf8_lossy(&r.ssid[..end]).into_owned();
            if ssid.is_empty() {
                continue;
            }
            let secured = r.authmode != wifi_auth_mode_t_WIFI_AUTH_OPEN;
            match found.iter_mut().find(|n| n.ssid == ssid) {
                Some(n) if n.rssi < r.rssi => *n = Network { ssid, rssi: r.rssi, secured },
                Some(_) => {}
                None => found.push(Network { ssid, rssi: r.rssi, secured }),
            }
        }
    }
    found.sort_by(|a, b| b.rssi.cmp(&a.rssi));
    found
}

/// The last scan, made again when it is older than 10 seconds.
pub fn networks() -> Vec<Network> {
    static CACHE: Mutex<Option<(u32, Vec<Network>)>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap();
    let now = millis();
    match &*cache {
        Some((at, list)) if now.wrapping_sub(*at) < 10_000 => list.clone(),
        _ => {
            let list = scan_now();
            *cache = Some((now, list.clone()));
            list
        }
    }
}

/// Answers every DNS question with `address` (no answer for anything but
/// IPv4 addresses), on a thread of its own.
pub fn start_dns(address: [u8; 4]) {
    let socket = match UdpSocket::bind("0.0.0.0:53") {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[wifi] portal DNS could not start: {e}");
            return;
        }
    };
    let _ = socket.set_read_timeout(Some(Duration::from_secs(1)));
    let spawned = std::thread::Builder::new().stack_size(4096).spawn(move || {
        let mut buf = [0u8; 512];
        loop {
            let Ok((len, from)) = socket.recv_from(&mut buf) else { continue };
            if let Some(reply) = answer(&buf[..len], address) {
                let _ = socket.send_to(&reply, from);
            }
        }
    });
    if spawned.is_err() {
        log::warn!("[wifi] portal DNS could not start");
    }
}

/// The reply to one standard query, or `None` when it is not one.
fn answer(query: &[u8], address: [u8; 4]) -> Option<Vec<u8>> {
    // Header (12 bytes), then exactly one question: a name, type and class.
    if query.len() < 12 || query[2] & 0x80 != 0 || query[2] & 0x78 != 0 {
        return None;
    }
    if u16::from_be_bytes([query[4], query[5]]) != 1 {
        return None;
    }
    let mut end = 12;
    loop {
        let label = *query.get(end)? as usize;
        if label == 0 {
            end += 1;
            break;
        }
        if label & 0xC0 != 0 {
            return None;
        }
        end += 1 + label;
    }
    let qtype = u16::from_be_bytes([*query.get(end)?, *query.get(end + 1)?]);
    end += 4; // type and class
    if end > query.len() {
        return None;
    }
    let ipv4 = qtype == 1;
    let mut reply = Vec::with_capacity(end + 16);
    reply.extend_from_slice(&query[..2]); // id
    reply.extend_from_slice(&[0x81, 0x80]); // response, recursion desired and available
    reply.extend_from_slice(&[0, 1, 0, ipv4 as u8, 0, 0, 0, 0]);
    reply.extend_from_slice(&query[12..end]);
    if ipv4 {
        // Name: pointer to the question's; type A, class IN, TTL 60 s.
        reply.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4]);
        reply.extend_from_slice(&address);
    }
    Some(reply)
}
