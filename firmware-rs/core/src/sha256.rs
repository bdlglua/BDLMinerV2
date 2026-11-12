//! Portable SHA-256 helpers and the software mining kernel.
//!
//! `MidState::new` precomputes everything that is constant for an 80-byte
//! header, `MidState::scan` hashes one nonce and rejects it early when the
//! top 32 bits of the double hash (read as a little-endian 256-bit number)
//! exceed a limit.
//!
//! The rounds and the message schedule are written out in full (unrolled),
//! as the C++ kernel does with its ROUND8 macro and `#pragma GCC unroll`:
//! the eight working registers then stay in named variables, which the
//! compiler keeps in CPU registers, instead of array slots selected by a
//! computed index. On a PC this kernel hashes about 1.7 times faster than a
//! loop over the rounds (`cargo bench -p bdlminer-core --bench sha256d`).

pub const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
    0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
    0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
    0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
    0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
    0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

pub const IV: [u32; 8] =
    [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];

#[inline(always)]
pub(crate) fn big_s0(x: u32) -> u32 {
    x.rotate_right(2) ^ x.rotate_right(13) ^ x.rotate_right(22)
}
#[inline(always)]
pub(crate) fn big_s1(x: u32) -> u32 {
    x.rotate_right(6) ^ x.rotate_right(11) ^ x.rotate_right(25)
}
#[inline(always)]
pub(crate) fn small_s0(x: u32) -> u32 {
    x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3)
}
#[inline(always)]
pub(crate) fn small_s1(x: u32) -> u32 {
    x.rotate_right(17) ^ x.rotate_right(19) ^ (x >> 10)
}
#[inline(always)]
pub(crate) fn ch(x: u32, y: u32, z: u32) -> u32 {
    z ^ (x & (y ^ z))
}
#[inline(always)]
pub(crate) fn maj(x: u32, y: u32, z: u32) -> u32 {
    (x & y) | (z & (x | y))
}

#[inline(always)]
fn be32(p: &[u8]) -> u32 {
    u32::from_be_bytes([p[0], p[1], p[2], p[3]])
}

/// One round. The caller rotates the register names instead of moving the
/// values, so every round is plain arithmetic on eight variables.
macro_rules! round {
    ($a:ident, $b:ident, $c:ident, $d:ident, $e:ident, $f:ident, $g:ident, $h:ident, $k:expr, $w:expr) => {{
        let t1 = $h.wrapping_add(big_s1($e)).wrapping_add(ch($e, $f, $g)).wrapping_add($k).wrapping_add($w);
        let t2 = big_s0($a).wrapping_add(maj($a, $b, $c));
        $d = $d.wrapping_add(t1);
        $h = t1.wrapping_add(t2);
    }};
}

/// Rounds i..i+7; after them the names are back in their starting roles.
macro_rules! round8 {
    ($i:expr, $w:ident, $a:ident, $b:ident, $c:ident, $d:ident, $e:ident, $f:ident, $g:ident, $h:ident) => {
        round!($a, $b, $c, $d, $e, $f, $g, $h, K[$i], $w[$i]);
        round!($h, $a, $b, $c, $d, $e, $f, $g, K[$i + 1], $w[$i + 1]);
        round!($g, $h, $a, $b, $c, $d, $e, $f, K[$i + 2], $w[$i + 2]);
        round!($f, $g, $h, $a, $b, $c, $d, $e, K[$i + 3], $w[$i + 3]);
        round!($e, $f, $g, $h, $a, $b, $c, $d, K[$i + 4], $w[$i + 4]);
        round!($d, $e, $f, $g, $h, $a, $b, $c, K[$i + 5], $w[$i + 5]);
        round!($c, $d, $e, $f, $g, $h, $a, $b, K[$i + 6], $w[$i + 6]);
        round!($b, $c, $d, $e, $f, $g, $h, $a, K[$i + 7], $w[$i + 7]);
    };
}

/// Message schedule words at the listed constant indices (no loop, no
/// bounds checks: every index is known at compile time).
macro_rules! expand {
    ($w:ident; $($i:literal)*) => {
        $( $w[$i] = small_s1($w[$i - 2]).wrapping_add($w[$i - 7]).wrapping_add(small_s0($w[$i - 15])).wrapping_add($w[$i - 16]); )*
    };
}

