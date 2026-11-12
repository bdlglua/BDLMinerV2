//! The screen renderer: a port of the C++ firmware's src/ui_view.cpp and
//! src/ui_font.cpp, on any `Panel` (the boards' displays, or an image on a
//! PC). It draws the pages that `screen` builds.
//!
//! "Big number" layout: each page shows one value as large as the panel
//! allows (metrics[0]: value, its unit in `detail`), up to three secondary
//! readings below it (metrics[1..3]), the page name and the miner state in a
//! thin header, and the page position as dots.
//!
//! Every field is painted in strips of at most 4096 pixels (8 KiB), then
//! pushed to the panel in one transfer; fields that did not change are not
//! painted again.

use crate::screen::{Metric, Model};

/// The pre-rendered faces of the C++ firmware (src/ui_font_data.h, DejaVu,
/// license in assets/display/): 16-level coverage, run-length encoded.
mod font {
    pub struct Glyph {
        pub offset: u32,
        pub length: u16,
        pub width: u8,
        pub advance: u8,
        pub left: i8,
    }

    pub struct Face {
        pub first: usize,
        pub height: u8,
        pub baseline: u8,
        pub numerals: bool,
    }

    include!(concat!(env!("OUT_DIR"), "/ui_font_data.rs"));
}

use font::{Face, Glyph, FACES, GLYPHS, NUMERAL_CHARACTERS, NUMERAL_FACES, TEXT_FACES};

pub const PAGES: i32 = 8;
pub const BACKGROUND: u32 = 0x020406;
pub const PANEL: u32 = 0x0C1119;
pub const BORDER: u32 = 0x2A3646;
pub const TEXT: u32 = 0xF1F5FA;
pub const MUTED: u32 = 0x94A3B8;
pub const GREEN: u32 = 0x42D99B;
pub const RED: u32 = 0xFF6577;
pub const AMBER: u32 = 0xFFBC55;

const SCRATCH_PIXELS: usize = 4096;
const LARGEST_NUMERALS: i32 = 80; // tools/pack_display_fonts.py NUMERAL_SIZES
const LARGEST_TEXT: i32 = 24; // TEXT_SIZES

/// A display: RGB565 pixels, row-major.
pub trait Panel {
    fn width(&self) -> i32;
    fn height(&self) -> i32;
    /// `pixels` holds w * h pixels; the rectangle is inside the panel.
    fn push(&mut self, x: i32, y: i32, w: i32, h: i32, pixels: &[u16]);
    /// One colour over a rectangle inside the panel.
    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, color: u16);
}

pub fn rgb565(rgb: u32) -> u16 {
    (((rgb >> 8) & 0xF800) | ((rgb >> 5) & 0x07E0) | ((rgb >> 3) & 0x001F)) as u16
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

const fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn blend(foreground: u32, background: u32, alpha: u32) -> u32 {
    let mut result = 0;
    for shift in [16, 8, 0] {
        let f = (foreground >> shift) & 255;
        let b = (background >> shift) & 255;
        result |= ((f * alpha + b * (255 - alpha) + 127) / 255) << shift;
    }
    result
}

// ---------------------------------------------------------------- fonts

fn face(height: i32, numerals: bool) -> &'static Face {
    let (base, count) = if numerals { (TEXT_FACES, NUMERAL_FACES) } else { (0, TEXT_FACES) };
    FACES[base..base + count].iter().rev().find(|f| f.height as i32 <= height).unwrap_or(&FACES[base])
}

/// True when every character has a glyph in the numeral faces.
fn numeric(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c != '?' && NUMERAL_CHARACTERS.contains(c))
}

fn glyph(face: &Face, c: char) -> &'static Glyph {
    let index = if face.numerals {
        NUMERAL_CHARACTERS.find(c).filter(|_| c >= ' ').unwrap_or_else(|| NUMERAL_CHARACTERS.find('?').unwrap())
    } else if (' '..='~').contains(&c) {
        c as usize - 32
    } else {
        '?' as usize - 32
    };
    &GLYPHS[face.first + index]
}

/// LovyanGFX's text width: advances, the last glyph's ink included.
fn text_width(face: &Face, text: &str) -> i32 {
    let (mut left, mut right) = (0i32, 0i32);
    for c in text.chars() {
        let g = glyph(face, c);
        if left == 0 && right == 0 && g.left < 0 {
            left = -(g.left as i32);
        }
        right = left + (g.advance as i32).max(g.width as i32 + g.left as i32);
        left += g.advance as i32;
    }
    right
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Datum {
    TopLeft = 0,
    TopCenter = 1,
    MiddleLeft = 4,
    MiddleCenter = 5,
    MiddleRight = 6,
    BottomLeft = 8,
    BottomRight = 10,
}

// ------------------------------------------------------------- drawing

/// Where a paint goes: the strip buffer (its own coordinates) or the panel
/// directly, with a background that antialiasing blends into.
struct Draw<'a> {
    pixels: Option<&'a mut [u16]>,
    panel: Option<&'a mut dyn Panel>,
    clip: Rect,
    /// The colour glyph edges blend into.
    base: u32,
    face: &'static Face,
    color: u32,
    datum: Datum,
}

