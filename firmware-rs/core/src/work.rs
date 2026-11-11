//! Stratum job -> 80-byte block header, plus difficulty and hex helpers.

use crate::coin::Coin;
use crate::sha256::{self, MidState};

/// A mining job as received from the pool, already decoded.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub id: String,
    /// Header byte order.
    pub prev_hash: [u8; 32],
    pub coinb1: Vec<u8>,
    pub coinb2: Vec<u8>,
    /// 32 bytes each.
    pub branches: Vec<[u8; 32]>,
    pub version: u32,
    /// Explicitly negotiated BIP310/BIP320 bits.
    pub version_mask: u32,
    pub nbits: u32,
    pub ntime: u32,
    pub clean: bool,

    // Session data (mining.subscribe / mining.set_difficulty).
    pub extranonce1: Vec<u8>,
    pub extranonce2_size: usize,
    pub difficulty: f64,
    /// Stratum V2 supplies a full U256 target.
    pub exact_share_target: bool,
    pub share_target: [u8; 32],
}

impl Default for Job {
    fn default() -> Self {
        Job {
            id: String::new(),
            prev_hash: [0; 32],
            coinb1: Vec::new(),
            coinb2: Vec::new(),
            branches: Vec::new(),
            version: 0,
            version_mask: 0,
            nbits: 0,
            ntime: 0,
            clean: false,
            extranonce1: Vec::new(),
            extranonce2_size: 4,
            difficulty: 1.0,
            exact_share_target: false,
            share_target: [0; 32],
        }
    }
}

impl Job {
    /// BIP34 height pushed at the start of the coinbase script, 0 if unknown.
    pub fn block_height(&self, coin: Coin) -> u32 {
        // version(4) | vin count(1) | prevout(36) | script len(1) | script...
        let c = &self.coinb1;
        if c.len() < 43 {
            return 0;
        }
        // Peercoin transaction versions 1/2 serialize nTime after nVersion.
        let tx_version = i32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        let offset = if coin == Coin::Ppc && tx_version < 3 { 46 } else { 42 };
        if c.len() <= offset {
            return 0;
        }
        let n = c[offset] as usize;
        if !(1..=4).contains(&n) || c.len() < offset + 1 + n {
            return 0;
        }
        (0..n).fold(0u32, |h, i| h | (c[offset + 1 + i] as u32) << (8 * i))
    }
}

/// Ready-to-hash work unit derived from a job and one extranonce2 value.
#[derive(Clone, Debug, PartialEq)]
pub struct Work {
    pub job_id: String,
    pub extranonce2_hex: String,
    pub ntime: u32,
    pub version_mask: u32,
    pub header: [u8; 80],
    pub mid: MidState,
    pub share_difficulty: f64,
    /// Top-32-bit filter passed to `MidState::scan`.
    pub quick_limit: u32,
    /// Little-endian.
    pub network_target: [u8; 32],
    pub exact_share_target: bool,
    pub share_target: [u8; 32],
}

impl Work {
    pub fn version(&self) -> u32 {
        u32::from_le_bytes([self.header[0], self.header[1], self.header[2], self.header[3]])
    }
}

/// Builds the work for `extranonce2` (written big-endian in the coinbase,
/// at the job's fixed width).
pub fn build(job: &Job, extranonce2: u64) -> Work {
    let size = job.extranonce2_size;
    let mut en2 = vec![0u8; size];
    for i in 0..size.min(8) {
        en2[size - 1 - i] = (extranonce2 >> (8 * i)) as u8;
    }
    let mut coinbase = Vec::with_capacity(job.coinb1.len() + job.extranonce1.len() + size + job.coinb2.len());
    coinbase.extend_from_slice(&job.coinb1);
    coinbase.extend_from_slice(&job.extranonce1);
    coinbase.extend_from_slice(&en2);
    coinbase.extend_from_slice(&job.coinb2);

    let mut merkle = [0u8; 64];
    merkle[..32].copy_from_slice(&sha256::sha256d(&coinbase));
    for branch in &job.branches {
        merkle[32..].copy_from_slice(branch);
        let next = sha256::sha256d(&merkle);
        merkle[..32].copy_from_slice(&next);
    }

    let mut header = [0u8; 80];
    header[..4].copy_from_slice(&job.version.to_le_bytes());
    header[4..36].copy_from_slice(&job.prev_hash);
    header[36..68].copy_from_slice(&merkle[..32]);
    header[68..72].copy_from_slice(&job.ntime.to_le_bytes());
    header[72..76].copy_from_slice(&job.nbits.to_le_bytes());

    let quick_limit = if job.exact_share_target {
        u32::from_le_bytes([job.share_target[28], job.share_target[29], job.share_target[30], job.share_target[31]])
    } else {
        quick_limit_for(job.difficulty)
    };
    Work {
        job_id: job.id.clone(),
        extranonce2_hex: to_hex(&en2),
        ntime: job.ntime,
        version_mask: job.version_mask,
        mid: MidState::new(&header),
        header,
        share_difficulty: job.difficulty,
        quick_limit,
        network_target: target_from_bits(job.nbits).unwrap_or([0; 32]),
        exact_share_target: job.exact_share_target,
        share_target: job.share_target,
    }
}

