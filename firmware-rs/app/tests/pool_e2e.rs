//! The firmware's pool task and software miner against local test pools:
//! Stratum V1 and Stratum V2 (Noise), every submitted share re-verified.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use bdlminer_app::miner::{software_worker, Miner};
use bdlminer_app::platform::{Clock, MemoryStorage, Storage};
use bdlminer_app::pool::{self, Endpoint, PoolConfig, PoolState};
use bdlminer_app::std_platform::{os_random, SystemClock, TcpNetwork};
use bdlminer_core::coin::Coin;
use bdlminer_core::sha256::sha256d;
use bdlminer_core::sv2_noise::responder;
use bdlminer_core::work::{self, from_hex_array, to_hex};
use serde_json::{json, Value};

const ADDRESS: &str = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
const PREV: &str = "ab02cd818b9e567ee21793cddef299feb29ad444a41b85b8000008a300000000";

fn config(port: u16, protocol: u8, authority: &str) -> PoolConfig {
    PoolConfig {
        coin: Coin::Btc,
        main: Endpoint { host: "127.0.0.1".into(), port, protocol, authority: authority.into() },
        backup: None,
        user: format!("{ADDRESS}.rig"),
        password: "x".into(),
        suggested_difficulty: 0.0001,
        user_agent: "BDLMinerV2/test".into(),
        native_agent: "BDLMinerV2/test".into(),
        rotate_hours: 0,
        asic: false,
        hardware: "ESP32".into(),
        firmware: "test".into(),
    }
}

/// Runs the miner and the pool task until `accepted` shares were accepted.
fn mine(cfg: PoolConfig, accepted_wanted: u32) -> Arc<Miner> {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let storage: Arc<Mutex<dyn Storage>> = Arc::new(Mutex::new(MemoryStorage::default()));
    let miner = Miner::new(Coin::Btc, clock.clone(), storage, |l| eprintln!("{l}"));
    miner.load_stats();
    let engine = miner.register_engine("sw").unwrap();
    let state = PoolState::new(|l| eprintln!("{l}"));
    let worker = {
        let miner = miner.clone();
        thread::spawn(move || software_worker(&miner, engine, false))
    };
    let net = {
        let (miner, state, clock) = (miner.clone(), state.clone(), clock.clone());
        thread::spawn(move || {
            let net = TcpNetwork { poll: Duration::from_millis(20), random: os_random };
            pool::run(&net, &cfg, &miner, &state, &*clock);
        })
    };
    let start = Instant::now();
    while miner.stats().accepted < accepted_wanted {
        assert!(start.elapsed() < Duration::from_secs(60), "no accepted shares: {}", state.status());
        miner.set_power(100, false);
        miner.tick();
        thread::sleep(Duration::from_millis(200));
    }
    assert!(state.status().starts_with("Mining"), "{}", state.status());
    miner.shutdown();
    worker.join().unwrap();
    net.join().unwrap();
    miner
}

