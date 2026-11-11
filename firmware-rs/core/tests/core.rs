//! Ported from test/host/test_core.cpp: SHA-256d kernel, header assembly,
//! Stratum V1 state machine and payout address checks.

use bdlminer_core::address::{self, Network, Type};
use bdlminer_core::coin::Coin;
use bdlminer_core::sha256::{sha256d, MidState};
use bdlminer_core::stratum::{Event, Protocol, RejectReason, Share, MAX_PENDING_SHARES};
use bdlminer_core::work::{self, from_hex, from_hex_array, reversed_hex, to_hex, Job};
use serde_json::Value;

const GENESIS_HEADER: &str = concat!(
    "01000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "3ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a",
    "29ab5f49",
    "ffff001d",
    "1dac2b7c"
);
const GENESIS_HASH: &str = "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f";
const GENESIS_NONCE: u32 = 2083236893;

fn lcg(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
    *seed
}

#[test]
fn sha256d_kernel_matches_reference() {
    let header: [u8; 80] = from_hex_array(GENESIS_HEADER).unwrap();
    let out = sha256d(&header);
    assert_eq!(reversed_hex(&out), GENESIS_HASH);
    let ms = MidState::new(&header);
    assert_eq!(ms.scan(GENESIS_NONCE, u32::MAX), Some(out));

    // Changing midstates, nonce byte order/wrap and exact equality at the top word.
    let mut seed = 0x93ac5f17u32;
    for job in 0..64 {
        let mut varied = [0u8; 80];
        for b in varied.iter_mut() {
            *b = (lcg(&mut seed) >> 24) as u8;
        }
        if job < 2 {
            varied = [if job == 1 { 0xff } else { 0 }; 80];
        }
        let prepared = MidState::new(&varied);
        for i in 0..32 {
            let nonce = if i < 4 { (i as u32).wrapping_sub(2) } else { lcg(&mut seed) };
            varied[76..].copy_from_slice(&nonce.to_le_bytes());
            let reference = sha256d(&varied);
            let top = u32::from_le_bytes(reference[28..32].try_into().unwrap());
            for limit in [0, 1, 0xffff, 0x0fff_ffff, u32::MAX, top, top.saturating_sub(1)] {
                let hit = prepared.scan(nonce, limit);
                assert_eq!(hit.is_some(), top <= limit);
                if let Some(h) = hit {
                    assert_eq!(h, reference);
                }
            }
        }
    }

    // Scanning around the genesis nonce at difficulty 1 finds exactly it.
    let limit = work::quick_limit_for(1.0);
    assert_eq!(limit, 1);
    let found: Vec<u32> = (GENESIS_NONCE - 50_000..GENESIS_NONCE + 50_000)
        .filter(|&n| ms.scan(n, limit).is_some_and(|h| work::hash_difficulty(&h) >= 1.0))
        .collect();
    assert_eq!(found, [GENESIS_NONCE]);
    let d = work::hash_difficulty(&out);
    assert!(d > 2536.0 && d < 2537.0, "{d}");

    let target = work::target_from_bits(0x1d00ffff).unwrap();
    assert!(work::meets_target(&out, &target));
    assert!((work::difficulty_from_bits(0x1d00ffff) - 1.0).abs() < 1e-9);
    for bits in [0u32, 0x1d80ffff, 0x01000001, 0x020000ff, 0x23000001, 0x22000100, 0x21010000] {
        assert!(work::target_from_bits(bits).is_none(), "{bits:08x}");
        assert_eq!(work::difficulty_from_bits(bits), 0.0);
    }
    assert_eq!(work::target_from_bits(0x01010000).unwrap()[0], 1);
    assert_eq!(work::target_from_bits(0x220000ff).unwrap()[31], 0xff);
    let t = work::target_from_bits(0x2100ffff).unwrap();
    assert!(t[31] == 0xff && t[30] == 0xff);
}

#[test]
fn hash_difficulty_is_exact() {
    let mut seed = 12345u32;
    let mut worst = 0f64;
    for k in 0..100_000 {
        let mut h = [0u8; 32];
        for b in h.iter_mut() {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            *b = (seed >> 16) as u8;
        }
        for z in 0..k % 32 {
            h[31 - z] = 0;
        }
        let v = h.iter().rev().fold(0f64, |v, &b| v * 256.0 + b as f64);
        let reference = work::DIFF1 / v;
        worst = worst.max((work::hash_difficulty(&h) / reference - 1.0).abs());
    }
    assert!(worst < 1e-15);
    assert!(work::hash_difficulty(&[0; 32]).is_infinite());
}

