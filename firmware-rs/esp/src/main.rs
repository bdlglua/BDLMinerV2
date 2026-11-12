//! BDLMiner V2 for ESP32 boards, in Rust on ESP-IDF.
//!
//! Boot: settings from NVS, WiFi (saved network, fallbacks, else the setup
//! portal), SNTP, the hardware SHA miner on the last core and a software
//! miner on the others, the pool task, the dashboard, then the once-a-second
//! supervision loop.
// global_asm! of the Xtensa kernels (src/hwsha.rs).
#![cfg_attr(target_arch = "xtensa", feature(asm_experimental_arch))]

#[cfg(any(feature = "gamma601", feature = "nerdnos"))]
mod asic;
mod captive;
mod crash;
#[cfg(feature = "display")]
mod display;
#[cfg(feature = "gamma601")]
mod gamma601;
mod http;
mod hwsha;
#[cfg(all(feature = "display", not(feature = "m5stack")))]
mod lcd;
#[cfg(feature = "m5stickc-plus2")]
mod m5stickc;
#[cfg(feature = "nerdnos")]
mod nerdnos;
mod nvs;
mod ota;
#[cfg(any(feature = "gamma601", feature = "nerdnos", feature = "display"))]
mod periph;
mod portal;
mod system;
mod wifi;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use bdlminer_app::app::{App, Platform};
#[cfg(not(any(feature = "gamma601", feature = "nerdnos")))]
use bdlminer_app::miner::{run_worker, WorkerOptions};
use bdlminer_app::notify::{BodySink, HttpClient};
use bdlminer_app::platform::{BoardKind, BoardProfile, Clock, Storage, SystemInfo};
use bdlminer_app::pool;
use bdlminer_app::std_platform::TcpNetwork;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::EspSntp;
use esp_idf_svc::wifi::{BlockingWifi, Configuration as Wifi};

use crate::system::{EspClock, EspSystem};

/// The board's name and C++ firmware environment (with "-rust"), as the
/// dashboard and the update channel know them.
fn board_identity() -> (&'static str, &'static str) {
    // The environment with a NUL: GitHub updates require "<env>\0" in the
    // downloaded image (the C++ firmware's board marker). Only this board's
    // string is compiled in, so an image never names another board.
    #[cfg(feature = "gamma601")]
    const ID: (&str, &str) = ("Bitaxe-Gamma-601", "bitaxe-gamma-601-rust\0");
    #[cfg(feature = "nerdnos")]
    const ID: (&str, &str) = ("NerdNOS", "nerdnos-rust\0");
    #[cfg(feature = "m5stickc-plus2")]
    const ID: (&str, &str) = ("M5StickC Plus2", "m5stickc-plus2-rust\0");
    #[cfg(all(feature = "m5stack", esp32s3))]
    const ID: (&str, &str) = ("M5Stack-S3", "m5stack-s3-rust\0");
    #[cfg(all(feature = "m5stack", not(esp32s3)))]
    const ID: (&str, &str) = ("M5Stack-ESP32", "m5stack-esp32-rust\0");
    #[cfg(feature = "lilygo-t-display-s3")]
    const ID: (&str, &str) = ("T-Display-S3", "lilygo-t-display-s3-rust\0");
    #[cfg(feature = "lilygo-t-display")]
    const ID: (&str, &str) = ("T-Display", "lilygo-t-display-rust\0");
    #[cfg(feature = "cyd")]
    const ID: (&str, &str) = ("CYD-2432S028R", "esp32-2432s028r-rust\0");
    #[cfg(not(any(
        feature = "gamma601",
        feature = "nerdnos",
        feature = "m5stickc-plus2",
        feature = "m5stack",
        feature = "lilygo-t-display-s3",
        feature = "lilygo-t-display",
        feature = "cyd"
    )))]
    const ID: (&str, &str) = if cfg!(esp32s3) {
        ("ESP32-S3", "esp32s3-rust\0")
    } else if cfg!(esp32s2) {
        ("ESP32-S2", "esp32s2-rust\0")
    } else if cfg!(esp32c3) {
        ("ESP32-C3", "esp32c3-rust\0")
    } else {
        ("ESP32", "esp32-rust\0")
    };
    let env: &'static str = std::hint::black_box(ID.1);
    (ID.0, env.trim_end_matches('\0'))
}

