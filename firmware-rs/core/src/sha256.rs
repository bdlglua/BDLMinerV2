//! Portable SHA-256 helpers and the software mining kernel.
//!
//! `prepare` precomputes everything that is constant for an 80-byte header,
//! `scan` hashes one nonce and rejects it early when the top 32 bits of the
//! double hash (read as a little-endian 256-bit number) exceed a limit.

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

/// The eight working registers a..h.
#[derive(Clone, Copy)]
struct Regs([u32; 8]);

impl Regs {
    /// One round; the register roles rotate instead of the values moving.
    #[inline(always)]
    fn round(&mut self, i: usize, k: u32, w: u32) {
        let r = &mut self.0;
        let o = 8 - i % 8;
        let (a, b, c, d) = (o % 8, (o + 1) % 8, (o + 2) % 8, (o + 3) % 8);
        let (e, f, g, h) = ((o + 4) % 8, (o + 5) % 8, (o + 6) % 8, (o + 7) % 8);
        let t1 = r[h].wrapping_add(big_s1(r[e])).wrapping_add(ch(r[e], r[f], r[g])).wrapping_add(k).wrapping_add(w);
        let t2 = big_s0(r[a]).wrapping_add(maj(r[a], r[b], r[c]));
        r[d] = r[d].wrapping_add(t1);
        r[h] = t1.wrapping_add(t2);
    }

    /// Registers in a..h order after `rounds` rounds from this slot layout.
    fn ordered(&self, rounds: usize) -> [u32; 8] {
        let mut out = [0; 8];
        for (j, slot) in out.iter_mut().enumerate() {
            *slot = self.0[(j + 8 - rounds % 8) % 8];
        }
        out
    }
}

fn expand(w: &mut [u32; 64], from: usize, to: usize) {
    for i in from..to {
        w[i] = small_s1(w[i - 2]).wrapping_add(w[i - 7]).wrapping_add(small_s0(w[i - 15])).wrapping_add(w[i - 16]);
    }
}

fn compress_words(state: &mut [u32; 8], w: &mut [u32; 64]) {
    expand(w, 16, 64);
    let mut r = Regs(*state);
    for (i, (&k, &word)) in K.iter().zip(w.iter()).enumerate() {
        r.round(i, k, word);
    }
    let v = r.ordered(64);
    for (s, x) in state.iter_mut().zip(v) {
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
        let mut r = Regs(ms.mid);
        for i in 0..3 {
            r.round(i, K[i], ms.w[i]);
        }
        ms.pre = r.ordered(3);

        // nerd_sha256_bake(): w[0..2], w[16..17], the registers after round 2
        // in their rotated slots and the nonce-independent parts of round 3.
        ms.bake[0] = ms.w[0];
        ms.bake[1] = ms.w[1];
        ms.bake[2] = ms.w[2];
        ms.bake[3] = ms.w[16];
        ms.bake[4] = ms.w[17];
        ms.bake[5..13].copy_from_slice(&r.0);
        let p = ms.pre; // a..h after round 2
        ms.bake[13] = p[7].wrapping_add(big_s1(p[4])).wrapping_add(ch(p[4], p[5], p[6])).wrapping_add(K[3]);
        ms.bake[14] = big_s0(p[0]).wrapping_add(maj(p[0], p[1], p[2]));
        ms
    }

    /// Hash header(nonce) twice. Returns `None` early when the top 32 bits of
    /// the hash, read as a little-endian 256-bit number, are above `limit`.
    pub fn scan(&self, nonce: u32, limit: u32) -> Option<[u8; 32]> {
        let mut w = [0u32; 64];
        w[0] = self.w[0];
        w[1] = self.w[1];
        w[2] = self.w[2];
        w[3] = nonce.swap_bytes();
        w[4] = 0x8000_0000;
        w[15] = 640;
        w[16] = self.w[16];
        w[17] = self.w[17];
        expand(&mut w, 18, 64);

        // Resume at round 3 with round 3's nonce-independent terms baked.
        let [a, b, c, mut d, e, f, g, _] = self.pre;
        let t3 = self.bake[13].wrapping_add(w[3]);
        d = d.wrapping_add(t3);
        let h = t3.wrapping_add(self.bake[14]);
        // After round 3 the a..h registers are (h, a, b, c, d, e, f, g).
        let mut r = Regs([h, a, b, c, d, e, f, g]);
        for i in 4..64 {
            r.round(i - 4, K[i], w[i]);
        }
        let first = r.ordered(60);

        let mut w2 = [0u32; 64];
        for i in 0..8 {
            w2[i] = self.mid[i].wrapping_add(first[i]);
        }
        w2[8] = 0x8000_0000;
        w2[15] = 256;
        expand(&mut w2, 16, 61);
        let mut r = Regs(IV);
        for i in 0..57 {
            r.round(i, K[i], w2[i]);
        }
        if limit != u32::MAX {
            // After round 60 the final h word (e61) is known; rounds 57..60
            // only need their e updates to compute it. Named as the
            // registers of round 56 (56 % 8 == 0: the slots themselves).
            let [a, b, c, d, e, f, g, h] = r.0;
            let e58 = c
                .wrapping_add(g)
                .wrapping_add(big_s1(d))
                .wrapping_add(ch(d, e, f))
                .wrapping_add(K[57])
                .wrapping_add(w2[57]);
            let e59 = b
                .wrapping_add(f)
                .wrapping_add(big_s1(e58))
                .wrapping_add(ch(e58, d, e))
                .wrapping_add(K[58])
                .wrapping_add(w2[58]);
            let e60 = a
                .wrapping_add(e)
                .wrapping_add(big_s1(e59))
                .wrapping_add(ch(e59, e58, d))
                .wrapping_add(K[59])
                .wrapping_add(w2[59]);
            let e61 = h
                .wrapping_add(d)
                .wrapping_add(big_s1(e60))
                .wrapping_add(ch(e60, e59, e58))
                .wrapping_add(K[60])
                .wrapping_add(w2[60]);
            if IV[7].wrapping_add(e61).swap_bytes() > limit {
                return None;
            }
        }
        expand(&mut w2, 61, 64);
        for i in 57..64 {
            r.round(i, K[i], w2[i]);
        }
        let v = r.ordered(64);
        let mut st = IV;
        for (s, x) in st.iter_mut().zip(v) {
            *s = s.wrapping_add(x);
        }
        Some(put_state(&st))
    }
}

#[cfg(test)]
mod tests {
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
    }
}
