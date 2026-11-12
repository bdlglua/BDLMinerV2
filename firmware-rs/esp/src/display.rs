//! The screen and buttons of the boards with a display (feature `display`):
//! the C++ firmware's renderer (src/ui_view.cpp, built by
//! components/bdl_ui) draws the pages that `bdlminer_app::screen` builds, on
//! the panel of the board (M5GFX's detection on M5Stack devices, the C++
//! firmware's panels on the LilyGO and CYD boards).
//!
//! Button A: next page; held 5 s: factory reset. Button B: brightness
//! 100 / 60 / 30 / 10 / off. On a touch panel, a tap is A and a press of
//! 1 s is B. Either dismisses a block alert.

use std::ffi::{c_char, CString};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bdlminer_app::platform::{SystemInfo, CLOCK_SET, FW_VERSION};
use bdlminer_app::screen::{self, Inputs, Model, Samples, BRIGHTNESS_STEPS, PAGES};
use bdlminer_app::watch::Indicators;
use bdlminer_app::App;
use bdlminer_core::display_config;

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

/// The panel of the board (components/bdl_ui/include/bdl_ui.h).
#[cfg(any(feature = "m5stickc-plus2", feature = "m5stack"))]
const PANEL: i32 = 0;
#[cfg(feature = "lilygo-t-display")]
const PANEL: i32 = 1;
#[cfg(any(feature = "lilygo-t-display-s3", feature = "nerdnos"))]
const PANEL: i32 = 2;
#[cfg(feature = "cyd")]
const PANEL: i32 = 3;
#[cfg(feature = "gamma601")]
const PANEL: i32 = 4;

extern "C" {
    fn bdl_ui_begin(panel: i32, rotation: u8, accent: u32, language: u8) -> bool;
    fn bdl_ui_controls(button_a: *mut i32, button_b: *mut i32, touch: *mut bool);
    fn bdl_ui_touched() -> bool;
    #[cfg(feature = "gamma601")]
    fn bdl_ui_lines(line1: *const c_char, line2: *const c_char, line3: *const c_char);
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
    *started = unsafe { bdl_ui_begin(PANEL, s.rotation, screen::accent(s.theme), s.language) };
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

/// A button wired to ground (active low). GPIO34-39 of the ESP32 have no
/// pull-up of their own: the boards carry one.
struct Button {
    pin: i32,
    since: Option<u32>,
}

impl Button {
    fn new(pin: i32) -> Option<Self> {
        use esp_idf_svc::sys::{
            gpio_config, gpio_config_t, gpio_mode_t_GPIO_MODE_INPUT, gpio_pullup_t_GPIO_PULLUP_ENABLE,
        };
        if pin < 0 {
            return None;
        }
        let config = gpio_config_t {
            pin_bit_mask: 1u64 << pin,
            mode: gpio_mode_t_GPIO_MODE_INPUT,
            pull_up_en: gpio_pullup_t_GPIO_PULLUP_ENABLE,
            ..unsafe { std::mem::zeroed() }
        };
        // Input-only pins refuse the pull-up: configure them without it.
        if unsafe { gpio_config(&config) } != 0 {
            let plain = gpio_config_t { pull_up_en: 0, ..config };
            unsafe { gpio_config(&plain) };
        }
        Some(Button { pin, since: None })
    }
    fn pressed(&self) -> bool {
        unsafe { esp_idf_svc::sys::gpio_get_level(self.pin) == 0 }
    }
}

/// Touch panels: a tap (released before 1 s) is button A, a press of 1 s
/// is button B, as in the C++ firmware.
#[derive(Default)]
struct Touch {
    since: Option<u32>,
    held: bool,
}

impl Touch {
    /// 1: tap, 2: long press (once, while still pressed).
    fn update(&mut self, now: u32, pressed: bool) -> u8 {
        match (pressed, self.since) {
            (true, None) => {
                self.since = Some(now);
                self.held = false;
                0
            }
            (true, Some(t)) if !self.held && now.wrapping_sub(t) >= 1000 => {
                self.held = true;
                2
            }
            (false, Some(t)) => {
                self.since = None;
                u8::from(!self.held && now.wrapping_sub(t) >= 30)
            }
            _ => 0,
        }
    }
}

const LONG_PRESS_MS: u32 = 5000;

/// The screen task: buttons every 20 ms, a redraw every second.
pub fn run(app: Arc<App>, system: Arc<EspSystem>, events: Arc<Events>) {
    let (mut pin_a, mut pin_b, mut has_touch) = (-1, -1, false);
    unsafe { bdl_ui_controls(&mut pin_a, &mut pin_b, &mut has_touch) };
    let mut a = Button::new(pin_a);
    let mut b = Button::new(pin_b);
    let mut touch = Touch::default();
    app.log(&format!(
        "[ui] controls: button A {}, button B {}, touch {}",
        if pin_a >= 0 { format!("GPIO{pin_a}") } else { "none".into() },
        if pin_b >= 0 { format!("GPIO{pin_b}") } else { "none".into() },
        if has_touch { "yes" } else { "no" }
    ));
    let mut page = 0u32;
    let mut samples = Samples::default();
    let mut last_render = 0u32;
    let mut backlight = events.backlight.load(Ordering::Relaxed);
    let mut alert_shown = false;
    let mut reset_fired = false;
    loop {
        let now = millis();
        let mut redraw = false;
        let gesture = if has_touch { touch.update(now, unsafe { bdl_ui_touched() }) } else { 0 };
        let mut click_a = gesture == 1;
        let mut click_b = gesture == 2;
        // A: next page on release, factory reset when held 5 s.
        if let Some(a) = a.as_mut() {
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
                    click_a |= !std::mem::take(&mut reset_fired);
                }
                _ => {}
            }
        }
        // B: brightness steps.
        if let Some(b) = b.as_mut() {
            match (b.pressed(), b.since) {
                (true, None) => b.since = Some(now),
                (false, Some(_)) => {
                    b.since = None;
                    click_b = true;
                }
                _ => {}
            }
        }
        if click_a {
            if alert_shown {
                alert_shown = false;
                events.block.store(0, Ordering::Relaxed);
            } else {
                page = display_config::next_page(page, PAGES, app.settings().screen_pages);
            }
            invalidate();
            redraw = true;
        }
        if click_b {
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
                    #[cfg(not(feature = "gamma601"))]
                    render(&screen::build(page, &s, &inputs(&app, &system, &events), &samples));
                    #[cfg(feature = "gamma601")]
                    gamma_screen(&app, &system);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The Gamma's 128x32 OLED: hashrate, temperatures and clock, then the
/// address or the ASIC's state, as the C++ firmware's ui_gamma.cpp sums up.
#[cfg(feature = "gamma601")]
fn gamma_screen(app: &App, system: &EspSystem) {
    let s = crate::asic::status();
    let rate = app.miner.hashrate_live() / 1e9;
    let line1 = format!("{rate:.1} GH/s  {:.0} MHz", s.frequency);
    let asic = if s.temperature.is_finite() { format!("{:.0}C", s.temperature) } else { "--".into() };
    let vr = if s.regulator_temperature.is_finite() { format!("{:.0}C", s.regulator_temperature) } else { "--".into() };
    let line2 = format!("ASIC {asic} VR {vr} {}rpm", s.rpm);
    let ip = system.ip();
    let line3 = if s.running && !ip.is_empty() { ip } else { s.state() };
    let _guard = LOCK.lock().unwrap();
    let (a, b, c3) = (c(&line1), c(&line2), c(&line3));
    unsafe { bdl_ui_lines(a.as_ptr(), b.as_ptr(), c3.as_ptr()) };
}
