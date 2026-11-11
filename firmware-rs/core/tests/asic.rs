//! Ported from test/host/test_bm1370.cpp and test_bm1397.cpp.

use bdlminer_core::bm1370::{self, crc5, Health};
use bdlminer_core::bm1397;
use bdlminer_core::sha256::{self, sha256, sha256d};
use bdlminer_core::work::{from_hex, to_hex};

fn value(p: &[u8]) -> u32 {
    u32::from_be_bytes([p[6], p[7], p[8], p[9]])
}

#[test]
fn bm1370_wire_vectors_and_pll() {
    assert_eq!(to_hex(&bm1370::version_mask(0x1fffe000)), "55aa510900a49000ffff1c");
    assert_eq!(to_hex(&bm1370::version_mask(0)), "55aa510900a49000000017");
    assert_eq!(to_hex(&bm1370::reg(0xa8, 0x00070000, true)), "55aa510900a80007000003");
    for (power, mask) in [(0, 0), (3, 0xe0), (4, 0xf0), (8, 0xff), (13, 0xf8ff)] {
        assert_eq!(value(&bm1370::ticket_mask(power)), mask);
    }
    assert!(bm1370::ticket_mask(32).is_empty());
    let (pll, actual) = bm1370::frequency(525.0).unwrap();
    assert!(pll.len() == 11 && (actual - 525.0).abs() < 0.01);
    assert!(
        bm1370::frequency(f32::NAN).is_none() && bm1370::frequency(801.0).is_none() && bm1370::frequency(0.0).is_none()
    );
    let mut f = 50.0f32;
    while f <= 800.0 {
        let (p, actual) = bm1370::frequency(f).unwrap();
        assert!(p.len() == 11 && (actual - f).abs() < 3.0);
        let (fb, refdiv) = (p[7] as f64, p[8] as f64);
        let (p1, p2) = ((p[9] >> 4) as f64 + 1.0, (p[9] & 15) as f64 + 1.0);
        assert!((160.0..=239.0).contains(&fb) && (refdiv == 1.0 || refdiv == 2.0) && p1 <= 7.0 && p1 > p2);
        assert!((25.0 * fb / (refdiv * p1 * p2) - actual as f64).abs() < 0.001);
        f += 6.25;
    }
    assert_eq!(bm1370::frequency(800.0).unwrap().1, 800.0);
}

#[test]
fn bm1370_job_and_receiver() {
    let header: [u8; 80] = std::array::from_fn(|i| i as u8);
    let p = bm1370::job(15, &header);
    assert!(p.len() == 88 && p[4] == 120 && p[5] == 1);
    assert_eq!(p[6..10], header[76..80]);
    assert_eq!(p[10..14], header[72..76]);
    assert_eq!(p[14..18], header[68..72]);
    for w in 0..8 {
        assert_eq!(p[18 + 4 * w..22 + 4 * w], header[36 + 4 * (7 - w)..40 + 4 * (7 - w)]);
        assert_eq!(p[50 + 4 * w..54 + 4 * w], header[4 + 4 * (7 - w)..8 + 4 * (7 - w)]);
    }
    assert_eq!(p[82..86], header[..4]);
    assert_eq!(p[86..88], [0xd6, 0xe1]); // binascii.crc_hqx
    assert!(bm1370::job(16, &header).is_empty());

    let mut rx = bm1370::Receiver::default();
    let mut wire = from_hex("aa559b044c0a0030112288").unwrap();
    for i in 0..32 {
        wire[10] = 0x80 | i;
        if crc5(&wire[2..11]) == 0 {
            break;
        }
    }
    let mut hits = 0;
    let mut last = None;
    let mut feed = |rx: &mut bm1370::Receiver, data: &[u8]| {
        for &b in data {
            if let Some(r) = rx.push(b) {
                hits += 1;
                last = Some(r);
            }
        }
        (hits, last)
    };
    feed(&mut rx, &[0, 0xaa, 1, 0xaa]);
    let (n, d) = feed(&mut rx, &wire);
    assert_eq!(n, 1);
    let d = d.unwrap();
    assert!(d.job && d.slot == 3 && d.nonce == 0x0a4c049b && d.version_bits == 0x02244000);
    for bit in 0..72 {
        let mut bad = wire.clone();
        bad[2 + bit / 8] ^= 1 << (bit % 8);
        assert_eq!(feed(&mut rx, &bad).0, 1);
    }
    assert_eq!(feed(&mut rx, &wire).0, 2);
    let mut chip = from_hex("aa55137000000000000000").unwrap();
    for i in 0..32 {
        chip[10] = i;
        if crc5(&chip[2..11]) == 0 {
            break;
        }
    }
    let (n, d) = feed(&mut rx, &chip);
    assert_eq!(n, 3);
    let d = d.unwrap();
    assert!(!d.job && d.value >> 16 == 0x1370);
}