/// The 64 rounds of one block on `w` (words 0..15 set), added into `state`.
#[inline(always)]
fn compress_words(state: &mut [u32; 8], w: &mut [u32; 64]) {
    expand!(w; 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39
               40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63);
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    round8!(0, w, a, b, c, d, e, f, g, h);
    round8!(8, w, a, b, c, d, e, f, g, h);
    round8!(16, w, a, b, c, d, e, f, g, h);
    round8!(24, w, a, b, c, d, e, f, g, h);
    round8!(32, w, a, b, c, d, e, f, g, h);
    round8!(40, w, a, b, c, d, e, f, g, h);
    round8!(48, w, a, b, c, d, e, f, g, h);
    round8!(56, w, a, b, c, d, e, f, g, h);
    for (s, x) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *s = s.wrapping_add(x);
    }
}

/// Compress one 64-byte block into `state` (standard big-endian words).
pub fn transform(state: &mut [u32; 8], block: &[u8]) {
    let mut w = [0u32; 64];
    for (i, word) in w.iter_mut().take(16).enumerate() {
        *word = be32(&block[4 * i..]);
    }
    compress_words(state, &mut w);
}

fn hash_state(data: &[u8]) -> [u32; 8] {
    let mut st = IV;
    let full = data.len() / 64;
    for i in 0..full {
        transform(&mut st, &data[64 * i..64 * i + 64]);
    }
    let mut tail = [0u8; 128];
    let rem = data.len() - full * 64;
    tail[..rem].copy_from_slice(&data[full * 64..]);
    tail[rem] = 0x80;
    let tail_len = if rem < 56 { 64 } else { 128 };
    let bits = (data.len() as u64).wrapping_mul(8);
    tail[tail_len - 8..tail_len].copy_from_slice(&bits.to_be_bytes());
    transform(&mut st, &tail[..64]);
    if tail_len == 128 {
        transform(&mut st, &tail[64..]);
    }
    st
}

fn put_state(state: &[u32; 8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, v) in state.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// SHA-256 over data given in pieces, without gathering it in one buffer
/// (an ESP32's heap may not hold the concatenation).
#[derive(Clone)]
pub struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    used: usize,
    length: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Sha256 { state: IV, block: [0; 64], used: 0, length: 0 }
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.length = self.length.wrapping_add(data.len() as u64);
        while !data.is_empty() {
            let take = (64 - self.used).min(data.len());
            self.block[self.used..self.used + take].copy_from_slice(&data[..take]);
            self.used += take;
            data = &data[take..];
            if self.used == 64 {
                transform(&mut self.state, &self.block);
                self.used = 0;
            }
        }
    }

    pub fn finish(mut self) -> [u8; 32] {
        let bits = self.length.wrapping_mul(8);
        self.update(&[0x80]);
        while self.used != 56 {
            self.update(&[0]);
        }
        self.block[56..].copy_from_slice(&bits.to_be_bytes());
        transform(&mut self.state, &self.block);
        put_state(&self.state)
    }
}

/// Plain SHA-256.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    put_state(&hash_state(data))
}

/// Double SHA-256 (coinbase, merkle tree, block header).
pub fn sha256d(data: &[u8]) -> [u8; 32] {
    let first = hash_state(data);
    // The first digest is already eight big-endian words: reuse them directly.
    let mut w = [0u32; 64];
    w[..8].copy_from_slice(&first);
    w[8] = 0x8000_0000;
    w[15] = 256;
    let mut st = IV;
    compress_words(&mut st, &mut w);
    put_state(&st)
}

/// Everything constant for a given 80-byte block header, precomputed once
/// per job so the per-nonce cost is as low as possible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MidState {
    /// State after the first 64 header bytes.
    pub mid: [u32; 8],
    /// `mid` advanced through rounds 0..2 of the second block (a..h order).
    pub pre: [u32; 8],
    /// Second block message words 0..17 (`w[3]` = nonce, patched per hash).
    pub w: [u32; 18],
    /// The layout of NerdMiner's `nerd_sha256_bake()`, used by the assembly
    /// prefilter of the ESP32; `bake[13..15]` also serve the portable kernel.
    pub bake: [u32; 15],
}

