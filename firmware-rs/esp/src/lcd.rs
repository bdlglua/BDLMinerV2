//! The boards' displays, in Rust: the panels of the C++ firmware (M5GFX's
//! M5StickC Plus2 configuration, src/display_panels.h for the others), with
//! the controllers' init sequences of LovyanGFX (FreeBSD license), driven
//! through ESP-IDF's esp_lcd panel IO (SPI or Intel 8080 bus), or I2C for
//! the Bitaxe Gamma's SSD1306. They are `ui_view::Panel`s.

use std::ffi::c_void;

use bdlminer_app::ui_view::Panel;
use esp_idf_svc::sys::*;

use crate::periph::{delay_ms, output};

// ---------------------------------------------------------- backlight

/// A PWM backlight (LovyanGFX's Light_PWM): LEDC timer 3, 8 bits.
pub struct Backlight {
    channel: u32,
    /// Lowest duty that still lights the panel (`offset` of Light_PWM).
    offset: u32,
}

impl Backlight {
    pub fn new(pin: i32, channel: u32, freq: u32, offset: u32) -> Option<Self> {
        let timer = ledc_timer_config_t {
            speed_mode: ledc_mode_t_LEDC_LOW_SPEED_MODE,
            duty_resolution: ledc_timer_bit_t_LEDC_TIMER_8_BIT,
            timer_num: 3,
            freq_hz: freq,
            ..unsafe { std::mem::zeroed() } // LEDC_AUTO_CLK
        };
        let ch = ledc_channel_config_t {
            gpio_num: pin,
            speed_mode: ledc_mode_t_LEDC_LOW_SPEED_MODE,
            channel,
            timer_sel: 3,
            duty: 0,
            hpoint: 0,
            ..unsafe { std::mem::zeroed() }
        };
        let ok = unsafe { ledc_timer_config(&timer) == ESP_OK && ledc_channel_config(&ch) == ESP_OK };
        if !ok {
            log::warn!("[ui] backlight PWM on GPIO{pin} could not start");
            return None;
        }
        Some(Backlight { channel, offset })
    }

    /// 0..255, 0 = off.
    pub fn set(&self, brightness: u8) {
        let b = brightness as u32;
        let duty = if b == 0 { 0 } else { ((b * (256 - self.offset)) >> 8) + self.offset };
        unsafe {
            ledc_set_duty(ledc_mode_t_LEDC_LOW_SPEED_MODE, self.channel, duty.min(255));
            ledc_update_duty(ledc_mode_t_LEDC_LOW_SPEED_MODE, self.channel);
        }
    }
}

// -------------------------------------------------------- MIPI-DBI LCDs

const CMD_SLPOUT: u8 = 0x11;
const CMD_INVOFF: u8 = 0x20;
const CMD_INVON: u8 = 0x21;
const CMD_DISPON: u8 = 0x29;
const CMD_CASET: u8 = 0x2A;
const CMD_RASET: u8 = 0x2B;
const CMD_RAMWR: u8 = 0x2C;
const CMD_MADCTL: u8 = 0x36;
const CMD_IDMOFF: u8 = 0x38;
const CMD_COLMOD: u8 = 0x3A;
/// In an init list: the argument count is followed by a delay (ms).
const DELAY: u8 = 0x80;

const MAD_MY: u8 = 0x80;
const MAD_MX: u8 = 0x40;
const MAD_MV: u8 = 0x20;
const MAD_ML: u8 = 0x10;
const MAD_BGR: u8 = 0x08;
const MAD_MH: u8 = 0x04;

/// LovyanGFX Panel_LCD::getMadCtl.
const MADCTL: [u8; 8] = [
    0,
    MAD_MV | MAD_MX | MAD_MH,
    MAD_MX | MAD_MH | MAD_MY | MAD_ML,
    MAD_MV | MAD_MY | MAD_ML,
    MAD_MY | MAD_ML,
    MAD_MV,
    MAD_MX | MAD_MH,
    MAD_MV | MAD_MX | MAD_MY | MAD_MH | MAD_ML,
];

