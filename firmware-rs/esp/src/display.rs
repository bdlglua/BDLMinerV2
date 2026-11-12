//! The screen and buttons of the boards with a display (feature `display`):
//! the renderer (`bdlminer_app::ui_view`, ported from the C++ firmware's
//! src/ui_view.cpp) draws the pages that `bdlminer_app::screen` builds, on
//! the panel of the board (src/lcd.rs; M5GFX's board detection on the
//! M5Stack image, components/bdl_ui).
//!
//! Button A: next page; held 5 s: factory reset. Button B: brightness
//! 100 / 60 / 30 / 10 / off. On a touch panel, a tap is A and a press of
//! 1 s is B. Either dismisses a block alert.

use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bdlminer_app::platform::{SystemInfo, CLOCK_SET, FW_VERSION};
use bdlminer_app::screen::{self, Inputs, Model, Samples, BRIGHTNESS_STEPS, PAGES};
use bdlminer_app::ui_view::{Panel, View};
use bdlminer_app::watch::Indicators;
use bdlminer_app::App;
use bdlminer_core::display_config;

use crate::system::{millis, EspSystem};

/// A board's display: pixels, backlight and rotation.
trait Screen: Panel + Send {
    /// 0..255, 0 = off.
    fn brightness(&mut self, value: u8);
}

#[cfg(not(any(feature = "m5stack", feature = "gamma601")))]
impl Screen for crate::lcd::Lcd {
    fn brightness(&mut self, value: u8) {
        if let Some(b) = &self.backlight {
            b.set(value);
        }
    }
}

#[cfg(feature = "gamma601")]
impl Screen for crate::lcd::Ssd1306 {
    fn brightness(&mut self, value: u8) {
        self.set_brightness(value);
    }
}

/// M5GFX's board detection and panels (components/bdl_ui).
#[cfg(feature = "m5stack")]
mod m5 {
    extern "C" {
        pub fn bdl_m5_begin(rotation: u8) -> bool;
        pub fn bdl_m5_width() -> i32;
        pub fn bdl_m5_height() -> i32;
        pub fn bdl_m5_push(x: i32, y: i32, w: i32, h: i32, pixels: *const u16);
        pub fn bdl_m5_fill(x: i32, y: i32, w: i32, h: i32, color: u16);
        pub fn bdl_m5_controls(button_a: *mut i32, button_b: *mut i32, touch: *mut bool);
        pub fn bdl_m5_touched() -> bool;
        pub fn bdl_m5_brightness(value: u8);
    }

    pub struct M5;

    impl bdlminer_app::ui_view::Panel for M5 {
        fn width(&self) -> i32 {
            unsafe { bdl_m5_width() }
        }
        fn height(&self) -> i32 {
            unsafe { bdl_m5_height() }
        }
        fn push(&mut self, x: i32, y: i32, w: i32, h: i32, pixels: &[u16]) {
            unsafe { bdl_m5_push(x, y, w, h, pixels.as_ptr()) };
        }
        fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, color: u16) {
            unsafe { bdl_m5_fill(x, y, w, h, color) };
        }
    }

    impl super::Screen for M5 {
        fn brightness(&mut self, value: u8) {
            unsafe { bdl_m5_brightness(value) };
        }
    }
}

