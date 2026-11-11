//! Ported from test/host/test_mining_safety.cpp and test_fan_control.cpp,
//! plus the small portable helpers.

use bdlminer_core::safety::{self, FanControl, SafetyLease, TemperatureSource, ThermalPolicy, Thermometer};
use bdlminer_core::{build_time, display_config, image_marker, pool_diagnostics, release_version, remote_data};

#[test]
fn thermometer_keeps_one_sensor() {
    let mut t = Thermometer::default();
    assert_eq!(t.sample(70.0, 40.0), 70.0);
    assert_eq!(t.source, TemperatureSource::Imu);
    assert!(t.sample(f32::NAN, 40.0).is_nan());
    assert_eq!(t.sample(55.0, 40.0), 55.0);
    let mut chip = Thermometer::default();
    assert!(chip.sample(f32::NAN, f32::NAN).is_nan());
    assert_eq!(chip.sample(f32::NAN, 40.0), 40.0);
    assert_eq!(chip.sample(80.0, 42.0), 42.0);
    assert!(chip.sample(80.0, f32::NAN).is_nan());
}

#[test]
fn thermal_policy_steps_and_holds() {
    let mut t = ThermalPolicy::default();
    assert_eq!(t.power(45.0, 65, 75, false, 0), 75);
    assert_eq!(t.power(45.0, 65, 75, true, 0), 50);
    assert_eq!(t.power(55.0, 65, 75, false, 0), 50);
    assert_eq!(t.power(60.0, 65, 75, false, 0), 25);
    assert_eq!(t.power(65.0, 65, 75, false, 0), 0);
    assert!(t.hot);
    assert_eq!(t.power(f32::NAN, 65, 75, false, 0), 0);
    assert!(t.hot && t.sensor_fault);
    assert_eq!(t.power(61.0, 65, 75, false, 0), 0);
    assert_eq!(t.power(60.0, 65, 75, false, 0), 25);
    assert!(!t.hot && !t.sensor_fault);
    assert_eq!(t.power(f32::INFINITY, 65, 75, false, 0), 0);
    assert_eq!(t.power(45.0, 65, 75, false, 0), 75);
    assert_eq!(t.power(f32::NAN, 0, 75, false, 0), 75);

    let mut asic = ThermalPolicy { steps: false, hold_ms: 120_000, ..Default::default() };
    assert_eq!(asic.power(55.0, 65, 100, false, 1000), 100);
    assert_eq!(asic.power(64.0, 65, 100, false, 2000), 100);
    assert!(asic.power(65.0, 65, 100, false, 3000) == 0 && asic.hot);
    assert_eq!(asic.power(40.0, 65, 100, false, 63000), 0);
    assert!(asic.power(40.0, 65, 100, false, 123000) == 100 && !asic.hot);
    assert_eq!(asic.power(66.0, 65, 100, false, u32::MAX - 1000), 0);
    assert_eq!(asic.power(40.0, 65, 100, false, 50000), 0);
    assert_eq!(asic.power(40.0, 65, 100, false, 120000), 100);

    let mut missing = ThermalPolicy::default();
    assert_eq!(missing.power(f32::NAN, 65, 75, false, 0), 25);
    assert!(missing.sensor_fault && !missing.sensor_seen);
    assert_eq!(missing.power(40.0, 65, 75, false, 0), 75);
    assert_eq!(missing.power(f32::NAN, 65, 75, false, 0), 0);
    assert_eq!(missing.power(45.0, 65, 10, false, 0), 10);
    assert_eq!(missing.power(45.0, 65, 0, false, 0), 0);
}