impl MidState {
    pub fn new(header: &[u8; 80]) -> Self {
        let mut ms = MidState { mid: IV, ..Default::default() };
        transform(&mut ms.mid, &header[..64]);
        let w = &mut ms.w;
        w[0] = be32(&header[64..]);
        w[1] = be32(&header[68..]);
        w[2] = be32(&header[72..]);
        w[3] = 0;
        w[4] = 0x8000_0000;
        w[15] = 640;
        w[16] = small_s1(w[14]).wrapping_add(w[9]).wrapping_add(small_s0(w[1])).wrapping_add(w[0]);
        w[17] = small_s1(w[15]).wrapping_add(w[10]).wrapping_add(small_s0(w[2])).wrapping_add(w[1]);

        // Rounds 0..2 only depend on w[0..2], fixed for the job.
        let w = ms.w;
        // Three rounds write d, h, then c, g, then b, f: a and e only read.
        let [a, mut b, mut c, mut d, e, mut f, mut g, mut h] = ms.mid;
        round!(a, b, c, d, e, f, g, h, K[0], w[0]);
        round!(h, a, b, c, d, e, f, g, K[1], w[1]);
        round!(g, h, a, b, c, d, e, f, K[2], w[2]);
        // After three rounds the roles are rotated by three positions.
        ms.pre = [f, g, h, a, b, c, d, e];

        // nerd_sha256_bake(): w[0..2], w[16..17], the registers after round 2
        // in their slots (a..h as named before the rounds) and the
        // nonce-independent parts of round 3.
        ms.bake[..3].copy_from_slice(&w[..3]);
        ms.bake[3] = w[16];
        ms.bake[4] = w[17];
        ms.bake[5..13].copy_from_slice(&[a, b, c, d, e, f, g, h]);
        let p = ms.pre; // a..h after round 2
        ms.bake[13] = p[7].wrapping_add(big_s1(p[4])).wrapping_add(ch(p[4], p[5], p[6])).wrapping_add(K[3]);
        ms.bake[14] = big_s0(p[0]).wrapping_add(maj(p[0], p[1], p[2]));
        ms
    }

