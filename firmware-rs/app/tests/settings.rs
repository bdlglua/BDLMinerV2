//! Settings: defaults, sanitizing, web validation, live changes and the
//! two-record store (the C++ firmware's format).

use bdlminer_app::platform::{BoardKind, BoardProfile, MemoryStorage, Storage};
use bdlminer_app::settings::{self, Settings, NAMESPACE, SECRET_MASK};
use serde_json::{json, Map, Value};

fn esp32() -> BoardProfile {
    BoardProfile {
        name: "Test",
        env: "test",
        kind: BoardKind::Esp32,
        m5: false,
        display_pages: 8,
        temperature_sensor: true,
    }
}
fn gamma() -> BoardProfile {
    BoardProfile {
        name: "Gamma",
        env: "gamma601",
        kind: BoardKind::Gamma601,
        m5: false,
        display_pages: 3,
        temperature_sensor: true,
    }
}
fn obj(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}
fn configured(board: &BoardProfile) -> Settings {
    let mut s = Settings::defaults(board);
    s.btc_address = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4".into();
    s
}

#[test]
fn defaults_follow_the_board() {
    let s = Settings::defaults(&esp32());
    assert!(s.pool_port == 21496 && s.suggested_difficulty == 0.0002 && s.max_temp == 75 && s.github_update == 2);
    let g = Settings::defaults(&gamma());
    assert!(g.pool_port == 3333 && g.suggested_difficulty == 1024.0 && g.max_temp == 65 && g.github_update == 1);
    assert!(settings::table(&esp32()).all(|i| i.key != "asicmhz"));
    assert!(settings::table(&gamma()).any(|i| i.key == "asicmhz"));
}

#[test]
fn sanitize_cleans_pasted_urls_and_ranges() {
    let board = esp32();
    let mut s = configured(&board);
    s.pool_host = "  stratum+tcp://solo.ckpool.org:3333/ ".into();
    s.backup_host = "stratum2+tcp://pool.example.com:3336/9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".into();
    s.ntfy_server = "https://ntfy.example.org/".into();
    s.currency = " eur ".into();
    s.brightness = 200;
    s.rotation = 7;
    s.max_temp = 10;
    s.screen_widgets = [0, 9, 4, 3];
    s.screen_pages = 0;
    s.summary_hour = 30;
    s.suggested_difficulty = f64::NAN;
    s.user_agent = "something else".into();
    settings::sanitize(&mut s, &board);
    assert_eq!((s.pool_host.as_str(), s.pool_port), ("solo.ckpool.org", 3333));
    assert_eq!((s.backup_host.as_str(), s.backup_port, s.backup_protocol), ("pool.example.com", 3336, 1));
    assert_eq!(s.backup_authority, "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72");
    assert_eq!(s.ntfy_server, "ntfy.example.org");
    assert_eq!(s.currency, "EUR");
    assert!(s.brightness == 100 && s.rotation == 3 && s.max_temp == 30);
    assert_eq!(s.screen_widgets, [1, 2, 4, 3]);
    assert!(s.screen_pages == 1 && s.summary_hour == 20 && s.suggested_difficulty == 0.0002);
    assert!(s.user_agent.starts_with("BDLMinerV2/"));
    let mut g = configured(&gamma());
    g.max_temp = 80;
    g.asic_frequency = 601;
    settings::sanitize(&mut g, &gamma());
    assert!(g.max_temp == 73 && g.asic_frequency == 525);
}

#[test]
fn web_validation_rejects_before_staging() {
    let board = esp32();
    let current = configured(&board);
    let err = |v: Value| settings::error(&obj(v), &current, &board);
    assert_eq!(err(json!({"bright": 50})), None);
    assert_eq!(err(json!({"bright": 101})), Some("Setting outside its allowed range"));
    assert_eq!(err(json!({"bright": "50"})), Some("Setting outside its allowed range"));
    assert_eq!(err(json!({"bright": null})), Some("Setting outside its allowed range"));
    assert_eq!(err(json!({"pool": " "})), Some("Pool host and payout address cannot be empty"));
    assert_eq!(err(json!({"pages": 2})), Some("The mining screen must remain enabled"));
    assert_eq!(err(json!({"maxtemp": 20})), Some("Temperature must be 0 or at least 30 C"));
    assert_eq!(err(json!({"nightfrom": 24})), Some("Night hours must be 0-23 or 255"));
    assert_eq!(err(json!({"diff": 0})), Some("Difficulty must be a finite positive number"));
    assert_eq!(err(json!({"beep": 1})), Some("Invalid checkbox setting"));
    assert_eq!(err(json!({"hook": "ftp://x"})), Some("The webhook URL must start with http:// or https://"));
    assert_eq!(err(json!({"ua": "curl"})), Some("Choose one of the available user agents"));
    assert_eq!(err(json!({"ua": "NerdMinerV2"})), None);
    assert_eq!(err(json!({"coin": 1})), Some("Use an address for the selected cryptocurrency"));
    assert_eq!(
        err(json!({"coin": 1, "addr": "bitcoincash:qpm2qsznhks23z7629mms6s4cwef74vcwvy22gdx6a"})),
        Some("Choose a pool for the selected cryptocurrency")
    );
    assert_eq!(
        err(
            json!({"coin": 1, "addr": "bitcoincash:qpm2qsznhks23z7629mms6s4cwef74vcwvy22gdx6a", "pool": "bch.solopool.org"})
        ),
        None
    );
    assert_eq!(err(json!({"proto": 1})), Some("A valid Stratum V2 authority key is required"));
    assert_eq!(err(json!({"proto": 1, "sv2key": "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72"})), None);
    assert_eq!(
        err(json!({"remote": true, "addr": "x"})),
        Some("Set a payout address before enabling remote monitoring")
    );
    assert_eq!(
        err(json!({"remotekey": "ABC"})),
        Some("The remote access key must contain 64 lowercase hexadecimal characters")
    );
    let g = configured(&gamma());
    assert_eq!(settings::error(&obj(json!({"maxtemp": 74})), &g, &gamma()), Some("Setting outside its allowed range"));
    assert_eq!(
        settings::error(&obj(json!({"asicmhz": 610})), &g, &gamma()),
        Some("Gamma frequency must be 525-800 MHz in 25 MHz steps, or 690 MHz")
    );
}

