//! BDLMiner V2 for ESP32 boards, in Rust on ESP-IDF.
//!
//! Boot: settings from NVS, WiFi (saved network, fallbacks, else the setup
//! portal), SNTP, the hardware SHA miner on the last core and a software
//! miner on the others, the pool task, the dashboard, then the once-a-second
//! supervision loop.

#[cfg(feature = "m5stickc-plus2")]
mod display;
mod http;
mod hwsha;
#[cfg(feature = "m5stickc-plus2")]
mod m5stickc;
mod nvs;
mod portal;
mod system;
mod wifi;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use bdlminer_app::app::{App, Platform};
use bdlminer_app::miner::{run_worker, WorkerOptions};
use bdlminer_app::notify::HttpClient;
use bdlminer_app::platform::{BoardKind, BoardProfile, Clock, Storage, SystemInfo};
use bdlminer_app::pool;
use bdlminer_app::std_platform::TcpNetwork;
use bdlminer_app::watch::NoIndicators;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::EspSntp;
use esp_idf_svc::wifi::{BlockingWifi, ClientConfiguration, Configuration as Wifi, EspWifi};

use crate::system::{EspClock, EspSystem};

fn board_profile() -> BoardProfile {
    BoardProfile {
        name: if cfg!(feature = "m5stickc-plus2") {
            "M5StickC Plus2"
        } else if cfg!(esp32s3) {
            "ESP32-S3"
        } else if cfg!(esp32c3) {
            "ESP32-C3"
        } else {
            "ESP32"
        },
        env: if cfg!(feature = "m5stickc-plus2") {
            "m5stickc-plus2-rust"
        } else if cfg!(esp32s3) {
            "esp32s3-rust"
        } else if cfg!(esp32c3) {
            "esp32c3-rust"
        } else {
            "esp32-rust"
        },
        kind: BoardKind::Esp32,
        m5: cfg!(feature = "m5"),
        display_pages: 0,
        temperature_sensor: cfg!(any(esp32s2, esp32s3, esp32c3, feature = "m5stickc-plus2")),
    }
}

/// Notifications over HTTPS, verified with ESP-IDF's certificate bundle.
struct EspHttp;

impl HttpClient for EspHttp {
    fn post(&self, url: &str, headers: &[(&str, &str)], body: &[u8]) -> Result<u16, String> {
        // TLS hashes with the SHA peripheral too: keep the miner off it.
        let _hold = hwsha::Hold::new();
        let config = HttpConfiguration {
            timeout: Some(Duration::from_secs(15)),
            crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
            ..Default::default()
        };
        let mut conn = EspHttpConnection::new(&config).map_err(|e| e.to_string())?;
        let length = body.len().to_string();
        let mut all: Vec<(&str, &str)> = headers.to_vec();
        all.push(("Content-Length", &length));
        conn.initiate_request(esp_idf_svc::http::Method::Post, url, &all).map_err(|e| e.to_string())?;
        conn.write_all(body).map_err(|e| e.to_string())?;
        conn.initiate_response().map_err(|e| e.to_string())?;
        Ok(conn.status())
    }
}