/// The panel of the board, started at `rotation`.
fn open(rotation: u8) -> Option<Box<dyn Screen>> {
    #[cfg(feature = "m5stack")]
    return unsafe { m5::bdl_m5_begin(rotation) }.then(|| Box::new(m5::M5) as Box<dyn Screen>);
    #[cfg(feature = "gamma601")]
    return crate::lcd::Ssd1306::new(0, 47, 48, rotation).map(|p| Box::new(p) as Box<dyn Screen>);
    #[cfg(not(any(feature = "m5stack", feature = "gamma601")))]
    {
        use crate::lcd::{Backlight, Bus, Lcd, Spec, ILI9341_INIT, ST7789_INIT};
        use esp_idf_svc::sys::*;
        // ST7789 135x240 (M5GFX's M5StickC Plus2 configuration and the
        // C++ firmware's T-Display).
        #[cfg(any(feature = "m5stickc-plus2", feature = "lilygo-t-display"))]
        static SPEC: Spec = Spec {
            panel_width: 135,
            panel_height: 240,
            memory_width: 240,
            memory_height: 320,
            offset_x: 52,
            offset_y: 40,
            invert: true,
            init: ST7789_INIT,
        };
        #[cfg(any(feature = "lilygo-t-display-s3", feature = "nerdnos"))]
        static SPEC: Spec = Spec {
            panel_width: 170,
            panel_height: 320,
            memory_width: 240,
            memory_height: 320,
            offset_x: 35,
            offset_y: 0,
            invert: true,
            init: ST7789_INIT,
        };
        #[cfg(feature = "cyd")]
        static SPEC: Spec = Spec {
            panel_width: 240,
            panel_height: 320,
            memory_width: 240,
            memory_height: 320,
            offset_x: 0,
            offset_y: 0,
            invert: false,
            init: ILI9341_INIT,
        };
        let _ = (&ILI9341_INIT, &ST7789_INIT);
        #[cfg(feature = "m5stickc-plus2")]
        let (bus, rst, bl, read_id) = (
            Bus::Spi { host: spi_host_device_t_SPI2_HOST, mosi: 15, miso: -1, sclk: 13, cs: 5, dc: 14, hz: 40_000_000 },
            12,
            Backlight::new(27, 7, 256, 40),
            false,
        );
        #[cfg(feature = "lilygo-t-display")]
        let (bus, rst, bl, read_id) = (
            Bus::Spi { host: spi_host_device_t_SPI3_HOST, mosi: 19, miso: -1, sclk: 18, cs: 5, dc: 16, hz: 40_000_000 },
            23,
            Backlight::new(4, 7, 1200, 0),
            false,
        );
        #[cfg(any(feature = "lilygo-t-display-s3", feature = "nerdnos"))]
        let (bus, rst, bl, read_id) = (
            Bus::I80 { data: [39, 40, 41, 42, 45, 46, 47, 48], wr: 8, rd: 9, dc: 7, cs: 6, hz: 20_000_000 },
            5,
            Backlight::new(38, 7, 1200, 0),
            false,
        );
        #[cfg(feature = "cyd")]
        let (bus, rst, bl, read_id) = (
            Bus::Spi { host: spi_host_device_t_SPI2_HOST, mosi: 13, miso: 12, sclk: 14, cs: 15, dc: 2, hz: 40_000_000 },
            -1,
            Backlight::new(21, 7, 1200, 0),
            true,
        );
        let mut lcd = Lcd::new(bus, rst, &SPEC, rotation, read_id)?;
        lcd.backlight = bl;
        Some(Box::new(lcd))
    }
}

/// The board's buttons (GPIO, -1 for none; active low) and whether it has a
/// touch panel, as in the C++ firmware.
fn controls() -> (i32, i32, bool) {
    #[cfg(feature = "m5stack")]
    {
        let (mut a, mut b, mut touch) = (-1, -1, false);
        unsafe { m5::bdl_m5_controls(&mut a, &mut b, &mut touch) };
        (a, b, touch)
    }
    #[cfg(feature = "m5stickc-plus2")]
    return (37, 39, false);
    #[cfg(feature = "lilygo-t-display")]
    return (35, 0, false);
    #[cfg(any(feature = "lilygo-t-display-s3", feature = "nerdnos"))]
    return (14, 0, false);
    #[cfg(feature = "cyd")]
    return (0, -1, true);
    #[cfg(feature = "gamma601")]
    return (0, -1, false);
}

fn touched() -> bool {
    #[cfg(feature = "m5stack")]
    return unsafe { m5::bdl_m5_touched() };
    #[cfg(feature = "cyd")]
    return cyd_touch::pressed();
    #[cfg(not(any(feature = "m5stack", feature = "cyd")))]
    false
}