#[test]
fn apply_compare_and_live_changes() {
    let board = esp32();
    let current = configured(&board);
    let mut pending = current.clone();
    let mut values = obj(json!({"bright": 40, "power": 50, "wpass2": SECRET_MASK, "unknown": 1}));
    settings::keep_secrets(&mut values, &board);
    assert!(!values.contains_key("wpass2"));
    assert_eq!(settings::apply(&mut pending, &values, &board), 2);
    let change = settings::compare(&pending, &current, &board);
    assert!(change.count == 2 && change.live);
    let mut live = current.clone();
    settings::copy_live(&mut live, &pending, &board);
    assert!(live.brightness == 40 && live.mining_power == 50);
    settings::apply(&mut pending, &obj(json!({"pool": "solo.ckpool.org"})), &board);
    assert!(!settings::compare(&pending, &current, &board).live);

    let mut with_secret = current.clone();
    with_secret.web_password = "hunter2".into();
    let shown = settings::values(&with_secret, &board, false);
    assert_eq!(shown["webpass"], SECRET_MASK);
    assert_eq!(shown["wpass2"], "");
    assert_eq!(settings::values(&with_secret, &board, true)["webpass"], "hunter2");

    let mut old = obj(json!({"pool": "p", "addr": "a", "ua": "BDLMinerV1/1.0", "uaauto": true}));
    settings::migrate_backup_coin(&mut old, false);
    settings::migrate_backup_agent(&mut old);
    assert!(old["coin"] == 0 && old["uaauto"] == false && old["ua"].as_str().unwrap().starts_with("BDLMinerV2/"));
}

#[test]
fn two_record_store() {
    let board = esp32();
    let mut store = MemoryStorage::default();
    let mut s = configured(&board);
    let mut loaded = Settings::defaults(&board);
    assert_eq!(settings::load(&mut store, &mut loaded, &board), None);
    let seq = settings::save(&mut store, &s, 0, &board).unwrap();
    assert_eq!(seq, 1);
    assert!(store.get(NAMESPACE, "cfgB").is_some() && store.get(NAMESPACE, "cfgA").is_none());
    s.brightness = 33;
    let seq = settings::save(&mut store, &s, seq, &board).unwrap();
    assert_eq!(seq, 2);
    assert_eq!(settings::load(&mut store, &mut loaded, &board), Some(2));
    assert_eq!(loaded, s);
    // A torn write of the newest record falls back to the previous one.
    let mut torn = store.get(NAMESPACE, "cfgA").unwrap();
    let n = torn.len();
    torn[n - 3] ^= 0x20;
    store.set(NAMESPACE, "cfgA", &torn);
    let mut back = Settings::defaults(&board);
    assert_eq!(settings::load(&mut store, &mut back, &board), Some(1));
    assert_eq!(back.brightness, 80);
    // Other chains use the magic older firmware refuses.
    s.mining_coin = 3;
    s.btc_address = "ecash:qpm2qsznhks23z7629mms6s4cwef74vcwva87rkuu2".into();
    settings::save(&mut store, &s, 2, &board).unwrap();
    let record = store.get(NAMESPACE, "cfgB").unwrap();
    assert_eq!(u32::from_le_bytes(record[..4].try_into().unwrap()), settings::NEW_COIN_MAGIC);
    // The header and JSON layout of the C++ firmware.
    let payload = &record[16..];
    assert_eq!(u32::from_le_bytes(record[8..12].try_into().unwrap()) as usize, payload.len());
    let doc: Value = serde_json::from_slice(payload).unwrap();
    assert!(
        doc["coin"] == 3
            && doc["pool"] == "public-pool.io"
            && doc.as_object().unwrap().len() == settings::table(&board).count()
    );
}
