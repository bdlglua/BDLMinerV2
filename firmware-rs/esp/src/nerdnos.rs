//! NerdNOS (feature `nerdnos`): one BM1397 on a board carrying a
//! T-Display-S3, ported from the C++ firmware's src/nerdnos.cpp.
//!
//! UART1 to the ASIC through a level shifter, the core buck converter's
//! enable, the ASIC reset (high holds it in reset on this board) and a
//! 10 kOhm NTC (beta 3380) on ADC1, as the NerdNOS firmware drives them.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use bdlminer_app::miner::MiningWork;
use bdlminer_app::App;
use bdlminer_core::bm1397;

use crate::asic::Status;
use crate::periph::{delay_ms, now, output, set, Adc, Uart};

const TX: i32 = 17;
const RX: i32 = 18;
const RESET: i32 = 12;
const POWER: i32 = 13;
const NTC: i32 = 2;
/// A new job every 30 ms, like the stock firmware.
const JOB_MS: u32 = 30;
/// No verified result for a minute: start the ASIC again.
const SILENCE_MS: u32 = 60_000;
/// Results of difficulty 64: about one a second at 200 GH/s.
const MAX_MASK_POWER: u8 = 6;
/// The stock firmware cuts the ASIC at 80 C. This board has no fan control
/// and its user limit can be off (0) or set higher: a hard stop of its own,
/// with 10 C of hysteresis, whatever the settings and the thermal policy.
const HARD_MAX_C: f32 = 80.0;
const HARD_RESUME_C: f32 = 70.0;
/// That hard stop needs a reading. Without one for this long (an open or
/// shorted NTC, or a main loop that no longer samples it) the ASIC is
/// stopped as well; it starts again by itself once the sensor reads.
const SENSOR_GRACE_MS: u32 = 10_000;

/// What the dashboard and the screen show, and the NTC reading.
struct Shared {
    status: Mutex<Status>,
    last_reading: AtomicU32,
    adc: Mutex<Option<Adc>>,
}

static SHARED: std::sync::OnceLock<Shared> = std::sync::OnceLock::new();

fn shared() -> &'static Shared {
    SHARED.get_or_init(|| Shared {
        status: Mutex::new(Status { temperature: f32::NAN, hardware_error_percent: -1.0, ..Default::default() }),
        last_reading: AtomicU32::new(0),
        adc: Mutex::new(None),
    })
}

#[derive(Clone, Default)]
struct Slot {
    work: Option<MiningWork>,
    versions: Vec<u32>,
    sent: u32,
    mask_power: u8,
    /// The chip can report a nonce twice.
    seen: [u64; 8],
    seen_head: usize,
}

struct Driver {
    app: Arc<App>,
    engine: usize,
    uart: Uart,
    receiver: bm1397::Receiver,
    slots: Vec<Slot>,
    next_slot: usize,
    powered: bool,
    active_mask: u32,
    mask_power: u8,
    last_result: u32,
    window_good: u32,
    window_bad: u32,
    overheated: bool,
    sensor_fault: bool,
    live: Status,
}

impl Driver {
    fn publish(&self) {
        *shared().status.lock().unwrap() = self.live.clone();
    }

    fn log(&self, line: &str) {
        self.app.log(line);
    }

    fn power_percent(&self) -> u8 {
        self.app.miner.power_percent()
    }

    fn target_frequency(&self) -> f32 {
        50f32.max(bm1397::DEFAULT_MHZ as f32 * self.power_percent() as f32 / 100.0)
    }

    fn off(&mut self, reason: &'static str) {
        set(RESET, true); // hold the ASIC first, then cut its supply
        set(POWER, false);
        self.powered = false;
        self.live.running = false;
        self.live.frequency = 0.0;
        self.live.set_state(reason);
        for slot in &mut self.slots {
            slot.work = None;
        }
        self.receiver.clear();
        self.next_slot = 0;
        self.publish();
    }

