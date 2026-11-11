// The NerdNOS backend on the host: pin order, the BM1397 start-up sequence,
// chip detection, result verification and the NTC conversion.
#define BOARD_NERDNOS 1
#define BOARD_ASIC 1
#include "../../src/nerdnos.cpp"
#include <cassert>
#include <cmath>
void logf(const char *, ...) {}
static uint8_t power = 100;
static uint32_t generation = 1;
static uint64_t hashes = 0;
static int candidates = 0;
static int verifications = 0;
static uint32_t candidateBits = 99;
static bool poolReady = true;
static double forcedDifficulty = 0;  // > 0: the digest's difficulty, without searching for such a nonce
uint8_t minerPowerPercent() { return power; }
bool minerAsicCurrent(uint32_t g) { return g == generation; }
bool minerAsicReady() { return poolReady; }
bool minerAsicWork(AsicWork &) { return false; }
double minerAsicCandidate(AsicWork &w, uint32_t nonce, uint32_t bits, double minimum) {
    ++verifications;
    uint8_t header[80],hash[32];uint32_t version;
    std::memcpy(header,w.header,80);std::memcpy(&version,header,4);version|=bits;
    std::memcpy(header,&version,4);std::memcpy(header+76,&nonce,4);
    sha::sha256d(header,80,hash);double difficulty=forcedDifficulty>0?forcedDifficulty:work::hashDifficulty(hash);
    if(difficulty>=minimum){++candidates;candidateBits=bits;}
    return difficulty;
}
void minerAsicHashes(uint64_t n) { hashes += n; }
using namespace nerdnos;
static uint32_t reg(const std::vector<uint8_t> &p) {
    return uint32_t(p[6]) << 24 | uint32_t(p[7]) << 16 | uint32_t(p[8]) << 8 | p[9];
}
static void reset() {
    testPins.clear(); testUartTx.clear(); testUartRx.clear(); testChips = 1; testMillis = 1000;
    power = 100; generation = 1; hashes = 0; candidates = verifications = 0; powered = false; maskPower = 6;
    poolReady = true;testNerdDelayHook=nullptr;
    live = {NAN, 0, false, "Starting", -1, 0}; publish();
}
int main() {
    reset(); begin();
    assert(testPins[kReset] == HIGH && testPins[kPower] == LOW);  // held and unpowered until there is work

    assert(initAsic(bm1397::kVersionMask) && powered && live.running);
    assert(testPins[kReset] == LOW && testPins[kPower] == HIGH && testBaud == int(bm1397::kMaxBaud));
    assert(testUartTx.front().size() == 7 && testUartTx.front()[2] == 0x52);  // chip id read first
    bool inactive = false, address = false, ticket = false, divider = false, baud = false;
    int pll = 0;
    for (auto &p : testUartTx) {
        if (p.size() == 7 && p[2] == 0x53) inactive = true;
        if (p.size() == 7 && p[2] == 0x40) address = true;
        if (p.size() != 11 || p[2] != 0x51) continue;
        if (p[5] == 0x14 && reg(p) == 0x000000fc) ticket = true;  // difficulty 64
        if (p[5] == 0x70 && reg(p) == 0x0f0f0f00) divider = true;
        if (p[5] == 0x08) { assert(reg(p) == 0x40780215); ++pll; }  // 300 MHz
        if (p[5] == 0x18 && reg(p) == 0x00006131) baud = true;
    }
    assert(inactive && address && ticket && divider && baud && pll == 2 && live.frequency == 300);

    off("Paused");
    assert(!powered && !live.running && testPins[kReset] == HIGH && testPins[kPower] == LOW);
    reset(); power = 50; assert(initAsic(bm1397::kVersionMask) && std::fabs(live.frequency - 150) < 0.01f);
    reset(); testChips = 0;
    assert(!initAsic(bm1397::kVersionMask) && !powered && testPins[kPower] == LOW && !strcmp(live.state, "BM1397 not detected"));
    reset(); testChips = 2; assert(!initAsic(bm1397::kVersionMask) && !powered);
    reset(); power = 0; assert(!initAsic(bm1397::kVersionMask) && !powered && testPins[kReset] == HIGH);

    // Results, with the genesis block header: its nonce is a real proof of work.
    reset(); assert(initAsic(bm1397::kVersionMask));
    std::vector<uint8_t> genesis;
    assert(work::fromHex("01000000" "0000000000000000000000000000000000000000000000000000000000000000"
                         "3ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a"
                         "29ab5f49" "ffff001d" "00000000", genesis));
    Slot &slot = slots[3];
    std::memcpy(slot.work.header, genesis.data(), 80);
    slot.work.generation = 1; slot.versions[0] = 1; slot.count = 1; slot.sent = testMillis; slot.valid = true; slot.maskPower = 6;
    bm1397::Response r; r.slot = 3; r.midstate = 0; r.nonce = 0x7c2bac1d;
    result(r, testMillis);
    assert(candidates == 1 && candidateBits == 0 && hashes == uint64_t(1) << 38 && live.results == 1);
    assert(status().results == 1); // publish each verified result, including the first one
    result(r, testMillis);  // the same result again
    assert(candidates == 1 && hashes == uint64_t(1) << 38);
    assert(verifications == 1); // no duplicate digest or submission for one UART proof
    r.nonce = 0x7c2bac1e; result(r, testMillis);  // corrupted: counted as an error, never submitted
    assert(candidates == 1 && windowBad == 1);
    r.midstate = 1; r.nonce = 0x7c2bac1d; result(r, testMillis);  // no such midstate in this job
    { bm1397::Response outside = r; outside.midstate = 0;  // a slot past the table (corrupted frame): ignored
      const uint32_t errorsBefore = windowBad, goodBefore = windowGood, resultsBefore = live.results;
      for (unsigned s = bm1397::kSlots; s < 256; ++s) { outside.slot = uint8_t(s); result(outside, testMillis); }
      assert(windowBad == errorsBefore && windowGood == goodBefore && live.results == resultsBefore); }
    assert(candidates == 1 && windowBad == 1);
    generation = 2; r.midstate = 0; slot.seenHead = 0; std::fill(std::begin(slot.seen), std::end(slot.seen), 0);
    result(r, testMillis);  // a new block made the job stale
    assert(candidates == 1);

    // Pool difficulty raised (filter 16 -> 64): a result of a job sent before the
    // change may still have passed the old filter. It is not a hardware error.
    reset(); assert(initAsic(bm1397::kVersionMask));
    Slot &before = slots[5];
    std::memcpy(before.work.header, genesis.data(), 80);
    before.work.generation = 1; before.versions[0] = 1; before.count = 1; before.sent = testMillis; before.valid = true;
    before.maskPower = 4; maskPower = 6; windowBad = windowGood = 0;
    // A result of difficulty 20, as the chip reports under filter 16.
    forcedDifficulty = 20;
    bm1397::Response old; old.slot = 5; old.midstate = 0; old.nonce = 1234;
    hashes = 0; result(old, testMillis);
    assert(windowBad == 0 && windowGood == 1 && hashes == uint64_t(1) << 36);
    // A job sent under the new filter: the same result would be an error.
    before.maskPower = 6; std::fill(std::begin(before.seen), std::end(before.seen), 0);
    result(old, testMillis); assert(windowBad == 1);
    forcedDifficulty = 0;

    // Hard stop at 80 C, whatever the user limit (it can be 0 = off), until 70 C.
    reset(); assert(initAsic(bm1397::kVersionMask) && powered);
    auto ntc = [](float c) { portENTER_CRITICAL(&mux); snapshot.temperature = c; portEXIT_CRITICAL(&mux); };
    ntc(79.9f); assert(!tooHot());
    ntc(80.0f); assert(tooHot());
    ntc(NAN); assert(tooHot());       // a lost reading never ends the cooling
    ntc(75.0f); assert(tooHot());
    ntc(70.0f); assert(!tooHot());
    ntc(25.0f); overheated = false;

    // NTC: half the supply is 10 kOhm, 25 C; the rails are a broken sensor.
    testMilliVolts = 1650; assert(std::fabs(temperature() - 25.0f) < 0.05f && std::fabs(status().temperature - 25.0f) < 0.05f);
    testMilliVolts = 0; assert(std::isnan(temperature()));
    testMilliVolts = 3300; assert(std::isnan(temperature()));
    // Without a reading the hard stop sees nothing: after 10 s the ASIC is stopped, whatever the
    // settings, and stays off until the sensor reads again.
    reset(); testMilliVolts = 1650; temperature();
    assert(initAsic(bm1397::kVersionMask) && powered && !sensorStopped(testMillis) && powered);
    temperature(); testMilliVolts = 0; temperature();  // a last reading, then the NTC opens
    testMillis += 10000; assert(!sensorStopped(testMillis) && powered);  // still within the grace
    testMillis += 1; assert(sensorStopped(testMillis) && !powered && testPins[kReset] == HIGH && testPins[kPower] == LOW);
    assert(!strcmp(status().state, "Temperature sensor not reading: ASIC stopped"));
    testMillis += 60000; temperature(); assert(sensorStopped(testMillis) && !powered);
    testMilliVolts = 1650; temperature(); assert(!sensorStopped(testMillis) && !sensorFault);
    // A sensor that never read since the start: the same stop once the grace is over.
    lastReading = 0; sensorFault = false;
    assert(!sensorStopped(10000) && sensorStopped(10001) && sensorFault);
    testMilliVolts = 1650; temperature(); sensorFault = false;
    // A lost job during chip detection must cut power without finishing startup.
    reset();testNerdDelayHook=[](uint32_t){if(testMillis>=1250)poolReady=false;};
    assert(!initAsic(bm1397::kVersionMask)&&!powered&&testPins[kReset]==HIGH&&testPins[kPower]==LOW&&testMillis<1300);
    puts("NerdNOS backend: reset before power, BM1397 start-up sequence, chip detection, power limit, verified/duplicate/corrupted/stale results and NTC passed");
}
