//! WiFi: the saved network (also the one the C++ firmware's portal saved in
//! the driver's NVS), the fallback networks of the settings, and the setup
//! portal when nothing can be joined.

use std::time::Duration;

use bdlminer_app::platform::Network;
use bdlminer_app::std_platform::{TcpConnection, TcpNetwork};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::ipv4::{self, Ipv4Addr, Mask, RouterConfiguration, Subnet};
use esp_idf_svc::netif::{EspNetif, NetifConfiguration, NetifStack};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys::*;
use esp_idf_svc::wifi::{AuthMethod, ClientConfiguration, EspWifi, WifiDriver};

use crate::system::{millis, random};

/// The setup portal's address: the access point's, its DHCP server's
/// gateway and DNS server (the C++ firmware's, WiFiManager's default).
pub const PORTAL_ADDRESS: Ipv4Addr = Ipv4Addr::new(192, 168, 4, 1);

/// The WiFi driver, its access point on 192.168.4.1. esp-idf-svc's default
/// router is 192.168.71.1 and hands out 8.8.8.8 as DNS server: phones then
/// never ask the portal's DNS, and the portal does not open by itself.
pub fn driver(
    modem: Modem,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> Result<EspWifi<'static>, EspError> {
    let ap = NetifConfiguration {
        ip_configuration: Some(ipv4::Configuration::Router(RouterConfiguration {
            subnet: Subnet { gateway: PORTAL_ADDRESS, mask: Mask(24) },
            dhcp_enabled: true,
            dns: Some(PORTAL_ADDRESS),
            secondary_dns: None,
        })),
        ..NetifConfiguration::wifi_default_router()
    };
    EspWifi::wrap_all(
        WifiDriver::new(modem, sysloop, Some(nvs))?,
        EspNetif::new(NetifStack::Sta)?,
        EspNetif::new_with_conf(&ap)?,
    )
}

/// The station configuration for the saved network, kept as it is: writing
/// an empty one would erase it from the driver's NVS (the portal used to,
/// when the network was only out of reach). Open networks and no network
/// at all ask for no authentication (else the driver warns that an empty
/// password cannot reach WPA2).
pub fn station_config() -> ClientConfiguration {
    let saved = saved_station();
    let (ssid, password) = saved.as_ref().map(|(s, p)| (s.as_str(), p.as_str())).unwrap_or(("", ""));
    ClientConfiguration {
        ssid: ssid.try_into().unwrap_or_default(),
        password: password.try_into().unwrap_or_default(),
        auth_method: if password.is_empty() { AuthMethod::None } else { AuthMethod::WPA2Personal },
        ..Default::default()
    }
}

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

/// A station configuration for `ssid` (open when there is no password).
fn station_raw(ssid: &str, password: &str) -> wifi_config_t {
    let mut conf: wifi_config_t = unsafe { std::mem::zeroed() };
    unsafe {
        conf.sta.ssid[..ssid.len()].copy_from_slice(ssid.as_bytes());
        conf.sta.password[..password.len()].copy_from_slice(password.as_bytes());
        conf.sta.threshold.authmode =
            if password.is_empty() { wifi_auth_mode_t_WIFI_AUTH_OPEN } else { wifi_auth_mode_t_WIFI_AUTH_WPA2_PSK };
    }
    conf
}

/// Sets the station configuration, in the driver's NVS (`persist`) or for
/// this boot only.
fn set_station(conf: &mut wifi_config_t, persist: bool) -> bool {
    unsafe {
        esp_wifi_set_storage(if persist { wifi_storage_t_WIFI_STORAGE_FLASH } else { wifi_storage_t_WIFI_STORAGE_RAM });
        let ok = esp_wifi_set_config(wifi_interface_t_WIFI_IF_STA, conf) == ESP_OK;
        esp_wifi_set_storage(wifi_storage_t_WIFI_STORAGE_FLASH);
        ok
    }
}

/// Joins a network; `persist` saves it as the driver's own (the portal's).
pub fn join(ssid: &str, password: &str, timeout_ms: u32, persist: bool) -> bool {
    if ssid.is_empty() || ssid.len() > 32 || password.len() > 64 {
        return false;
    }
    let mut conf = station_raw(ssid, password);
    unsafe { esp_wifi_disconnect() };
    if !set_station(&mut conf, persist) || unsafe { esp_wifi_connect() } != ESP_OK {
        return false;
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

/// Phones and computers on the setup portal's access point.
pub fn portal_clients() -> usize {
    let mut list: wifi_sta_list_t = unsafe { std::mem::zeroed() };
    if unsafe { esp_wifi_ap_get_sta_list(&mut list) } != ESP_OK {
        return 0;
    }
    list.num.max(0) as usize
}

/// Saves a network that was joined as the driver's own: the one the miner
/// joins at boot.
pub fn save_station(ssid: &str, password: &str) -> bool {
    let mut conf = station_raw(ssid, password);
    set_station(&mut conf, true)
}

/// Stops trying a network that could not be joined and puts `saved` back
/// for this boot (the driver's NVS still holds it): the station leaves the
/// radio, and the access point stays on its channel.
pub fn restore_station(saved: Option<&(String, String)>) {
    unsafe { esp_wifi_disconnect() };
    let mut conf = match saved {
        Some((ssid, password)) if ssid.len() <= 32 && password.len() <= 64 => station_raw(ssid, password),
        _ => unsafe { std::mem::zeroed() },
    };
    set_station(&mut conf, false);
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