    /// Short waits that give up as soon as mining is paused.
    fn wait_safe(&mut self, ms: u32) -> bool {
        let since = now();
        loop {
            if self.power_percent() == 0 || !self.app.miner.board_valid() {
                self.off("Paused / waiting for pool");
                return false;
            }
            delay_ms(5);
            if now().wrapping_sub(since) >= ms {
                return true;
            }
        }
    }

    fn drain(&mut self) {
        self.uart.drain();
        self.receiver.clear();
    }

    fn send(&self, packet: &[u8]) -> bool {
        self.uart.write(packet)
    }

    fn init_asic(&mut self, version_mask: u32) -> bool {
        self.active_mask = version_mask;
        self.live.set_state("Initializing BM1397");
        self.publish();
        set(RESET, true);
        set(POWER, true);
        self.powered = true;
        if !self.wait_safe(100) {
            return false;
        }
        set(RESET, false);
        if !self.wait_safe(100) {
            return false;
        }
        if !self.uart.begin(115_200, 2048) {
            return false;
        }
        self.drain();
        if !self.send(&bm1397::read_register(0)) {
            return false;
        }
        let mut chips = 0;
        let since = now();
        loop {
            while let Some(byte) = self.uart.read() {
                if let Some(r) = self.receiver.push(byte) {
                    if r.chip_id {
                        chips += 1;
                    }
                }
            }
            if !self.wait_safe(5) {
                return false;
            }
            if now().wrapping_sub(since) >= 1000 {
                break;
            }
        }
        if chips != 1 {
            self.log(&format!("[nerdnos] {chips} BM1397 chip(s) answered, 1 expected"));
            self.off(if chips != 0 { "Unexpected number of ASIC chips" } else { "BM1397 not detected" });
            return false;
        }
        if !self.send(&bm1397::chain_inactive()) || !self.send(&bm1397::set_address(0)) {
            return false;
        }
        for (address, value) in [(0x80u8, 0u32), (0x84, 0), (0x20, 1), (0x3c, 0x8000_8074)] {
            if !self.send(&bm1397::reg(address, value)) || !self.wait_safe(2) {
                return false;
            }
        }
        if !self.send(&bm1397::ticket_mask(self.mask_power)) {
            return false;
        }
        if !self.send(&bm1397::reg(0x68, 0xc070_0111)) || !self.send(&bm1397::reg(0x28, 0x0600_000f)) {
            return false;
        }
        let target = self.target_frequency();
        for _ in 0..2 {
            if !self.send(&bm1397::pll_divider()) || !self.wait_safe(10) {
                return false;
            }
        }
        for _ in 0..2 {
            let Some((packet, actual)) = bm1397::frequency(target) else { return false };
            self.live.frequency = actual;
            if !self.send(&packet) || !self.wait_safe(10) {
                return false;
            }
        }
        if !self.send(&bm1397::max_baud()) {
            return false;
        }
        self.uart.flush();
        if !self.wait_safe(100) {
            return false;
        }
        self.uart.set_baud(bm1397::MAX_BAUD);
        if !self.wait_safe(100) {
            return false;
        }
        self.drain();
        self.last_result = now();
        self.window_good = 0;
        self.window_bad = 0;
        self.live.running = true;
        self.live.hardware_error_percent = -1.0;
        self.live.set_state("Mining (experimental)");
        self.log(&format!(
            "[nerdnos] BM1397 at {:.0} MHz, results of difficulty {} and above",
            self.live.frequency,
            1u32 << self.mask_power
        ));
        self.publish();
        true
    }