/// LovyanGFX Panel_ST7789.
pub const ST7789_INIT: &[u8] = &[
    0xB7, 1, 0x35, // GCTRL
    0xBB, 1, 0x28, // VCOMS (JLX240 display datasheet)
    0xC0, 1, 0x0C, // LCMCTRL
    0xC2, 2, 0x01, 0xFF, // VDVVRHEN
    0xC3, 1, 0x10, // VRHS
    0xC4, 1, 0x20, // VDVSET
    0xD0, 2, 0xa4, 0xa1, // PWCTRL1
    0xB0, 2, 0x00, 0xC0, // RAMCTRL
    0xE0, 14, 0xd0, 0x00, 0x02, 0x07, 0x0a, 0x28, 0x32, 0x44, 0x42, 0x06, 0x0e, 0x12, 0x14, 0x17, // PVGAMCTRL
    0xE1, 14, 0xd0, 0x00, 0x02, 0x07, 0x0a, 0x28, 0x31, 0x54, 0x47, 0x0e, 0x1c, 0x17, 0x1b, 0x1e, // NVGAMCTRL
    CMD_SLPOUT, DELAY, 130, CMD_IDMOFF, 0, CMD_DISPON, 0, 0xFF, 0xFF,
];

/// The C++ firmware's Panel_ILI9341 (src/display_panels.h).
pub const ILI9341_INIT: &[u8] = &[
    0xEF, 3, 0x03, 0x80, 0x02, 0xCF, 3, 0x00, 0xC1, 0x30, 0xED, 4, 0x64, 0x03, 0x12, 0x81, 0xE8, 3, 0x85, 0x00, 0x78,
    0xCB, 5, 0x39, 0x2C, 0x00, 0x34, 0x02, 0xF7, 1, 0x20, 0xEA, 2, 0x00, 0x00, 0xC0, 1, 0x23, // power control 1
    0xC1, 1, 0x10, // power control 2
    0xC5, 2, 0x3E, 0x28, // VCOM control 1
    0xC7, 1, 0x86, // VCOM control 2
    0xB1, 2, 0x00, 0x18, // frame rate
    0xB6, 3, 0x08, 0x82, 0x27, 0xF2, 1, 0x00, // 3-gamma off
    0x26, 1, 0x01, // gamma curve 1
    0xE0, 15, 0x0F, 0x31, 0x2B, 0x0C, 0x0E, 0x08, 0x4E, 0xF1, 0x37, 0x07, 0x10, 0x03, 0x0E, 0x09, 0x00, 0xE1, 15, 0x00,
    0x0E, 0x14, 0x03, 0x11, 0x07, 0x31, 0xC1, 0x48, 0x08, 0x0F, 0x0C, 0x31, 0x36, 0x0F, CMD_SLPOUT, DELAY, 120,
    CMD_IDMOFF, 0, CMD_DISPON, DELAY, 20, 0xFF, 0xFF,
];

/// A panel's geometry and controller (LovyanGFX's Panel_LCD config).
pub struct Spec {
    pub panel_width: i32,
    pub panel_height: i32,
    pub memory_width: i32,
    pub memory_height: i32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub invert: bool,
    pub init: &'static [u8],
}

/// Where the panel's controller is wired.
pub enum Bus {
    Spi {
        host: spi_host_device_t,
        mosi: i32,
        miso: i32,
        sclk: i32,
        cs: i32,
        dc: i32,
        hz: u32,
    },
    /// Intel 8080, 8 data lines.
    I80 {
        data: [i32; 8],
        wr: i32,
        rd: i32,
        dc: i32,
        cs: i32,
        hz: u32,
    },
}

const MAX_TRANSFER: usize = 8192;

pub struct Lcd {
    io: esp_lcd_panel_io_handle_t,
    spec: &'static Spec,
    width: i32,
    height: i32,
    colstart: i32,
    rowstart: i32,
    rotation: u8,
    /// Big-endian pixels for the bus, in DMA-capable memory.
    swap: *mut u16,
    pub backlight: Option<Backlight>,
}

unsafe impl Send for Lcd {}

fn check(what: &str, err: esp_err_t) -> Option<()> {
    if err == ESP_OK {
        Some(())
    } else {
        log::warn!("[ui] {what} failed: {err}");
        None
    }
}

