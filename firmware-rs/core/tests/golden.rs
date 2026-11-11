//! The core against an independent reference set: `fixtures/golden.json`,
//! computed by `fixtures/make_golden.py` with Python's hashlib and integers
//! only. Covers the double SHA-256 kernel and its early-rejection filter,
//! hash difficulty, compact targets and the job -> header assembly.

use bdlminer_core::sha256::{sha256d, MidState};
use bdlminer_core::work::{self, from_hex, from_hex_array, Job};
use serde_json::Value;

fn golden() -> Value {
    serde_json::from_str(include_str!("fixtures/golden.json")).expect("golden.json")
}

fn close(a: f64, b: f64) -> bool {
    a == b || ((a - b) / b).abs() < 1e-15
}

#[test]
fn headers_hashes_and_filter() {
    let g = golden();
    let cases = g["headers"].as_array().unwrap();
    assert!(cases.len() >= 48);
    for case in cases {
        let header: [u8; 80] = from_hex_array(case["header"].as_str().unwrap()).unwrap();
        let expected: [u8; 32] = from_hex_array(case["hash"].as_str().unwrap()).unwrap();
        assert_eq!(sha256d(&header), expected);
        let nonce = u32::from_le_bytes(header[76..].try_into().unwrap());
        let mid = MidState::new(&header);
        let top = case["top32"].as_u64().unwrap() as u32;
        assert_eq!(mid.scan(nonce, top), Some(expected), "limit equal to the top word must pass");
        if top > 0 {
            assert_eq!(mid.scan(nonce, top - 1), None, "limit just below the top word must reject");
        }
        match case["difficulty"].as_f64() {
            Some(d) => assert!(close(work::hash_difficulty(&expected), d)),
            None => assert!(work::hash_difficulty(&expected).is_infinite()),
        }
    }
}

#[test]
fn compact_targets() {
    let g = golden();
    for case in g["targets"].as_array().unwrap() {
        let nbits = case["nbits"].as_u64().unwrap() as u32;
        match case["target"].as_str() {
            Some(hex) => assert_eq!(work::target_from_bits(nbits), from_hex_array::<32>(hex), "{nbits:08x}"),
            None => assert!(work::target_from_bits(nbits).is_none(), "{nbits:08x}"),
        }
        let d = case["difficulty"].as_f64().unwrap();
        assert!(
            close(work::difficulty_from_bits(nbits), d),
            "{nbits:08x}: {} != {d}",
            work::difficulty_from_bits(nbits)
        );
    }
}

#[test]
fn jobs_build_headers() {
    let g = golden();
    for case in g["jobs"].as_array().unwrap() {
        let hex = |k: &str| from_hex(case[k].as_str().unwrap()).unwrap();
        let num = |k: &str| case[k].as_u64().unwrap();
        let job = Job {
            id: "golden".into(),
            prev_hash: from_hex_array(case["prev_hash"].as_str().unwrap()).unwrap(),
            coinb1: hex("coinb1"),
            coinb2: hex("coinb2"),
            branches: case["branches"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| from_hex_array(b.as_str().unwrap()).unwrap())
                .collect(),
            version: num("version") as u32,
            nbits: num("nbits") as u32,
            ntime: num("ntime") as u32,
            extranonce1: hex("extranonce1"),
            extranonce2_size: num("extranonce2_size") as usize,
            ..Default::default()
        };
        let w = work::build(&job, num("extranonce2"));
        assert_eq!(w.extranonce2_hex, case["extranonce2_hex"].as_str().unwrap());
        assert_eq!(work::to_hex(&w.header), case["header"].as_str().unwrap());
        assert_eq!(w.mid, MidState::new(&w.header));
    }
}