/// XPT2046 resistive touch of the ESP32-2432S028R, on its own pins, as in
/// the C++ firmware (board.cpp): only "pressed or not", from its pressure
/// (Z1) measurement, with no calibration and the display's SPI bus left
/// alone.
#[cfg(feature = "cyd")]
mod cyd_touch {
    use esp_idf_svc::sys::{gpio_get_level, gpio_set_level};
    use std::sync::atomic::{AtomicBool, Ordering};

    const CLK: i32 = 25;
    const MOSI: i32 = 32;
    const MISO: i32 = 39;
    const CS: i32 = 33;
    static READY: AtomicBool = AtomicBool::new(false);

    fn transfer(command: u8) -> u16 {
        unsafe {
            gpio_set_level(CS, 0);
            for bit in (0..8).rev() {
                gpio_set_level(MOSI, ((command >> bit) & 1) as u32);
                gpio_set_level(CLK, 1);
                gpio_set_level(CLK, 0);
            }
            let mut value = 0u16;
            for _ in 0..16 {
                gpio_set_level(CLK, 1);
                value = value << 1 | gpio_get_level(MISO) as u16;
                gpio_set_level(CLK, 0);
            }
            gpio_set_level(CS, 1);
            (value >> 3) & 0xFFF
        }
    }

    pub fn pressed() -> bool {
        if !READY.swap(true, Ordering::Relaxed) {
            crate::periph::output(CS, true);
            crate::periph::output(CLK, false);
            crate::periph::output(MOSI, false);
            crate::periph::input(MISO);
        }
        transfer(0xB0) > 200 // Z1, 12 bits, pen interrupt left enabled
    }
}

/// The renderer and its panel: every call goes through this lock.
struct Display {
    screen: Box<dyn Screen>,
    view: View,
}

static DISPLAY: Mutex<Option<Display>> = Mutex::new(None);

fn with(f: impl FnOnce(&mut dyn Screen, &mut View)) {
    if let Some(d) = DISPLAY.lock().unwrap().as_mut() {
        f(&mut *d.screen, &mut d.view);
    }
}

/// Starts the panel at the configured brightness. False without a screen.
pub fn begin(app: &App) -> bool {
    let s = app.settings();
    let mut display = DISPLAY.lock().unwrap();
    if display.is_none() {
        match open(s.rotation) {
            Some(screen) => {
                *display = Some(Display { screen, view: View::new(screen::accent(s.theme), s.language) });
            }
            None => {
                app.log("[ui] display not found");
                return false;
            }
        }
    }
    let d = display.as_mut().unwrap();
    d.screen.brightness(percent_to_level(app.brightness()));
    true
}

fn percent_to_level(percent: u8) -> u8 {
    if percent > 100 {
        255
    } else {
        (percent as u32 * 255 / 100) as u8
    }
}

pub fn message(title: &str, line1: &str, line2: &str) {
    with(|panel, view| view.message(panel, title, line1, line2));
}

/// The setup portal: its network, password and a QR code to join it.
pub fn portal(app: &App, ssid: &str, password: &str) {
    let lang = app.settings().language;
    let qr = format!("WIFI:T:WPA;S:{ssid};P:{password};;");
    with(|panel, view| view.portal(panel, screen::tr("WiFi setup", lang), ssid, password, &qr));
}

fn render(model: &Model) {
    with(|panel, view| view.render(panel, model));
}

fn alert(title: &str, value: &str, line: &str) {
    with(|panel, view| view.alert(panel, title, value, line));
}

fn set_brightness(percent: u8) {
    with(|panel, _| panel.brightness(percent_to_level(percent)));
}

fn invalidate() {
    with(|_, view| view.invalidate());
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
    let (pin_a, pin_b, has_touch) = controls();
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
        let gesture = if has_touch { touch.update(now, touched()) } else { 0 };
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
    with(|panel, view| view.lines(panel, &[&line1, &line2, &line3]));
}