#[test]
fn subscribe_refused() {
    let mut p = Protocol::new();
    assert_eq!(
        p.handle_line(r#"{"result":"Only allowed user agents may subscribe","id":1,"error":null}"#),
        Event::SubscribeFailed
    );
    assert_eq!(p.last_error(), "Only allowed user agents may subscribe");
    assert_eq!(p.handle_line(r#"{"id":1,"result":null,"error":[20,"Other error",null]}"#), Event::SubscribeFailed);
    assert_eq!(p.last_error(), "Other error");
}

#[test]
fn stratum_robustness() {
    let mut p = Protocol::new();
    assert_eq!(p.handle_line(r#"{"id":null,"method":"mining.set_extranonce","params":["aabb",4]}"#), Event::None);
    assert!(!p.has_job());
    assert_eq!(p.handle_line(r#"{"id":null,"method":"mining.set_extranonce","params":["zz",4]}"#), Event::Error);
    assert_eq!(p.job().extranonce1.len(), 2);
    assert_eq!(p.handle_line(r#"{"id":null,"method":"mining.set_extranonce","params":["aabb",0]}"#), Event::Error);
    assert_eq!(p.handle_line(r#"{"id":1,"result":[[],"08000002",-1],"error":null}"#), Event::SubscribeFailed);
    assert_eq!(p.handle_line(r#"{"id":1,"result":[[],"08000002",64],"error":null}"#), Event::SubscribeFailed);
    let notify = r#"{"params":[12,"ab02cd818b9e567ee21793cddef299feb29ad444a41b85b8000008a300000000","01000000","00000000",[],"00000001","1a44b9f2","4dd7f5c7",true],"id":null,"method":"mining.notify"}"#;
    assert_eq!(p.handle_line(notify), Event::Error);
    assert!(!p.has_job());
}

const NOTIFY: &str = r#"{"method":"mining.notify","params":["job-a","ab02cd818b9e567ee21793cddef299feb29ad444a41b85b8000008a300000000","01000000","00000000",[],"00000001","1a44b9f2","4dd7f5c7",true]}"#;

#[test]
fn stratum_validation() {
    let mut p = Protocol::new();
    for line in [
        r#"{"id":2,"result":"true","error":null}"#,
        r#"{"id":2,"result":1,"error":null}"#,
        r#"{"id":2,"result":[],"error":null}"#,
        r#"{"id":2,"result":{},"error":null}"#,
    ] {
        p.reset();
        assert_eq!(p.handle_line(line), Event::AuthFailed);
        assert!(!p.authorized() && !p.last_error().is_empty());
    }
    assert_eq!(p.handle_line(r#"{"id":2,"result":true,"error":null}"#), Event::Authorized);
    assert_eq!(p.handle_line(r#"{"id":2,"result":true,"error":null}"#), Event::None);
    p.reset();
    assert_eq!(p.handle_line(r#"{"id":1,"result":[[],"aabb",4],"error":null}"#), Event::Subscribed);
    for line in [
        r#"{"id":1,"result":[],"error":null}"#,
        r#"{"id":1,"result":[[],null,4],"error":null}"#,
        r#"{"id":1,"result":[[],"ccdd"],"error":null}"#,
        r#"{"id":1,"result":[[],"ccdd",64],"error":null}"#,
        r#"{"id":1,"result":[[],"11zz",4],"error":null}"#,
    ] {
        assert_eq!(p.handle_line(line), Event::SubscribeFailed);
        assert_eq!(to_hex(&p.job().extranonce1), "aabb");
        assert_eq!(p.job().extranonce2_size, 4);
    }
    assert_eq!(p.handle_line(NOTIFY), Event::NewJob);
    for bits in ["1d80ffff", "23000001", "00000000"] {
        assert_eq!(p.handle_line(&NOTIFY.replace("1a44b9f2", bits)), Event::Error);
        assert_eq!(p.job().nbits, 0x1a44b9f2);
    }
    let original = work::build(p.job(), 7);
    let bad_fields: [(usize, &str); 18] = [
        (0, "12"),
        (0, "null"),
        (0, r#""""#),
        (0, r#""0123456789012345678901234567890123456789012345678901234567890123456789""#),
        (1, r#""00""#),
        (2, "null"),
        (3, "false"),
        (4, "null"),
        (4, "{}"),
        (4, r#"["11"]"#),
        (5, r#""0000000z""#),
        (5, r#""100000000""#),
        (5, "1"),
        (6, r#""1a44b9f2junk""#),
        (6, "null"),
        (7, r#""-0000001""#),
        (7, r#""4dd7f5c""#),
        (8, r#""false""#),
    ];
    for (index, json) in bad_fields {
        let mut notify: Value = serde_json::from_str(NOTIFY).unwrap();
        notify["params"][index] = serde_json::from_str(json).unwrap();
        assert_eq!(p.handle_line(&notify.to_string()), Event::Error, "{index} {json}");
        assert!(p.has_job() && p.job().id == "job-a" && !p.last_error().is_empty());
        assert_eq!(work::build(p.job(), 7).header, original.header);
    }
    let mut oversized: Value = serde_json::from_str(NOTIFY).unwrap();
    for _ in 0..33 {
        oversized["params"][4].as_array_mut().unwrap().push(Value::String("0".repeat(64)));
    }
    assert_eq!(p.handle_line(&oversized.to_string()), Event::Error);
    assert!(p.job().branches.is_empty());
}

#[test]
fn stratum_session_updates() {
    let mut p = Protocol::new();
    assert_eq!(p.handle_line(r#"{"id":1,"result":[[],"aabb",4],"error":null}"#), Event::Subscribed);
    assert_eq!(p.handle_line(NOTIFY), Event::NewJob);
    assert_eq!(p.job().difficulty, 1.0);
    let original = work::build(p.job(), 7);
    assert_eq!(p.handle_line(r#"{"method":"mining.set_difficulty","params":[0.0005]}"#), Event::Difficulty);
    assert_eq!(p.difficulty(), 0.0005);
    assert_eq!(p.job().difficulty, 1.0);
    assert_eq!(p.handle_line(r#"{"method":"mining.set_extranonce","params":["ccdd",8]}"#), Event::None);
    assert_eq!(p.job().extranonce2_size, 4);
    assert_eq!(work::build(p.job(), 7).header, original.header);
    assert_eq!(p.handle_line(r#"{"method":"mining.set_difficulty","params":[1e999]}"#), Event::Error);
    assert_eq!(p.handle_line(r#"{"method":"mining.set_extranonce","params":[null,4]}"#), Event::Error);
    assert_eq!(p.handle_line(r#"{"method":"mining.set_extranonce","params":["1122"]}"#), Event::Error);
    assert_eq!(p.handle_line(NOTIFY), Event::NewJob);
    assert_eq!(p.job().difficulty, 0.0005);
    assert_eq!(p.job().extranonce2_size, 8);
    assert_eq!(to_hex(&p.job().extranonce1), "ccdd");
    p.reset();
    assert!(!p.has_job());
    assert_eq!(p.handle_line(NOTIFY), Event::NewJob);
    assert_eq!(p.job().difficulty, 1.0);
    assert!(p.job().extranonce1.is_empty());
    assert_eq!(p.handle_line(r#"{"method":"mining.set_difficulty","params":[16777216]}"#), Event::Difficulty);
    assert_eq!(p.difficulty(), 16777216.0);
    assert_eq!(p.handle_line(NOTIFY), Event::NewJob);
    assert_eq!(p.job().difficulty, 16777216.0);
}

#[test]
fn stratum_job_and_shares() {
    let mut p = Protocol::new();
    p.set_credentials("addr.worker", "x");
    assert_eq!(
        p.handle_line(
            r#"{"id":1,"result":[[["mining.notify","ae6812eb4cd7735a302a8a9dd95cf71f"]],"08000002",4],"error":null}"#
        ),
        Event::Subscribed
    );
    assert_eq!(p.job().extranonce1.len(), 4);
    assert_eq!(p.handle_line(r#"{"id":null,"method":"mining.set_difficulty","params":[0.0005]}"#), Event::Difficulty);
    assert_eq!(p.handle_line(r#"{"id":2,"result":true,"error":null}"#), Event::Authorized);
    let notify = concat!(
        r#"{"params":["bf","ab02cd818b9e567ee21793cddef299feb29ad444a41b85b8000008a300000000","#,
        r#""01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff20020862062f503253482f04b8864e5008","#,
        r#""072f736c7573682f000000000100f2052a010000001976a914d23fcdf86f7e756a64a7a9688ef9903327048ed988ac00000000","#,
        r#"[],"00000001","1a44b9f2","4dd7f5c7",true],"id":null,"method":"mining.notify"}"#
    );
    assert_eq!(p.handle_line(notify), Event::NewJob);
    let j = p.job().clone();
    assert!(j.clean && j.difficulty == 0.0005);
    assert_eq!(j.block_height(Coin::Btc), 0x6208);
    let w = work::build(&j, 0);
    assert_eq!(to_hex(&w.header[..36]), "0100000081cd02ab7e569e8bcd9317e2fe99f2de44d49ab2b8851ba4a308000000000000");
    assert_eq!(to_hex(&w.header[68..76]), "c7f5d74df2b9441a");
    assert_eq!(w.extranonce2_hex, "00000000");
    let h125552: [u8; 80] = from_hex_array(concat!(
        "0100000081cd02ab7e569e8bcd9317e2fe99f2de44d49ab2b8851ba4a308000000000000",
        "e320b6c2fffc8d750423db8b1eb942ae710e951ed797f7affc8892b0f1fc122b",
        "c7f5d74df2b9441a42a14695"
    ))
    .unwrap();
    assert_eq!(reversed_hex(&sha256d(&h125552)), "00000000000000001e8d6829a8a21adc5d38d0a473b144b6765798e61f98bd1d");

    let mut s = Share {
        job_id: "bf".into(),
        extranonce2: "00000001".into(),
        ntime: 0x4dd7f5c7,
        nonce: 0x9546a142,
        pool_difficulty: w.share_difficulty,
        ..Default::default()
    };
    assert_eq!(
        p.submit(&s).unwrap(),
        "{\"id\":10,\"method\":\"mining.submit\",\"params\":[\"addr.worker\",\"bf\",\"00000001\",\"4dd7f5c7\",\"9546a142\"]}\n"
    );
    assert_eq!(p.handle_line(r#"{"id":10,"result":true,"error":null}"#), Event::ShareAccepted);
    assert!(p.acknowledgment().id == 10 && p.acknowledgment().difficulty == 0.0005);
    assert_eq!(p.handle_line(r#"{"id":10,"result":true,"error":null}"#), Event::None);
    assert_eq!(p.handle_line(r#"{"id":999,"result":true,"error":null}"#), Event::None);
    p.submit(&s);
    assert_eq!(
        p.handle_line(r#"{"id":11,"result":null,"error":[23,"Low difficulty share",null]}"#),
        Event::ShareRejected
    );
    assert_eq!(p.last_error(), "Low difficulty share");
    assert_eq!(p.acknowledgment().reason, RejectReason::LowDifficulty);
    {
        let mut a = Protocol::new();
        a.set_credentials("bc1qtypo.worker", "x");
        a.handle_line(r#"{"id":1,"result":[[],"00",4],"error":null}"#);
        assert_eq!(
            a.handle_line(r#"{"id":2,"result":null,"error":[20,"Authorization validation error",", bc1qtypo"]}"#),
            Event::AuthFailed
        );
        assert_eq!(a.last_error(), "Authorization validation error (bc1qtypo)");
        let long = "x".repeat(200);
        a.handle_line(&format!("{{\"id\":2,\"result\":null,\"error\":[20,\"Refused\",\"{long}\"]}}"));
        assert_eq!(a.last_error(), format!("Refused ({}...)", "x".repeat(96)));
        a.handle_line(r#"{"id":2,"result":null,"error":[20,"Refused",""]}"#);
        assert_eq!(a.last_error(), "Refused");
    }
    assert_eq!(p.handle_line(r#"{"id":11,"result":false,"error":null}"#), Event::None);
    assert_eq!(p.pending_shares(), 0);
    p.submit(&s);
    assert_eq!(p.handle_line(r#"{"id":12,"result":false,"error":null}"#), Event::ShareRejected);
    assert!(p.acknowledgment().reason == RejectReason::Unknown && p.last_error() == "share rejected");
    for _ in 0..MAX_PENDING_SHARES {
        assert!(p.submit(&s).is_some());
    }
    assert!(!p.can_submit() && p.submit(&s).is_none());
    assert_eq!(p.handle_line(r#"{"id":13,"result":{},"error":null}"#), Event::Error);
    assert_eq!(p.pending_shares(), MAX_PENDING_SHARES);
    assert_eq!(p.handle_line(r#"{"id":"13","result":true,"error":null}"#), Event::None);
    assert_eq!(p.handle_line(r#"{"id":13.5,"result":true,"error":null}"#), Event::None);
    assert_eq!(p.handle_line(r#"{"id":13,"result":true,"error":null}"#), Event::ShareAccepted);
    assert!(p.can_submit());
    p.reset();
    assert_eq!(p.pending_shares(), 0);

    // Targets follow the submitted work even when vardiff and notify arrive first.
    s.pool_difficulty = 0.25;
    s.difficulty = 2536.0;
    assert!(p.submit(&s).is_some() && p.submission_id() == 10);
    s.pool_difficulty = 0.5;
    assert!(p.submit(&s).is_some() && p.submission_id() == 11);
    assert_eq!(p.handle_line(r#"{"method":"mining.set_difficulty","params":[100]}"#), Event::Difficulty);
    assert!(p.handle_line(notify) == Event::NewJob && p.job().difficulty == 100.0);
    s.pool_difficulty = 0.125;
    assert!(p.submit(&s).is_some() && p.submission_id() == 12);
    assert_eq!(p.handle_line(r#"{"id":11,"result":false,"error":null}"#), Event::ShareRejected);
    assert!(p.acknowledgment().id == 11 && p.acknowledgment().difficulty == 0.5);
    assert_eq!(p.handle_line(r#"{"id":10,"result":true,"error":null}"#), Event::ShareAccepted);
    assert!(p.acknowledgment().id == 10 && p.acknowledgment().difficulty == 0.25);
    assert_eq!(p.handle_line(r#"{"id":12,"result":true,"error":null}"#), Event::ShareAccepted);
    assert!(p.acknowledgment().id == 12 && p.acknowledgment().difficulty == 0.125);
    assert_eq!(p.pending_shares(), 0);
}

#[test]
fn version_rolling_negotiation() {
    let mut proto = Protocol::new();
    let configure: Value = serde_json::from_str(&proto.configure_version_rolling()).unwrap();
    assert_eq!(configure["method"], "mining.configure");
    proto.handle_line(r#"{"id":4,"result":{"version-rolling":true,"version-rolling.mask":"1fffe000"},"error":null}"#);
    let mut share = Share {
        job_id: "a".into(),
        extranonce2: "00000001".into(),
        version_mask: 0x1fffe000,
        version: 0x20002000,
        ..Default::default()
    };
    let line: Value = serde_json::from_str(&proto.submit(&share).unwrap()).unwrap();
    assert_eq!(line["params"].as_array().unwrap().len(), 6);
    assert_eq!(line["params"][5], "00002000");
    proto.reset();
    assert!(proto.submit(&share).is_none());
    share.version_mask = 0;
    let line: Value = serde_json::from_str(&proto.submit(&share).unwrap()).unwrap();
    assert_eq!(line["params"].as_array().unwrap().len(), 5);
    proto.configure_version_rolling();
    assert_eq!(
        proto.handle_line(r#"{"id":4,"result":{"version-rolling":true,"version-rolling.mask":"ffffffff"}}"#),
        Event::Error
    );
    proto.reset();
    assert_eq!(proto.handle_line(r#"{"method":"mining.set_version_mask","params":["1fffe000"]}"#), Event::Error);
}

#[test]
fn bitcoin_addresses() {
    let cases: [(&str, Type, Network); 15] = [
        ("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa", Type::P2pkh, Network::Main),
        ("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa.rig1", Type::P2pkh, Network::Main),
        ("3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy", Type::P2sh, Network::Main),
        ("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4", Type::P2wpkh, Network::Main),
        ("BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4", Type::P2wpkh, Network::Main),
        ("bc1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qccfmv3", Type::P2wsh, Network::Main),
        ("bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0", Type::P2tr, Network::Main),
        ("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx", Type::P2wpkh, Network::Test),
        ("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNb", Type::Invalid, Network::Unknown),
        ("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t5", Type::Invalid, Network::Unknown),
        ("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kV8F3T4", Type::Invalid, Network::Unknown),
        ("bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqh2y7hd", Type::Invalid, Network::Unknown),
        ("ltc1qw508d6qejxtdg4y5r3zarvary0c5xw7kgmn4n9", Type::Invalid, Network::Unknown),
        ("myaccount.worker1", Type::Invalid, Network::Unknown),
        ("", Type::Invalid, Network::Unknown),
    ];
    for (user, kind, network) in cases {
        let info = address::parse(user, Coin::Btc);
        assert_eq!((info.kind, info.network), (kind, network), "{user}");
    }
}

#[test]
fn cash_addresses() {
    let p2pkh = [
        "qpm2qsznhks23z7629mms6s4cwef74vcwvy22gdx6a",
        "qr95sy3j9xwd2ap32xkykttr4cvcu7as4y0qverfuy",
        "qqq3728yw0y47sqn6l2na30mcw6zm78dzqre909m2r",
    ];
    let p2sh = ["ppm2qsznhks23z7629mms6s4cwef74vcwvn0h829pq", "pr95sy3j9xwd2ap32xkykttr4cvcu7as4yc93ky28e"];
    for (set, expected) in [(&p2pkh[..], Type::P2pkh), (&p2sh[..], Type::P2sh)] {
        for bare in set {
            let full = format!("bitcoincash:{bare}");
            assert_eq!(address::parse(bare, Coin::Bch).kind, expected);
            assert_eq!(address::parse(&format!("{full}.worker"), Coin::Bch).kind, expected);
            assert_eq!(address::parse(&full, Coin::Bch).network, Network::Main);
            assert!(!address::parse(&full, Coin::Btc).valid());
            assert_eq!(address::parse(&full.to_ascii_uppercase(), Coin::Bch).kind, expected);
            assert!(!address::parse(&format!("B{}", &full[1..]), Coin::Bch).valid());
            for j in 0..bare.len() {
                let mut broken = bare.as_bytes().to_vec();
                broken[j] = if broken[j] == b'q' { b'p' } else { b'q' };
                assert!(!address::parse(std::str::from_utf8(&broken).unwrap(), Coin::Bch).valid());
            }
        }
    }
    for bad in [
        "bitcoincash:",
        "bitcoincash:qpzry9x8gf2tvdw0s3jn54khce6mua7lcw20ayyn",
        "bchtest:qpm2qsznhks23z7629mms6s4cwef74vcwvy22gdx6a",
        "ecash:qpm2qsznhks23z7629mms6s4cwef74vcwvy22gdx6a",
        "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh",
    ] {
        assert!(!address::parse(bad, Coin::Bch).valid(), "{bad}");
    }
    assert_eq!(address::parse("1BpEi6DfDAUFd7GtittLSdBeYJvcoaVggu.worker", Coin::Bch).kind, Type::P2pkh);
    assert!(!address::parse(&"q".repeat(2000), Coin::Bch).valid());
}

#[test]
fn more_coin_addresses() {
    let cases: [(&str, Coin, Type, Network); 27] = [
        ("ecash:qpm2qsznhks23z7629mms6s4cwef74vcwva87rkuu2", Coin::Xec, Type::P2pkh, Network::Main),
        ("ecash:qr95sy3j9xwd2ap32xkykttr4cvcu7as4ykdcjcn6n", Coin::Xec, Type::P2pkh, Network::Main),
        ("ecash:qqq3728yw0y47sqn6l2na30mcw6zm78dzq653y7pv5", Coin::Xec, Type::P2pkh, Network::Main),
        ("ecash:ppm2qsznhks23z7629mms6s4cwef74vcwv2zrv3l8h", Coin::Xec, Type::P2sh, Network::Main),
        ("ecash:pr95sy3j9xwd2ap32xkykttr4cvcu7as4ypg9alspw", Coin::Xec, Type::P2sh, Network::Main),
        ("ecash:pqq3728yw0y47sqn6l2na30mcw6zm78dzqd3vtezhf", Coin::Xec, Type::P2sh, Network::Main),
        ("ectest:qpm2qsznhks23z7629mms6s4cwef74vcwvmvqr33lm", Coin::Xec, Type::P2pkh, Network::Test),
        ("ectest:qr95sy3j9xwd2ap32xkykttr4cvcu7as4ysxxjl7ez", Coin::Xec, Type::P2pkh, Network::Test),
        ("ectest:qqq3728yw0y47sqn6l2na30mcw6zm78dzqul0yev09", Coin::Xec, Type::P2pkh, Network::Test),
        ("ectest:ppm2qsznhks23z7629mms6s4cwef74vcwvvfavkjyx", Coin::Xec, Type::P2sh, Network::Test),
        ("ectest:pr95sy3j9xwd2ap32xkykttr4cvcu7as4y8rmacazl", Coin::Xec, Type::P2sh, Network::Test),
        ("ectest:pqq3728yw0y47sqn6l2na30mcw6zm78dzqt6jt705c", Coin::Xec, Type::P2sh, Network::Test),
        ("DSC6tGC2NvN6AHio2p8pU4seTbPKDi7KZb", Coin::Dgb, Type::P2pkh, Network::Main),
        ("SefNWSMmXAxc4N8sxHyg8hHXTTU9GojgzX", Coin::Dgb, Type::P2sh, Network::Main),
        ("sxnDwNPKaPKZd5ZgJfVfom8cX4ZJkYr1zL", Coin::Dgb, Type::P2pkh, Network::Test),
        ("yf4ztby3Z5cFgxjbTpnwzU2TNxn5oiorbW", Coin::Dgb, Type::P2sh, Network::Test),
        ("dgb1qhyz5jznu8rxjz6emktwwkz5m0m4gr9q9rvhegc", Coin::Dgb, Type::P2wpkh, Network::Main),
        ("dgb1qs6parp27xaccjznw4sruj9nrtuyl3wtraf58ml7nen522da2vxqql3zq6n", Coin::Dgb, Type::P2wsh, Network::Main),
        ("dgb1pva0l3ng2q0gt2t3rqqpqxk5y57l4u0xyryxdslrt76k5u54rvpssjpmuhq", Coin::Dgb, Type::P2tr, Network::Main),
        ("dgbt1qznyt7p9zln65ydpuzfq8ksuz4hkvcwdl9qtcty", Coin::Dgb, Type::P2wpkh, Network::Test),
        ("dgbt1qu2au468tx43wqwzkhgruhxzksgt2j785kekzsw0hymg7kyzgumtq02uk88", Coin::Dgb, Type::P2wsh, Network::Test),
        ("dgbt1pykapk8r8wlwrl60zwz8rhfy5x3dc3k545utuea798ye2qk6vay9q3l9vt7", Coin::Dgb, Type::P2tr, Network::Test),
        ("dgbrt1qgmzq7xt3pkclw9d6emultxnhj4lumdldzeyypq", Coin::Dgb, Type::P2wpkh, Network::Regtest),
        ("dgbrt1qdmn2glunkfcmahndk3ukygkwgwsr26842gzlctfz4mnscp2mz6uql5jdft", Coin::Dgb, Type::P2wsh, Network::Regtest),
        ("dgbrt1pwvzf9ga8e7gp5v5eut66fctgkf29uu7efty3aseha3tzzc3sp92qjz3afh", Coin::Dgb, Type::P2tr, Network::Regtest),
        ("1BpEi6DfDAUFd7GtittLSdBeYJvcoaVggu", Coin::Bsv, Type::P2pkh, Network::Main),
        ("3CWFddi6m4ndiGyKqzYvsFYagqDLPVMTzC", Coin::Bsv, Type::P2sh, Network::Main),
    ];
    for (text, coin, kind, network) in cases {
        let parsed = address::parse(&format!("{text}.worker"), coin);
        assert_eq!((parsed.kind, parsed.network), (kind, network), "{text}");
        if coin == Coin::Xec {
            assert!(!address::parse(text, Coin::Bch).valid());
            let split = text.find(':').unwrap();
            if network == Network::Main {
                assert_eq!(address::parse(&text[split + 1..], coin).kind, kind);
            }
            assert_eq!(address::parse(&text.to_ascii_uppercase(), coin).kind, kind);
        }
        if coin == Coin::Dgb {
            assert!(!address::parse(text, Coin::Btc).valid());
        }
        let mut broken = text.to_owned();
        let last = broken.pop().unwrap();
        broken.push(if last == 'q' { 'p' } else { 'q' });
        assert!(!address::parse(&broken, coin).valid(), "{broken}");
    }
    assert!(!address::parse("bitcoincash:qpm2qsznhks23z7629mms6s4cwef74vcwvy22gdx6a", Coin::Xec).valid());
    assert!(!address::parse("bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh", Coin::Bsv).valid());
    assert!(!address::parse("1BpEi6DfDAUFd7GtittLSdBeYJvcoaVggu", Coin::Dgb).valid());
    assert_eq!(Coin::Dgb.halving_interval(), 0);
    for coin in Coin::ALL {
        assert_eq!(Coin::from_ticker(coin.ticker()), Some(coin));
        let version = if coin == Coin::Dgb { 0x20000200 } else { 0x20000000 };
        assert!(coin.job_compatible(version, 0x1fffe000));
    }
    for algo in 0..16u32 {
        assert_eq!(Coin::Dgb.job_compatible(0x20000000 | (algo << 8), 0), algo == 2);
    }
    assert!(!Coin::Dgb.job_compatible(0x20000200, 0x100));
}

#[test]
fn peercoin() {
    let cases: [(&str, Type, Network); 10] = [
        ("PGWPnLCWjUMNfGSSu9zpLLnrbBfoXrkhmB", Type::P2pkh, Network::Main),
        ("p77CZFn9jvg9waCzKBzkQfSvBBzPH1nRre", Type::P2sh, Network::Main),
        ("mzK2FFDEhxqHcmrJw1ysqFkVyhUULo45hZ", Type::P2pkh, Network::Test),
        ("2NC2hEhe28ULKAJkW5MjZ3jtTMJdvXmByvK", Type::P2sh, Network::Test),
        ("pc1qvzvkjn4q3nszqxrv3nraga2r822xjty36ylgu7", Type::P2wpkh, Network::Main),
        ("pc1qwqdg6squsna38e46795at95yu9atm8azzmyvckulcc7kytlcckxsu3esav", Type::P2wsh, Network::Main),
        ("tpc1qvzvkjn4q3nszqxrv3nraga2r822xjty3qzdpq4", Type::P2wpkh, Network::Test),
        ("pcrt1qvzvkjn4q3nszqxrv3nraga2r822xjty3h5fumu", Type::P2wpkh, Network::Regtest),
        ("tb1qvzvkjn4q3nszqxrv3nraga2r822xjty3wsh98a", Type::P2wpkh, Network::Test),
        ("pc1pqqqsyqcyq5rqwzqfpg9scrgwpugpzysnzs23v9ccrydpk8qarc0s6f7fhu", Type::P2tr, Network::Main),
    ];
    for (text, kind, network) in cases {
        let parsed = address::parse(&format!("{text}.worker"), Coin::Ppc);
        assert_eq!((parsed.kind, parsed.network), (kind, network), "{text}");
        let mut broken = text.to_owned();
        let last = broken.pop().unwrap();
        broken.push(if last == 'q' { 'p' } else { 'q' });
        assert!(!address::parse(&broken, Coin::Ppc).valid());
        if network == Network::Main {
            assert!(!address::parse(text, Coin::Btc).valid());
        }
        if text.starts_with("pc1") {
            assert_eq!(address::parse(&text.to_ascii_uppercase(), Coin::Ppc).kind, kind);
            assert!(!Coin::Btc.address_compatible(text));
        }
    }
    assert!(!address::parse("1BpEi6DfDAUFd7GtittLSdBeYJvcoaVggu", Coin::Ppc).valid());
    assert!(Coin::Ppc.pool_compatible("stratum+tcp://peermining.org:3333"));
    assert!(!Coin::Btc.pool_compatible("peermining.org"));
    assert!(Coin::Ppc.pool_error("peermining.org", 1).is_some());
    assert!(Coin::Ppc.halving_interval() == 0 && !Coin::Ppc.bitcoin_data());

    // Official genesis: legacy timestamped coinbase, standard 80-byte header.
    let mut job = Job { version: 1, ntime: 1345084287, nbits: 0x1d00ffff, extranonce2_size: 0, ..Default::default() };
    job.coinb1 = from_hex("01000000a2592c50010000000000000000000000000000000000000000000000000000000000000000ffffffff5404ffff001d020f274b4d61746f6e69732030372d4155472d3230313220506172616c6c656c2043757272656e6369657320416e642054686520526f61646d617020546f204d6f6e65746172792046726565646f6dffffffff0100000000000000000000000000").unwrap();
    let mut built = work::build(&job, 0);
    let merkle: [u8; 32] = built.header[36..68].try_into().unwrap();
    assert_eq!(reversed_hex(&merkle), "3c2d8f85fab4d17aac558cc648a1a58acff0de6deb890c29985690052c5993c2");
    built.header[76..].copy_from_slice(&2179302059u32.to_le_bytes());
    let hash = sha256d(&built.header);
    assert_eq!(reversed_hex(&hash), "0000000032fe677166d54963b62a4677d8957e87c508eaa4fd7eb1c880cd27e3");
    assert!(work::meets_target(&hash, &built.network_target));
    for version in [1u8, 2, 3] {
        job.coinb1 = vec![0; if version < 3 { 51 } else { 47 }];
        job.coinb1[0] = version;
        let offset = if version < 3 { 46 } else { 42 };
        job.coinb1[offset..offset + 4].copy_from_slice(&[3, 0xc8, 0xb8, 0x0d]);
        assert_eq!(job.block_height(Coin::Ppc), 899272);
        job.coinb1.truncate(offset + 3);
        assert_eq!(job.block_height(Coin::Ppc), 0);
    }
}
