#include "nerdnos.h"
#ifdef BOARD_NERDNOS
#include <Arduino.h>
#include <algorithm>
#include <cmath>
#include <cstring>
#include "core/bm1397.h"
#include "core/sha256.h"
#include "core/work.h"
#include "log.h"
#include "miner.h"
#include "runtime.h"

namespace nerdnos {
namespace {
// As the NerdNOS firmware drives them: UART1 to the ASIC through a level
// shifter, the core buck converter's enable, the ASIC reset (high holds it in
// reset on this board) and a 10 kOhm NTC (beta 3380) on ADC1.
constexpr int kTx = 17, kRx = 18, kReset = 12, kPower = 13, kNtc = 2;
constexpr uint32_t kJobMs = 30;         // a new job every 30 ms, like the stock firmware
constexpr uint32_t kSilenceMs = 60000;  // no verified result for a minute: start the ASIC again
constexpr uint8_t kMaxMaskPower = 6;    // results of difficulty 64: about one a second at 200 GH/s

HardwareSerial uart(1);
portMUX_TYPE mux = portMUX_INITIALIZER_UNLOCKED;
Status snapshot{NAN, 0, false, "Starting", -1, 0};
Status live = snapshot;
bool powered = false;
uint32_t activeMask = 0;
uint8_t maskPower = 0;
bm1397::Receiver receiver;
struct Slot {
    AsicWork work;
    uint32_t versions[bm1397::kMidstates]{};
    uint8_t count = 0;
    uint32_t sent = 0;
    bool valid = false;
    uint8_t maskPower = 0;  // the chip's result filter when this job was sent
    uint64_t seen[8]{};  // the chip can report a nonce twice
    uint8_t seenHead = 0;
};
Slot slots[bm1397::kSlots];
uint8_t nextSlot = 0;
uint32_t lastResult = 0, windowGood = 0, windowBad = 0;
// The stock firmware cuts the ASIC at 80 C. This board has no fan control and
// its user limit can be off (0) or set higher: a hard stop of its own, with
// 10 C of hysteresis, whatever the settings and the thermal policy.
constexpr float kHardMaxC = 80, kHardResumeC = 70;
bool overheated = false;

void publish() { portENTER_CRITICAL(&mux); snapshot = live; portEXIT_CRITICAL(&mux); }
bool send(const bm1397::Packet &p) { return p.size && uart.write(p.bytes.data(), p.size) == p.size; }
float targetFrequency() {
    return std::max(50.0f, float(bm1397::kDefaultMHz) * minerPowerPercent() / 100);
}
void off(const char *reason) {
    digitalWrite(kReset, HIGH);  // hold the ASIC first, then cut its supply
    digitalWrite(kPower, LOW);
    powered = false;
    live.running = false; live.frequency = 0; live.state = reason;
    for (auto &slot : slots) slot.valid = false;
    receiver.clear(); nextSlot = 0;
    publish();
}
// Short waits that give up as soon as mining is paused.
bool waitSafe(uint32_t ms) {
    uint32_t since = millis();
    do {
        if (!minerPowerPercent() || !minerAsicReady()) { off("Paused / waiting for pool"); return false; }
        delay(5);
    } while (millis() - since < ms);
    return true;
}
void drain() { while (uart.available()) uart.read(); receiver.clear(); }

bool initAsic(uint32_t versionMask) {
    activeMask = versionMask;
    live.state = "Initializing BM1397"; publish();
    digitalWrite(kReset, HIGH);
    digitalWrite(kPower, HIGH);
    powered = true;
    if (!waitSafe(100)) return false;
    digitalWrite(kReset, LOW);
    if (!waitSafe(100)) return false;
    uart.end(); uart.setRxBufferSize(2048); uart.begin(115200, SERIAL_8N1, kRx, kTx);
    drain();
    if (!send(bm1397::readRegister(0))) return false;
    int chips = 0;
    bm1397::Response r;
    uint32_t since = millis();
    do {
        while (uart.available()) if (receiver.push(uart.read(), r) && r.chipId) ++chips;
        if (!waitSafe(5)) return false;
    } while (millis() - since < 1000);
    if (chips != 1) {
        logf("[nerdnos] %d BM1397 chip(s) answered, 1 expected\n", chips);
        off(chips ? "Unexpected number of ASIC chips" : "BM1397 not detected");
        return false;
    }
    const struct { uint8_t address; uint32_t value; } setup[]{
        {0x80, 0}, {0x84, 0}, {0x20, 1}, {0x3c, 0x80008074}};
    if (!send(bm1397::chainInactive()) || !send(bm1397::setAddress(0))) return false;
    for (auto &step : setup) if (!send(bm1397::reg(step.address, step.value)) || !waitSafe(2)) return false;
    if (!send(bm1397::ticketMask(maskPower))) return false;
    if (!send(bm1397::reg(0x68, 0xc0700111)) || !send(bm1397::reg(0x28, 0x0600000f))) return false;
    float target = targetFrequency();
    for (int i = 0; i < 2; ++i) if (!send(bm1397::pllDivider()) || !waitSafe(10)) return false;
    for (int i = 0; i < 2; ++i) if (!send(bm1397::frequency(target, &live.frequency)) || !waitSafe(10)) return false;
    if (!send(bm1397::maxBaud())) return false;
    uart.flush();
    if (!waitSafe(100)) return false;
    uart.updateBaudRate(bm1397::kMaxBaud);
    if (!waitSafe(100)) return false;
    drain();
    lastResult = millis(); windowGood = windowBad = 0;
    live.running = true; live.hardwareErrorPercent = -1; live.state = "Mining (experimental)";
    logf("[nerdnos] BM1397 at %.0f MHz, results of difficulty %u and above\n", live.frequency, 1u << maskPower);
    publish();
    return true;
}

// A result: verified by hashing, counted as the work it stands for, then
// handed to the miner, which submits it if it meets the pool's difficulty.
void result(const bm1397::Response &r, uint32_t now) {
    if (r.slot >= bm1397::kSlots) return;  // never index past the table, whatever the UART delivered
    Slot &slot = slots[r.slot];
    if (!slot.valid || r.midstate >= slot.count || now - slot.sent > 2000 || !minerAsicCurrent(slot.work.generation)) return;
    uint64_t proof = uint64_t(r.midstate) << 32 | r.nonce;
    for (uint64_t seen : slot.seen) if (seen == proof + 1) return;
    slot.seen[slot.seenHead] = proof + 1; slot.seenHead = (slot.seenHead + 1) % 8;
    uint32_t version = slot.versions[r.midstate];
    uint32_t base;
    std::memcpy(&base, slot.work.header, 4);
    // After a difficulty change, results of this job can still have passed the
    // previous filter: a lower one is not a hardware error. The lower filter
    // is also the conservative hash credit for such a result.
    uint8_t filter = std::min(slot.maskPower, maskPower);
    double minimum = double(1u << filter) * 0.999;
    // Verification and submission share one digest. The common adapter still
    // rejects stale generations, unnegotiated versions and corrupted proofs.
    double difficulty = minerAsicCandidate(slot.work, r.nonce, version & ~base, minimum);
    if (difficulty < 0) return;
    if (difficulty < minimum) {
        ++windowBad;  // UART corruption or an ASIC hardware error
    } else {
        ++windowGood; lastResult = now; ++live.results;
        minerAsicHashes(uint64_t(1) << (32 + filter));
    }
    if (windowGood + windowBad >= 50) {
        live.hardwareErrorPercent = 100.0f * windowBad / (windowGood + windowBad);
        windowGood = windowBad = 0;
    }
    publish(); // make the first verified result visible without waiting for a 50-result window
}

// The NTC reading, sampled every second by the main loop (temperature()).
bool tooHot() {
    portENTER_CRITICAL(&mux); float t = snapshot.temperature; portEXIT_CRITICAL(&mux);
    if (std::isfinite(t) && t >= kHardMaxC) overheated = true;
    else if (overheated && std::isfinite(t) && t <= kHardResumeC) overheated = false;
    return overheated;
}

void worker(void *) {
    uint32_t sent = 0;
    AsicWork next;
    for (;;) {
        uint32_t now = millis();
        if (!minerPowerPercent()) {
            if (powered) off("Paused");
            delay(10); continue;
        }
        if (tooHot()) {
            if (powered) {
                logf("[nerdnos] ASIC at %.1f C: stopped until %.0f C\n", status().temperature, kHardResumeC);
                off("ASIC overtemperature: cooling");
            }
            delay(100); continue;
        }
        if (!powered || now - sent >= kJobMs) {
            if (!minerAsicWork(next)) {
                if (powered) off("Waiting for pool");
                delay(10); continue;
            }
            // Four midstates need rolling bits.
            if (__builtin_popcount(next.versionMask) < 2) {
                const char *why = "Pool needs version rolling";
                if (powered) off(why);
                live.state = why; publish();
                delay(100); continue;
            }
            // The hardware filter follows the pool difficulty, from 16 up: below it
            // (a pool taking the miner for a NerdMiner) only shares from 16 are sent,
            // as in ESP-Miner. Block odds do not change.
            uint8_t power = std::min<int>(kMaxMaskPower, std::max(4, int(std::floor(std::log2(next.shareDifficulty)))));
            if (powered && next.versionMask != activeMask) off("Changing version mask");
            if (powered && power != maskPower) {
                maskPower = power;
                if (!send(bm1397::ticketMask(maskPower))) { off("ASIC UART write failed"); continue; }
            }
            if (!powered) {
                maskPower = power;
                if (!initAsic(next.versionMask)) {
                    if (powered) off("BM1397 initialization failed");
                    delay(1000); continue;
                }
                now = millis();
            }
            if (std::fabs(live.frequency - targetFrequency()) > 3) { off("Applying power limit"); continue; }
            Slot &slot = slots[nextSlot];
            slot.work = next; slot.sent = now; slot.valid = true; slot.maskPower = maskPower;
            std::fill(std::begin(slot.seen), std::end(slot.seen), 0);
            if (!send(bm1397::job(nextSlot, next.header, next.versionMask, slot.versions, slot.count))) {
                off("ASIC UART write failed"); continue;
            }
            nextSlot = (nextSlot + 1) % bm1397::kSlots;
            sent = now;
        }
        bm1397::Response r;
        size_t budget = 2048;
        while (powered && budget-- && uart.available())
            if (receiver.push(uart.read(), r) && !r.chipId) result(r, now);
        if (powered && live.running && now - lastResult > kSilenceMs) off("ASIC not responding; starting it again");
        delay(2);
    }
}
}  // namespace

void begin() {
    pinMode(kReset, OUTPUT); digitalWrite(kReset, HIGH);
    pinMode(kPower, OUTPUT); digitalWrite(kPower, LOW);
    analogSetPinAttenuation(kNtc, ADC_11db);
    live.state = "Ready / waiting for pool"; publish();
}
bool start() {
    bool ok = runtime::started(xTaskCreatePinnedToCore(worker, "bm1397", 8192, nullptr, 3, nullptr, 1) == pdPASS,
                               runtime::Miner, "BM1397 task");
    if (!ok) off("ASIC task allocation failed");
    return ok;
}
Status status() { portENTER_CRITICAL(&mux); Status out = snapshot; portEXIT_CRITICAL(&mux); return out; }
float temperature() {
    uint32_t mv = 0;
    for (int i = 0; i < 8; ++i) mv += analogReadMilliVolts(kNtc);
    float v = mv / 8000.0f, t = NAN;
    // Divider to 3.3 V, as the stock firmware reads it; rails mean an open or shorted sensor.
    if (v > 0.05f && v < 3.25f) {
        float r = 10000.0f * v / (3.3f - v);
        t = 1.0f / (std::log(r / 10000.0f) / 3380.0f + 1.0f / 298.15f) - 273.15f;
        if (!(t > -40 && t < 150)) t = NAN;
    }
    portENTER_CRITICAL(&mux); live.temperature = snapshot.temperature = t; portEXIT_CRITICAL(&mux);
    return t;
}
}  // namespace nerdnos
#endif