#[test]
fn bm1370_pmbus_and_interlocks() {
    assert!((bm1370::linear11(bm1370::encode_linear11(4.8)) - 4.8).abs() < 0.01);
    assert!((bm1370::linear11(bm1370::encode_linear11(-12.5)) + 12.5).abs() < 0.01);
    assert_eq!(bm1370::voltage_word(1.15, 0x17), Some(589));
    assert_eq!(bm1370::voltage_word(1.15, 0x97), Some(589));
    assert!(bm1370::voltage_word(1.15, 0x37).is_none() && bm1370::voltage_word(f32::NAN, 0x17).is_none());
    assert!(bm1370::voltage_word(3.0, 0x17).is_none());
    assert_eq!(bm1370::diode_temperature(55, 0x80), 55.5);
    assert!(bm1370::diode_temperature(0x7f, 0).is_nan() && bm1370::diode_temperature(0x7f, 0xe0).is_nan());
    assert!(
        bm1370::fan_rpm(1800) == 3000
            && bm1370::fan_rpm(0) == 0
            && bm1370::fan_rpm(0xffff) == 0
            && bm1370::fan_rpm(1) == 0
    );
    let healthy = Health {
        temperature: 55.0,
        regulator_temperature: 60.0,
        input_voltage: 5.0,
        core_voltage: 1.15,
        current: 15.0,
        rpm: 3000,
        status: 0,
        core_target: 0.0,
    };
    let mut h = healthy;
    assert!(h.fault(true, true).is_none());
    h.temperature = 74.9;
    assert!(h.fault(true, true).is_none());
    h.temperature = 75.0;
    assert!(h.fault(true, true).is_some());
    h = healthy;
    h.temperature = f32::NAN;
    assert!(h.fault(true, true).is_some() && h.fault(false, false).is_none() && h.not_ready().is_none());
    h = healthy;
    h.rpm = 0;
    assert!(h.fault(true, true).is_some() && h.fault(false, false).is_none() && h.not_ready().is_some());
    h = healthy;
    h.input_voltage = 4.7;
    assert!(h.fault(true, true).is_none() && h.not_ready().is_some());
    h.input_voltage = 4.55;
    assert!(h.fault(true, true).is_some());
    h.input_voltage = 5.6;
    assert!(h.fault(true, true).is_some() && h.fault(false, false).is_some());
    for flag in [0x0020, 0x0010, 0x0004] {
        h = healthy;
        h.status = flag;
        assert!(h.fault(true, true).is_some() && h.fault(false, false).is_some());
    }
    for flag in [0x0040, 0x0008] {
        h = healthy;
        h.status = flag;
        assert!(h.fault(true, true).is_some() && h.fault(false, false).is_none());
    }
    for flag in [0x8000, 0x4000, 0x2000, 0x1000, 0x0800, 0x0200, 0x0080, 0x0002, 0x0001] {
        h = healthy;
        h.status = flag;
        assert!(h.fault(true, true).is_none() && h.fault(false, false).is_none());
    }
    h = healthy;
    h.status = 0x4851;
    h.input_voltage = 5.38;
    h.core_voltage = 0.027;
    h.current = -0.3;
    h.temperature = 72.6;
    assert_eq!(h.fault(true, true), Some("Regulator output overcurrent; restart required"));
    h.status = 0x4841;
    assert_eq!(h.fault(true, true), Some("Regulator reports a fault; restart required"));
    assert!(h.fault(false, false).is_none());
    h.status = 0;
    assert_eq!(h.fault(true, true), Some("ASIC voltage fault; restart required"));
    let mut v = healthy;
    for (target, voltage, ok) in [
        (0.0, 1.0, true),
        (0.0, 1.24, true),
        (0.0, 1.26, false),
        (0.0, 0.98, false),
        (1.25, 1.25, true),
        (1.25, 1.34, true),
        (1.25, 1.36, false),
        (1.25, 1.09, false),
        (1.0, 0.9, true),
        (1.0, 0.84, false),
    ] {
        v.core_target = target;
        v.core_voltage = voltage;
        assert_eq!(v.fault(true, true).is_none(), ok, "{target} {voltage}");
    }
    for (value, word) in [(0, 0), (3, 3), (105, 105), (650, 650), (2000, 1 << 11 | 1000)] {
        assert_eq!(bm1370::encode_linear11_int(value), word);
    }
    assert_eq!(bm1370::linear11(bm1370::encode_linear11_int(2000)), 2000.0);
}

