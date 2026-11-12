//! The mining loop with an engine other than the portable kernel, as the
//! ESP32's SHA peripheral plugs in: batch sizes, hash accounting, hardware
//! release when paused, and candidates reaching the share queue.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use bdlminer_app::miner::{run_worker, Engine, Miner, MiningWork, WorkerOptions, BATCH_HW};
use bdlminer_app::platform::{Clock, MemoryStorage, Storage};
use bdlminer_app::std_platform::SystemClock;
use bdlminer_core::coin::Coin;
use bdlminer_core::work::{self, from_hex_array, Job};

/// Records its calls, reports each batch as done and hands nonce 7 of the
/// work to the miner as a candidate, as the hardware's hit callback does.
#[derive(Default)]
struct FakeHardware {
    starts: u32,
    idles: Arc<AtomicU32>,
    batches: Arc<Mutex<Vec<(u32, u32)>>>,
}

impl Engine for FakeHardware {
    fn start(&mut self, _miner: &Miner, _work: &MiningWork) {
        self.starts += 1;
    }
    fn batch(&mut self, miner: &Miner, w: &MiningWork, first: u32, left: u32) -> u32 {
        self.batches.lock().unwrap().push((first, left));
        if let Some(hash) = w.work.mid.scan(7, u32::MAX) {
            miner.on_candidate(7, &hash, w);
        }
        if left != 0 && left < BATCH_HW {
            left
        } else {
            BATCH_HW
        }
    }
    fn idle(&mut self) {
        self.idles.fetch_add(1, Ordering::Relaxed);
    }
}

fn genesis_job() -> Job {
    // The genesis block as a job: one transaction, no branches, so the
    // coinbase hash is the merkle root of the real header.
    let header: [u8; 80] = from_hex_array(concat!(
        "01000000",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "3ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a",
        "29ab5f49",
        "ffff001d",
        "1dac2b7c"
    ))
    .unwrap();
    let mut job =
        Job { id: "g".into(), version: 1, ntime: 0x495fab29, nbits: 0x1d00ffff, difficulty: 1.0, ..Default::default() };
    job.prev_hash.copy_from_slice(&header[4..36]);
    job
}

#[test]
fn hardware_engine_batches_and_accounting() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let storage: Arc<Mutex<dyn Storage>> = Arc::new(Mutex::new(MemoryStorage::default()));
    let miner = Miner::new(Coin::Btc, clock, storage, |_| {});
    let engine = miner.register_engine("hw").unwrap();
    miner.set_power(100, false);
    miner.publish_job(&genesis_job(), true);
    let idles = Arc::new(AtomicU32::new(0));
    let batches = Arc::new(Mutex::new(Vec::new()));
    let worker = {
        let (miner, idles, batches) = (miner.clone(), idles.clone(), batches.clone());
        thread::spawn(move || {
            let mut hw = FakeHardware { idles, batches, ..Default::default() };
            run_worker(&miner, engine, &mut hw, WorkerOptions { owns_core: true, pause: Duration::from_millis(1) });
            hw.starts
        })
    };
    while batches.lock().unwrap().len() < 5 {
        miner.renew_safety();
        thread::sleep(Duration::from_millis(5));
    }
    // Pausing releases the hardware.
    miner.set_power(0, false);
    let before = idles.load(Ordering::Relaxed);
    thread::sleep(Duration::from_millis(400));
    assert!(idles.load(Ordering::Relaxed) > before, "idle() not called while paused");
    miner.shutdown();
    let starts = worker.join().unwrap();
    assert_eq!(starts, 1, "one work for one job");
    let batches = batches.lock().unwrap().clone();
    for (i, (first, left)) in batches.iter().enumerate() {
        assert_eq!(*first, i as u32 * BATCH_HW, "consecutive positions");
        assert_eq!(*left, 0u32.wrapping_sub(*first));
    }
    let stats = miner.stats();
    assert_eq!(stats.hashes[engine], batches.len() as u64 * BATCH_HW as u64, "{} batches", batches.len());
    // The candidate the "hardware" reported reached the miner's statistics:
    // the first work uses extranonce2 0.
    let expected = work::hash_difficulty(&work::build(&genesis_job(), 0).mid.scan(7, u32::MAX).unwrap());
    assert_eq!(stats.session_best_diff, expected);
}
