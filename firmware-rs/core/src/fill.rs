//! Data for the software SHA-256d that the original ESP32's hardware loop
//! runs in its idle cycles (the C++ firmware's src/hw_sha_fill.S, generated
//! by tools/hw_fill/gen_hw_fill.py), ported from src/core/fill.cpp.
//!
//! The software hash goes round by round through records of 5 words, one per
//! round index i: A (the `a` register before round i), E (the `e` register),
//! W (message word), KW (K[i] + W[i]) and K (the round constant). Round i
//! reads records i-3..i and writes A, E of record i+1; it also expands
//! W[i+16] (and KW[i+16]). The first hash (second header block) starts at
//! round 3: rounds 0..2 do not depend on the nonce. The second hash stops
//! after round 60, when the top 16 bits of the result are known.

use crate::sha256::{MidState, IV, K};

pub const RECORD_WORDS: usize = 5; // A, E, W, KW, K
pub const RECORD_BYTES: usize = RECORD_WORDS * 4;
pub const B1_RECORDS: usize = 81; // first hash: rounds 0..80
pub const B2_RECORDS: usize = 84; // second hash: rounds -3..80
pub const STEPS: usize = 62; // rounds per hash done by the generated code

const A: usize = 0;
const E: usize = 1;
const W: usize = 2;
const KW: usize = 3;
const KF: usize = 4;

/// The generated code's buffers, laid out as the C++ `fill::Buffers`.
#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buffers {
    /// Record i at i.
    pub b1: [u32; B1_RECORDS * RECORD_WORDS],
    /// Record i at i + 3.
    pub b2: [u32; B2_RECORDS * RECORD_WORDS],
    /// State after the first 64 header bytes.
    pub mid: [u32; 8],
    /// W[18] of the first hash minus s0(W[3]).
    pub c18: u32,
}

impl Default for Buffers {
    fn default() -> Self {
        Buffers { b1: [0; B1_RECORDS * RECORD_WORDS], b2: [0; B2_RECORDS * RECORD_WORDS], mid: [0; 8], c18: 0 }
    }
}

fn rotr(x: u32, n: u32) -> u32 {
    x.rotate_right(n)
}
fn big_s0(x: u32) -> u32 {
    rotr(x, 2) ^ rotr(x, 13) ^ rotr(x, 22)
}
fn big_s1(x: u32) -> u32 {
    rotr(x, 6) ^ rotr(x, 11) ^ rotr(x, 25)
}
fn small_s1(x: u32) -> u32 {
    rotr(x, 17) ^ rotr(x, 19) ^ (x >> 10)
}
fn ch(x: u32, y: u32, z: u32) -> u32 {
    z ^ (x & (y ^ z))
}
fn maj(x: u32, y: u32, z: u32) -> u32 {
    (x & y) | (z & (x | y))
}

impl Buffers {
    fn b1(&mut self, round: usize, field: usize) -> &mut u32 {
        &mut self.b1[round * RECORD_WORDS + field]
    }

    fn b2(&mut self, round: isize, field: usize) -> &mut u32 {
        &mut self.b2[(round + 3) as usize * RECORD_WORDS + field]
    }

    /// Fills the buffers for an 80-byte header (nonce bytes ignored). Until
    /// the generated code has run one full nonce, the pending second-hash
    /// result is set so that it cannot look like a candidate.
    pub fn prepare(&mut self, header: &[u8; 80]) {
        *self = Buffers::default();
        let ms = MidState::new(header);
        self.mid = ms.mid;

        for i in 0..B1_RECORDS {
            *self.b1(i, KF) = if i < 64 { K[i] } else { 0 };
        }
        for i in -3..(B2_RECORDS as isize - 3) {
            *self.b2(i, KF) = if (0..64).contains(&i) { K[i as usize] } else { 0 };
        }

        // First hash: message words 0..17, W[3] (the nonce) and W[18] (which
        // depends on it) are written per nonce by the generated code.
        let mut w = [0u32; 18];
        w[..3].copy_from_slice(&ms.w[..3]);
        w[4] = 0x8000_0000;
        w[15] = 640;
        w[16] = ms.w[16];
        w[17] = ms.w[17];
        for (i, &word) in w.iter().enumerate() {
            *self.b1(i, W) = word;
            if i >= 4 {
                *self.b1(i, KW) = K[i].wrapping_add(word);
            }
        }
        self.c18 = small_s1(w[16]).wrapping_add(w[11]).wrapping_add(w[2]);

        // `a` and `e` before rounds 0..3: rounds 0..2 do not depend on the nonce.
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = ms.mid;
        for i in 0..=3 {
            *self.b1(i, A) = a;
            *self.b1(i, E) = e;
            if i == 3 {
                break;
            }
            let t1 = h.wrapping_add(big_s1(e)).wrapping_add(ch(e, f, g)).wrapping_add(K[i]).wrapping_add(w[i]);
            let t2 = big_s0(a).wrapping_add(maj(a, b, c));
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        // Second hash: starts from the initial state, words 8..15 are padding.
        for i in 0..4 {
            *self.b2(-(i as isize), A) = IV[i];
            *self.b2(-(i as isize), E) = IV[4 + i];
        }
        for i in 8..16 {
            let v = match i {
                8 => 0x8000_0000,
                15 => 256,
                _ => 0,
            };
            *self.b2(i, W) = v;
            *self.b2(i, KW) = K[i as usize].wrapping_add(v);
        }
        // No second hash done yet: `e` after round 60 + IV[7] must not end in
        // 16 zero bits.
        *self.b2(61, E) = 1u32.wrapping_sub(IV[7]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_follow_the_midstate() {
        let mut header = [0u8; 80];
        for (i, b) in header.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(37).wrapping_add(11);
        }
        let ms = MidState::new(&header);
        let mut buf = Buffers::default();
        buf.prepare(&header);
        assert_eq!(buf.mid, ms.mid);
        // After round 2, a and e are those of MidState's precomputation.
        assert_eq!(buf.b1[3 * RECORD_WORDS + A], ms.pre[0]);
        assert_eq!(buf.b1[3 * RECORD_WORDS + E], ms.pre[4]);
        assert_eq!(buf.b1[2 * RECORD_WORDS + A], ms.pre[1]);
        assert_eq!(buf.b1[2 * RECORD_WORDS + E], ms.pre[5]);
        assert_eq!(buf.b1[4 * RECORD_WORDS + KW], K[4].wrapping_add(0x8000_0000));
        assert_eq!(buf.b2[3 * RECORD_WORDS + A], IV[0]);
        assert_eq!(buf.b2[E], IV[7]);
        assert_eq!(buf.b2[(61 + 3) * RECORD_WORDS + E].wrapping_add(IV[7]), 1);
        assert_eq!(core::mem::size_of::<Buffers>(), (81 + 84) * 20 + 36);
    }
}