#[test]
fn bm1370_profiles_monitors_and_counters() {
    for mhz in (525..=800).step_by(25) {
        assert!(bm1370::valid_profile(mhz));
    }
    assert!(bm1370::valid_profile(690));
    for bad in [524, 825, 801, 691, 601, 526, 790] {
        assert!(!bm1370::valid_profile(bad));
    }
    for mv in [1000, 1060, 1100, 1150, 1200, 1250, 1300] {
        assert!(bm1370::valid_voltage(mv));
    }
    for bad in [999, 1050, 1251, 1301, 1350, 0] {
        assert!(!bm1370::valid_voltage(bad));
    }
    let mut previous = 0;
    for mhz in (525..=800).step_by(25) {
        let mv = bm1370::recommended_mv(mhz);
        assert!(bm1370::valid_voltage(mv as u32) && mv >= bm1370::STOCK_MV && mv >= previous);
        previous = mv;
    }
    assert_eq!(bm1370::MAX_USER_TEMP_C, 73);

    let mut guard = bm1370::ErrorMonitor::default();
    assert!(!guard.update(100, 1000, 0));
    assert!(!guard.update(110, 1300, 1000));
    assert!(!guard.update(200, 4000, 10000) && guard.percent > 3.0);
    assert!(!guard.update(300, 7000, 20000));
    assert!(guard.update(400, 10000, 30000));
    guard.reset();
    assert!(!guard.update(u32::MAX - 20, u32::MAX - 200, 100));
    assert!(!guard.update(9, 2799, 10100) && guard.percent == 1.0);
    assert!(!guard.update(20, 2800, 30101) && guard.percent < 0.0);
    guard.reset();
    guard.update(0, 0, 0);
    guard.update(100, 3000, 10000);
    assert!(!guard.update(100, 6000, 20000) && guard.bad_windows == 0);

    let mut c = bm1370::Counter::default();
    assert_eq!(c.update(u32::MAX - 100, 100), 0);
    assert_eq!(c.update(99, 1100), 200 << 32);
    assert_eq!(c.update(0, 2100), 0);
    assert_eq!(c.update(250, 3100), 250 << 32);
    c.reset();
    assert_eq!(c.update(500, 4100), 0);
    assert_eq!(c.update(1000, 10100), 0);
    assert_eq!(c.update(1233, 11100), 1000727379968);
}

/// What the chip computes for one midstate of a job packet and a nonce.
fn chip_hash(p: &[u8], index: usize, nonce: u32) -> [u8; 32] {
    let data = &p[4..];
    let mut state = [0u32; 8];
    for w in 0..8 {
        let b = &data[18 + 32 * index + 4 * w..];
        state[7 - w] = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    }
    let mut block = [0u8; 64];
    block[..4].copy_from_slice(&data[14..18]);
    block[4..8].copy_from_slice(&data[10..14]);
    block[8..12].copy_from_slice(&data[6..10]);
    block[12..16].copy_from_slice(&nonce.to_le_bytes());
    block[16] = 0x80;
    block[62] = 0x02;
    block[63] = 0x80;
    sha256::transform(&mut state, &block);
    let mut first = [0u8; 32];
    for w in 0..8 {
        first[4 * w..4 * w + 4].copy_from_slice(&state[w].to_be_bytes());
    }
    sha256(&first)
}

