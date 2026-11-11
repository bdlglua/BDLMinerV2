#include "gamma601.h"
#ifdef BOARD_GAMMA601
#include <Arduino.h>
#include <M5GFX.h>
#include <cmath>
#include <algorithm>
#include <atomic>
#include <ctime>
#include <Preferences.h>
#include "core/bm1370.h"
#include "core/fan_control.h"
#include "miner.h"
#include "settings.h"
#include "log.h"
#include "runtime.h"

namespace gamma601 {
namespace {
constexpr uint8_t kTps = 0x24, kEmc = 0x4c;
constexpr int kReset = 1;
HardwareSerial uart(1);
portMUX_TYPE mux = portMUX_INITIALIZER_UNLOCKED;
Status snapshot{NAN, NAN, NAN, NAN, NAN, 0, 0, 0, false, "Starting", 525, false, "", -1,
                -1, 100, false, true, "Startup or stopped ASIC"};
Status live = snapshot;
std::atomic<uint32_t> fanSetting{100 | (65u << 16)}; // percent, auto bit 8, thermal limit byte 2
mining::FanControl fanPolicy;
uint8_t appliedFanDuty = 255; // unknown until the EMC2101 write/read-back succeeds
bool configured = false, latched = false, powered = false;
bool logSetup = true;  // the setup's failures are logged once, not at every retry
const char kNotFound[] = "Regulator or sensors not found: plug in the 5V supply";
uint8_t voltageMode = 0;
// The board's own temperature (EMC2101 internal sensor), for the thermal policy
// while the ASIC diode has no reading: before the ASIC first runs, or paused.
std::atomic<float> boardTemperature{NAN};
// The ASIC diode alone can stop the ASIC: a reading out of bounds must hold for
// kDiodeConfirm readings in a row (0.75 s while mining). A real overheat takes
// far longer; one false reading (102 C between two 52 C on a Gamma) no longer
// stops it until restart. Meanwhile the last good reading stands.
constexpr uint8_t kDiodeConfirm = 3;
uint8_t diodeStrikes = 0;
float lastDiode = NAN;
// Once powered, the diode only reads after the BM1370 routes it (analog mux,
// register 54h): before that, a real Gamma read 106 C on a 25 C board.
bool diodeReady = false;
// The last fault that stopped the ASIC, with every reading, kept across restarts.
char lastFaultText[192] = "";
uint32_t lastFaultTime = 0;  // Unix time, 0 if the clock was not set
uint32_t powerEnabledAt = 0, lastHealthSample = 0;
bm1370::Receiver receiver;
bm1370::Counter counter;
bm1370::ErrorMonitor errors;
bool forceStock = false, errorSeen = false;
uint32_t errorValue = 0, lastErrorReply = 0;
float targetFrequency() {
    unsigned requested = bm1370::validProfile(settings.asicFrequency) ? settings.asicFrequency : bm1370::kStockMHz;
    return std::max(50.0f, float(forceStock ? bm1370::kStockMHz : requested)*minerPowerPercent()/100);
}
struct Slot {
    AsicWork work;
    uint32_t sent = 0;
    bool valid = false;
    // Suppress duplicate UART results, without conflating two rolled versions.
    uint64_t proofs[32]{};
    uint8_t proofHead = 0, proofCount = 0;
};
Slot slots[bm1370::kSlots];
uint8_t nextSlot = 0;
uint32_t activeGeneration = 0, activeMask = 0, lastHashReply = 0, lastHashProgress = 0;
uint32_t noWorkSince = 0;  // powered without pool work since then (0: has work); see step()

void publish() { portENTER_CRITICAL(&mux); snapshot = live; portEXIT_CRITICAL(&mux); }
bool read(uint8_t addr, uint8_t reg, uint8_t *out, size_t size) {
    return lgfx::i2c::transactionWriteRead(0, addr, &reg, 1, out, size, 400000).has_value();
}
bool write8(uint8_t addr, uint8_t reg, uint8_t value) {
    uint8_t bytes[]{reg, value};
    return lgfx::i2c::transactionWrite(0, addr, bytes, sizeof(bytes), 400000).has_value();
}
bool write16(uint8_t reg, uint16_t value) {
    uint8_t bytes[]{reg, uint8_t(value), uint8_t(value >> 8)};
    return lgfx::i2c::transactionWrite(0, kTps, bytes, sizeof(bytes), 400000).has_value();
}
bool read16(uint8_t addr, uint8_t reg, uint16_t &value) {
    uint8_t bytes[2]; if (!read(addr, reg, bytes, 2)) return false;
    value = uint16_t(bytes[0]) | uint16_t(bytes[1]) << 8; return true;
}
// Every setting is read back; one that does not hold stops the setup, and
// the log says which.
bool unset(uint8_t addr, uint8_t reg, const char *why) {
    if (logSetup) logf("[gamma] device 0x%02x register 0x%02x: %s\n", addr, reg, why);
    return false;
}
bool verify8(uint8_t addr, uint8_t reg, uint8_t value) {
    uint8_t actual;
    if (!write8(addr, reg, value) || !read(addr, reg, &actual, 1)) return unset(addr, reg, "no answer");
    if (actual != value) return unset(addr, reg, "reads back another value");
    return true;
}
bool setFanDuty(uint8_t duty, bool force = false) {
    if (!force && duty == appliedFanDuty) return true;
    if (!verify8(kEmc, 0x4c, duty)) { appliedFanDuty = 255; live.fanPercent = -1; return false; }
    appliedFanDuty = duty;
    live.fanPercent = mining::fanPwmPercent(duty);
    return true;
}
bool verify16(uint8_t reg, uint16_t value) {
    uint16_t actual;
    if (!write16(reg, value) || !read16(kTps, reg, actual)) return unset(kTps, reg, "no answer");
    if (actual != value) return unset(kTps, reg, "reads back another value");
    return true;
}
// A number may come back in the regulator's own format (another exponent),
// or rounded to its resolution (VIN_ON: 0.25 V steps): compared as values.
bool linearWord(uint8_t reg, uint16_t word, float value, float tolerance = NAN) {
    uint16_t actual;
    if (!write16(reg, word) || !read16(kTps, reg, actual)) return unset(kTps, reg, "no answer");
    if (!std::isfinite(tolerance)) tolerance = std::max(0.01f, std::fabs(value) * 0.03f);
    if (std::fabs(bm1370::linear11(actual) - value) > tolerance) return unset(kTps, reg, "reads back another value");
    return true;
}
bool linear(uint8_t reg, float value, float tolerance = NAN) {
    return linearWord(reg, bm1370::encodeLinear11(value), value, tolerance);
}
// Temperatures and timings: whole numbers, written as ESP-Miner does.
bool linearInt(uint8_t reg, unsigned value) { return linearWord(reg, bm1370::encodeLinear11Int(value), value); }
bool voltage(uint8_t reg, float value, float tolerance = 0.004f) {
    uint16_t word, actual;
    if (!bm1370::voltageWord(value, voltageMode, word)) return unset(kTps, reg, "unsupported VOUT_MODE");
    if (!write16(reg, word) || !read16(kTps, reg, actual)) return unset(kTps, reg, "no answer");
    int e = voltageMode & 31; if (e & 16) e -= 32;
    if (std::fabs(std::ldexp(float(actual) - float(word), e)) > tolerance)
        return unset(kTps, reg, "reads back another value");
    return true;
}
// The core voltage chosen in the settings (ESP-Miner's BM1370 values), read
// while the regulator is configured with its output off.
float coreVolts() {
    return (bm1370::validVoltage(settings.asicVoltage) ? settings.asicVoltage : bm1370::kStockMv) / 1000.0f;
}
// The margin and OV/UV limits, as a fraction of VOUT_COMMAND. The TPS546
// keeps them relative by default (VOUT_MODE bit 7, as on the Bitaxe), else
// in volts. Its hardware rounds them to steps of 1 to 10% (the OV fault
// limit: 10% while the output is off): compared within 6%.
bool limit(uint8_t reg, float fraction) {
    bool relative = voltageMode & 0x80;
    return relative ? voltage(reg, fraction, 0.06f) : voltage(reg, fraction * coreVolts(), 0.06f * coreVolts());
}
// PIN_DETECT_OVERRIDE (EEh) only matters when the regulator restores itself at
// power-on; written as ESP-Miner does (FFFFh). Some of its bits are reserved:
// a different read-back is logged, not a failure.
bool pinDetectOverride() {
    uint16_t actual;
    if (!write16(0xee, 0xffff) || !read16(kTps, 0xee, actual)) return unset(kTps, 0xee, "no answer");
    if (actual != 0xffff && logSetup) logf("[gamma] PIN_DETECT_OVERRIDE reads 0x%04x\n", actual);
    return true;
}
bool clearFaults() {
    uint8_t clear = 0x03;
    return lgfx::i2c::transactionWrite(0, kTps, &clear, 1, 400000).has_value();
}
void invalidate() {
    for (auto &slot : slots) slot.valid = false;
    receiver.clear(); counter.reset(); errors.reset(); errorSeen=false; nextSlot = 0;
}
void saveFault(const char *reason) {
    char text[sizeof(lastFaultText)];
    snprintf(text, sizeof(text), "%s (regulator status 0x%04x, input %.2f V, core %.3f V, %.1f A, ASIC %.1f C, "
             "regulator %.1f C, fan %u rpm)", reason, live.regulatorStatus, live.inputVoltage, live.coreVoltage,
             live.current, live.temperature, live.regulatorTemperature, live.rpm);
    time_t now = time(nullptr);
    portENTER_CRITICAL(&mux);
    memcpy(lastFaultText, text, sizeof(text));
    lastFaultTime = now > 1700000000 ? uint32_t(now) : 0;
    portEXIT_CRITICAL(&mux);
    Preferences p;
    if (p.begin("gamma", false)) {
        p.putBytes("fault", text, strlen(text));
        p.putBytes("faultt", &lastFaultTime, sizeof(lastFaultTime));
        p.end();
    }
}
void loadFault() {
    Preferences p;
    if (!p.begin("gamma", true)) return;
    size_t n = p.getBytesLength("fault");
    if (n && n < sizeof(lastFaultText) && p.getBytes("fault", lastFaultText, n) == n) {
        lastFaultText[n] = 0;
        if (p.getBytes("faultt", &lastFaultTime, sizeof(lastFaultTime)) != sizeof(lastFaultTime)) lastFaultTime = 0;
    }
    p.end();
}
void off(const char *reason, bool fault = false) {
    // Assert reset first: even a broken I2C bus must stop the ASIC immediately.
    digitalWrite(kReset, LOW);
    bool disabled = write8(kTps, 0x01, 0);
    bool fan = setFanDuty(63, true);
    fanPolicy.update(false, false, 100, NAN, NAN, millis()); // retain thermal hysteresis through pauses
    live.fanOverride = true; live.fanReason = "Startup or stopped ASIC";
    powered = false; diodeReady = false; live.running = false; live.frequency = 0; noWorkSince = 0;
    if (fault || !disabled || !fan) latched = true;
    live.state = !disabled || !fan ? "I2C shutdown fault; restart required" : reason;
    if (latched) saveFault(live.state);
    invalidate(); publish();
}
// The 525 MHz fallback is kept across restarts while the requested profile is
// the one that failed, at the same core voltage: each restart, update or
// power cut otherwise went back to that profile and to its failure. Choosing
// another frequency or voltage clears it.
struct SavedFallback { uint16_t mhz, mv; char reason[60]; };
char fallbackText[sizeof(SavedFallback::reason)] = "";
void saveFallback(uint16_t mhz, const char *reason) {
    SavedFallback saved{mhz, settings.asicVoltage, ""};
    snprintf(saved.reason, sizeof(saved.reason), "%s", reason);
    Preferences p;
    if (p.begin("gamma", false)) { p.putBytes("fallback", &saved, sizeof(saved)); p.end(); }
}
void loadFallback(uint16_t mhz) {
    SavedFallback saved{};
    Preferences p;
    if (!p.begin("gamma", false)) return;
    bool found = p.getBytesLength("fallback") == sizeof(saved) && p.getBytes("fallback", &saved, sizeof(saved)) == sizeof(saved);
    bool same = found && saved.mhz == mhz && saved.mv == settings.asicVoltage;
    if (found && !same) p.remove("fallback");  // another frequency or voltage was chosen since
    p.end();
    if (!same || mhz <= bm1370::kStockMHz) return;
    saved.reason[sizeof(saved.reason) - 1] = 0;
    memcpy(fallbackText, saved.reason, sizeof(fallbackText));
    forceStock = true; live.stockFallback = true; live.fallbackReason = fallbackText;
    logf("[gamma] %u MHz at %u mV failed before (%s): mining at 525 MHz; choose another frequency or voltage "
         "to clear it\n", (unsigned)mhz, (unsigned)saved.mv, fallbackText);
}
// A BM1370 that stops answering on its UART, or whose hash counter stops, is
// reset and started again, as NerdNOS does: no safety limit is involved, the
// regulator, sensor, fan and temperature interlocks are separate and still
// latch. Only a chip that keeps failing stops until the board restarts.
constexpr uint8_t kAutoRestarts = 3;
constexpr uint32_t kAutoRestartWindowMs = 60UL * 60 * 1000;
uint8_t autoRestarts = 0;
uint32_t autoRestartSince = 0;
bool autoRestart(uint32_t now) {
    if (!autoRestarts || now - autoRestartSince >= kAutoRestartWindowMs) { autoRestarts = 0; autoRestartSince = now; }
    if (autoRestarts >= kAutoRestarts) return false;
    ++autoRestarts;
    return true;
}
void instability(const char *reason) {
    if (!forceStock && live.frequency > bm1370::kStockMHz) {
        logf("[gamma] %s at %.2f MHz; falling back to 525 MHz\n", reason, live.frequency);
        forceStock=true; live.stockFallback=true; live.fallbackReason=reason;
        saveFallback(live.requestedMHz, reason);
        off("Restarting at 525 MHz (fallback)");
    } else if (autoRestart(millis())) {
        // Kept as the last fault, with every reading, for the log and the dashboard.
        saveFault(reason);
        logf("[gamma] %s at %.2f MHz: ASIC restarted automatically (%u of %u this hour)\n", reason,
             live.frequency, (unsigned)autoRestarts, (unsigned)kAutoRestarts);
        off("Restarting the ASIC");
    } else off(reason, true);
}
// A start that does not complete with the ASIC powered (the chip not answering
// its identification, a UART write refused) involves no safety limit either:
// those stop inside the start, latched, with the power already off. One such
// failure used to stop mining until someone restarted the board; it now gets
// the same bounded retries as a chip that stops answering while mining.
void initFailed() {
    const char *reason = "BM1370 initialization failed";
    if (!autoRestart(millis())) { off(reason, true); return; }
    saveFault(reason);
    logf("[gamma] %s: ASIC started again automatically (%u of %u this hour)\n", reason,
         (unsigned)autoRestarts, (unsigned)kAutoRestarts);
    off("Restarting the ASIC");
    delay(500);
}
void registerResult(const bm1370::Response &r) {
    if (!powered || r.address) return;
    uint32_t now=millis();
    if (r.reg==0x4c) { errorValue=r.value; lastErrorReply=now; errorSeen=true; return; }
    if (r.reg!=0x8c) return;
    uint64_t delta=counter.update(r.value,now);
    // Hashing the last job while the pool is away is no mining: not counted.
    if (minerAsicReady()) minerAsicHashes(delta);
    lastHashReply=now;
    if (delta) lastHashProgress=now;
    if (errorSeen && now-lastErrorReply<=2000) {
        bool unstable=errors.update(errorValue,r.value,now);
        live.hardwareErrorPercent=errors.percent; publish();
        if (unstable && live.frequency>bm1370::kStockMHz) instability("High ASIC hardware error rate");
    } else { errors.reset(); live.hardwareErrorPercent=-1; }
}
void checkWatchdogs(uint32_t now) {
    if (!powered || !live.running) return;
    if (now-lastHashReply>3000) instability("ASIC not responding; restart required");
    else if (now-lastHashProgress>5000) instability("ASIC counter stalled; restart required");
    else if (live.frequency>bm1370::kStockMHz && now-lastErrorReply>5000)
        instability("ASIC error telemetry unavailable");
}
bool send(const bm1370::Packet &p) {
    return p.size && uart.write(p.bytes.data(), p.size) == p.size;
}
bool reg(uint8_t address, uint32_t value, bool all = true) { return send(bm1370::reg(address, value, all)); }
bool request(uint8_t address, bool broadcast = true) {
    uint8_t payload[]{0, address}; return send(bm1370::command(broadcast ? 0x52 : 0x42, payload, 2));
}
bm1370::Health readings() {
    return {live.temperature, live.regulatorTemperature, live.inputVoltage,
            live.coreVoltage, live.current, live.rpm, live.regulatorStatus, coreVolts()};
}
bool updateFan(float diode) {
    uint32_t command = fanSetting.load();
    live.fanAutomatic = (command & 0x100) != 0;
    live.requestedFanPercent = uint8_t(command);
    // Use the raw diode reading: even an unconfirmed anomaly demands full
    // cooling while the existing three-reading ASIC fault filter decides.
    mining::FanDecision decision = fanPolicy.update(powered && live.running, live.fanAutomatic,
        live.requestedFanPercent, diode, live.regulatorTemperature, millis(), (command >> 16) & 255);
    live.fanOverride = decision.forced; live.fanReason = decision.reason;
    if (!setFanDuty(mining::fanPwm(decision.percent))) {
        off("Fan control I2C fault; restart required", true);
        return false;
    }
    return true;
}
bool collect(bool requireDiode) {
    lastHealthSample = millis();
    uint8_t internal, msb, lsb, tachLow, tachHigh; uint16_t vin, vout, current, vr, status;
    bool ok = read(kEmc, 0x00, &internal, 1) && read(kEmc, 0x01, &msb, 1) && read(kEmc, 0x10, &lsb, 1) &&
        read(kEmc, 0x46, &tachLow, 1) && read(kEmc, 0x47, &tachHigh, 1) && read16(kTps, 0x88, vin) && read16(kTps, 0x8b, vout) &&
        read16(kTps, 0x8c, current) && read16(kTps, 0x8d, vr) && read16(kTps, 0x79, status);
    if (!ok) {
        logf("[gamma] sensors not answering on I2C\n");
        live.temperature = NAN; off("Sensor I2C fault; restart required", true); return false;
    }
    int e = voltageMode & 31; if (e & 16) e -= 32;
    live.regulatorStatus = status;
    boardTemperature = internal < 0x80 ? float(internal) : NAN;  // two's complement; below 0 C is not a reading here
    // The ASIC's analog mux is only trustworthy while its core is powered
    // and its routing register has been initialized. Off/paused readings
    // can float to 106 C and must not latch an overheat or prevent cooldown.
    // The EMC2101 board sensor and regulator interlocks stay active then.
    float diode = powered && diodeReady ? bm1370::diodeTemperature(msb, lsb) : NAN;
    bool suspect = std::isfinite(diode) ? diode >= bm1370::kAsicMaxC : requireDiode;
    bool unconfirmed = suspect && ++diodeStrikes < kDiodeConfirm;
    if (unconfirmed) {
        logf("[gamma] ASIC temperature reading %.1f C not confirmed yet (%u of %u)\n", diode, diodeStrikes, kDiodeConfirm);
        live.temperature = lastDiode;
    } else {
        if (!suspect) { diodeStrikes = 0; lastDiode = diode; }
        live.temperature = diode;
    }
    live.regulatorTemperature = bm1370::linear11(vr);
    live.inputVoltage = bm1370::linear11(vin);
    live.coreVoltage = std::ldexp(float(vout), e);
    live.current = bm1370::linear11(current); live.rpm = bm1370::fanRpm(uint16_t(tachLow) | uint16_t(tachHigh) << 8);
    const char *fault = readings().fault(powered, requireDiode && !unconfirmed);
    if (fault) {
        // What the first boards will need to tell: every reading at the fault.
        logf("[gamma] %s: regulator status 0x%04x, input %.2f V, core %.3f V, %.1f A, ASIC %.1f C, "
             "regulator %.1f C, fan %u rpm\n", fault, live.regulatorStatus, live.inputVoltage, live.coreVoltage,
             live.current, live.temperature, live.regulatorTemperature, live.rpm);
        off(fault, true); return false;
    }
    if (!updateFan(diode)) return false;
    publish(); return true;
}
// All startup waits are short and keep the main-loop/OTA safety lease checked.
bool waitSafe(uint32_t ms, bool requireDiode = false) {
    uint32_t since = millis();
    do {
        uint8_t power = minerPowerPercent();
        if (power < 10 || !minerAsicReady()) {
            off(power && power < 10 ? "Gamma power limit must be 10-100% or paused" : "Paused / waiting for pool");
            return false;
        }
        // Respect TPS546 power-good settling; continue monitoring during long PLL ramps.
        uint32_t now = millis();
        if (powered && now - powerEnabledAt >= 250 && now - lastHealthSample >= 100 && !collect(requireDiode)) return false;
        delay(5);
    } while (millis() - since < ms);
    return true;
}
bool initAsic(const AsicWork &first) {
    activeGeneration = first.generation; activeMask = first.versionMask;
    live.requestedMHz = bm1370::validProfile(settings.asicFrequency) ? settings.asicFrequency : bm1370::kStockMHz;
    live.hardwareErrorPercent=-1; errors.reset(); errorSeen=false;
    if (!collect(false)) return false;
    // On USB alone there is no 5 V for the ASIC and its fan: wait for it.
    if (const char *wait = readings().notReady()) { live.state = wait; publish(); return false; }
    if (minerPowerPercent() < 10 || !minerAsicReady()) return false;
    // An undervoltage latched while the supply was missing must not stop this start.
    if (!clearFaults() || !write8(kTps, 0x01, 0x80)) { off("ASIC power enable failed", true); return false; }
    powered = true; diodeReady = false; powerEnabledAt = lastHealthSample = millis();
    live.state = "Initializing BM1370"; publish();
    if (!waitSafe(500)) return false;
    digitalWrite(kReset, HIGH);
    if (!waitSafe(100)) return false;
    uart.end(); uart.setRxBufferSize(4096); uart.begin(115200, SERIAL_8N1, 18, 17);
    while (uart.available()) uart.read();
    receiver.clear();
    for (int i = 0; i < 3; ++i) if (!send(bm1370::versionMask(activeMask))) return false;
    if (!request(0)) return false;
    uint32_t since = millis(); int chips = 0; bm1370::Response response;
    do {
        while (uart.available()) if (receiver.push(uart.read(), response) && !response.job && response.reg == 0) {
            if ((response.value >> 16) != 0x1370) return false;
            ++chips;
        }
        if (!waitSafe(5)) return false;
    } while (millis() - since < 1000);
    if (chips != 1) return false;
    uint8_t zero[]{0, 0};
    if (!send(bm1370::versionMask(activeMask)) || !reg(0xa8, 0x00070000) || !reg(0x18, 0xf000c100) ||
        !send(bm1370::command(0x53, zero, 2)) || !send(bm1370::command(0x40, zero, 2))) return false;
    const struct { uint8_t address; uint32_t value; bool all; } setup[]{
        {0x3c,0x80008b00,true},{0x3c,0x80008010,true},{0x14,0x000000ff,true},
        {0x54,3,true},{0x58,0x00011111,true},{0x68,0x5aa55aa5,true},
        {0xa8,0x000701f0,false},{0x18,0xf000c100,false},{0x3c,0x80008b00,false},
        {0x3c,0x80008010,false},{0x3c,0x800082aa,false},{0xb9,0x00004480,true},
        {0x54,2,true},{0xb9,0x00004480,true},{0x3c,0x80008dee,true}
    };
    for (auto &step : setup) if (!reg(step.address, step.value, step.all) || !waitSafe(10)) return false;
    // Minimum supported ASIC difficulty 16: do not lose valid pool shares by using a higher hardware filter.
    if (!send(bm1370::ticketMask(4))) return false;
    if (!reg(0x28, 0x11300200)) return false;
    uart.flush(); uart.updateBaudRate(1000000);
    // Let the external-diode conversion settle after configuring the ASIC mux.
    if (!waitSafe(100)) return false;
    diodeReady = true;
    if (!collect(true)) return false;
    // Match ESP-Miner's 6.25 MHz / 100 ms steps, with live safety checks.
    float target = targetFrequency();
    for (float f = 50; f < target; f += 6.25f) {
        if (!send(bm1370::frequency(f, &live.frequency)) || !waitSafe(100, true)) return false;
        target = targetFrequency();
    }
    if (!send(bm1370::frequency(target, &live.frequency))) return false;
    uint32_t hcn = uint32_t((4294967296.0 / 128 * 25 / live.frequency * 0.5) - 268);
    if (!reg(0x10, hcn) || !collect(true)) return false;
    counter.reset(); lastHashReply = lastHashProgress = lastErrorReply = millis();
    live.running = true; live.state = forceStock ? "Mining (525 MHz fallback profile)" : "Mining (experimental)";
    logf("[gamma] BM1370 %.2f MHz at %u mV, requested profile %u MHz%s\n", live.frequency,
         (unsigned)std::lround(coreVolts() * 1000), live.requestedMHz, forceStock ? " (stock fallback)" : "");
    publish(); return true;
}
// The regulator and sensor setup, volatile and verified; false if anything
// does not answer or hold (a regulator without its 5 V supply does not answer).
void lowDifficultyNote(double difficulty) {
    static double noted = 0;
    if (difficulty < 16.0 && difficulty != noted)
        logf("[gamma] pool difficulty %g is below 16: only shares from 16 up are sent, the pool shows a lower hashrate\n",
             difficulty);
    noted = difficulty;
}
bool configure() {
    // The regulator is kept off throughout configuration and WiFi provisioning.
    bool disabled = write8(kTps, 0x01, 0);
    uint8_t id, device[7];
    if (!read(kEmc, 0xfd, &id, 1) || (id != 0x16 && id != 0x28)) {
        if (logSetup) logf("[gamma] EMC2101 fan/temperature sensor not found\n");
        return logSetup = false;
    }
    bool emc = verify8(kEmc, 0x4a, 0x23) && setFanDuty(63, true) &&
        verify8(kEmc, 0x03, 0x04) && verify8(kEmc, 0x17, 0x24) && verify8(kEmc, 0x18, 0);
    const uint8_t prefix[]{6, 0x54, 0x49, 0x54, 0x6d, 0x24};
    if (!emc || !disabled || !read(kTps, 0xad, device, 7) || memcmp(device, prefix, 6) ||
        (device[6] != 0x41 && device[6] != 0x62) || !read(kTps, 0x20, &voltageMode, 1)) {
        if (logSetup && emc) logf("[gamma] TPS546 regulator not found (is the 5V supply plugged in?)\n");
        return logSetup = false;
    }
    // Volatile PMBus writes only, the values of ESP-Miner. Never issue
    // STORE_USER_ALL or any NVM command.
    bool tps = verify8(kTps,0x02,0x1b) && verify16(0xec,0) && verify8(kTps,0xe4,0x10) &&
        verify8(kTps,0x04,0) && linearInt(0x33,650) && linear(0x35,4.8) && linear(0x36,4.5) &&
        linear(0x55,6.5) && verify8(kTps,0x56,0xb7) && linear(0x29,0.25) &&
        voltage(0x24,2.0) && voltage(0x2b,1.0) && voltage(0x21,coreVolts()) &&
        limit(0x40,1.25) && limit(0x42,1.16) && limit(0x25,1.1) &&
        limit(0x26,0.9) && limit(0x43,0.9) && limit(0x44,0.75) &&
        // Current limits: the hardware rounds them up to 2 A steps.
        linear(0x4a,25,2) && linear(0x46,30,2) && verify8(kTps,0x47,0xc0) &&
        linearInt(0x51,105) && linearInt(0x4f,145) && verify8(kTps,0x50,0xff) &&
        linearInt(0x60,0) && linearInt(0x61,3) && linearInt(0x62,0) && verify8(kTps,0x63,0x3b) &&
        // TOFF_FALL (65h) accepts 0.5 to 31.75 ms only: the 0 ESP-Miner writes is
        // refused, so the regulator keeps its own value; left as it is here too.
        linearInt(0x64,0) && pinDetectOverride() && clearFaults();
    if (tps) logf("[gamma] regulator and sensors configured\n");
    return logSetup = tps;
}
// Pool work can disappear for a few seconds (a reconnection, a failover, a
// pool restart). Powering the ASIC off then meant its whole start again:
// 500 ms of power settling and a 77-step PLL ramp, about 9 s of
// "Initializing BM1370" and no hashing. Meanwhile the chip keeps running on
// its last job, whose results are dropped by their stale generation; it is
// only powered off once the pool stays away this long. Safety limits (power
// 0, faults, the thermal policy and its lease) still stop it at once.
constexpr uint32_t kNoWorkGraceMs = 60000;
const char *miningState = "Mining (experimental)";
uint32_t sampled = 0, sent = 0, requested = 0, setupAt = 0;
AsicWork next;
// Waiting for work while powered: true while the grace period lasts.
bool keepRunningWithoutWork(uint32_t now) {
    if (!powered) return false;
    if (!noWorkSince) {
        noWorkSince = now ? now : 1;
        live.state = "Waiting for pool"; publish();
        logf("[gamma] no pool work: ASIC kept running for up to %u s\n", (unsigned)(kNoWorkGraceMs / 1000));
    }
    if (now - noWorkSince < kNoWorkGraceMs) return true;
    off("Paused / waiting for pool");
    return false;
}
void workAvailable() {
    if (!noWorkSince) return;
    noWorkSince = 0;
    if (powered && live.running) { live.state = miningState; publish(); }
}
// One pass of the worker loop.
void step() {
    uint32_t now = millis();
    if (latched) { noWorkSince = 0; delay(100); return; }
    // A regulator without its 5 V does not answer: try again until it does.
    if (!configured) {
        if (now - setupAt >= 5000) {
            setupAt = now;
            configured = configure();
            live.state = configured ? "Ready / waiting for pool" : kNotFound; publish();
        }
        delay(100); return;
    }
    if (minerPowerPercent() > 0 && minerPowerPercent() < 10) {
        if (powered) off("Gamma power limit must be 10-100% or paused");
        live.state = "Gamma power limit must be 10-100% or paused"; publish(); delay(10); return;
    }
    // Keep sampling while paused so thermal hysteresis can observe cooldown.
    if (now - sampled >= 250) { sampled = now; if (!collect(powered)) { noWorkSince = 0; return; } }
    if (!minerPowerPercent()) {
        noWorkSince = 0;
        if (powered) off("Paused / waiting for pool");
        delay(10); return;
    }
    bool ready = minerAsicReady();
    if (!ready && !keepRunningWithoutWork(now)) { noWorkSince = 0; delay(10); return; }
    if (ready && (!powered || now - sent >= 100)) {
        // False with work available: the job board was busy for 20 ms (the
        // network task publishing a job). Not a reason to stop: try again.
        if (!minerAsicWork(next)) {
            if (!powered) { delay(10); return; }
        } else {
            workAvailable();
            // Without enough negotiated rolling bits the chip exhausts its nonce space.
            // Refuse that pool instead of displaying a fictitious stock hashrate.
            if (__builtin_popcount(next.versionMask) < 8) {
                if (powered) off("Pool needs version rolling");
                live.state = "Pool needs version rolling"; publish(); delay(100); return;
            }
            // Below difficulty 16 (a pool taking the miner for a NerdMiner) the ASIC
            // keeps mining, as in ESP-Miner: its filter stays at 16, so only those
            // shares are sent. Block odds do not change; the pool's view of the
            // hashrate is lower.
            lowDifficultyNote(next.shareDifficulty);
            if (powered && activeMask != next.versionMask) off("Changing version mask");
            // A clean job (a new block): the ASIC keeps running on the new work, as
            // in ESP-Miner. Results of earlier jobs are dropped by their generation
            // (and would fail the SHA-256 check of the job now in their slot).
            if (powered && next.generation != activeGeneration) {
                activeGeneration = next.generation;
                for (auto &slot : slots) slot.valid = false;
            }
            if (!powered) {
                if (!initAsic(next)) {
                    if (!latched && powered) initFailed();
                    else if (!powered) delay(500);  // waiting for the supply, the fan or the pool
                    return;
                }
                miningState = live.state;
                now = millis(); sampled = now;
            }
            float target = targetFrequency();
            if (std::fabs(live.frequency - target) > 3) {
                // Power changes reset the chip: no mixing of in-flight results/PLL state.
                off("Applying power limit"); return;
            }
            Slot &slot = slots[nextSlot];
            slot.work = next; slot.sent = now; slot.valid = true; slot.proofHead = slot.proofCount = 0;
            if (!send(bm1370::job(nextSlot, slot.work.header))) { off("ASIC UART write failed", true); return; }
            nextSlot = (nextSlot + 1) % bm1370::kSlots; sent = now;
        }
    }
    bm1370::Response response;
    size_t budget = 2048;
    while (powered && budget-- && uart.available()) if (receiver.push(uart.read(), response)) {
        if (!response.job) {
            registerResult(response);
            continue;
        }
        Slot &slot = slots[response.slot];
        if (!slot.valid || now - slot.sent > 1500 || !minerAsicCurrent(slot.work.generation)) continue;
        uint64_t proof = uint64_t(response.versionBits) << 32 | response.nonce;
        bool duplicate = false;
        for (int i = 0; i < slot.proofCount; ++i) if (slot.proofs[i] == proof) duplicate = true;
        if (duplicate) continue;
        slot.proofs[slot.proofHead] = proof; slot.proofHead = (slot.proofHead + 1) % 32;
        if (slot.proofCount < 32) ++slot.proofCount;
        minerAsicCandidate(slot.work, response.nonce, response.versionBits);
    }
    if (!powered) return;
    if (now - requested >= 1000) {
        requested = now;
        if (!request(0x4c, false) || !request(0x8c, false)) { off("ASIC counter request failed", true); return; }
    }
    checkWatchdogs(millis());
    delay(2);
}
void worker(void *) {
    delay(2000); // fan spin-up, ASIC held in reset and unpowered
    sampled = setupAt = millis();
    for (;;) step();
}
} // namespace

void begin() {
    pinMode(kReset, OUTPUT); digitalWrite(kReset, LOW);
    loadFault();
    if (lastFaultText[0]) logf("[gamma] last fault before this start: %s\n", lastFaultText);
    configured = configure();
    live.state = configured ? "Ready / waiting for pool" : kNotFound; publish();
    // A first reading at once: the thermal policy then has a temperature from
    // the start, without a "temperature unavailable" notification at boot.
    if (configured) collect(false);
}
bool start() {
    setFanControl(settings.fanAuto, settings.fanPercent, settings.maxTemp);
    live.fanAutomatic = settings.fanAuto; live.requestedFanPercent = settings.fanPercent;
    live.requestedMHz = bm1370::validProfile(settings.asicFrequency) ? settings.asicFrequency : bm1370::kStockMHz;
    loadFallback(live.requestedMHz);
    publish();
    // Started even when the setup failed: the task tries it again.
    bool ok = runtime::started(xTaskCreatePinnedToCore(worker, "bm1370", 8192, nullptr, 3, nullptr, 1) == pdPASS,
        runtime::Miner, "BM1370 task");
    if (!ok) off("ASIC task allocation failed", true);
    return ok;
}
void setFanControl(bool automatic, uint8_t percent, uint8_t thermalLimit) {
    fanSetting = (uint32_t(thermalLimit) << 16) | (automatic ? 0x100u : 0u) | mining::fanPercent(percent);
}
Status status() { portENTER_CRITICAL(&mux); Status out = snapshot; portEXIT_CRITICAL(&mux); return out; }
// The ASIC diode when it reads; else the board's sensor, so that a pause
// (the ASIC off, its diode silent) never looks like a lost sensor.
float temperature() {
    float asic = status().temperature;
    return std::isfinite(asic) ? asic : boardTemperature.load();
}
uint32_t lastFault(char *out, size_t size) {
    portENTER_CRITICAL(&mux);
    snprintf(out, size, "%s", lastFaultText);
    uint32_t when = lastFaultTime;
    portEXIT_CRITICAL(&mux);
    return when;
}
const char *temperatureSource() {
    if (std::isfinite(status().temperature)) return "asic";
    return std::isfinite(boardTemperature.load()) ? "board" : "unavailable";
}
} // namespace gamma601
#endif
