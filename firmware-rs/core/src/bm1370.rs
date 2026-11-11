//! Bitaxe Gamma 601 (BM1370) wire protocol. Derived from bitaxeorg/ESP-Miner
//! (GPL-3.0); hardware references: docs/hardware/bitaxe-gamma-601.md.

pub const VERSION_MASK: u32 = 0x1fff_e000;
pub const SLOTS: usize = 16;
/// Profiles: 525 to 800 MHz in 25 MHz steps, and 690 (ESP-Miner's highest preset).
pub const STOCK_MHZ: u16 = 525;
pub const MAX_MHZ: u16 = 800;
pub const PLL_MAX_MHZ: u16 = 800;
pub const ESP_MINER_MAX_MHZ: u16 = 690;
pub const STOCK_MV: u16 = 1150;
pub const MIN_MV: u16 = 1000;
pub const MAX_MV: u16 = 1300;
pub const VOLTAGE_OPTIONS: [u16; 7] = [1000, 1060, 1100, 1150, 1200, 1250, 1300];

pub fn valid_profile(mhz: u32) -> bool {
    mhz == ESP_MINER_MAX_MHZ as u32
        || ((STOCK_MHZ as u32..=MAX_MHZ as u32).contains(&mhz) && (mhz - STOCK_MHZ as u32) % 25 == 0)
}

pub fn valid_voltage(mv: u32) -> bool {
    VOLTAGE_OPTIONS.iter().any(|&o| o as u32 == mv)
}

/// The core voltage that goes with a profile: stock up to 600 MHz, then one
/// 50 mV step per range.
pub fn recommended_mv(mhz: u32) -> u16 {
    match mhz {
        0..=600 => STOCK_MV,
        601..=675 => 1200,
        676..=725 => 1250,
        _ => 1300,
    }
}

/// Compares cumulative ASIC error/total counters in ten-second windows.
#[derive(Clone, Debug)]
pub struct ErrorMonitor {
    pub errors: u32,
    pub total: u32,
    pub at: u32,
    pub bad_windows: u8,
    pub ready: bool,
    pub percent: f32,
}

impl Default for ErrorMonitor {
    fn default() -> Self {
        ErrorMonitor { errors: 0, total: 0, at: 0, bad_windows: 0, ready: false, percent: -1.0 }
    }
}

impl ErrorMonitor {
    pub fn reset(&mut self) {
        self.ready = false;
        self.bad_windows = 0;
        self.percent = -1.0;
    }

    /// True after three bad windows (>= 2 % errors) in a row.
    pub fn update(&mut self, error_value: u32, total_value: u32, now: u32) -> bool {
        if !self.ready {
            (self.errors, self.total, self.at, self.ready) = (error_value, total_value, now, true);
            return false;
        }
        let elapsed = now.wrapping_sub(self.at);
        if elapsed < 10_000 {
            return false;
        }
        let e = error_value.wrapping_sub(self.errors);
        let t = total_value.wrapping_sub(self.total);
        (self.errors, self.total, self.at) = (error_value, total_value, now);
        if elapsed > 15_000 || t == 0 || t > elapsed {
            self.percent = -1.0;
            self.bad_windows = 0;
            return false;
        }
        self.percent = (100.0 * e as f64 / t as f64) as f32;
        self.bad_windows = if self.percent >= 2.0 { (self.bad_windows + 1).min(3) } else { 0 };
        self.bad_windows >= 3
    }
}

/// A command for the chip (`bytes.len()` = 0: could not be encoded).
pub type Packet = Vec<u8>;

pub fn crc5(data: &[u8]) -> u8 {
    let mut crc: u8 = 31;
    for &byte in data {
        for bit in (0..8).rev() {
            let next = ((crc >> 4) ^ (byte >> bit)) & 1;
            crc = (((crc << 1) | next) ^ (next << 2)) & 31;
        }
    }
    crc
}

pub fn crc16_false(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xffff;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = (crc << 1) ^ if crc & 0x8000 != 0 { 0x1021 } else { 0 };
        }
    }
    crc
}