impl Draw<'_> {
    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: u32) {
        let x0 = x.max(self.clip.x);
        let y0 = y.max(self.clip.y);
        let x1 = (x + w).min(self.clip.x + self.clip.w);
        let y1 = (y + h).min(self.clip.y + self.clip.h);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let color = rgb565(rgb);
        if let Some(pixels) = self.pixels.as_deref_mut() {
            let stride = self.clip.w as usize;
            for row in y0..y1 {
                let start = row as usize * stride;
                pixels[start + x0 as usize..start + x1 as usize].fill(color);
            }
        } else if let Some(panel) = self.panel.as_deref_mut() {
            panel.fill(x0, y0, x1 - x0, y1 - y0, color);
        }
    }

    fn pixel(&mut self, x: i32, y: i32, rgb: u32) {
        self.fill(x, y, 1, 1, rgb);
    }

    fn hline(&mut self, x: i32, y: i32, w: i32, rgb: u32) {
        self.fill(x, y, w, 1, rgb);
    }

    fn vline(&mut self, x: i32, y: i32, h: i32, rgb: u32) {
        self.fill(x, y, 1, h, rgb);
    }

    fn label_font(&mut self, size: i32) {
        self.face = face(size, false);
    }

    /// The largest face, numeral when possible, that fits `value` in w x h.
    fn fit_font(&mut self, value: &str, w: i32, h: i32, largest: i32) {
        let numerals = numeric(value);
        let top = h.min(if numerals { largest } else { largest.min(LARGEST_TEXT) });
        let mut size = top.max(8);
        while size >= 8 {
            self.face = face(size, numerals);
            if self.text_width(value) <= w {
                return;
            }
            size -= 1;
        }
    }

    fn text_width(&self, text: &str) -> i32 {
        text_width(self.face, text)
    }

    fn font_height(&self) -> i32 {
        self.face.height as i32
    }

    /// Bounded with "..." to fit `width`, measured in the current face.
    fn ellipsis(&self, value: &str, width: i32) -> String {
        if self.text_width(value) <= width {
            return value.to_string();
        }
        let mut value = value.to_string();
        while !value.is_empty() && self.text_width(&format!("{value}...")) > width {
            value.pop();
        }
        if value.is_empty() && self.text_width("...") > width {
            String::new()
        } else {
            value + "..."
        }
    }

    fn draw_char(&mut self, x: i32, y: i32, c: char) -> i32 {
        let g = glyph(self.face, c);
        let mut shades = [0u32; 16];
        for (a, shade) in shades.iter_mut().enumerate() {
            let mut rgb = 0;
            for shift in [16, 8, 0] {
                let fore = (self.color >> shift) & 255;
                let back = (self.base >> shift) & 255;
                rgb |= ((fore * a as u32 + back * (15 - a as u32) + 7) / 15) << shift;
            }
            *shade = rgb;
        }
        let width = g.width as u32;
        let mut position = 0u32;
        for &token in &font::COVERAGE[g.offset as usize..g.offset as usize + g.length as usize] {
            let mut remaining = (token as u32 >> 4) + 1;
            let alpha = (token & 15) as usize;
            while remaining > 0 && width > 0 {
                let col = position % width;
                let row = position / width;
                let run = remaining.min(width - col);
                if alpha != 0 {
                    self.fill(x + g.left as i32 + col as i32, y + row as i32, run as i32, 1, shades[alpha]);
                }
                position += run;
                remaining -= run;
            }
        }
        g.advance as i32
    }

    /// LovyanGFX's drawString with the current datum, face and colour.
    fn draw_string(&mut self, text: &str, mut x: i32, mut y: i32) {
        if text.is_empty() {
            return;
        }
        let d = self.datum as i32;
        let height = self.font_height();
        if d & 4 != 0 {
            y -= height >> 1;
        } else if d & 8 != 0 {
            y -= height;
        }
        if d & 3 != 0 {
            let w = self.text_width(text);
            x -= if d & 1 != 0 { w >> 1 } else { w };
        }
        for c in text.chars() {
            x += self.draw_char(x, y, c);
        }
    }

    fn smooth_dot(&mut self, x: i32, y: i32, radius: i32, color: u32, background: u32) {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let coverage = (radius as f32 + 0.5 - ((dx * dx + dy * dy) as f32).sqrt()).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    self.pixel(x + dx, y + dy, blend(color, background, (coverage * 255.0) as u32));
                }
            }
        }
    }

    fn smooth_line(&mut self, mut x0: i32, mut y0: i32, mut x1: i32, mut y1: i32, color: u32) {
        let steep = (y1 - y0).abs() > (x1 - x0).abs();
        if steep {
            std::mem::swap(&mut x0, &mut y0);
            std::mem::swap(&mut x1, &mut y1);
        }
        if x0 > x1 {
            std::mem::swap(&mut x0, &mut x1);
            std::mem::swap(&mut y0, &mut y1);
        }
        let dx = x1 - x0;
        let mut position = y0 * 65536;
        let slope = if dx != 0 { (y1 - y0) * 65536 / dx } else { 0 };
        for x in x0..=x1 {
            let y = (position as f64 / 65536.0).floor() as i32;
            let alpha = ((position - y * 65536) as u32) >> 8;
            let mut plot = |yy: i32, a: u32| {
                if a != 0 {
                    let (px, py) = if steep { (yy, x) } else { (x, yy) };
                    self.pixel(px, py, blend(color, BACKGROUND, a));
                }
            };
            plot(y, 255 - alpha);
            plot(y + 1, alpha);
            position += slope;
        }
    }
}

