//! BM1397 (one on the NerdNOS board) wire protocol. Derived from
//! bitaxeorg/ESP-Miner and WantClue/NerdMiner_v2 (nerdnos branch), GPL-3.0.
//! The chip takes SHA-256 midstates: one per rolled version (up to four).

use crate::bm1370::{self, Packet};
use crate::sha256;

pub const VERSION_MASK: u32 = 0x1fff_e000;
/// Job ids 0, 4 ... 124: the low 2 bits name the midstate.
pub const SLOTS: usize = 32;
pub const MIDSTATES: usize = 4;
/// NerdNOS firmware's setting, about 8 W.
pub const DEFAULT_MHZ: u16 = 300;
/// 25 MHz / ((1 + 1) * 8).
pub const MAX_BAUD: u32 = 1_562_500;

const JOB: u8 = 0x21;
const WRITE: u8 = 0x51;
const READ: u8 = 0x52;
const INACTIVE: u8 = 0x53;
const SET_ADDRESS: u8 = 0x40;
const CHIP_ID: [u8; 6] = [0xaa, 0x55, 0x13, 0x97, 0x18, 0x00];

pub fn command(header: u8, data: &[u8]) -> Packet {
    bm1370::frame(header, data, 160)
}

pub fn reg(address: u8, value: u32) -> Packet {
    let v = value.to_be_bytes();
    command(WRITE, &[0, address, v[0], v[1], v[2], v[3]])
}
pub fn read_register(address: u8) -> Packet {
    command(READ, &[0, address])
}
pub fn chain_inactive() -> Packet {
    command(INACTIVE, &[0, 0])
}
pub fn set_address(address: u8) -> Packet {
    command(SET_ADDRESS, &[address, 0])
}
/// Results of difficulty >= 2^power.
pub fn ticket_mask(power: u8) -> Packet {
    if power > 31 {
        return Vec::new();
    }
    reg(0x14, bm1370::reversed_mask(power))
}
/// misc_control: UART divider 1, switching the chip to `MAX_BAUD`.
pub fn max_baud() -> Packet {
    reg(0x18, 0x0000_6131)
}
pub fn pll_divider() -> Packet {
    reg(0x70, 0x0f0f_0f00)
}

/// PLL0 for `mhz` (bounded to 50-650), cgminer's gekko calculation.
pub fn frequency(mhz: f32) -> Option<(Packet, f32)> {
    if !mhz.is_finite() {
        return None;
    }
    let f = mhz.clamp(50.0, 650.0);
    let fb = if f >= 500.0 { 1 } else { 2 };
    let fc1 = if f <= 150.0 {
        3
    } else if f <= 250.0 {
        2
    } else {
        1
    };
    let fc2 = 5;
    let base = 25.0 * (f * (fb * fc1 * fc2) as f32 / 25.0).ceil();
    let fa = (base / 25.0) as i32;
    if !(0x10..=0x104).contains(&fa) {
        return None;
    }
    let value = ((0x40 + (fa >> 8)) as u32) << 24
        | ((fa & 0xff) as u32) << 16
        | (fb as u32) << 8
        | (((fc1 & 7) << 4) | (fc2 & 7)) as u32;
    Some((reg(0x08, value), base / (fb * fc1 * fc2) as f32))
}

/// Adds 1 to the bits of `value` selected by `mask`, carrying across them.
pub fn increment_mask(mut value: u32, mut mask: u32) -> u32 {
    while mask != 0 {
        let sum = (value & mask).wrapping_add(mask & mask.wrapping_neg());
        let overflow = sum & !mask;
        value = (value & !mask) | (sum & mask);
        if overflow == 0 {
            break;
        }
        mask = overflow << 1;
    }
    value
}

/// SHA-256 state after header bytes 0..63 with `version` in place, in the
/// chip's order: state words 7..0, each little-endian.
pub fn midstate(header: &[u8; 80], version: u32) -> [u8; 32] {
    let mut block = [0u8; 64];
    block.copy_from_slice(&header[..64]);
    block[..4].copy_from_slice(&version.to_le_bytes());
    let mut state = sha256::IV;
    sha256::transform(&mut state, &block);
    let mut out = [0u8; 32];
    for w in 0..8 {
        out[4 * w..4 * w + 4].copy_from_slice(&state[7 - w].to_le_bytes());
    }
    out
}

/// A job for slot 0..31, and the version of each midstate (four with a
/// rolling mask, one without).
pub fn job(slot: u8, header: &[u8; 80], version_mask: u32) -> (Packet, Vec<u32>) {
    if slot as usize >= SLOTS {
        return (Vec::new(), Vec::new());
    }
    let mut version = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let count = if version_mask != 0 { MIDSTATES } else { 1 };
    let mut payload = vec![0u8; 18 + 32 * count];
    payload[0] = slot * 4; // reported back with the midstate index in bits 0..1
    payload[1] = count as u8;
    payload[6..10].copy_from_slice(&header[72..76]);
    payload[10..14].copy_from_slice(&header[68..72]);
    payload[14..18].copy_from_slice(&header[64..68]);
    let mut versions = Vec::with_capacity(count);
    for i in 0..count {
        versions.push(version);
        payload[18 + 32 * i..50 + 32 * i].copy_from_slice(&midstate(header, version));
        version = increment_mask(version, version_mask);
    }
    (command(JOB, &payload), versions)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Response {
    pub chip_id: bool,
    pub slot: u8,
    pub midstate: u8,
    /// As the header stores it.
    pub nonce: u32,
}

/// 9-byte frames (AA 55 ...), resynchronized on the preamble. The CRC is not
/// trusted to filter: every nonce is verified by hashing it.
#[derive(Clone, Debug, Default)]
pub struct Receiver {
    bytes: [u8; 9],
    length: usize,
}

impl Receiver {
    pub fn clear(&mut self) {
        self.length = 0;
    }

    pub fn push(&mut self, byte: u8) -> Option<Response> {
        if self.length == 0 && byte != 0xaa {
            return None;
        }
        if self.length == 1 && byte != 0x55 {
            self.length = if byte == 0xaa { 1 } else { 0 };
            return None;
        }
        self.bytes[self.length] = byte;
        self.length += 1;
        if self.length < self.bytes.len() {
            return None;
        }
        self.length = 0;
        let b = self.bytes;
        if b[..6] == CHIP_ID {
            return Some(Response { chip_id: true, ..Default::default() });
        }
        // With bit 7 set (a corrupted byte) the slot would be past the table.
        if b[7] & 0x80 != 0 {
            return None;
        }
        Some(Response {
            chip_id: false,
            nonce: u32::from_le_bytes([b[2], b[3], b[4], b[5]]),
            slot: (b[7] & 0xfc) >> 2,
            midstate: b[7] & 0x03,
        })
    }
}