/// Opens the bus. SPI: at `hz`, or `read_hz` for a register read first.
fn open_io(bus: &Bus, hz_override: Option<u32>) -> Option<esp_lcd_panel_io_handle_t> {
    let mut io: esp_lcd_panel_io_handle_t = std::ptr::null_mut();
    match *bus {
        Bus::Spi { host, mosi, miso, sclk, cs, dc, hz } => {
            let config = esp_lcd_panel_io_spi_config_t {
                cs_gpio_num: cs,
                dc_gpio_num: dc,
                spi_mode: 0,
                pclk_hz: hz_override.unwrap_or(hz),
                trans_queue_depth: 4,
                lcd_cmd_bits: 8,
                lcd_param_bits: 8,
                ..unsafe { std::mem::zeroed() }
            };
            let _ = (mosi, miso, sclk);
            check("LCD SPI IO", unsafe { esp_lcd_new_panel_io_spi(host as _, &config, &mut io) })?;
        }
        Bus::I80 { data, wr, rd, dc, cs, hz } => {
            output(rd, true); // reads unused: RD held high
            let mut bus_config: esp_lcd_i80_bus_config_t = unsafe { std::mem::zeroed() };
            bus_config.dc_gpio_num = dc;
            bus_config.wr_gpio_num = wr;
            bus_config.clk_src = soc_periph_lcd_clk_src_t_LCD_CLK_SRC_DEFAULT;
            bus_config.data_gpio_nums[..8].copy_from_slice(&data);
            bus_config.bus_width = 8;
            bus_config.max_transfer_bytes = MAX_TRANSFER;
            let mut i80: esp_lcd_i80_bus_handle_t = std::ptr::null_mut();
            check("LCD i80 bus", unsafe { esp_lcd_new_i80_bus(&bus_config, &mut i80) })?;
            let mut config: esp_lcd_panel_io_i80_config_t = unsafe { std::mem::zeroed() };
            config.cs_gpio_num = cs;
            config.pclk_hz = hz;
            config.trans_queue_depth = 4;
            config.lcd_cmd_bits = 8;
            config.lcd_param_bits = 8;
            config.dc_levels.set_dc_data_level(1);
            check("LCD i80 IO", unsafe { esp_lcd_new_panel_io_i80(i80, &config, &mut io) })?;
        }
    }
    Some(io)
}

/// The SPI bus of a panel, once.
fn open_spi_bus(bus: &Bus) -> Option<()> {
    let Bus::Spi { host, mosi, miso, sclk, .. } = *bus else { return Some(()) };
    let config = spi_bus_config_t {
        __bindgen_anon_1: spi_bus_config_t__bindgen_ty_1 { mosi_io_num: mosi },
        __bindgen_anon_2: spi_bus_config_t__bindgen_ty_2 { miso_io_num: miso },
        sclk_io_num: sclk,
        __bindgen_anon_3: spi_bus_config_t__bindgen_ty_3 { quadwp_io_num: -1 },
        __bindgen_anon_4: spi_bus_config_t__bindgen_ty_4 { quadhd_io_num: -1 },
        max_transfer_sz: MAX_TRANSFER as i32 + 16,
        ..unsafe { std::mem::zeroed() }
    };
    check("LCD SPI bus", unsafe { spi_bus_initialize(host, &config, spi_common_dma_t_SPI_DMA_CH_AUTO) })
}

impl Lcd {
    fn command(&self, cmd: u8, params: &[u8]) {
        let p = if params.is_empty() { std::ptr::null() } else { params.as_ptr() as *const c_void };
        unsafe { esp_lcd_panel_io_tx_param(self.io, cmd as i32, p, params.len()) };
    }

    /// Resets the panel (if `rst` is wired), sends its init sequence and
    /// clears it. `id_check`: SPI panels whose ID decides the inversion (the
    /// CYD's ST7789 variant), read first at a low clock.
    pub fn new(bus: Bus, rst: i32, spec: &'static Spec, rotation: u8, read_id: bool) -> Option<Self> {
        open_spi_bus(&bus)?;
        if rst >= 0 {
            output(rst, false);
            delay_ms(8);
            output(rst, true);
            delay_ms(64);
        }
        let mut invert = spec.invert;
        if read_id {
            if let Some(id) = read_display_id(&bus) {
                // The CYD with two USB ports carries an ST7789 instead of
                // the ILI9341: the same commands drive it, but its colours
                // come out inverted. Its ID (RDDID, 04h: 85 85 52) tells
                // them apart.
                if id[0] == 0x85 && id[2] == 0x52 {
                    invert = !invert;
                    log::warn!("[ui] ST7789 display (CYD with two USB ports): colours inverted");
                }
            }
        }
        let io = open_io(&bus, None)?;
        let swap = unsafe { heap_caps_malloc(MAX_TRANSFER, MALLOC_CAP_DMA | MALLOC_CAP_8BIT) } as *mut u16;
        if swap.is_null() {
            log::warn!("[ui] no DMA memory for the display");
            return None;
        }
        let mut lcd = Lcd {
            io,
            spec,
            width: spec.panel_width,
            height: spec.panel_height,
            colstart: 0,
            rowstart: 0,
            rotation: 0,
            swap,
            backlight: None,
        };
        let mut list = spec.init;
        while list.len() >= 2 && !(list[0] == 0xFF && list[1] == 0xFF) {
            let (cmd, n) = (list[0], list[1]);
            let count = (n & !DELAY) as usize;
            lcd.command(cmd, &list[2..2 + count]);
            list = &list[2 + count..];
            if n & DELAY != 0 {
                let ms = list[0];
                delay_ms(if ms == 255 { 500 } else { ms as u32 });
                list = &list[1..];
            }
        }
        lcd.command(if invert { CMD_INVON } else { CMD_INVOFF }, &[]);
        lcd.set_rotation(rotation);
        let (w, h) = (lcd.width, lcd.height);
        lcd.fill(0, 0, w, h, 0);
        Some(lcd)
    }