/// Header 0x2x: job with CRC16; otherwise a command with CRC5. `capacity`
/// is the largest frame the chip family takes.
pub(crate) fn frame(kind: u8, data: &[u8], capacity: usize) -> Packet {
    let job = kind & 0x20 != 0;
    let extra = if job { 6 } else { 5 };
    if data.len() > capacity - extra {
        return Vec::new();
    }
    let mut p = Vec::with_capacity(data.len() + extra);
    p.extend_from_slice(&[0x55, 0xaa, kind, (data.len() + if job { 4 } else { 3 }) as u8]);
    p.extend_from_slice(data);
    if job {
        let crc = crc16_false(&p[2..]);
        p.extend_from_slice(&crc.to_be_bytes());
    } else {
        let crc = crc5(&p[2..]);
        p.push(crc);
    }
    p
}

pub fn command(kind: u8, data: &[u8]) -> Packet {
    frame(kind, data, 128)
}

pub fn reg(address: u8, value: u32, broadcast: bool) -> Packet {
    let v = value.to_be_bytes();
    command(if broadcast { 0x51 } else { 0x41 }, &[0, address, v[0], v[1], v[2], v[3]])
}

pub fn version_mask(mask: u32) -> Packet {
    reg(0xa4, 0x9000_0000 | ((mask & VERSION_MASK) >> 13), true)
}

/// The chip reads each byte of the mask bit-reversed.
pub(crate) fn reversed_mask(power: u8) -> u32 {
    let mask: u32 = if power != 0 { (1u32 << power) - 1 } else { 0 };
    let mut reversed = 0;
    for byte in 0..4 {
        for bit in 0..8 {
            reversed |= ((mask >> (byte * 8 + bit)) & 1) << (byte * 8 + 7 - bit);
        }
    }
    reversed
}

pub fn ticket_mask(power: u8) -> Packet {
    if power > 31 {
        return Vec::new();
    }
    reg(0x14, reversed_mask(power), true)
}

pub fn job(slot: u8, header: &[u8; 80]) -> Packet {
    if slot as usize >= SLOTS {
        return Vec::new();
    }
    let mut payload = [0u8; 82];
    payload[0] = slot << 3;
    payload[1] = 1;
    payload[2..6].copy_from_slice(&header[76..80]);
    payload[6..10].copy_from_slice(&header[72..76]);
    payload[10..14].copy_from_slice(&header[68..72]);
    for w in 0..8 {
        payload[14 + 4 * w..18 + 4 * w].copy_from_slice(&header[36 + 4 * (7 - w)..40 + 4 * (7 - w)]);
        payload[46 + 4 * w..50 + 4 * w].copy_from_slice(&header[4 + 4 * (7 - w)..8 + 4 * (7 - w)]);
    }
    payload[78..82].copy_from_slice(&header[..4]);
    command(0x21, &payload)
}