fn compact_label(label: &str) -> String {
    const PAIRS: [(&str, &str); 36] = [
        ("FIRMWARE", "FW"),
        ("FREE RAM", "RAM"),
        ("RAM LIBRE", "RAM"),
        ("NEXT RETARGET", "ADJ."),
        ("AJUSTEMENT", "AJ."),
        ("NETWORK DIFF", "DIFF"),
        ("DIFF RESEAU", "DIFF"),
        ("TOTAL HASHES", "HASHES"),
        ("HASHES TOTAUX", "HASHES"),
        ("BITCOIN PRICE", "BTC"),
        ("PRIX DU BITCOIN", "BTC"),
        ("BTC PRICE", "BTC"),
        ("PRIX BTC", "BTC"),
        ("BEST EVER", "BEST"),
        ("LIFE HASHES", "HASH"),
        ("RUN TIME", "TIME"),
        ("DUREE TOTALE", "DUREE"),
        ("SESSION", "SESS"),
        ("BEST DIFF", "BEST"),
        ("MEILL. DIFF", "DIFF"),
        ("POWER", "PWR"),
        ("PUISSANCE", "PUIS."),
        ("LOCAL IP", "IP"),
        ("IP LOCALE", "IP"),
        ("NET HASHRATE", "NET"),
        ("HASHRATE RESEAU", "RESEAU"),
        ("MEMPOOL TX", "TX"),
        ("TX MEMPOOL", "TX"),
        ("FEES sat/vB", "FEES"),
        ("FRAIS sat/vB", "FRAIS"),
        ("HALVING", "HALV."),
        ("SHARES", "SHR"),
        ("32-BIT", "32b"),
        ("32 BITS", "32b"),
        ("64-BIT", "64b"),
        ("64 BITS", "64b"),
    ];
    PAIRS.iter().find(|p| p.0 == label).map_or(label, |p| p.1).to_string()
}

fn rate_label(value: f32) -> String {
    if value >= 1e6 {
        format!("{:.2}M", value / 1e6)
    } else if value >= 1e3 {
        format!("{:.0}k", value / 1e3)
    } else {
        format!("{value:.0}")
    }
}

#[derive(Clone, Default)]
struct Cached {
    key: String,
    rect: Rect,
    valid: bool,
}

/// The renderer of one panel.
pub struct View {
    strip: Vec<u16>,
    cache: Vec<Cached>,
    page: i32,
    width: i32,
    height: i32,
    accent: u32,
    language: u8,
    graph_revision: u32,
    graph_valid: bool,
}

impl View {
    pub fn new(accent: u32, language: u8) -> Self {
        View {
            strip: vec![0; SCRATCH_PIXELS],
            cache: vec![Cached::default(); 24],
            page: -99,
            width: 0,
            height: 0,
            accent,
            language,
            graph_revision: 0,
            graph_valid: false,
        }
    }

    /// Everything is painted again at the next call (after a rotation).
    pub fn invalidate(&mut self) {
        self.page = -99;
        for c in &mut self.cache {
            c.valid = false;
        }
        self.graph_valid = false;
    }

    fn margin(&self) -> i32 {
        if self.width >= 240 {
            8
        } else if self.width >= 120 {
            5
        } else {
            3
        }
    }

    fn header_height(&self) -> i32 {
        if self.height >= 200 {
            24
        } else if self.height >= 120 {
            18
        } else {
            13
        }
    }

    fn dots_height(&self) -> i32 {
        if self.height >= 200 {
            10
        } else {
            7
        }
    }

    fn label_size(&self) -> i32 {
        if self.height >= 200 && self.width >= 200 {
            11
        } else {
            8
        }
    }

    fn body(&self) -> Rect {
        let m = self.margin();
        let y = self.header_height() + if self.height >= 120 { 4 } else { 2 };
        rect(m, y, self.width - 2 * m, self.height - self.dots_height() - y)
    }

    fn changed(&mut self, slot: usize, r: Rect, key: String) -> bool {
        let c = &mut self.cache[slot];
        if c.valid && c.key == key && c.rect == r {
            return false;
        }
        *c = Cached { key, rect: r, valid: true };
        true
    }

