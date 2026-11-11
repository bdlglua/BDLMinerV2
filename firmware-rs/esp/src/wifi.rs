//! WiFi: the saved network (also the one the C++ firmware's portal saved in
//! the driver's NVS), the fallback networks of the settings, and the setup
//! portal when nothing can be joined.

use std::time::Duration;

use bdlminer_app::platform::Network;
use bdlminer_app::std_platform::{TcpConnection, TcpNetwork};
use esp_idf_svc::sys::*;

use crate::system::{millis, random};

/// SSID and password of the station configuration the driver loaded.
pub fn saved_station() -> Option<(String, String)> {
    let mut conf: wifi_config_t = unsafe { std::mem::zeroed() };
    if unsafe { esp_wifi_get_config(wifi_interface_t_WIFI_IF_STA, &mut conf) } != ESP_OK {
        return None;
    }
    let sta = unsafe { conf.sta };
    let text = |b: &[u8]| String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]).into_owned();
    let ssid = text(&sta.ssid);
    (!ssid.is_empty()).then(|| (ssid, text(&sta.password)))
}

/// Whether the station is associated and has an address.
pub fn link_up() -> bool {
    let mut ap: wifi_ap_record_t = unsafe { std::mem::zeroed() };
    if unsafe { esp_wifi_sta_get_ap_info(&mut ap) } != ESP_OK {
        return false;
    }
    station_ip().is_some()
}

pub fn station_ip() -> Option<String> {
    let key = b"WIFI_STA_DEF\0";
    let netif = unsafe { esp_netif_get_handle_from_ifkey(key.as_ptr().cast()) };
    if netif.is_null() {
        return None;
    }
    let mut info: esp_netif_ip_info_t = unsafe { std::mem::zeroed() };
    if unsafe { esp_netif_get_ip_info(netif, &mut info) } != ESP_OK || info.ip.addr == 0 {
        return None;
    }
    let a = info.ip.addr.to_le_bytes();
    Some(format!("{}.{}.{}.{}", a[0], a[1], a[2], a[3]))
}

/// Joins a network; `persist` saves it as the driver's own (the portal's).
pub fn join(ssid: &str, password: &str, timeout_ms: u32, persist: bool) -> bool {
    if ssid.is_empty() || ssid.len() > 32 || password.len() > 64 {
        return false;
    }
    let mut conf: wifi_config_t = unsafe { std::mem::zeroed() };
    unsafe {
        conf.sta.ssid[..ssid.len()].copy_from_slice(ssid.as_bytes());
        conf.sta.password[..password.len()].copy_from_slice(password.as_bytes());
        conf.sta.threshold.authmode =
            if password.is_empty() { wifi_auth_mode_t_WIFI_AUTH_OPEN } else { wifi_auth_mode_t_WIFI_AUTH_WPA2_PSK };
        esp_wifi_set_storage(if persist { wifi_storage_t_WIFI_STORAGE_FLASH } else { wifi_storage_t_WIFI_STORAGE_RAM });
        esp_wifi_disconnect();
        let ok = esp_wifi_set_config(wifi_interface_t_WIFI_IF_STA, &mut conf) == ESP_OK && esp_wifi_connect() == ESP_OK;
        esp_wifi_set_storage(wifi_storage_t_WIFI_STORAGE_FLASH);
        if !ok {
            return false;
        }
    }
    let start = millis();
    while millis().wrapping_sub(start) < timeout_ms {
        if link_up() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    link_up()
}

/// The pool task's network: std TCP over lwIP, and the WiFi state.
pub struct EspNetwork {
    pub tcp: TcpNetwork,
    /// (ssid, password) of the main network and the fallbacks, in order.
    pub networks: Vec<(String, String)>,
}

impl Network for EspNetwork {
    type Conn = TcpConnection;
    fn connect(&self, host: &str, port: u16, timeout: Duration) -> std::io::Result<TcpConnection> {
        self.tcp.connect(host, port, timeout)
    }
    fn link_up(&self) -> bool {
        link_up()
    }
    fn reconnect(&self) {
        unsafe { esp_wifi_connect() };
    }
    fn try_fallbacks(&self) -> bool {
        // Fallbacks are joined without replacing the saved network.
        self.networks.iter().any(|(ssid, pass)| join(ssid, pass, 15_000, false))
    }
    fn random(&self, out: &mut [u8]) {
        random(out)
    }
}