#[test]
fn bm1397_vectors_midstates_and_frames() {
    assert_eq!(to_hex(&bm1397::chain_inactive()), "55aa5305000003");
    assert_eq!(to_hex(&bm1397::read_register(0)), "55aa520500000a");
    assert_eq!(to_hex(&bm1397::set_address(0)), "55aa400500001c");
    assert!(value(&bm1397::max_baud()) == 0x00006131 && bm1397::max_baud()[5] == 0x18);
    assert_eq!(value(&bm1397::pll_divider()), 0x0f0f0f00);
    assert!(value(&bm1397::ticket_mask(6)) == 0xfc && value(&bm1397::ticket_mask(0)) == 0);
    assert!(bm1397::ticket_mask(32).is_empty());
    for (mhz, actual, reg) in [
        (300.0, 300.0, 0x40780215),
        (210.0, 210.0, 0x40a80225),
        (50.0, 50.0, 0x403c0235),
        (600.0, 600.0, 0x40780115),
        (650.0, 650.0, 0x40820115),
        (20.0, 50.0, 0x403c0235),
        (900.0, 650.0, 0x40820115),
    ] {
        let (p, got) = bm1397::frequency(mhz).unwrap();
        assert!(p.len() == 11 && p[5] == 0x08 && value(&p) == reg && (got - actual).abs() < 0.01);
    }
    assert!(bm1397::frequency(f32::NAN).is_none());
    assert_eq!(bm1397::increment_mask(0x20000000, bm1397::VERSION_MASK), 0x20002000);
    assert_eq!(bm1397::increment_mask(0x20002000, bm1397::VERSION_MASK), 0x20004000);
    assert_eq!(bm1397::increment_mask(0x20002000, 0x0000a000), 0x20008000);
    assert_eq!(bm1397::increment_mask(0x20000000, 0), 0x20000000);

    let mut header: [u8; 80] = std::array::from_fn(|i| (i * 7 + 3) as u8);
    header[..4].copy_from_slice(&0x20000000u32.to_le_bytes());
    let (p, versions) = bm1397::job(10, &header, bm1397::VERSION_MASK);
    assert!(versions.len() == 4 && p.len() == 4 + 18 + 128 + 2 && p[2] == 0x21 && p[3] == 18 + 128 + 4);
    assert!(p[4] == 40 && p[5] == 4);
    assert_eq!(versions, [0x20000000, 0x20002000, 0x20004000, 0x20006000]);
    for (i, version) in versions.iter().enumerate() {
        for nonce in [0u32, 1, 0x12345678, 0xffffffff] {
            let mut full = header;
            full[..4].copy_from_slice(&version.to_le_bytes());
            full[76..].copy_from_slice(&nonce.to_le_bytes());
            assert_eq!(sha256d(&full), chip_hash(&p, i, nonce));
        }
    }
    let (single, versions) = bm1397::job(0, &header, 0);
    assert!(versions == [0x20000000] && single.len() == 4 + 18 + 32 + 2 && single[5] == 1);
    assert!(bm1397::job(32, &header, bm1397::VERSION_MASK).0.is_empty());

    let mut rx = bm1397::Receiver::default();
    let id = [0xaa, 0x55, 0x13, 0x97, 0x18, 0x00, 0x00, 0x00, 0x06];
    let nonce = [0xaa, 0x55, 0x78, 0x56, 0x34, 0x12, 0x01, 41, 0x99];
    let stream: Vec<u8> = [&[0x00, 0x13, 0xaa][..], &id, &nonce].concat();
    let got: Vec<_> = stream.iter().filter_map(|&b| rx.push(b)).collect();
    assert!(got.len() == 2 && got[0].chip_id && !got[1].chip_id);
    assert!(got[1].nonce == 0x12345678 && got[1].slot == 10 && got[1].midstate == 1);
    rx.clear();
    for b in [0xaa, 0x55, 0x01, 0x02] {
        rx.push(b);
    }
    rx.clear();
    let got: Vec<_> = nonce.iter().filter_map(|&b| rx.push(b)).collect();
    assert!(got.len() == 1 && got[0].slot == 10);
    rx.clear();
    for id in 0..=255u8 {
        let frame = [0xaa, 0x55, 0x78, 0x56, 0x34, 0x12, 0x01, id, 0x99];
        let r = frame.iter().map(|&b| rx.push(b)).last().unwrap();
        assert_eq!(r.is_some(), id < 128);
        if let Some(r) = r {
            assert!(r.slot == id / 4 && (r.slot as usize) < bm1397::SLOTS && r.midstate == id % 4);
        }
    }
    let got: Vec<_> = nonce.iter().filter_map(|&b| rx.push(b)).collect();
    assert!(got.len() == 1 && got[0].slot == 10);
}