#[test]
fn safety_lease_expires() {
    let t = safety::SAFETY_TIMEOUT_MS;
    let lease = SafetyLease::default();
    assert_eq!(lease.power(1000), 0);
    lease.renew(500);
    assert_eq!(lease.power(500), 0);
    lease.update(1000, 75);
    assert_eq!(lease.power(1000 + t), 75);
    assert_eq!(lease.power(1001 + t), 0);
    lease.update(20000, 80);
    lease.renew(20980);
    assert_eq!(lease.power(20980 + 2500), 80);
    lease.renew(30000);
    assert_eq!(lease.power(30000 + t), 80);
    lease.update(40000, 0);
    assert_eq!(lease.power(40000), 0);
    lease.renew(40500);
    assert_eq!(lease.power(40500), 0);
    lease.update(u32::MAX - 1000, 100);
    assert_eq!(lease.power(500), 100);
    assert_eq!(lease.power(t - 500), 0);
    assert_eq!(safety::rest_ms(100, 75), 34);
    assert_eq!(safety::rest_ms(100, 50), 100);
    assert_eq!(safety::rest_ms(100, 25), 300);
}

#[test]
fn gamma_fan_policy() {
    let mut fan = FanControl::default();
    let d = fan.update(false, false, 30, 40.0, 50.0, 1000, 65);
    assert!(d.percent == 100 && d.forced);
    let d = fan.update(true, false, 30, 50.0, 50.0, 1001, 65);
    assert!(d.percent == 30 && !d.forced && d.reason == "Manual");
    assert_eq!(fan.update(true, false, 30, 60.0, 50.0, 1002, 65).percent, 100);
    assert!(fan.update(true, false, 30, 58.0, 50.0, 1003, 65).forced);
    assert_eq!(fan.update(true, false, 30, 55.0, 75.0, 1004, 65).percent, 30);
    assert!(fan.update(true, false, 30, 50.0, 80.0, 1005, 65).forced);
    assert!(fan.update(true, false, 30, 50.0, 76.0, 1006, 65).forced);
    assert!(!fan.update(true, false, 30, 50.0, 75.0, 1007, 65).forced);
    for bad in [f32::NAN, f32::INFINITY, -1.0] {
        assert_eq!(fan.update(true, false, 30, bad, 50.0, 1008, 65).percent, 100);
    }
    assert!(fan.update(true, false, 30, 50.0, f32::NAN, 1009, 65).forced);
    for percent in 0..256u32 {
        let bounded = percent.clamp(30, 100);
        assert_eq!(fan.update(true, false, percent, 50.0, 50.0, 2000, 65).percent as u32, bounded);
        let pwm = safety::fan_pwm(percent);
        assert!((19..=63).contains(&pwm) && pwm as u32 * 100 >= bounded * 63);
        assert!((30..=100).contains(&safety::fan_pwm_percent(pwm)));
    }
    let mut fan = FanControl::default();
    fan.update(false, true, 100, 40.0, 50.0, 1000, 65);
    assert_eq!(fan.update(true, true, 100, 40.0, 50.0, 5999, 65).percent, 100);
    assert_eq!(fan.update(true, true, 100, 40.0, 50.0, 6000, 65).percent, 30);
    assert_eq!(fan.update(true, true, 100, 50.0, 50.0, 6001, 65).percent, 65);
    assert_eq!(fan.update(true, true, 100, 45.0, 50.0, 6002, 65).percent, 65);
    assert_eq!(fan.update(true, true, 100, 45.0, 50.0, 11001, 65).percent, 48);
    assert_eq!(fan.update(true, true, 100, 40.0, 75.0, 11002, 65).percent, 83);
    fan.update(false, true, 100, 40.0, 50.0, u32::MAX - 1000, 65);
    assert_eq!(fan.update(true, true, 100, 40.0, 50.0, 4000, 65).percent, 30);
    let mut custom = FanControl::default();
    assert!(custom.update(true, false, 30, 50.0, 50.0, 1000, 55).forced);
    assert!(!custom.update(true, false, 30, 45.0, 50.0, 1001, 55).forced);
    assert_eq!(custom.update(true, false, 30, 50.0, 50.0, 1002, 0).percent, 30);
    assert!(custom.update(true, false, 30, 60.0, 50.0, 1003, 0).forced);
}