/// PLL setting closest to `target` MHz, and the frequency it gives.
pub fn frequency(target: f32) -> Option<(Packet, f32)> {
    if !target.is_finite() || target < 50.0 || target > PLL_MAX_MHZ as f32 {
        return None;
    }
    let (mut best, mut error, mut vco_best) = (0.0f32, f32::MAX, f32::MAX);
    let (mut product_best, mut fb_best, mut ref_best, mut p1_best, mut p2_best) = (65535, 0, 0, 0, 0);
    for refdiv in (1..=2).rev() {
        for p1 in (1..=7).rev() {
            for p2 in (1..=7).rev() {
                let fb = (target / 25.0 * refdiv as f32 * p1 as f32 * p2 as f32).round() as i32;
                if p1 <= p2 || !(160..=239).contains(&fb) {
                    continue;
                }
                let f = 25.0 * fb as f32 / (refdiv * p1 * p2) as f32;
                let e = (f - target).abs();
                let vco = 25.0 * fb as f32 / refdiv as f32;
                if e < error
                    || ((e - error).abs() < 0.0001
                        && (vco < vco_best || ((vco - vco_best).abs() < 0.0001 && p1 * p2 < product_best)))
                {
                    (best, error, vco_best, product_best) = (f, e, vco, p1 * p2);
                    (fb_best, ref_best, p1_best, p2_best) = (fb, refdiv, p1, p2);
                }
            }
        }
    }
    if fb_best == 0 {
        return None;
    }
    let value = ((if vco_best >= 2400.0 { 0x50u32 } else { 0x40 }) << 24)
        | (fb_best as u32) << 16
        | (ref_best as u32) << 8
        | ((p1_best as u32 - 1) << 4)
        | (p2_best as u32 - 1);
    Some((reg(0x08, value, true), best))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Response {
    pub job: bool,
    pub slot: u8,
    pub address: u8,
    pub reg: u8,
    pub nonce: u32,
    pub version_bits: u32,
    pub value: u32,
}

/// Incremental resynchronization and CRC validation of 11-byte frames,
/// including fragmented UART reads.
#[derive(Clone, Debug, Default)]
pub struct Receiver {
    bytes: [u8; 11],
    length: usize,
}

impl Receiver {
    pub fn clear(&mut self) {
        self.length = 0;
    }

    pub fn push(&mut self, byte: u8) -> Option<Response> {
        if self.length == self.bytes.len() {
            self.length = 0;
        }
        self.bytes[self.length] = byte;
        self.length += 1;
        while self.length > 0 && (self.bytes[0] != 0xaa || (self.length > 1 && self.bytes[1] != 0x55)) {
            self.bytes.copy_within(1..self.length, 0);
            self.length -= 1;
        }
        if self.length != self.bytes.len() {
            return None;
        }
        let b = self.bytes;
        if crc5(&b[2..11]) != 0 {
            self.bytes.copy_within(1..self.length, 0);
            self.length -= 1;
            return None;
        }
        self.length = 0;
        Some(Response {
            job: b[10] & 0x80 != 0,
            slot: b[7] >> 4,
            nonce: u32::from_le_bytes([b[2], b[3], b[4], b[5]]),
            version_bits: (((b[8] as u32) << 8) | b[9] as u32) << 13,
            value: u32::from_be_bytes([b[2], b[3], b[4], b[5]]),
            address: b[6],
            reg: b[7],
        })
    }
}

/// PMBus LINEAR11 decoding.
pub fn linear11(word: u16) -> f32 {
    let mut exponent = ((word >> 11) & 31) as i32;
    let mut mantissa = (word & 2047) as i32;
    if exponent & 16 != 0 {
        exponent -= 32;
    }
    if mantissa & 1024 != 0 {
        mantissa -= 2048;
    }
    mantissa as f32 * 2f32.powi(exponent)
}

pub fn encode_linear11(value: f32) -> u16 {
    for e in -16i32..=15 {
        let m = (value * 2f32.powi(-e)).round();
        if m.is_finite() && (-1024.0..=1023.0).contains(&m) {
            return (((e & 31) << 11) | (m as i32 & 2047)) as u16;
        }
    }
    0
}

/// A whole number with the smallest exponent from 0 up (0 -> 0x0000, 105 -> 0x0069).
pub fn encode_linear11_int(value: u32) -> u16 {
    (0..=15).find(|e| (value >> e) < 1024).map(|e| (e << 11 | (value >> e)) as u16).unwrap_or(0)
}

/// VOUT_COMMAND word for `volts` in the regulator's VOUT_MODE.
pub fn voltage_word(volts: f32, mode: u8) -> Option<u16> {
    let mut e = (mode & 31) as i32;
    if e & 16 != 0 {
        e -= 32;
    }
    if mode & 0x60 != 0 || !(-16..0).contains(&e) || !volts.is_finite() || !(0.7..=2.0).contains(&volts) {
        return None;
    }
    let word = (volts * 2f32.powi(-e)).round();
    (0.0..=65535.0).contains(&word).then_some(word as u16)
}

pub fn diode_temperature(msb: u8, lsb: u8) -> f32 {
    let mut value = (((msb as u16) << 8 | lsb as u16) >> 5) as i32;
    if value == 0x3f8 || value == 0x3ff {
        return f32::NAN;
    }
    if value & 0x400 != 0 {
        value -= 0x800;
    }
    let t = value as f32 / 8.0;
    if (0.0..=120.0).contains(&t) {
        t
    } else {
        f32::NAN
    }
}

pub fn fan_rpm(tach: u16) -> u16 {
    if !(83..0xfff0).contains(&tach) {
        0
    } else {
        (5_400_000u32 / tach as u32) as u16
    }
}

pub const ASIC_MAX_C: f32 = 75.0;
pub const REGULATOR_MAX_C: f32 = 105.0;
/// Highest user thermal limit: that pause must come before the hard stop.
pub const MAX_USER_TEMP_C: u8 = ASIC_MAX_C as u8 - 2;
pub const STATUS_OFF: u16 = 0x0040;
pub const STATUS_VIN_UV: u16 = 0x0008;
pub const STATUS_IOUT_OC: u16 = 0x0010;
pub const STATUS_FAULTS: u16 = 0x0020 | STATUS_IOUT_OC | 0x0004;

#[derive(Clone, Copy, Debug, Default)]
pub struct Health {
    pub temperature: f32,
    pub regulator_temperature: f32,
    pub input_voltage: f32,
    pub core_voltage: f32,
    pub current: f32,
    pub rpm: u16,
    pub status: u16,
    /// Core voltage asked of the regulator; 0: the stock 1150 mV.
    pub core_target: f32,
}

impl Health {
    /// A fault stops the ASIC until restart.
    pub fn fault(&self, powered: bool, require_diode: bool) -> Option<&'static str> {
        if require_diode && !self.temperature.is_finite() {
            return Some("ASIC temperature sensor fault; restart required");
        }
        if self.temperature.is_finite() && self.temperature >= ASIC_MAX_C {
            return Some("ASIC overtemperature; restart required");
        }
        let rt = self.regulator_temperature;
        if !rt.is_finite() || !(-20.0..REGULATOR_MAX_C).contains(&rt) {
            return Some("Regulator temperature fault; restart required");
        }
        let vin = self.input_voltage;
        if !vin.is_finite() || vin > 5.5 || (powered && vin < 4.6) {
            return Some("5V supply fault; restart required");
        }
        if powered && self.rpm < 500 {
            return Some("Fan stopped; restart required");
        }
        if !self.current.is_finite() || self.current < -1.0 || self.current >= 30.0 {
            return Some("ASIC current fault; restart required");
        }
        if self.status & STATUS_IOUT_OC != 0 {
            return Some("Regulator output overcurrent; restart required");
        }
        let mask = if powered { STATUS_FAULTS | STATUS_OFF | STATUS_VIN_UV } else { STATUS_FAULTS };
        if self.status & mask != 0 {
            return Some("Regulator reports a fault; restart required");
        }
        let target = if self.core_target > 0.0 { self.core_target } else { STOCK_MV as f32 / 1000.0 };
        let v = self.core_voltage;
        if powered && (!v.is_finite() || v < target - 0.1505 || v > target + 0.1005) {
            return Some("ASIC voltage fault; restart required");
        }
        None
    }

    /// Before power-on: why the ASIC cannot start yet (waiting, not a fault).
    pub fn not_ready(&self) -> Option<&'static str> {
        if !self.input_voltage.is_finite() || self.input_voltage < 4.8 {
            return Some("Waiting for the 5V supply (barrel jack)");
        }
        if self.rpm < 500 {
            return Some("Waiting for the fan to turn");
        }
        None
    }
}

/// Hardware hash counter (units of 2^32 hashes), rejecting resets and bursts.
#[derive(Clone, Debug, Default)]
pub struct Counter {
    pub previous: u32,
    pub at: u32,
    pub ready: bool,
}

impl Counter {
    pub fn reset(&mut self) {
        self.ready = false;
    }

    pub fn update(&mut self, value: u32, now: u32) -> u64 {
        let elapsed = now.wrapping_sub(self.at);
        let delta = value.wrapping_sub(self.previous);
        let valid = self.ready && elapsed > 0 && elapsed <= 5000 && delta as u64 * 1000 <= elapsed as u64 * 1000;
        (self.previous, self.at, self.ready) = (value, now, true);
        if valid {
            (delta as u64) << 32
        } else {
            0
        }
    }
}