#[test]
fn stratum_v1_pool() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let verified = Arc::new(AtomicU32::new(0));
    let server_verified = verified.clone();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut out = stream.try_clone().unwrap();
        let mut send = |v: Value| {
            let _ = out.write_all(format!("{v}\n").as_bytes());
        };
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { return };
            let msg: Value = serde_json::from_str(&line).unwrap();
            let id = msg["id"].clone();
            match msg["method"].as_str().unwrap() {
                "mining.subscribe" => {
                    assert_eq!(msg["params"][0], "BDLMinerV2/test");
                    send(json!({"id": id, "result": [[], "aabb", 4], "error": null}));
                }
                "mining.suggest_difficulty" => send(json!({"id": id, "result": true, "error": null})),
                "mining.authorize" => {
                    assert_eq!(msg["params"][0], format!("{ADDRESS}.rig"));
                    send(json!({"id": id, "result": true, "error": null}));
                    send(json!({"id": null, "method": "mining.set_difficulty", "params": [0.0001]}));
                    send(json!({"id": null, "method": "mining.notify",
                        "params": ["j1", PREV, "01000000", "00000000", [], "20000000", "1d00ffff", "65000000", true]}));
                }
                "mining.submit" => {
                    let p = &msg["params"];
                    assert_eq!(p[1], "j1");
                    let mut job = work::Job {
                        id: "j1".into(),
                        coinb1: vec![1, 0, 0, 0],
                        coinb2: vec![0; 4],
                        version: 0x20000000,
                        nbits: 0x1d00ffff,
                        ntime: u32::from_str_radix(p[3].as_str().unwrap(), 16).unwrap(),
                        extranonce1: vec![0xaa, 0xbb],
                        extranonce2_size: 4,
                        ..Default::default()
                    };
                    let prev: [u8; 32] = from_hex_array(PREV).unwrap();
                    for w in 0..8 {
                        for b in 0..4 {
                            job.prev_hash[4 * w + b] = prev[4 * w + 3 - b];
                        }
                    }
                    let en2 = u64::from_str_radix(p[2].as_str().unwrap(), 16).unwrap();
                    let mut w = work::build(&job, en2);
                    assert_eq!(w.extranonce2_hex, p[2]);
                    let nonce = u32::from_str_radix(p[4].as_str().unwrap(), 16).unwrap();
                    w.header[76..].copy_from_slice(&nonce.to_le_bytes());
                    let ok = work::hash_difficulty(&sha256d(&w.header)) >= 0.0001;
                    if ok {
                        server_verified.fetch_add(1, Ordering::Relaxed);
                    }
                    send(
                        json!({"id": id, "result": ok, "error": if ok { Value::Null } else { json!([23, "Low difficulty share", null]) }}),
                    );
                }
                other => panic!("unexpected {other}"),
            }
        }
    });
    let miner = mine(config(port, 0, ""), 3);
    let stats = miner.stats();
    assert!(stats.accepted >= 3 && stats.rejected == 0, "{stats:?}");
    assert!(verified.load(Ordering::Relaxed) >= 3);
    assert!(stats.accepted_work > 0.0 && stats.pool_diff == 0.0001);
}

struct Sv2Server {
    stream: TcpStream,
    pool: responder::Responder,
}

impl Sv2Server {
    fn read_frame(&mut self) -> (u8, Vec<u8>) {
        let mut header = [0u8; 22];
        self.stream.read_exact(&mut header).unwrap();
        let h = self.pool.decrypt(&header).expect("header record");
        let length = h[3] as usize | (h[4] as usize) << 8 | (h[5] as usize) << 16;
        let mut payload = vec![0u8; length + 16];
        self.stream.read_exact(&mut payload).unwrap();
        (h[2], self.pool.decrypt(&payload).expect("payload record"))
    }
    fn send(&mut self, extension: u16, kind: u8, payload: &[u8]) {
        let n = payload.len();
        let header = [extension as u8, (extension >> 8) as u8, kind, n as u8, (n >> 8) as u8, 0];
        let h = self.pool.encrypt(&header);
        let p = self.pool.encrypt(payload);
        self.stream.write_all(&h).unwrap();
        self.stream.write_all(&p).unwrap();
    }
}

fn le(v: &mut Vec<u8>, x: u32, n: usize) {
    v.extend_from_slice(&x.to_le_bytes()[..n]);
}