#[test]
fn versions_markers_and_dates() {
    assert_eq!(build_time::epoch("Nov  9 2026"), 1794182400);
    assert_eq!(build_time::epoch("Jan  1 1970"), 0); // not after the epoch
    assert_eq!(build_time::epoch("Foo  1 2026"), 0);
    assert_eq!(build_time::oldest_plausible("Nov  9 2026", 1_700_000_000), 1794182400 - 2 * 86400);
    assert_eq!(build_time::oldest_plausible("garbage", 1_700_000_000), 1_700_000_000);

    assert_eq!(release_version::compare("2.6.1", "2.6.0"), 1);
    assert_eq!(release_version::compare("v2.6.1", "2.6.1"), 0);
    assert_eq!(release_version::compare("2.7.0-beta.2", "2.7.0"), -1);
    assert_eq!(release_version::compare("2.7.0-beta.2", "2.7.0-beta.10"), -1);
    assert_eq!(release_version::compare("2.7.0-beta", "2.7.0-beta.1"), -1);
    assert_eq!(release_version::compare("2.7.0-1", "2.7.0-alpha"), -1);
    assert_eq!(release_version::compare("1.9", "1.10.0"), -1);
    assert!(release_version::parse("01.2.3").is_none() && release_version::parse("1.2.3.4").is_none());
    assert!(release_version::parse("1.2.3-01").is_none() && release_version::parse("1.2.3+01").is_some());
    assert_eq!(release_version::find("BDLMiner V2 2.6.1 (stable)").as_deref(), Some("2.6.1"));

    let mut scanner = image_marker::Scanner::default();
    assert!(scanner.set("BDLMinerV1/", "2.6.1"));
    scanner.feed(b"xxBDLMinerV1/2.6.");
    assert!(!scanner.found());
    scanner.feed(b"1\0yy");
    assert!(scanner.found());
    let mut overlap = image_marker::Scanner::default();
    overlap.set("aab", "a");
    overlap.feed(b"aaaba\0");
    assert!(overlap.found());
    assert!(!image_marker::Scanner::default().set("x", ""));
}

#[test]
fn diagnostics_remote_and_display() {
    let input = pool_diagnostics::Input {
        connected: true,
        work: true,
        mask: 0x1fffe000,
        required_bits: 8,
        difficulty: 1.0,
        hashrate: 1e6,
        status: "Mining",
        ..Default::default()
    };
    let r = pool_diagnostics::diagnose(&input);
    assert_eq!(r.issue.code(), "infrequent_shares");
    assert_eq!(r.rolling_bits, 16);
    let r = pool_diagnostics::diagnose(&pool_diagnostics::Input { mask: 0x3, ..input.clone() });
    assert_eq!(r.issue.code(), "version_rolling_required");
    let r = pool_diagnostics::diagnose(&pool_diagnostics::Input { hashrate: 1e12, ..input.clone() });
    assert!(r.compatible && r.issue.code() == "ready");

    assert!(remote_data::valid_key(&"a".repeat(64)) && !remote_data::valid_key(&"A".repeat(64)));
    assert_eq!(remote_data::payout_address(" bc1qxyz.rig").as_deref(), Some("bc1qxyz"));
    assert!(remote_data::payout_address("ab").is_none() && remote_data::payout_address("bad/addr").is_none());
    assert_eq!(remote_data::device_id(&"0".repeat(64), 1).len(), 16);
    assert_eq!(remote_data::address_device_id("BC1Q", 1), remote_data::address_device_id("bc1q", 1));

    assert_eq!(display_config::next_page(0, 8, 0b1010), 1);
    assert_eq!(display_config::next_page(3, 8, 0b1010), 0);
    let s = display_config::Snapshot { hashrate: 351_000.0, accepted: 3, uptime: 3720, ..Default::default() };
    assert_eq!(display_config::value(1, &s, false), "351.00 kH/s");
    assert_eq!(display_config::value(5, &s, false), "1h 02m");
    assert_eq!(display_config::value(6, &s, true), "HORS LIGNE");
    assert_eq!(display_config::value(3, &s, false), "--");
    assert_eq!(display_config::label(4, true), "RECORD");
}