fn spawn_pinned(name: &'static [u8], core: Core, priority: u8, stack: usize, f: impl FnOnce() + Send + 'static) {
    let config = ThreadSpawnConfiguration {
        name: Some(name),
        stack_size: stack,
        priority,
        pin_to_core: Some(core),
        ..Default::default()
    };
    if let Err(e) = config.set() {
        log::warn!("thread configuration failed: {e:?}");
    }
    let spawned = thread::Builder::new().stack_size(stack).spawn(f);
    let _ = ThreadSpawnConfiguration::default().set();
    if spawned.is_err() {
        log::error!("could not start {}", String::from_utf8_lossy(name));
    }
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    #[cfg(feature = "m5stickc-plus2")]
    m5stickc::hold_power(peripherals.pins.gpio4);
    #[cfg(feature = "m5stickc-plus2")]
    let imu = m5stickc::imu();
    #[cfg(not(feature = "m5stickc-plus2"))]
    let imu = None;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let storage: Arc<Mutex<dyn Storage>> = Arc::new(Mutex::new(nvs::NvsStorage));
    let clock = Arc::new(EspClock::new(bdlminer_app::settings::DEFAULT_TIMEZONE));
    let espsystem = Arc::new(EspSystem { ip: Mutex::new(String::new()), ssid: Mutex::new(String::new()), imu });
    let mut boot = [0u8; 4];
    system::random(&mut boot);
    let app = App::new(Platform {
        board: board_profile(),
        clock: clock.clone(),
        storage,
        system: espsystem.clone(),
        console: Box::new(|line| println!("{line}")),
        mac: system::mac(),
        boot_id: u32::from_le_bytes(boot),
        random: Arc::new(system::random),
    });
    let settings = app.settings();
    EspClock::new(&settings.timezone); // the configured time zone
    app.log_configuration();
    app.log(&format!("[boot] reset reason: {}", espsystem.boot_reason()));
    #[cfg(feature = "m5stickc-plus2")]
    if display::begin(&app) {
        let version = format!("v{} (Rust)", bdlminer_app::platform::FW_VERSION);
        display::message("BDLMiner V2", app.board.name, &version);
    } else {
        app.log("[ui] display could not start");
    }

    // WiFi: keep the network the driver saved (also the C++ portal's).
    let mut wifi = BlockingWifi::wrap(EspWifi::new(peripherals.modem, sysloop.clone(), Some(nvs))?, sysloop)?;
    let saved = wifi::saved_station();
    let client = match &saved {
        Some((ssid, pass)) => ClientConfiguration {
            ssid: ssid.as_str().try_into().unwrap_or_default(),
            password: pass.as_str().try_into().unwrap_or_default(),
            ..Default::default()
        },
        None => ClientConfiguration::default(),
    };
    wifi.set_configuration(&Wifi::Client(client))?;
    if let Ok(name) = std::ffi::CString::new(app.hostname.clone()) {
        let netif = unsafe { esp_idf_svc::sys::esp_netif_get_handle_from_ifkey(b"WIFI_STA_DEF\0".as_ptr().cast()) };
        if !netif.is_null() {
            unsafe { esp_idf_svc::sys::esp_netif_set_hostname(netif, name.as_ptr()) };
        }
    }
    wifi.start()?;
    unsafe { esp_idf_svc::sys::esp_wifi_set_ps(esp_idf_svc::sys::wifi_ps_type_t_WIFI_PS_NONE) };
    let mut networks: Vec<(String, String)> = saved.iter().cloned().collect();
    for (ssid, pass) in [(&settings.wifi2_ssid, &settings.wifi2_pass), (&settings.wifi3_ssid, &settings.wifi3_pass)] {
        if !ssid.is_empty() {
            networks.push((ssid.clone(), pass.clone()));
        }
    }
    // Without a payout address there is nothing useful to mine for.
    let joined = !settings.btc_address.is_empty()
        && networks.iter().enumerate().any(|(i, (ssid, pass))| {
            app.log(&format!("[wifi] trying \"{ssid}\""));
            wifi::join(ssid, pass, 15_000, i == 0 && saved.is_some())
        });
    if !joined {
        #[cfg(feature = "m5stickc-plus2")]
        display::portal(&app, &bdlminer_app::app::portal_ap_name(app.mac), &app.portal_password());
        portal::run(app.clone(), &mut wifi);
    }
    let ip = wifi::station_ip().unwrap_or_default();
    *espsystem.ip.lock().unwrap() = ip.clone();
    *espsystem.ssid.lock().unwrap() = wifi::saved_station().map(|w| w.0).unwrap_or_default();
    app.log(&format!("[wifi] connected, IP {ip}"));
    let _sntp = EspSntp::new_default()?;

    // One miner per core. With a working SHA peripheral, the hardware miner
    // gets the last core to itself (WiFi runs on core 0) at priority 19 and
    // software runs on the others, as in the C++ firmware.
    app.miner.load_stats();
    let caps = {
        let app = app.clone();
        hwsha::init(move |line| app.log(line))
    };
    let cores: &[Core] = if cfg!(any(esp32s2, esp32c3)) { &[Core::Core0] } else { &[Core::Core1, Core::Core0] };
    for (i, &core) in cores.iter().enumerate() {
        let hardware = caps.hardware && i == 0;
        let kind = if hardware { "hw" } else { "sw" };
        let Some(engine) = app.miner.register_engine(kind) else { break };
        // Original ESP32: a second work hashed in the idle cycles of the
        // hardware loop, counted as its own engine.
        let fill = if hardware && caps.fill { app.miner.register_engine("fill") } else { None };
        let miner = app.miner.clone();
        // On a single core the network needs the CPU more than any miner.
        let owns_core = hardware && cores.len() > 1;
        // A real sleep (one tick): the idle task and lower priorities run.
        let options = WorkerOptions { owns_core, pause: Duration::from_millis(1) };
        let name: &'static [u8] = if i == 0 { b"miner0\0" } else { b"miner1\0" };
        let priority = if owns_core { 19 } else { 1 };
        spawn_pinned(name, core, priority, 8 * 1024, move || {
            if hardware {
                run_worker(&miner, engine, &mut hwsha::HardwareEngine::new(fill), options);
            } else {
                run_worker(&miner, engine, &mut hwsha::SoftwareEngine, options);
            }
        });
        app.log(&format!("[miner] miner{i} engine={kind} core={core:?}"));
        if fill.is_some() {
            app.log("[miner] engine=fill: a second work in the idle cycles of miner0");
        }
    }
    {
        let app = app.clone();
        spawn_pinned(b"stratum\0", Core::Core0, 3, 20 * 1024, move || {
            let net = wifi::EspNetwork {
                tcp: TcpNetwork { poll: Duration::from_millis(20), random: system::random },
                networks,
            };
            pool::run(&net, &app.pool_config, &app.miner, &app.pool, &*app.clock);
        });
    }
    {
        let app = app.clone();
        spawn_pinned(b"notify\0", Core::Core0, 1, 12 * 1024, move || loop {
            app.notifier.poll(&EspHttp);
            thread::sleep(Duration::from_secs(5));
        });
    }
    #[cfg(feature = "m5stickc-plus2")]
    let events = {
        let events = display::Events::new(app.brightness());
        let (app, system, ui) = (app.clone(), espsystem.clone(), events.clone());
        let (a, b) = (peripherals.pins.gpio37, peripherals.pins.gpio39);
        spawn_pinned(b"ui\0", Core::Core0, 2, 10 * 1024, move || display::run(app, system, ui, a, b));
        events
    };
    #[cfg(not(feature = "m5stickc-plus2"))]
    let events = std::sync::Arc::new(NoIndicators);
    let _server = http::start(app.clone())?;
    app.log(&format!("[web] dashboard on http://{ip}/ (http://{}.local/)", app.hostname));

    let mut last_tick = clock.millis();
    let mut last_log = last_tick;
    loop {
        app.miner.renew_safety();
        let now = clock.millis();
        if now.wrapping_sub(last_tick) >= 1000 {
            last_tick = now;
            app.second_tick(&*events);
            if let Some(why) = app.restart_due().or_else(|| app.watch.take_restart()) {
                app.prepare_restart(&why);
                thread::sleep(Duration::from_millis(500));
                unsafe { esp_idf_svc::sys::esp_restart() };
            }
        }
        if now.wrapping_sub(last_log) >= 30_000 {
            last_log = now;
            let s = app.miner.stats();
            app.log(&format!(
                "[stats] {:.2} kH/s | shares {} ok / {} rejected | best {:.6} (ever {:.6}) | {}",
                app.miner.hashrate() / 1000.0,
                s.accepted,
                s.rejected,
                s.session_best_diff,
                s.best_diff,
                app.pool.status()
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}
