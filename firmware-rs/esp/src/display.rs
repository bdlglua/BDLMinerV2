//! The M5StickC Plus2 screen and buttons (feature `m5stickc-plus2`): the C++
//! firmware's renderer (src/ui_view.cpp, built by components/bdl_ui) draws
//! the pages that `bdlminer_app::screen` builds, as on the C++ firmware.
//!
//! Button A (front): next page; held 5 s: factory reset. Button B (side):
//! brightness 100 / 60 / 30 / 10 / off. Either dismisses a block alert.

use std::ffi::{c_char, CString};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bdlminer_app::platform::{SystemInfo, CLOCK_SET, FW_VERSION};
use bdlminer_app::screen::{self, Inputs, Model, Samples, BRIGHTNESS_STEPS, PAGES};
use bdlminer_app::watch::Indicators;
use bdlminer_app::App;
use bdlminer_core::display_config;
use esp_idf_svc::hal::gpio::{Gpio37, Gpio39, Input, InputPin, PinDriver};

use crate::system::{millis, EspSystem};

#[repr(C)]
struct CMetric {
    label: *const c_char,
    value: *const c_char,
    detail: *const c_char,
    color: u32,
}

#[repr(C)]
struct CPoint {
    ms: u32,
    rate: f32,
}

#[repr(C)]
struct CModel {
    page: i32,
    language: u8,
    accent: u32,
    status_color: u32,
    title: *const c_char,
    status: *const c_char,
    battery: *const c_char,
    footer: *const c_char,
    metrics: [CMetric; 6],
    points: *const CPoint,
    point_count: i32,
    graph_revision: u32,
}

extern "C" {
    fn bdl_ui_begin(rotation: u8, accent: u32, language: u8) -> bool;
    fn bdl_ui_brightness(percent: u8);
    fn bdl_ui_invalidate();
    fn bdl_ui_render(model: *const CModel);
    fn bdl_ui_message(title: *const c_char, line1: *const c_char, line2: *const c_char);
    fn bdl_ui_portal(title: *const c_char, ssid: *const c_char, password: *const c_char, qr: *const c_char);
    fn bdl_ui_alert(title: *const c_char, value: *const c_char, line: *const c_char);
}

/// The renderer is not thread-safe: every call goes through this lock.
static LOCK: Mutex<bool> = Mutex::new(false);