    /// Paints `r` strip by strip: `draw` gets the strip and the offset of
    /// `r`'s top left corner in it.
    fn paint(&mut self, panel: &mut dyn Panel, r: Rect, background: u32, mut draw: impl FnMut(&mut Draw, i32, i32)) {
        let r = clip(r, panel);
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let rows = (SCRATCH_PIXELS / r.w as usize).max(1) as i32;
        let mut y = 0;
        while y < r.h {
            let h = rows.min(r.h - y);
            let pixels = &mut self.strip[..(r.w * h) as usize];
            pixels.fill(rgb565(background));
            {
                let mut d = Draw {
                    pixels: Some(pixels),
                    panel: None,
                    clip: rect(0, 0, r.w, h),
                    base: background,
                    face: face(8, false),
                    color: TEXT,
                    datum: Datum::TopLeft,
                };
                draw(&mut d, 0, -y);
            }
            panel.push(r.x, r.y + y, r.w, h, &self.strip[..(r.w * h) as usize]);
            y += h;
        }
    }

    /// Static parts of a page: background, the coin and the page dots.
    /// Called on every render; it repaints only when the page, panel or
    /// accent changes.
    fn frame(&mut self, panel: &mut dyn Panel, page: i32, accent: u32, language: u8) {
        if self.page == page
            && self.width == panel.width()
            && self.height == panel.height()
            && self.accent == accent
            && self.language == language
        {
            return;
        }
        self.invalidate();
        self.page = page;
        self.accent = accent;
        self.language = language;
        self.width = panel.width();
        self.height = panel.height();
        panel.fill(0, 0, self.width, self.height, rgb565(BACKGROUND));
        let (hh, m) = (self.header_height(), self.margin());
        let r = (hh / 2 - 2).max(4);
        let accent = self.accent;
        self.paint(panel, rect(m - 1, hh / 2 - r - 1, 2 * r + 3, 2 * r + 3), BACKGROUND, |d, x, y| {
            let (cx, cy) = (x + r + 1, y + r + 1);
            d.smooth_dot(cx, cy, r, accent, BACKGROUND);
            d.label_font((r * 2 - 2).clamp(8, 16));
            d.base = accent; // antialiasing blends into the coin
            d.color = BACKGROUND;
            d.datum = Datum::MiddleCenter;
            d.draw_string("B", cx + 1, cy + 1);
        });
        if page >= 0 {
            let gap = if self.width >= 200 { 9 } else { 6 };
            let y = self.height - self.dots_height() / 2 - 1;
            let x0 = self.width / 2 - (PAGES - 1) * gap / 2;
            for i in 0..PAGES {
                let (radius, color) = if i == page { (2, accent) } else { (1, BORDER) };
                self.paint(panel, rect(x0 + i * gap - 3, y - 3, 7, 7), BACKGROUND, |d, x, y| {
                    d.smooth_dot(x + 3, y + 3, radius, color, BACKGROUND);
                });
            }
        }
    }