    /// LovyanGFX Panel_LCD::setRotation: 0..3 (4..7 mirrored).
    pub fn set_rotation(&mut self, r: u8) {
        let r = r & 7;
        self.rotation = r;
        let s = self.spec;
        let (mut ox, mut oy, mut pw, mut ph, mut mw, mut mh) =
            (s.offset_x, s.offset_y, s.panel_width, s.panel_height, s.memory_width, s.memory_height);
        if r & 1 != 0 {
            std::mem::swap(&mut ox, &mut oy);
            std::mem::swap(&mut pw, &mut ph);
            std::mem::swap(&mut mw, &mut mh);
        }
        self.width = pw;
        self.height = ph;
        self.colstart = if r & 2 != 0 { mw - (pw + ox) } else { ox };
        self.rowstart = if (1u32 << r) & 0b1001_0110 != 0 { mh - (ph + oy) } else { oy };
        self.command(CMD_COLMOD, &[0x55]); // RGB565
        self.command(CMD_MADCTL, &[MADCTL[r as usize] | MAD_BGR]);
    }

    pub fn rotation(&self) -> u8 {
        self.rotation
    }

    fn window(&self, x: i32, y: i32, w: i32, h: i32) {
        let (xs, xe) = ((x + self.colstart) as u16, (x + w - 1 + self.colstart) as u16);
        let (ys, ye) = ((y + self.rowstart) as u16, (y + h - 1 + self.rowstart) as u16);
        let [a, b] = xs.to_be_bytes();
        let [c, d] = xe.to_be_bytes();
        self.command(CMD_CASET, &[a, b, c, d]);
        let [a, b] = ys.to_be_bytes();
        let [c, d] = ye.to_be_bytes();
        self.command(CMD_RASET, &[a, b, c, d]);
    }

    /// Sends `count` pixels from the swap buffer: RAMWR, then a NOP that
    /// waits for the transfer (the buffer is reused right after).
    fn send(&self, first: bool, count: usize) {
        let cmd = if first { CMD_RAMWR as i32 } else { -1 };
        unsafe {
            esp_lcd_panel_io_tx_color(self.io, cmd, self.swap as *const c_void, count * 2);
            esp_lcd_panel_io_tx_param(self.io, -1, std::ptr::null(), 0);
        }
    }

    fn buffer(&self) -> &mut [u16] {
        unsafe { std::slice::from_raw_parts_mut(self.swap, MAX_TRANSFER / 2) }
    }
}

/// RDDID (04h): three bytes after one dummy clock, read at 6 MHz (both CYD
/// chips read at 6.6 MHz at most).
fn read_display_id(bus: &Bus) -> Option<[u8; 3]> {
    if !matches!(bus, Bus::Spi { miso, .. } if *miso >= 0) {
        return None;
    }
    let io = open_io(bus, Some(6_000_000))?;
    let mut raw = [0u8; 4];
    let ok = unsafe { esp_lcd_panel_io_rx_param(io, 0x04, raw.as_mut_ptr() as *mut c_void, 4) } == ESP_OK;
    unsafe { esp_lcd_panel_io_del(io) };
    if !ok {
        return None;
    }
    let bits = u32::from_be_bytes(raw) << 1;
    let b = bits.to_be_bytes();
    Some([b[0], b[1], b[2]])
}

impl Panel for Lcd {
    fn width(&self) -> i32 {
        self.width
    }

    fn height(&self) -> i32 {
        self.height
    }

    fn push(&mut self, x: i32, y: i32, w: i32, h: i32, pixels: &[u16]) {
        self.window(x, y, w, h);
        let buf = self.buffer();
        let total = (w * h) as usize;
        let mut first = true;
        for chunk in pixels[..total].chunks(buf.len()) {
            for (d, s) in buf.iter_mut().zip(chunk) {
                *d = s.swap_bytes();
            }
            self.send(first, chunk.len());
            first = false;
        }
    }

    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, color: u16) {
        self.window(x, y, w, h);
        let buf = self.buffer();
        buf.fill(color.swap_bytes());
        let mut left = (w * h) as usize;
        let mut first = true;
        while left > 0 {
            let n = left.min(buf.len());
            self.send(first, n);
            first = false;
            left -= n;
        }
    }
}