/// 0xFFFF * 2^208, the difficulty-1 target.
pub const DIFF1: f64 = 26959535291011309493156476344723991336010898738574164086137773096960.0;

fn le_to_f64(v: &[u8; 32]) -> f64 {
    v.iter().rev().fold(0.0, |r, &b| r * 256.0 + b as f64)
}

/// Difficulty of a hash (little-endian 256-bit number as produced by sha256d).
pub fn hash_difficulty(hash: &[u8; 32]) -> f64 {
    // The 8 bytes from the most significant non-zero one carry more bits than
    // a double holds: one conversion instead of 32 multiply-adds.
    let Some(top) = hash.iter().rposition(|&b| b != 0) else {
        return f64::INFINITY;
    };
    let low = top.saturating_sub(7);
    let v = hash[low..=top].iter().rev().fold(0u64, |v, &b| v << 8 | b as u64);
    DIFF1 / (v as f64 * 2f64.powi(8 * low as i32))
}

/// Top 32 bits of the target matching `difficulty` (little-endian view),
/// rounded up so the quick filter never rejects a valid share.
pub fn quick_limit_for(difficulty: f64) -> u32 {
    if difficulty.is_nan() || difficulty <= 0.0 {
        return u32::MAX;
    }
    let top = 65535.0 / (65536.0 * difficulty);
    if top >= 4294967295.0 {
        return u32::MAX;
    }
    top.ceil() as u32
}

/// nBits -> positive 256-bit little-endian target; `None` for negative, zero
/// or overflowing compact values.
pub fn target_from_bits(nbits: u32) -> Option<[u8; 32]> {
    let exp = nbits >> 24;
    let mant = nbits & 0x007F_FFFF;
    if mant == 0 || nbits & 0x0080_0000 != 0 || exp > 34 || (mant > 0xff && exp > 33) || (mant > 0xffff && exp > 32) {
        return None;
    }
    let mut target = [0u8; 32];
    for i in 0..3 {
        let pos = exp as i32 - 3 + i;
        if (0..32).contains(&pos) {
            target[pos as usize] = (mant >> (8 * i)) as u8;
        }
    }
    target.iter().any(|&b| b != 0).then_some(target)
}

pub fn difficulty_from_bits(nbits: u32) -> f64 {
    match target_from_bits(nbits) {
        Some(t) => DIFF1 / le_to_f64(&t),
        None => 0.0,
    }
}

/// True if the little-endian 256-bit `hash` <= `target`.
pub fn meets_target(hash: &[u8; 32], target: &[u8; 32]) -> bool {
    for i in (0..32).rev() {
        if hash[i] != target[i] {
            return hash[i] < target[i];
        }
    }
    true
}

fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Decodes an even-length hex string.
pub fn from_hex(hex: &str) -> Option<Vec<u8>> {
    let b = hex.as_bytes();
    if b.len() % 2 != 0 {
        return None;
    }
    b.chunks(2).map(|p| Some(nibble(p[0])? << 4 | nibble(p[1])?)).collect()
}

/// Decodes exactly `N` bytes.
pub fn from_hex_array<const N: usize>(hex: &str) -> Option<[u8; N]> {
    from_hex(hex)?.try_into().ok()
}

pub fn to_hex(data: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(data.len() * 2);
    for &b in data {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 15) as usize] as char);
    }
    s
}

/// Human readable number: 12.3k, 4.56M ...
pub fn format_si(mut v: f64, decimals: usize) -> String {
    const UNITS: [&str; 9] = ["", "k", "M", "G", "T", "P", "E", "Z", "Y"];
    let mut u = 0;
    while v.abs() >= 1000.0 && u < 8 {
        v /= 1000.0;
        u += 1;
    }
    format!("{v:.decimals$}{}", UNITS[u])
}

/// A 32-byte digest in the usual block-identifier order (reversed hex).
pub fn reversed_hex(hash: &[u8; 32]) -> String {
    let mut r = *hash;
    r.reverse();
    to_hex(&r)
}