    fn header(&mut self, panel: &mut dyn Panel, title: &str, status: &str, status_color: u32) {
        let (hh, m) = (self.header_height(), self.margin());
        let coin = 2 * (hh / 2 - 2).max(4) + 5;
        let size = self.label_size();
        let width = self.width;
        let status_w = if width >= 280 {
            110
        } else if width >= 200 {
            80
        } else if width >= 120 {
            46
        } else {
            10
        };
        self.text(panel, 16, rect(m + coin, 0, width - 2 * m - coin - status_w, hh), title, MUTED, size, false);
        let r = rect(width - m - status_w, 0, status_w, hh);
        if !self.changed(17, r, format!("{status}\n{status_color}")) {
            return;
        }
        self.paint(panel, r, BACKGROUND, |d, x, y| {
            if status.is_empty() {
                return;
            }
            let dot = if hh >= 18 { 3 } else { 2 };
            if status_w < 20 {
                d.smooth_dot(x + r.w - dot - 1, y + hh / 2, dot, status_color, BACKGROUND);
                return;
            }
            d.label_font(size);
            d.color = status_color;
            d.datum = Datum::MiddleRight;
            let shown = d.ellipsis(status, r.w - 2 * dot - 5);
            d.draw_string(&shown, x + r.w, y + hh / 2);
            let w = d.text_width(&shown);
            d.smooth_dot(x + r.w - w - dot - 4, y + hh / 2, dot, status_color, BACKGROUND);
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, panel: &mut dyn Panel, slot: usize, r: Rect, value: &str, color: u32, size: i32, right: bool) {
        if !self.changed(slot, r, format!("{value}\n{color}:{size}")) {
            return;
        }
        self.paint(panel, r, BACKGROUND, |d, x, y| {
            d.label_font(size);
            d.color = color;
            d.datum = if right { Datum::MiddleRight } else { Datum::MiddleLeft };
            let shown = d.ellipsis(value, r.w);
            d.draw_string(&shown, x + if right { r.w } else { 0 }, y + r.h / 2);
        });
    }

    /// The page's one big value, with its unit beside it on the same
    /// baseline.
    fn hero(&mut self, panel: &mut dyn Panel, slot: usize, r: Rect, metric: &Metric, centered: bool) {
        if !self.changed(slot, r, format!("{}\n{}\n{}", metric.value, metric.detail, metric.color)) {
            return;
        }
        let tall = self.height >= 200;
        self.paint(panel, r, BACKGROUND, |d, x, y| {
            let unit_size = (r.h / 4).min(if tall { 18 } else { 12 }).max(8);
            d.label_font(unit_size);
            let unit_font = d.face;
            let unit_text_w = if metric.detail.is_empty() { 0 } else { d.text_width(&metric.detail) };
            let unit_h = d.font_height();
            let max_h = r.h - r.h / 8; // a little air above and below
                                       // The unit beside the value, or under it when that leaves a
                                       // bigger value (narrow portrait panels).
            let beside_w = if unit_text_w != 0 { (r.w / 3).min(unit_text_w) + 4 } else { 0 };
            d.fit_font(&metric.value, r.w - beside_w, max_h, LARGEST_NUMERALS);
            let beside_font = d.face;
            let beside_h = d.font_height();
            let mut below = false;
            if unit_text_w != 0 && max_h > 2 * unit_h {
                d.fit_font(&metric.value, r.w, max_h - unit_h - 2, LARGEST_NUMERALS);
                below = d.font_height() > beside_h;
            }
            if !below {
                d.face = beside_font;
            }
            let unit_w = if below { 0 } else { beside_w };
            let shown = d.ellipsis(&metric.value, r.w - unit_w);
            let value_w = d.text_width(&shown);
            let value_h = d.font_height();
            let block_h = value_h + if below { unit_h + 2 } else { 0 };
            let left = x + if centered { ((r.w - value_w - unit_w) / 2).max(0) } else { 0 };
            let top = y + (r.h - block_h) / 2;
            let base = top + d.face.baseline as i32;
            d.color = metric.color;
            d.datum = Datum::TopLeft;
            d.draw_string(&shown, left, top);
            if unit_text_w == 0 {
                return;
            }
            d.face = unit_font;
            d.color = MUTED;
            if below {
                d.datum = Datum::TopCenter;
                let unit = d.ellipsis(&metric.detail, r.w);
                d.draw_string(&unit, if centered { x + r.w / 2 } else { left + value_w / 2 }, top + value_h + 2);
                return;
            }
            d.datum = Datum::TopLeft;
            let unit = d.ellipsis(&metric.detail, unit_w - 4);
            d.draw_string(&unit, left + value_w + 4, base - unit_font.baseline as i32);
        });
    }

    /// A secondary reading: label above value (stacked) or "LABEL  value" on
    /// one line.
    fn stat(&mut self, panel: &mut dyn Panel, slot: usize, r: Rect, metric: &Metric, stacked: bool, divider: bool) {
        let key = format!("{}\n{}\n{}{}", metric.label, metric.value, metric.color, if stacked { "s" } else { "i" });
        if !self.changed(slot, r, key) {
            return;
        }
        let (size, wide, tall) = (self.label_size(), self.width >= 200, self.height >= 200);
        self.paint(panel, r, BACKGROUND, |d, x, y| {
            let pad = if divider {
                if wide {
                    7
                } else {
                    4
                }
            } else {
                0
            };
            if divider {
                d.vline(x, y + 2, r.h - 4, BORDER);
            }
            d.label_font(size);
            d.color = MUTED;
            let mut label = metric.label.clone();
            if stacked {
                if d.text_width(&label) > r.w - pad {
                    label = compact_label(&label);
                }
                d.datum = Datum::TopLeft;
                let shown = d.ellipsis(&label, r.w - pad);
                d.draw_string(&shown, x + pad, y);
                let vy = d.font_height() + 2;
                let vh = r.h - vy;
                d.fit_font(&metric.value, r.w - pad, vh.min(if tall { 24 } else { 16 }), 24);
                d.color = metric.color;
                d.datum = Datum::MiddleLeft;
                let shown = d.ellipsis(&metric.value, r.w - pad);
                d.draw_string(&shown, x + pad, y + vy + vh / 2);
                return;
            }
            label = compact_label(&label);
            d.datum = Datum::MiddleLeft;
            let label_w = d.text_width(&label).min(r.w * 45 / 100);
            let shown = d.ellipsis(&label, label_w);
            d.draw_string(&shown, x + pad, y + r.h / 2);
            let value_w = r.w - pad - label_w - 4;
            d.fit_font(&metric.value, value_w, r.h, 16);
            d.color = metric.color;
            d.datum = Datum::MiddleRight;
            let shown = d.ellipsis(&metric.value, value_w);
            d.draw_string(&shown, x + r.w, y + r.h / 2);
        });
    }

    /// Up to three readings: side by side on landscape panels, stacked
    /// otherwise.
    fn stats(&mut self, panel: &mut dyn Panel, m: &Model, r: Rect, first: usize, mut count: i32) {
        let row = self.width >= self.height && self.width >= 160;
        if row && self.width < 200 {
            count = count.min(2);
        }
        if row {
            let w = r.w / count;
            let stacked = r.h >= 22;
            for i in 0..count {
                let cell = rect(r.x + i * w, r.y, if i == count - 1 { r.w - i * w } else { w }, r.h);
                self.stat(panel, first + i as usize, cell, &m.metrics[first + i as usize], stacked, i > 0);
            }
            return;
        }
        let h = r.h / count;
        for i in 0..count {
            let cell = rect(r.x, r.y + i * h, r.w, if i == count - 1 { r.h - i * h } else { h });
            self.stat(panel, first + i as usize, cell, &m.metrics[first + i as usize], false, false);
        }
    }

    fn graph(&mut self, panel: &mut dyn Panel, m: &Model, r: Rect) {
        if self.graph_valid && self.graph_revision == m.graph_revision {
            return;
        }
        self.graph_valid = true;
        self.graph_revision = m.graph_revision;
        let (wide, language, accent) = (self.width >= 200, self.language, self.accent);
        let points = &m.points;
        self.paint(panel, r, BACKGROUND, |d, ox, oy| {
            let maximum = points.iter().map(|p| p.1).filter(|v| v.is_finite()).fold(1.0f32, f32::max);
            let ceiling = maximum * 1.12;
            let (y0, h) = (3, r.h - 15);
            let ticks = if h < 24 { 2 } else { 3 };
            // The axis takes the width of its widest label ("1.14M" did not
            // fit the 23 px of small screens and lost its first digit).
            d.label_font(8);
            let mut labels: Vec<String> = Vec::new();
            let mut label_w = if wide { 32 } else { 23 };
            for i in 0..ticks {
                let l = if i == ticks - 1 {
                    "0".to_string()
                } else {
                    rate_label(ceiling * (ticks - 1 - i) as f32 / (ticks - 1) as f32)
                };
                label_w = label_w.max(d.text_width(&l) + 4);
                labels.push(l);
            }
            label_w = label_w.min(r.w / 3);
            // Still too wide (80 px screens): one decimal less, "1.14M" ->
            // "1.1M" -> "1M".
            for l in &mut labels {
                while d.text_width(l) + 4 > label_w && l.contains('.') {
                    let last = l.rfind(|c: char| c.is_ascii_digit()).unwrap();
                    l.remove(last);
                    if l.as_bytes()[last - 1] == b'.' {
                        l.remove(last - 1);
                    }
                }
            }
            let x0 = label_w;
            let w = r.w - label_w - 3;
            d.color = MUTED;
            for (i, l) in labels.iter().enumerate() {
                let yy = y0 + i as i32 * (h - 1) / (ticks - 1);
                d.hline(ox + x0, oy + yy, w, BORDER);
                d.datum = Datum::MiddleRight;
                d.draw_string(l, ox + x0 - 3, oy + yy);
            }
            let mut now = if language != 0 { "maint." } else { "now" };
            let span = match (points.first(), points.last()) {
                (Some(a), Some(b)) => b.0.wrapping_sub(a.0),
                _ => 0,
            };
            let mut age = String::new();
            if !points.is_empty() {
                // The time span, shortened, then without "now", when both do
                // not fit under the graph (80 px wide screens).
                let room = r.w - 3 - x0 - 4;
                age = format!("-{}m{:02}s", span / 60000, span / 1000 % 60);
                if d.text_width(&age) + d.text_width(now) > room {
                    age = format!("-{}min", span / 60000);
                }
                if d.text_width(&age) + d.text_width(now) > room {
                    now = "";
                }
            }
            d.datum = Datum::BottomRight;
            d.draw_string(now, ox + r.w - 3, oy + r.h);
            if points.is_empty() {
                return;
            }
            d.datum = Datum::BottomLeft;
            d.draw_string(&age, ox + x0, oy + r.h);
            let mut previous = false;
            let (mut px, mut py, mut previous_ms) = (0, 0, 0u32);
            let first_ms = points[0].0;
            for &(ms, rate) in points {
                if !rate.is_finite() || rate < 0.0 {
                    previous = false;
                    continue;
                }
                let x = x0
                    + if span != 0 {
                        (ms.wrapping_sub(first_ms) as u64 * (w - 1) as u64 / span as u64) as i32
                    } else {
                        w - 1
                    };
                let y = y0 + h - 1 - (rate / ceiling * (h - 1) as f32) as i32;
                if previous && ms.wrapping_sub(previous_ms) <= 20000 {
                    d.smooth_line(ox + px, oy + py, ox + x, oy + y, accent);
                } else {
                    d.smooth_dot(ox + x, oy + y, 1, accent, BACKGROUND);
                }
                px = x;
                py = y;
                previous_ms = ms;
                previous = true;
            }
            if previous {
                d.smooth_dot(ox + px, oy + py, 2, accent, BACKGROUND);
            }
        });
    }

    pub fn render(&mut self, panel: &mut dyn Panel, m: &Model) {
        self.frame(panel, m.page as i32, m.accent, m.language);
        self.header(panel, &m.title, &m.status, m.status_color);
        let b = self.body();
        let landscape = self.width >= self.height && self.width >= 160;
        let gap = if self.height >= 120 { 4 } else { 2 };
        if m.page == 5 {
            // Graph: the average as a big value, minimum / maximum beside it.
            let top = if self.height >= 200 {
                46
            } else if self.height >= 120 {
                30
            } else {
                22
            };
            let stats_w = if landscape { b.w * 45 / 100 } else { 0 };
            self.hero(panel, 0, rect(b.x, b.y, b.w - stats_w, top), &m.metrics[0], false);
            if stats_w != 0 {
                let h = top / 2;
                self.stat(panel, 1, rect(b.x + b.w - stats_w, b.y, stats_w, h), &m.metrics[1], false, false);
                self.stat(panel, 2, rect(b.x + b.w - stats_w, b.y + h, stats_w, top - h), &m.metrics[2], false, false);
            }
            self.graph(panel, m, rect(b.x, b.y + top + gap, b.w, b.h - top - gap));
            return;
        }
        if m.page == 7 {
            // Blocks: the block being mined, then the latest blocks found.
            let row_h = if self.height >= 200 {
                20
            } else if self.height >= 120 {
                15
            } else {
                12
            };
            let rows = ((b.h * 45 / 100) / row_h).clamp(1, 3);
            let top = b.h - rows * row_h - gap;
            self.hero(panel, 0, rect(b.x, b.y, b.w, top), &m.metrics[0], true);
            for i in 0..rows {
                let v = &m.metrics[i as usize + 1];
                let mut line = format!("{}  {}", v.label, v.value);
                if !v.detail.is_empty() {
                    line += &format!("  {}", v.detail);
                }
                let shown = if v.label.is_empty() { "" } else { &line };
                let size = self.label_size();
                self.text(
                    panel,
                    i as usize + 1,
                    rect(b.x, b.y + top + gap + i * row_h, b.w, row_h),
                    shown,
                    v.color,
                    size,
                    false,
                );
            }
            return;
        }
        let mut count = 3;
        let stats_h = if landscape {
            if self.height >= 200 {
                44
            } else if self.height >= 120 {
                30
            } else {
                13
            }
        } else {
            count
                * if self.height >= 200 {
                    20
                } else if self.height >= 140 {
                    16
                } else {
                    13
                }
        };
        if landscape && self.width < 200 {
            count = 2;
        }
        self.hero(panel, 0, rect(b.x, b.y, b.w, b.h - stats_h - gap), &m.metrics[0], true);
        self.stats(panel, m, rect(b.x, b.y + b.h - stats_h, b.w, stats_h), 1, count);
    }

    pub fn message(&mut self, panel: &mut dyn Panel, title: &str, line1: &str, line2: &str) {
        let splash = title == "BDLMiner V2";
        let (accent, language) = (self.accent, self.language);
        self.frame(panel, if splash { -4 } else { -1 }, accent, language);
        self.header(panel, title, "", MUTED);
        let b = self.body();
        let line_h = self.label_size() + 6;
        let big = Metric { value: line1.into(), color: if splash { accent } else { TEXT }, ..Default::default() };
        self.hero(panel, 0, rect(b.x, b.y, b.w, b.h - line_h), &big, true);
        let size = self.label_size();
        self.text(panel, 1, rect(b.x, b.y + b.h - line_h, b.w, line_h), line2, MUTED, size, false);
    }

    pub fn portal(&mut self, panel: &mut dyn Panel, title: &str, ssid: &str, password: &str, qr: &str) {
        let (accent, language) = (self.accent, self.language);
        self.frame(panel, -2, accent, language);
        self.header(panel, title, "192.168.4.1", MUTED);
        let b = self.body();
        let size = b.h.min(b.w / 2 - 3);
        if self.changed(0, rect(b.x, b.y, size, size), qr.into()) {
            let r = clip(rect(b.x, b.y, size, size), panel);
            panel.fill(r.x, r.y, r.w, r.h, rgb565(0xFFFFFF));
            qrcode(panel, qr, b.x, b.y, size);
        }
        let x = b.x + size + 6;
        let w = b.w - size - 6;
        let h = b.h / 2;
        let ap = Metric { label: "WiFi".into(), value: ssid.into(), color: TEXT, ..Default::default() };
        let pass = Metric {
            label: if language != 0 { "MOT DE PASSE" } else { "PASSWORD" }.into(),
            value: password.into(),
            color: accent,
            ..Default::default()
        };
        self.stat(panel, 1, rect(x, b.y, w, h), &ap, true, false);
        self.stat(panel, 2, rect(x, b.y + h, w, b.h - h), &pass, true, false);
    }

    /// Full-screen announcement in an accent frame: `value` as large as it
    /// fits.
    pub fn alert(&mut self, panel: &mut dyn Panel, title: &str, value: &str, line: &str) {
        let (accent, language) = (self.accent, self.language);
        self.frame(panel, -5, accent, language);
        let (width, height) = (self.width, self.height);
        if self.changed(3, rect(0, 0, width, height), "border".into()) {
            let thick = if height >= 200 { 3 } else { 2 };
            let c = rgb565(accent);
            for i in 0..thick {
                let (w, h) = (width - 2 * i, height - 2 * i);
                panel.fill(i, i, w, 1, c);
                panel.fill(i, i + h - 1, w, 1, c);
                panel.fill(i, i, 1, h, c);
                panel.fill(i + w - 1, i, 1, h, c);
            }
        }
        let b = self.body();
        let title_h = (self.label_size() + 6).max(28.min(b.h / 4));
        let line_h = self.label_size() + 6;
        // The whole title, narrow portrait panels included.
        let mut size = 24.min(title_h - 4);
        while size > 8 && text_width(face(size, false), title) > b.w {
            size -= 1;
        }
        self.text(panel, 1, rect(b.x, b.y, b.w, title_h), title, accent, size, false);
        let big = Metric { value: value.into(), color: TEXT, ..Default::default() };
        self.hero(panel, 0, rect(b.x, b.y + title_h, b.w, b.h - title_h - line_h), &big, true);
        let size = self.label_size();
        self.text(panel, 2, rect(b.x, b.y + b.h - line_h, b.w, line_h), line, MUTED, size, false);
    }

    /// A few lines of plain text from the top, 11 pixels apart (the Bitaxe
    /// Gamma's 128x32 screen, as the C++ firmware's src/ui_gamma.cpp), in
    /// the smallest face, white on black.
    pub fn lines(&mut self, panel: &mut dyn Panel, lines: &[&str]) {
        let (w, h) = (panel.width(), panel.height());
        if !self.changed(23, rect(0, 0, w, h), lines.join("\n")) {
            return;
        }
        self.paint(panel, rect(0, 0, w, h), 0, |d, x, y| {
            d.label_font(8);
            d.color = 0xFFFFFF;
            d.datum = Datum::TopLeft;
            for (i, l) in lines.iter().enumerate() {
                d.draw_string(l, x, y + i as i32 * 11);
            }
        });
    }
}

fn clip(r: Rect, panel: &dyn Panel) -> Rect {
    let x0 = r.x.max(0);
    let y0 = r.y.max(0);
    let x1 = (r.x + r.w).min(panel.width());
    let y1 = (r.y + r.h).min(panel.height());
    rect(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
}

/// LovyanGFX's qrcode(): the smallest version that holds `text` (low error
/// correction), centred in a white w x w square, whole-pixel modules.
fn qrcode(panel: &mut dyn Panel, text: &str, x: i32, y: i32, w: i32) {
    use qrcodegen::{QrCode, QrCodeEcc, QrSegment, Version};
    let segments = QrSegment::make_segments(text);
    let Ok(code) = QrCode::encode_segments_advanced(&segments, QrCodeEcc::Low, Version::MIN, Version::MAX, None, false)
    else {
        return;
    };
    let size = code.size();
    let thickness = w / size;
    if thickness == 0 {
        return;
    }
    let length = size * thickness;
    let (xo, yo) = (x + (w - length) / 2, y + (w - length) / 2);
    let black = rgb565(0);
    for row in 0..size {
        for col in 0..size {
            if code.get_module(col, row) {
                let r = clip(rect(xo + col * thickness, yo + row * thickness, thickness, thickness), panel);
                if r.w > 0 && r.h > 0 {
                    panel.fill(r.x, r.y, r.w, r.h, black);
                }
            }
        }
    }
}

/// An RGB565 image in memory: for previews and tests.
pub struct Framebuffer {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u16>,
}

impl Framebuffer {
    pub fn new(width: i32, height: i32) -> Self {
        Framebuffer { width, height, pixels: vec![0; (width * height) as usize] }
    }

    /// The image as RGB888 bytes, for a PPM or PNG writer.
    pub fn rgb(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 3);
        for &p in &self.pixels {
            let r = (p >> 11) & 31;
            let g = (p >> 5) & 63;
            let b = p & 31;
            out.extend_from_slice(&[(r << 3 | r >> 2) as u8, (g << 2 | g >> 4) as u8, (b << 3 | b >> 2) as u8]);
        }
        out
    }
}

impl Panel for Framebuffer {
    fn width(&self) -> i32 {
        self.width
    }

    fn height(&self) -> i32 {
        self.height
    }

    fn push(&mut self, x: i32, y: i32, w: i32, h: i32, pixels: &[u16]) {
        for row in 0..h {
            let dst = ((y + row) * self.width + x) as usize;
            let src = (row * w) as usize;
            self.pixels[dst..dst + w as usize].copy_from_slice(&pixels[src..src + w as usize]);
        }
    }

    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, color: u16) {
        for row in y..y + h {
            let dst = (row * self.width + x) as usize;
            self.pixels[dst..dst + w as usize].fill(color);
        }
    }
}