    /// A result: verified by hashing, counted as the work it stands for,
    /// then handed to the miner, which submits it if it meets the pool's
    /// difficulty.
    fn result(&mut self, r: &bm1397::Response, now: u32) {
        let slot_index = r.slot as usize;
        if slot_index >= self.slots.len() {
            return; // never index past the table, whatever the UART delivered
        }
        let mask_power = self.mask_power;
        let slot = &mut self.slots[slot_index];
        let Some(work) = slot.work.clone() else { return };
        if r.midstate as usize >= slot.versions.len()
            || now.wrapping_sub(slot.sent) > 2000
            || !self.app.miner.current(work.generation)
        {
            return;
        }
        let proof = (r.midstate as u64) << 32 | r.nonce as u64;
        if slot.seen.contains(&(proof + 1)) {
            return;
        }
        slot.seen[slot.seen_head] = proof + 1;
        slot.seen_head = (slot.seen_head + 1) % slot.seen.len();
        let version = slot.versions[r.midstate as usize];
        let base = u32::from_le_bytes(work.work.header[..4].try_into().unwrap());
        // After a difficulty change, results of this job can still have
        // passed the previous filter: a lower one is not a hardware error.
        // The lower filter is also the conservative hash credit for such a
        // result.
        let filter = slot.mask_power.min(mask_power);
        let minimum = (1u32 << filter) as f64 * 0.999;
        let Some(difficulty) = self.app.miner.asic_candidate(&work, r.nonce, version & !base, minimum) else {
            return;
        };
        if difficulty < minimum {
            self.window_bad += 1; // UART corruption or an ASIC hardware error
        } else {
            self.window_good += 1;
            self.last_result = now;
            self.live.results += 1;
            self.app.miner.add_hashes(self.engine, 1u64 << (32 + filter));
        }
        if self.window_good + self.window_bad >= 50 {
            self.live.hardware_error_percent =
                100.0 * self.window_bad as f32 / (self.window_good + self.window_bad) as f32;
            self.window_good = 0;
            self.window_bad = 0;
        }
        self.publish(); // the first verified result visible without waiting for a window
    }

    /// The NTC reading, sampled every second by the supervision.
    fn too_hot(&mut self) -> bool {
        let t = shared().status.lock().unwrap().temperature;
        if t.is_finite() && t >= HARD_MAX_C {
            self.overheated = true;
        } else if self.overheated && t.is_finite() && t <= HARD_RESUME_C {
            self.overheated = false;
        }
        self.overheated
    }

    /// True while the temperature sensor gives no reading: the ASIC stays
    /// off.
    fn sensor_stopped(&mut self, now: u32) -> bool {
        let lost = now.wrapping_sub(shared().last_reading.load(Ordering::Relaxed)) > SENSOR_GRACE_MS;
        if lost && (!self.sensor_fault || self.powered) {
            if !self.sensor_fault {
                self.log(&format!(
                    "[nerdnos] no temperature reading for {} s: ASIC stopped until the sensor reads",
                    SENSOR_GRACE_MS / 1000
                ));
            }
            self.off("Temperature sensor not reading: ASIC stopped");
        } else if !lost && self.sensor_fault {
            self.log("[nerdnos] temperature sensor reads again");
        }
        self.sensor_fault = lost;
        lost
    }