    /// Hash header(nonce) twice. Returns `None` early when the top 32 bits of
    /// the hash, read as a little-endian 256-bit number, are above `limit`.
    pub fn scan(&self, nonce: u32, limit: u32) -> Option<[u8; 32]> {
        // ---- first hash, second block (header bytes 64..79 + padding) ----
        let mut w = [0u32; 64];
        w[0] = self.w[0];
        w[1] = self.w[1];
        w[2] = self.w[2];
        w[3] = nonce.swap_bytes();
        w[4] = 0x8000_0000;
        w[15] = 640;
        w[16] = self.w[16];
        w[17] = self.w[17];
        expand!(w; 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40
                   41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63);

        // Resume at round 3; MidState::new baked its nonce-independent terms.
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, _] = self.pre;
        let t3 = self.bake[13].wrapping_add(w[3]);
        d = d.wrapping_add(t3);
        let mut h = t3.wrapping_add(self.bake[14]);
        round!(h, a, b, c, d, e, f, g, K[4], w[4]);
        round!(g, h, a, b, c, d, e, f, K[5], w[5]);
        round!(f, g, h, a, b, c, d, e, K[6], w[6]);
        round!(e, f, g, h, a, b, c, d, K[7], w[7]);
        // Rename so that round8! continues with rounds 8..63.
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h) = (d, e, f, g, h, a, b, c);
        round8!(8, w, a, b, c, d, e, f, g, h);
        round8!(16, w, a, b, c, d, e, f, g, h);
        round8!(24, w, a, b, c, d, e, f, g, h);
        round8!(32, w, a, b, c, d, e, f, g, h);
        round8!(40, w, a, b, c, d, e, f, g, h);
        round8!(48, w, a, b, c, d, e, f, g, h);
        round8!(56, w, a, b, c, d, e, f, g, h);

        // ---- second hash, of the 32-byte digest ----
        let mut x = [0u32; 64];
        x[0] = self.mid[0].wrapping_add(a);
        x[1] = self.mid[1].wrapping_add(b);
        x[2] = self.mid[2].wrapping_add(c);
        x[3] = self.mid[3].wrapping_add(d);
        x[4] = self.mid[4].wrapping_add(e);
        x[5] = self.mid[5].wrapping_add(f);
        x[6] = self.mid[6].wrapping_add(g);
        x[7] = self.mid[7].wrapping_add(h);
        x[8] = 0x8000_0000;
        x[15] = 256;
        expand!(x; 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38
                   39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60);
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = IV;
        round8!(0, x, a, b, c, d, e, f, g, h);
        round8!(8, x, a, b, c, d, e, f, g, h);
        round8!(16, x, a, b, c, d, e, f, g, h);
        round8!(24, x, a, b, c, d, e, f, g, h);
        round8!(32, x, a, b, c, d, e, f, g, h);
        round8!(40, x, a, b, c, d, e, f, g, h);
        round8!(48, x, a, b, c, d, e, f, g, h);
        round!(a, b, c, d, e, f, g, h, K[56], x[56]);
        if limit != u32::MAX {
            // After round 60 the final h word (e61) is known; rounds 57..60
            // only need their e updates to compute it. Their new a values
            // cannot reach e61, and the full state after round 56 stays for
            // the rare candidate's completion.
            let e58 = c
                .wrapping_add(g)
                .wrapping_add(big_s1(d))
                .wrapping_add(ch(d, e, f))
                .wrapping_add(K[57])
                .wrapping_add(x[57]);
            let e59 = b
                .wrapping_add(f)
                .wrapping_add(big_s1(e58))
                .wrapping_add(ch(e58, d, e))
                .wrapping_add(K[58])
                .wrapping_add(x[58]);
            let e60 = a
                .wrapping_add(e)
                .wrapping_add(big_s1(e59))
                .wrapping_add(ch(e59, e58, d))
                .wrapping_add(K[59])
                .wrapping_add(x[59]);
            let e61 = h
                .wrapping_add(d)
                .wrapping_add(big_s1(e60))
                .wrapping_add(ch(e60, e59, e58))
                .wrapping_add(K[60])
                .wrapping_add(x[60]);
            if IV[7].wrapping_add(e61).swap_bytes() > limit {
                return None;
            }
        }
        // Finish only hashes that pass the top-word filter.
        expand!(x; 61 62 63);
        round!(h, a, b, c, d, e, f, g, K[57], x[57]);
        round!(g, h, a, b, c, d, e, f, K[58], x[58]);
        round!(f, g, h, a, b, c, d, e, K[59], x[59]);
        round!(e, f, g, h, a, b, c, d, K[60], x[60]);
        round!(d, e, f, g, h, a, b, c, K[61], x[61]);
        round!(c, d, e, f, g, h, a, b, K[62], x[62]);
        round!(b, c, d, e, f, g, h, a, K[63], x[63]);
        let mut st = IV;
        for (s, v) in st.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *s = s.wrapping_add(v);
        }
        Some(put_state(&st))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn incremental_matches_one_shot() {
        let data: Vec<u8> = (0..300u32).map(|i| (i * 7 + 3) as u8).collect();
        for len in [0, 1, 55, 56, 63, 64, 65, 119, 120, 128, 300] {
            for split in [0, 1, len / 3, len] {
                let mut h = Sha256::new();
                h.update(&data[..split.min(len)]);
                h.update(&data[split.min(len)..len]);
                assert_eq!(h.finish(), sha256(&data[..len]), "len {len} split {split}");
            }
        }
    }

    use super::*;

    #[test]
    fn known_digests() {
        let hex = |d: [u8; 32]| d.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(hex(sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            hex(sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(hex(sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        // A message whose padding needs a second block (56..63 byte tail).
        assert_eq!(hex(sha256(&[b'a'; 60])), "11ee391211c6256460b6ed375957fadd8061cafbb31daf967db875aebd5aaad4");
    }
}