#[test]
fn stratum_v2_noise_pool() {
    use secp256k1::{Keypair, SecretKey};
    let authority = Keypair::from_secret_key(&SecretKey::from_secret_bytes([5; 32]).unwrap());
    let key = to_hex(&authority.x_only_public_key().0.to_byte_array());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let verified = Arc::new(AtomicU32::new(0));
    let server_verified = verified.clone();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut hello = [0u8; 64];
        stream.read_exact(&mut hello).unwrap();
        let server = SecretKey::from_secret_bytes([6; 32]).unwrap();
        let (response, pool) =
            responder::respond(&hello, server, &authority, 1_700_000_000, 2_100_000_000, &mut os_random);
        stream.write_all(&response).unwrap();
        let mut s = Sv2Server { stream, pool };
        let (kind, setup) = s.read_frame();
        assert!(kind == 0 && setup[0] == 0 && setup[5] == 0, "SetupConnection");
        let mut ok = vec![];
        le(&mut ok, 2, 2);
        le(&mut ok, 0, 4);
        s.send(0, 0x01, &ok);
        let (kind, open) = s.read_frame();
        assert_eq!(kind, 0x13);
        assert_eq!(open[4] as usize, ADDRESS.len() + 4);
        // A target with the top 32 bits at 10000: a share every ~430k hashes.
        let mut target = [0xffu8; 32];
        target[28..].copy_from_slice(&10000u32.to_le_bytes());
        let mut channel = vec![];
        le(&mut channel, 1, 4);
        le(&mut channel, 7, 4);
        channel.extend_from_slice(&target);
        le(&mut channel, 8, 2);
        channel.extend_from_slice(&[2, 0xaa, 0xbb]);
        le(&mut channel, 11, 4);
        s.send(0, 0x14, &channel);
        let mut job = vec![];
        le(&mut job, 7, 4);
        le(&mut job, 1, 4);
        job.push(0); // future job
        le(&mut job, 0x20000000, 4);
        job.extend_from_slice(&[0, 0]); // no rolling, no branches
        le(&mut job, 4, 2);
        job.extend_from_slice(&[1, 0, 0, 0]);
        le(&mut job, 4, 2);
        job.extend_from_slice(&[0; 4]);
        s.send(0x8000, 0x1f, &job);
        let prev = [0x22u8; 32];
        let ntime = 1_800_000_000u32;
        let mut prevhash = vec![];
        le(&mut prevhash, 7, 4);
        le(&mut prevhash, 1, 4);
        prevhash.extend_from_slice(&prev);
        le(&mut prevhash, ntime, 4);
        le(&mut prevhash, 0x1d00ffff, 4);
        s.send(0x8000, 0x20, &prevhash);
        loop {
            let (kind, submit) = s.read_frame();
            assert_eq!(kind, 0x1b);
            let word = |i: usize| u32::from_le_bytes(submit[4 * i..4 * i + 4].try_into().unwrap());
            let (sequence, job_id, nonce, time, version) = (word(1), word(2), word(3), word(4), word(5));
            assert!(word(0) == 7 && job_id == 1 && time == ntime && version == 0x20000000);
            assert_eq!(submit[24], 8);
            let extranonce = &submit[25..33];
            let coinbase = [&[1u8, 0, 0, 0][..], &[0xaa, 0xbb], extranonce, &[0; 4]].concat();
            let mut header = [0u8; 80];
            header[..4].copy_from_slice(&version.to_le_bytes());
            header[4..36].copy_from_slice(&prev);
            header[36..68].copy_from_slice(&sha256d(&coinbase));
            header[68..72].copy_from_slice(&ntime.to_le_bytes());
            header[72..76].copy_from_slice(&0x1d00ffffu32.to_le_bytes());
            header[76..].copy_from_slice(&nonce.to_le_bytes());
            assert!(work::meets_target(&sha256d(&header), &target), "share above target");
            server_verified.fetch_add(1, Ordering::Relaxed);
            let mut ack = vec![];
            for x in [7, sequence, 1, 0, 0] {
                le(&mut ack, x, 4);
            }
            s.send(0x8000, 0x1c, &ack);
        }
    });
    let miner = mine(config(port, 1, &key), 3);
    assert!(miner.stats().accepted >= 3 && miner.stats().rejected == 0);
    assert!(verified.load(Ordering::Relaxed) >= 3);
}