fn board_profile() -> BoardProfile {
    let (name, env) = board_identity();
    BoardProfile {
        name,
        env,
        kind: if cfg!(feature = "gamma601") {
            BoardKind::Gamma601
        } else if cfg!(feature = "nerdnos") {
            BoardKind::NerdNos
        } else {
            BoardKind::Esp32
        },
        m5: cfg!(feature = "m5"),
        // The screen pages the settings may choose from, as the C++ firmware
        // reports them.
        // The Gamma's 128x32 OLED has its own summary, no pages.
        display_pages: if cfg!(all(feature = "display", not(feature = "gamma601"))) {
            bdlminer_app::screen::PAGES as u8
        } else {
            0
        },
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

    /// GitHub sends release files to another host with a link that is only
    /// valid for a few minutes (refused with the non-standard 618, or 403,
    /// 404, 410): a refused link is not retried as is, the whole chain is
    /// asked again for a fresh one.
    fn get(&self, url: &str, headers: &[(&str, &str)], sink: &mut dyn BodySink) -> Result<u16, String> {
        let _hold = hwsha::Hold::new();
        let config = HttpConfiguration {
            timeout: Some(Duration::from_secs(20)),
            // GitHub's redirect links (signed tokens) exceed the default 512 bytes.
            buffer_size: Some(4096),
            buffer_size_tx: Some(1024),
            crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
            ..Default::default()
        };
        for attempt in 1..=3 {
            let mut url = url.to_owned();
            let mut redirected = false;
            let mut hops = 0;
            loop {
                hops += 1;
                if hops > 5 {
                    return Err("too many redirects".into());
                }
                // Plain HTTP only for a first address given as such (a pool's
                // statistics on the local network); never after a redirect.
                if !url.starts_with("https://") && (redirected || !url.starts_with("http://")) {
                    return Err("refused a non-HTTPS link".into());
                }
                let mut conn = EspHttpConnection::new(&config).map_err(|e| e.to_string())?;
                conn.initiate_request(esp_idf_svc::http::Method::Get, &url, headers).map_err(|e| e.to_string())?;
                conn.initiate_response().map_err(|e| e.to_string())?;
                let status = conn.status();
                if (300..400).contains(&status) {
                    if let Some(location) = conn.header("Location") {
                        url = location.to_owned();
                        redirected = true;
                        continue;
                    }
                }
                if status == 200 {
                    let length = conn.header("Content-Length").and_then(|l| l.trim().parse::<u64>().ok());
                    sink.begin(length)?;
                    let mut buf = [0u8; 2048];
                    loop {
                        let n = conn.read(&mut buf).map_err(|e| e.to_string())?;
                        if n == 0 {
                            break;
                        }
                        sink.data(&buf[..n])?;
                    }
                    return Ok(200);
                }
                let refused = redirected && matches!(status, 618 | 403 | 404 | 410);
                if !refused || attempt == 3 {
                    return Ok(status);
                }
                log::info!("download link refused (HTTP {status}); asking GitHub for a fresh one");
                thread::sleep(Duration::from_secs(2));
                break;
            }
        }
        Err("download link refused".into())
    }
}

fn spawn_pinned(
    name: &'static [u8],
    core: Core,
    priority: u8,
    stack: usize,
    f: impl FnOnce() + Send + 'static,
) -> bool {
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
    let started = spawned.is_ok();
    if !started {
        log::error!("could not start {}", String::from_utf8_lossy(name));
    }
    started
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    // Bitaxe Gamma 601: the ASIC held in reset before anything else.
    #[cfg(any(feature = "gamma601", feature = "nerdnos"))]
    asic::hold_reset();
    // T-Display-S3 (also on the NerdNOS): GPIO15 keeps the panel powered on
    // battery.
    #[cfg(any(feature = "lilygo-t-display-s3", feature = "nerdnos"))]
    unsafe {
        use esp_idf_svc::sys::{gpio_config, gpio_config_t, gpio_mode_t_GPIO_MODE_OUTPUT, gpio_set_level};
        gpio_set_level(15, 1);
        gpio_config(&gpio_config_t { pin_bit_mask: 1 << 15, mode: gpio_mode_t_GPIO_MODE_OUTPUT, ..std::mem::zeroed() });
        gpio_set_level(15, 1);
    }
    #[cfg(feature = "m5stickc-plus2")]
    m5stickc::hold_power(peripherals.pins.gpio4);
    // Started on USB, the board's screen and IMU are only powered from the
    // moment the hold is on: give them time (an ST7789 needs 120 ms) first.
    #[cfg(feature = "m5stickc-plus2")]
    thread::sleep(Duration::from_millis(300));
    #[cfg(feature = "m5stickc-plus2")]
    let (imu, sensor) = (m5stickc::imu(), "imu");
    // ASIC boards: the driver's thermometer (ASIC diode or board sensor on the
    // Gamma, NTC on the NerdNOS), which their own safety needs read.
    #[cfg(any(feature = "gamma601", feature = "nerdnos"))]
    let (imu, sensor): (Option<Box<dyn Fn() -> f32 + Send + Sync>>, _) = (Some(Box::new(asic::temperature)), "asic");
    #[cfg(not(any(feature = "m5stickc-plus2", feature = "gamma601", feature = "nerdnos")))]
    let (imu, sensor) = (None, "imu");
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let storage: Arc<Mutex<dyn Storage>> = Arc::new(Mutex::new(nvs::NvsStorage));
    let clock = Arc::new(EspClock::new(bdlminer_app::settings::DEFAULT_TIMEZONE));
    let espsystem = Arc::new(EspSystem { ip: Mutex::new(String::new()), ssid: Mutex::new(String::new()), imu, sensor });
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
    ota::begin_trial(&app);
    crash::begin(&app);
    #[cfg(feature = "display")]
    if display::begin(&app) {
        let version = format!("v{} (Rust)", bdlminer_app::platform::FW_VERSION);
        display::message("BDLMiner V2", app.board.name, &version);
    } else {
        app.log("[ui] display could not start");
    }
    // The ASIC board's regulator, sensors and chip (still off), after the
    // screen: the Gamma's shares its I2C bus.
    #[cfg(any(feature = "gamma601", feature = "nerdnos"))]
    asic::begin(&app);

    // WiFi: keep the network the driver saved (also the C++ portal's).
    let mut wifi = BlockingWifi::wrap(wifi::driver(peripherals.modem, sysloop.clone(), nvs)?, sysloop)?;
    let saved = wifi::saved_station();
    wifi.set_configuration(&Wifi::Client(wifi::station_config()))?;
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
        #[cfg(feature = "display")]
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
    // ASIC boards: the chip mines, through its Rust driver.
    #[cfg(any(feature = "gamma601", feature = "nerdnos"))]
    if !asic::start(app.clone()) {
        app.log("[miner] ASIC task could not start");
    }
    #[cfg(not(any(feature = "gamma601", feature = "nerdnos")))]
    {
        let caps = {
            let app = app.clone();
            hwsha::init(move |line| app.log(line))
        };
        // Single-core chips (ESP32-S2, ESP32-C3): one miner, next to WiFi.
        #[cfg(any(esp32, esp32s3))]
        let cores: &[Core] = &[Core::Core1, Core::Core0];
        #[cfg(not(any(esp32, esp32s3)))]
        let cores: &[Core] = &[Core::Core0];
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
    }
    // MQTT for Home Assistant, when a broker is set.
    if !settings.mqtt_host.is_empty() {
        let (app, networks) = (app.clone(), networks.clone());
        spawn_pinned(b"mqtt\0", Core::Core0, 1, 8 * 1024, move || {
            let net = wifi::EspNetwork {
                tcp: TcpNetwork { poll: Duration::from_millis(200), random: system::random },
                networks,
            };
            bdlminer_app::mqtt::run(&net, &app, &app.mqtt);
        });
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
        // Notifications and the anonymous statistics report share one HTTPS task.
        // Notifications, the anonymous statistics report and GitHub updates
        // share one HTTPS task (one TLS connection at a time).
        spawn_pinned(b"notify\0", Core::Core0, 1, 16 * 1024, move || {
            let mut share = bdlminer_app::share::Share::default();
            let mut remote = bdlminer_app::remote::Remote::default();
            let firmware = ota::EspFirmware { app: app.clone() };
            loop {
                app.notifier.poll(&EspHttp);
                share.poll(&app, &EspHttp, wifi::link_up());
                app.updater.poll(&app, &EspHttp, &firmware, wifi::link_up());
                app.market.poll(&app, &EspHttp, wifi::link_up());
                remote.poll(&app, &EspHttp, wifi::link_up());
                app.block_status.poll(&app, &EspHttp, wifi::link_up());
                thread::sleep(Duration::from_secs(5));
            }
        });
    }
    #[cfg(feature = "display")]
    let events = {
        let events = display::Events::new(app.brightness());
        let (app, system, ui) = (app.clone(), espsystem.clone(), events.clone());
        spawn_pinned(b"ui\0", Core::Core0, 2, 10 * 1024, move || display::run(app, system, ui));
        events
    };
    #[cfg(not(feature = "display"))]
    let events = std::sync::Arc::new(bdlminer_app::watch::NoIndicators);
    let _server = http::start(app.clone())?;
    // http://<host name>.local/ on the local network.
    let _mdns = match esp_idf_svc::mdns::EspMdns::take() {
        Ok(mut mdns) => {
            let ok = mdns.set_hostname(&app.hostname).is_ok()
                && mdns.set_instance_name(format!("BDLMiner {}", app.board.name)).is_ok()
                && mdns.add_service(None, "_http", "_tcp", 80, &[]).is_ok();
            if !ok {
                app.log("[web] mDNS could not announce the dashboard");
            }
            Some(mdns)
        }
        Err(e) => {
            app.log(&format!("[web] mDNS unavailable: {e}"));
            None
        }
    };
    app.log(&format!("[web] dashboard on http://{ip}/ (http://{}.local/)", app.hostname));

    let mut last_tick = clock.millis();
    let mut last_log = last_tick;
    loop {
        app.miner.renew_safety();
        let now = clock.millis();
        if now.wrapping_sub(last_tick) >= 1000 {
            last_tick = now;
            app.second_tick(&*events);
            ota::poll_trial(&app);
            // Gamma: frequency, voltage and fan as the settings now say.
            #[cfg(feature = "gamma601")]
            asic::apply_settings(&app);
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
