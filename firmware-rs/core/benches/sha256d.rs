//! Software SHA-256d hashrate of the mining kernel, one thread:
//!
//!     cargo bench -p bdlminer-core --bench sha256d
//!
//! "rejected" is the common case (early exit after round 60 of the second
//! hash), "full" computes every hash completely. The C++ firmware's
//! equivalent is tools/benchmark_sha256d.py.

use std::hint::black_box;
use std::time::Instant;

use bdlminer_core::sha256::MidState;

fn rate(mid: &MidState, limit: u32, nonces: u32) -> f64 {
    let start = Instant::now();
    let mut hits = 0u32;
    for n in 0..nonces {
        hits += black_box(mid.scan(black_box(n), limit)).is_some() as u32;
    }
    black_box(hits);
    nonces as f64 / start.elapsed().as_secs_f64()
}

fn main() {
    let header: [u8; 80] = std::array::from_fn(|i| (i as u8).wrapping_mul(37).wrapping_add(11));
    let mid = MidState::new(&header);
    let nonces = std::env::var("BDL_BENCH_NONCES").ok().and_then(|n| n.parse().ok()).unwrap_or(4_000_000);
    rate(&mid, 0, nonces / 10); // warm-up
    let rejected = rate(&mid, 0, nonces);
    let full = rate(&mid, u32::MAX, nonces);
    println!("sha256d rejected: {:.2} MH/s", rejected / 1e6);
    println!("sha256d full:     {:.2} MH/s", full / 1e6);
}