// ----------------------------------------------------------- SSD1306

/// The Bitaxe Gamma's 128x32 OLED on I2C (LovyanGFX Panel_SSD1306 with COM
/// pins 0x02, as src/display_panels.h): a 1-bit frame buffer, sent by
/// pages; rotations 2 and 3 turn it upside down.
pub struct Ssd1306 {
    i2c: crate::periph::I2c,
    buf: [u8; 128 * 4],
    flipped: bool,
}

const SSD1306: u8 = 0x3c;

impl Ssd1306 {
    pub fn new(port: i32, sda: i32, scl: i32, rotation: u8) -> Option<Self> {
        let i2c = crate::periph::I2c::new(port, sda, scl, &[SSD1306])?;
        let mut oled = Ssd1306 { i2c, buf: [0; 512], flipped: rotation & 2 != 0 };
        const INIT: &[u8] = &[
            0xAE, // display off
            0xD5, 0x80, // clock divide
            0xA8, 0x3F, // multiplex
            0xD3, 0x00, // display offset
            0x40, // start line
            0x20, 0x00, // horizontal addressing
            0xA0, // segment remap
            0xC0, // COM scan increasing
            0xDB, 0x10, // VCOM detect
            0xA4, // display follows RAM
            0x2E, // no scrolling
            0x8D, 0x14, // charge pump
            0xAF, // display on
            0x81, 0x00, // contrast
            0xD9, 0x11, // precharge
            0xA6, // not inverted
            0xDA, 0x02, // COM pins
        ];
        if !oled.commands(INIT) {
            log::warn!("[ui] SSD1306 not answering");
            return None;
        }
        // LovyanGFX's default brightness after init.
        oled.set_brightness(127);
        oled.flush();
        Some(oled)
    }

    fn commands(&self, bytes: &[u8]) -> bool {
        let mut message = Vec::with_capacity(bytes.len() + 1);
        message.push(0x00);
        message.extend_from_slice(bytes);
        self.i2c.write(SSD1306, &message)
    }

    /// LovyanGFX Panel_SSD1306::setBrightness (0..255).
    pub fn set_brightness(&self, b: u8) {
        let precharge = ((b as u32 + 15) / 17 * 0x11) as u8;
        self.commands(&[0xD9, precharge, 0xDB, b >> 1, 0x81, b]);
    }

    pub fn set_rotation(&mut self, rotation: u8) {
        self.flipped = rotation & 2 != 0;
    }

    /// Display off (sleep) or on.
    pub fn sleep(&self, asleep: bool) {
        self.commands(&[if asleep { 0xAE } else { 0xAF }]);
    }

    fn flush(&self) {
        if !self.commands(&[0x21, 0, 127, 0x22, 0, 3]) {
            return;
        }
        for page in self.buf.chunks(128) {
            let mut message = [0u8; 129];
            message[0] = 0x40;
            message[1..].copy_from_slice(page);
            self.i2c.write(SSD1306, &message);
        }
    }

    fn set(&mut self, x: i32, y: i32, on: bool) {
        let (x, y) = if self.flipped { (127 - x, 31 - y) } else { (x, y) };
        let (i, bit) = ((y / 8 * 128 + x) as usize, 1u8 << (y % 8));
        if on {
            self.buf[i] |= bit;
        } else {
            self.buf[i] &= !bit;
        }
    }
}

/// Lit when brighter than half.
fn lit(c: u16) -> bool {
    let (r, g, b) = ((c >> 11) as u32 * 255 / 31, ((c >> 5) & 63) as u32 * 255 / 63, (c & 31) as u32 * 255 / 31);
    r * 299 + g * 587 + b * 114 >= 128_000
}

impl Panel for Ssd1306 {
    fn width(&self) -> i32 {
        128
    }

    fn height(&self) -> i32 {
        32
    }

    fn push(&mut self, x: i32, y: i32, w: i32, h: i32, pixels: &[u16]) {
        for row in 0..h {
            for col in 0..w {
                self.set(x + col, y + row, lit(pixels[(row * w + col) as usize]));
            }
        }
        self.flush();
    }

    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, color: u16) {
        let on = lit(color);
        for row in y..y + h {
            for col in x..x + w {
                self.set(col, row, on);
            }
        }
        self.flush();
    }
}