fn c(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

/// Starts the panel at the configured brightness. False without a screen.
pub fn begin(app: &App) -> bool {
    let s = app.settings();
    let mut started = LOCK.lock().unwrap();
    *started = unsafe { bdl_ui_begin(s.rotation, screen::accent(s.theme), s.language) };
    if *started {
        unsafe { bdl_ui_brightness(app.brightness()) };
    }
    *started
}

pub fn message(title: &str, line1: &str, line2: &str) {
    let started = LOCK.lock().unwrap();
    if *started {
        let (t, a, b) = (c(title), c(line1), c(line2));
        unsafe { bdl_ui_message(t.as_ptr(), a.as_ptr(), b.as_ptr()) };
    }
}

/// The setup portal: its network, password and a QR code to join it.
pub fn portal(app: &App, ssid: &str, password: &str) {
    let started = LOCK.lock().unwrap();
    if *started {
        let lang = app.settings().language;
        let qr = c(&format!("WIFI:T:WPA;S:{ssid};P:{password};;"));
        let (t, n, p) = (c(screen::tr("WiFi setup", lang)), c(ssid), c(password));
        unsafe { bdl_ui_portal(t.as_ptr(), n.as_ptr(), p.as_ptr(), qr.as_ptr()) };
    }
}

fn render(model: &Model) {
    let started = LOCK.lock().unwrap();
    if !*started {
        return;
    }
    let texts: Vec<[CString; 3]> = model.metrics.iter().map(|m| [c(&m.label), c(&m.value), c(&m.detail)]).collect();
    let metrics: [CMetric; 6] = std::array::from_fn(|i| CMetric {
        label: texts[i][0].as_ptr(),
        value: texts[i][1].as_ptr(),
        detail: texts[i][2].as_ptr(),
        color: model.metrics[i].color,
    });
    let points: Vec<CPoint> = model.points.iter().map(|&(ms, rate)| CPoint { ms, rate }).collect();
    let (title, status, battery, footer) = (c(&model.title), c(&model.status), c(&model.battery), c(&model.footer));
    let cm = CModel {
        page: model.page as i32,
        language: model.language,
        accent: model.accent,
        status_color: model.status_color,
        title: title.as_ptr(),
        status: status.as_ptr(),
        battery: battery.as_ptr(),
        footer: footer.as_ptr(),
        metrics,
        points: points.as_ptr(),
        point_count: points.len() as i32,
        graph_revision: model.graph_revision,
    };
    unsafe { bdl_ui_render(&cm) };
}

fn alert(title: &str, value: &str, line: &str) {
    let started = LOCK.lock().unwrap();
    if *started {
        let (t, v, l) = (c(title), c(value), c(line));
        unsafe { bdl_ui_alert(t.as_ptr(), v.as_ptr(), l.as_ptr()) };
    }
}

fn set_brightness(percent: u8) {
    if *LOCK.lock().unwrap() {
        unsafe { bdl_ui_brightness(percent) };
    }
}

fn invalidate() {
    if *LOCK.lock().unwrap() {
        unsafe { bdl_ui_invalidate() };
    }
}

/// What the supervision reports to the screen (`App::second_tick`).
pub struct Events {
    /// Height + 1 of a block candidate to announce, 0 for none.
    block: AtomicU32,
    /// Millis until which "+1 SHARE" is shown, 0 for none.
    share_until: AtomicU32,
    /// Backlight the supervision wants (night mode, battery saver).
    backlight: AtomicU8,
}

impl Events {
    pub fn new(brightness: u8) -> Arc<Self> {
        Arc::new(Events {
            block: AtomicU32::new(0),
            share_until: AtomicU32::new(0),
            backlight: AtomicU8::new(brightness),
        })
    }
}

impl Indicators for Events {
    fn set_backlight(&self, percent: u8) {
        self.backlight.store(percent, Ordering::Relaxed);
    }
    fn block_found(&self, height: u32) {
        self.block.store(height.wrapping_add(1).max(1), Ordering::Relaxed);
    }
    fn share_accepted(&self) {
        self.share_until.store(millis().wrapping_add(1600).max(1), Ordering::Relaxed);
    }
}

/// Local "HH:MM" and "Thu 23 Oct" (French: "jeu 23 oct").
fn local_clock(language: u8) -> Option<(String, String)> {
    use esp_idf_svc::sys::{localtime_r, time, time_t, tm};
    let mut now: time_t = 0;
    unsafe { time(&mut now) };
    if (now as i64) < CLOCK_SET as i64 {
        return None;
    }
    let mut t: tm = unsafe { std::mem::zeroed() };
    unsafe { localtime_r(&now, &mut t) };
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    const DAYS_FR: [&str; 7] = ["dim", "lun", "mar", "mer", "jeu", "ven", "sam"];
    const MONTHS_FR: [&str; 12] =
        ["janv", "fevr", "mars", "avr", "mai", "juin", "juil", "aout", "sept", "oct", "nov", "dec"];
    let (day, month) = (t.tm_wday.clamp(0, 6) as usize, t.tm_mon.clamp(0, 11) as usize);
    let date = if language == 1 {
        format!("{} {} {}", DAYS_FR[day], t.tm_mday, MONTHS_FR[month])
    } else {
        format!("{} {:02} {}", DAYS[day], t.tm_mday, MONTHS[month])
    };
    Some((format!("{:02}:{:02}", t.tm_hour, t.tm_min), date))
}

fn inputs(app: &App, system: &EspSystem, events: &Events) -> Inputs {
    let s = app.settings();
    let on_backup = app.pool.on_backup();
    let until = events.share_until.load(Ordering::Relaxed);
    Inputs {
        stats: app.miner.stats(),
        hashrate: app.miner.hashrate_live(),
        temperature: system.temperature(),
        limit: app.watch.limit_reason(&app.miner, app.ota_busy.load(Ordering::Relaxed)),
        power_percent: app.miner.power_percent(),
        battery: -1,
        clock: local_clock(s.language),
        pool_host: app.pool_config.endpoint(on_backup).host.clone(),
        on_backup,
        ip: system.ip(),
        board: app.board.name.into(),
        firmware: FW_VERSION.into(),
        free_heap: system.free_heap(),
        uptime: app.miner.uptime_seconds(),
        share_flash: until != 0 && (until.wrapping_sub(millis()) as i32) > 0,
    }
}

/// A button wired to ground with an external pull-up (GPIO37 and 39 are
/// input only).
struct Button {
    pin: PinDriver<'static, esp_idf_svc::hal::gpio::AnyInputPin, Input>,
    since: Option<u32>,
}

impl Button {
    fn pressed(&self) -> bool {
        self.pin.is_low()
    }
}

const LONG_PRESS_MS: u32 = 5000;

/// The screen task: buttons every 20 ms, a redraw every second.
pub fn run(app: Arc<App>, system: Arc<EspSystem>, events: Arc<Events>, a: Gpio37, b: Gpio39) {
    let input = |pin: esp_idf_svc::hal::gpio::AnyInputPin| PinDriver::input(pin).ok();
    let (Some(a), Some(b)) = (input(a.downgrade_input()), input(b.downgrade_input())) else {
        app.log("[ui] buttons unavailable");
        return;
    };
    let mut a = Button { pin: a, since: None };
    let mut b = Button { pin: b, since: None };
    let mut page = 0u32;
    let mut samples = Samples::default();
    let mut last_render = 0u32;
    let mut backlight = events.backlight.load(Ordering::Relaxed);
    let mut alert_shown = false;
    let mut reset_fired = false;
    loop {
        let now = millis();
        let mut redraw = false;
        // A: next page on release, factory reset when held 5 s.
        match (a.pressed(), a.since) {
            (true, None) => a.since = Some(now),
            (true, Some(t)) if !reset_fired && now.wrapping_sub(t) >= LONG_PRESS_MS => {
                reset_fired = true;
                app.log("[ui] button A held 5 s: factory reset");
                message("Reset", "", "");
                app.factory_reset();
                app.request_restart("factory reset (button)");
            }
            (false, Some(_)) => {
                a.since = None;
                if !std::mem::take(&mut reset_fired) {
                    if alert_shown {
                        alert_shown = false;
                        events.block.store(0, Ordering::Relaxed);
                    } else {
                        page = display_config::next_page(page, PAGES, app.settings().screen_pages);
                    }
                    invalidate();
                    redraw = true;
                }
            }
            _ => {}
        }
        // B: brightness steps.
        match (b.pressed(), b.since) {
            (true, None) => b.since = Some(now),
            (false, Some(_)) => {
                b.since = None;
                if alert_shown {
                    alert_shown = false;
                    events.block.store(0, Ordering::Relaxed);
                } else {
                    let current = app.brightness();
                    let i = BRIGHTNESS_STEPS.iter().position(|&s| s == current).map_or(0, |i| i + 1);
                    let next = BRIGHTNESS_STEPS[i % BRIGHTNESS_STEPS.len()];
                    app.set_brightness(next);
                    events.backlight.store(next, Ordering::Relaxed);
                }
                invalidate();
                redraw = true;
            }
            _ => {}
        }
        let wanted = events.backlight.load(Ordering::Relaxed);
        if wanted != backlight {
            let was_off = backlight == 0;
            backlight = wanted;
            set_brightness(wanted);
            if was_off && wanted > 0 {
                invalidate();
                redraw = true;
            }
        }
        if events.block.load(Ordering::Relaxed) != 0 && !alert_shown {
            alert_shown = true;
            redraw = true;
        }
        if redraw || now.wrapping_sub(last_render) >= 1000 {
            last_render = now;
            samples.sample(now, app.miner.hashrate_live());
            if backlight > 0 && app.brightness() > 0 {
                let s = app.settings();
                if alert_shown {
                    let height = events.block.load(Ordering::Relaxed).wrapping_sub(1);
                    let value = if height > 0 { height.to_string() } else { "!".into() };
                    alert(
                        screen::tr("BLOCK CANDIDATE", s.language),
                        &value,
                        screen::tr("Check pool / chain", s.language),
                    );
                } else {
                    if s.screen_pages & (1 << page) == 0 {
                        page = 0;
                    }
                    render(&screen::build(page, &s, &inputs(&app, &system, &events), &samples));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