    fn run(mut self) -> ! {
        let mut sent = 0u32;
        loop {
            let mut t = now();
            if self.power_percent() == 0 {
                if self.powered {
                    self.off("Paused");
                }
                delay_ms(10);
                continue;
            }
            if self.too_hot() {
                if self.powered {
                    let temperature = shared().status.lock().unwrap().temperature;
                    self.log(&format!("[nerdnos] ASIC at {temperature:.1} C: stopped until {HARD_RESUME_C:.0} C"));
                    self.off("ASIC overtemperature: cooling");
                }
                delay_ms(100);
                continue;
            }
            if self.sensor_stopped(t) {
                delay_ms(100);
                continue;
            }
            if !self.powered || t.wrapping_sub(sent) >= JOB_MS {
                let Some(next) = self.app.miner.asic_work() else {
                    if self.powered {
                        self.off("Waiting for pool");
                    }
                    delay_ms(10);
                    continue;
                };
                // Four midstates need rolling bits.
                if next.work.version_mask.count_ones() < 2 {
                    if self.powered {
                        self.off("Pool needs version rolling");
                    }
                    self.live.set_state("Pool needs version rolling");
                    self.publish();
                    delay_ms(100);
                    continue;
                }
                // The hardware filter follows the pool difficulty, from 16 up:
                // below it (a pool taking the miner for a NerdMiner) only shares
                // from 16 are sent, as in ESP-Miner. Block odds do not change.
                let power = (next.work.share_difficulty.log2().floor() as i32).clamp(4, MAX_MASK_POWER as i32) as u8;
                if self.powered && next.work.version_mask != self.active_mask {
                    self.off("Changing version mask");
                }
                if self.powered && power != self.mask_power {
                    self.mask_power = power;
                    if !self.send(&bm1397::ticket_mask(self.mask_power)) {
                        self.off("ASIC UART write failed");
                        continue;
                    }
                }
                if !self.powered {
                    self.mask_power = power;
                    if !self.init_asic(next.work.version_mask) {
                        if self.powered {
                            self.off("BM1397 initialization failed");
                        }
                        delay_ms(1000);
                        continue;
                    }
                    t = now();
                }
                if (self.live.frequency - self.target_frequency()).abs() > 3.0 {
                    self.off("Applying power limit");
                    continue;
                }
                let (packet, versions) = bm1397::job(self.next_slot as u8, &next.work.header, next.work.version_mask);
                let mask_power = self.mask_power;
                let slot = &mut self.slots[self.next_slot];
                *slot = Slot { work: Some(next), versions, sent: t, mask_power, ..Default::default() };
                if !self.send(&packet) {
                    self.off("ASIC UART write failed");
                    continue;
                }
                self.next_slot = (self.next_slot + 1) % bm1397::SLOTS;
                sent = t;
            }
            let mut budget = 2048;
            while self.powered && budget > 0 {
                let Some(byte) = self.uart.read() else { break };
                budget -= 1;
                if let Some(r) = self.receiver.push(byte) {
                    if !r.chip_id {
                        self.result(&r, t);
                    }
                }
            }
            if self.powered && self.live.running && t.wrapping_sub(self.last_result) > SILENCE_MS {
                self.off("ASIC not responding; starting it again");
            }
            delay_ms(2);
        }
    }
}

/// ASIC unpowered and held in reset until there is work; the NTC's ADC.
pub fn begin() {
    output(RESET, true);
    output(POWER, false);
    *shared().adc.lock().unwrap() = Adc::new(NTC);
    let mut s = shared().status.lock().unwrap();
    s.set_state("Ready / waiting for pool");
}

pub fn start(app: Arc<App>, engine: usize) -> bool {
    let driver = Driver {
        app,
        engine,
        uart: Uart::new(1, TX, RX),
        receiver: bm1397::Receiver::default(),
        slots: vec![Slot::default(); bm1397::SLOTS],
        next_slot: 0,
        powered: false,
        active_mask: 0,
        mask_power: 0,
        last_result: 0,
        window_good: 0,
        window_bad: 0,
        overheated: false,
        sensor_fault: false,
        live: shared().status.lock().unwrap().clone(),
    };
    crate::spawn_pinned(b"bm1397\0", esp_idf_svc::hal::cpu::Core::Core1, 3, 8 * 1024, move || driver.run())
}

pub fn status() -> Status {
    shared().status.lock().unwrap().clone()
}

/// The NTC next to the ASIC; NaN without a usable reading. Sampled once a
/// second by the supervision: the driver stops the ASIC when it stops.
pub fn temperature() -> f32 {
    let mv: u32 = {
        let adc = shared().adc.lock().unwrap();
        match adc.as_ref() {
            Some(adc) => (0..8).map(|_| adc.millivolts()).sum(),
            None => 0,
        }
    };
    let v = mv as f32 / 8000.0;
    let mut t = f32::NAN;
    // Divider to 3.3 V, as the stock firmware reads it; rails mean an open or
    // shorted sensor.
    if v > 0.05 && v < 3.25 {
        let r = 10_000.0 * v / (3.3 - v);
        t = 1.0 / ((r / 10_000.0).ln() / 3380.0 + 1.0 / 298.15) - 273.15;
        if !(t > -40.0 && t < 150.0) {
            t = f32::NAN;
        }
    }
    shared().status.lock().unwrap().temperature = t;
    if t.is_finite() {
        shared().last_reading.store(now(), Ordering::Relaxed);
    }
    t
}
