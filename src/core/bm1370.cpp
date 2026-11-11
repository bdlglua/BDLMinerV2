#include "bm1370.h"
#include <algorithm>
#include <cmath>
#include <cstring>
#include <limits>

namespace bm1370 {
uint8_t crc5(const uint8_t *data, size_t length) {
    uint8_t crc = 31;
    for (size_t i = 0; i < length; ++i) for (int bit = 7; bit >= 0; --bit) {
        uint8_t next = ((crc >> 4) ^ (data[i] >> bit)) & 1;
        crc = ((crc << 1) | next) ^ (next << 2);
        crc &= 31;
    }
    return crc;
}
Packet command(uint8_t type, const uint8_t *data, size_t length) {
    Packet p;
    bool job = (type & 0x20) != 0;
    size_t extra = job ? 6 : 5;
    if (length > p.bytes.size() - extra) return p;
    p.size = length + extra;
    p.bytes[0] = 0x55; p.bytes[1] = 0xaa;
    p.bytes[2] = type; p.bytes[3] = length + (job ? 4 : 3);
    if (length) std::memcpy(p.bytes.data() + 4, data, length);
    if (job) {
        uint16_t crc = 0xffff;
        for (size_t i = 2; i < length + 4; ++i) {
            crc ^= uint16_t(p.bytes[i]) << 8;
            for (int bit = 0; bit < 8; ++bit) crc = (crc << 1) ^ ((crc & 0x8000) ? 0x1021 : 0);
        }
        p.bytes[length + 4] = crc >> 8; p.bytes[length + 5] = crc;
    } else p.bytes[length + 4] = crc5(p.bytes.data() + 2, length + 2);
    return p;
}
Packet reg(uint8_t address, uint32_t value, bool broadcast) {
    uint8_t data[] = {0, address, uint8_t(value >> 24), uint8_t(value >> 16), uint8_t(value >> 8), uint8_t(value)};
    return command(broadcast ? 0x51 : 0x41, data, sizeof(data));
}
Packet versionMask(uint32_t mask) { return reg(0xa4, 0x90000000 | ((mask & kVersionMask) >> 13)); }
Packet ticketMask(uint8_t power) {
    if (power > 31) return {};
    uint32_t mask = power ? (uint32_t(1) << power) - 1 : 0;
    uint32_t reversed = 0;
    for (int byte = 0; byte < 4; ++byte)
        for (int bit = 0; bit < 8; ++bit) reversed |= ((mask >> (byte * 8 + bit)) & 1) << (byte * 8 + 7 - bit);
    return reg(0x14, reversed);
}
Packet job(uint8_t slot, const uint8_t header[80]) {
    if (slot >= kSlots) return {};
    uint8_t payload[82]{};
    payload[0] = slot << 3; payload[1] = 1;
    std::memcpy(payload + 2, header + 76, 4);
    std::memcpy(payload + 6, header + 72, 4);
    std::memcpy(payload + 10, header + 68, 4);
    for (int w = 0; w < 8; ++w) {
        std::memcpy(payload + 14 + 4 * w, header + 36 + 4 * (7 - w), 4);
        std::memcpy(payload + 46 + 4 * w, header + 4 + 4 * (7 - w), 4);
    }
    std::memcpy(payload + 78, header, 4);
    return command(0x21, payload, sizeof(payload));
}
Packet frequency(float target, float *actual) {
    if (!std::isfinite(target) || target < 50 || target > kPllMaxMHz) return {};
    float best = 0, error = std::numeric_limits<float>::max(), vcoBest = error;
    int productBest = 65535, fbBest = 0, refBest = 0, p1Best = 0, p2Best = 0;
    for (int ref = 2; ref >= 1; --ref) for (int p1 = 7; p1 >= 1; --p1) for (int p2 = 7; p2 >= 1; --p2) {
        int fb = std::lround(target / 25 * ref * p1 * p2);
        if (p1 <= p2 || fb < 160 || fb > 239) continue;
        float f = 25.0f * fb / (ref * p1 * p2), e = std::fabs(f - target), vco = 25.0f * fb / ref;
        if (e < error || (std::fabs(e - error) < 0.0001f &&
            (vco < vcoBest || (std::fabs(vco - vcoBest) < 0.0001f && p1 * p2 < productBest)))) {
            best = f; error = e; vcoBest = vco; productBest = p1 * p2;
            fbBest = fb; refBest = ref; p1Best = p1; p2Best = p2;
        }
    }
    if (!fbBest) return {};
    if (actual) *actual = best;
    return reg(0x08, (uint32_t(vcoBest >= 2400 ? 0x50 : 0x40) << 24) |
        uint32_t(fbBest) << 16 | uint32_t(refBest) << 8 | ((p1Best - 1) << 4) | (p2Best - 1));
}
bool Receiver::push(uint8_t byte, Response &out) {
    if (length == bytes.size()) length = 0;
    bytes[length++] = byte;
    while (length && (bytes[0] != 0xaa || (length > 1 && bytes[1] != 0x55))) {
        std::memmove(bytes.data(), bytes.data() + 1, --length);
    }
    if (length != bytes.size()) return false;
    if (crc5(bytes.data() + 2, 9) != 0) {
        std::memmove(bytes.data(), bytes.data() + 1, --length);
        return false;
    }
    out = {};
    out.job = (bytes[10] & 0x80) != 0;
    out.slot = bytes[7] >> 4;
    out.nonce = uint32_t(bytes[2]) | uint32_t(bytes[3]) << 8 | uint32_t(bytes[4]) << 16 | uint32_t(bytes[5]) << 24;
    out.versionBits = (uint32_t(bytes[8]) << 8 | bytes[9]) << 13;
    out.value = uint32_t(bytes[2]) << 24 | uint32_t(bytes[3]) << 16 | uint32_t(bytes[4]) << 8 | bytes[5];
    out.address = bytes[6]; out.reg = bytes[7]; length = 0;
    return true;
}
float linear11(uint16_t word) {
    int exponent = (word >> 11) & 31, mantissa = word & 2047;
    if (exponent & 16) exponent -= 32;
    if (mantissa & 1024) mantissa -= 2048;
    return std::ldexp(float(mantissa), exponent);
}
uint16_t encodeLinear11(float value) {
    for (int e = -16; e <= 15; ++e) {
        float m = std::round(std::ldexp(value, -e));
        if (std::isfinite(m) && m >= -1024 && m <= 1023)
            return ((e & 31) << 11) | (int(m) & 2047);
    }
    return 0;
}
uint16_t encodeLinear11Int(unsigned value) {
    for (int e = 0; e <= 15; ++e)
        if ((value >> e) < 1024) return uint16_t(e << 11 | (value >> e));
    return 0;
}
bool voltageWord(float volts, uint8_t mode, uint16_t &out) {
    int e = mode & 31; if (e & 16) e -= 32;
    // Bits 6:5 must be 00 (linear); bit 7 (REL, relative limits, the TPS546's
    // default) does not change the encoding.
    if ((mode & 0x60) || e >= 0 || e < -16 || !std::isfinite(volts) || volts < 0.7f || volts > 2.0f) return false;
    float word = std::round(std::ldexp(volts, -e));
    if (word < 0 || word > 65535) return false;
    out = uint16_t(word); return true;
}
float diodeTemperature(uint8_t msb, uint8_t lsb) {
    int value = (uint16_t(msb) << 8 | lsb) >> 5;
    if (value == 0x3f8 || value == 0x3ff) return NAN;
    if (value & 0x400) value -= 0x800;
    float t = value / 8.0f;
    return t >= 0 && t <= 120 ? t : NAN;
}
uint16_t fanRpm(uint16_t tach) { return tach < 83 || tach >= 0xfff0 ? 0 : 5400000UL / tach; }
const char *Health::fault(bool powered, bool requireDiode) const {
    if (requireDiode && !std::isfinite(temperature)) return "ASIC temperature sensor fault; restart required";
    if (std::isfinite(temperature) && temperature >= kAsicMaxC) return "ASIC overtemperature; restart required";
    if (!std::isfinite(regulatorTemperature) || regulatorTemperature < -20 || regulatorTemperature >= kRegulatorMaxC)
        return "Regulator temperature fault; restart required";
    // Mining needs 4.8 V to start (notReady); a 5 V supply then sags under
    // load, and the regulator itself stops at 4.5 V (VIN_OFF).
    if (!std::isfinite(inputVoltage) || inputVoltage > 5.5f || (powered && inputVoltage < 4.6f))
        return "5V supply fault; restart required";
    if (powered && rpm < 500) return "Fan stopped; restart required";
    if (!std::isfinite(current) || current < -1 || current >= 30) return "ASIC current fault; restart required";
    // A hardware fault can already have disabled the output: diagnose that
    // status before its resulting near-zero core voltage masks the cause.
    if (status & kStatusIoutOc) return "Regulator output overcurrent; restart required";
    if (status & (powered ? kStatusFaults | kStatusOff | kStatusVinUv : kStatusFaults))
        return "Regulator reports a fault; restart required";
    // Around the chosen voltage: 1.00-1.25 V at the stock 1150 mV (0.5 mV for float rounding).
    float target = coreTarget > 0 ? coreTarget : kStockMv / 1000.0f;
    if (powered && (!std::isfinite(coreVoltage) || coreVoltage < target - 0.1505f || coreVoltage > target + 0.1005f))
        return "ASIC voltage fault; restart required";
    return nullptr;
}
const char *Health::notReady() const {
    if (!std::isfinite(inputVoltage) || inputVoltage < 4.8f) return "Waiting for the 5V supply (barrel jack)";
    if (rpm < 500) return "Waiting for the fan to turn";
    return nullptr;
}
bool ErrorMonitor::update(uint32_t errorValue, uint32_t totalValue, uint32_t now) {
    if (!ready) { errors=errorValue; total=totalValue; at=now; ready=true; return false; }
    uint32_t elapsed=now-at;
    if (elapsed<10000) return false;
    uint32_t e=errorValue-errors, t=totalValue-total;
    errors=errorValue; total=totalValue; at=now;
    if (elapsed>15000 || !t || uint64_t(t)>uint64_t(elapsed)) { percent=-1; badWindows=0; return false; }
    percent=100.0f*double(e)/t;
    badWindows=percent>=2 ? uint8_t(badWindows<3?badWindows+1:3) : 0;
    return badWindows>=3;
}
uint64_t Counter::update(uint32_t value, uint32_t now) {
    uint32_t elapsed = now - at, delta = value - previous;
    bool valid = ready && elapsed > 0 && elapsed <= 5000 && uint64_t(delta) * 1000 <= uint64_t(elapsed) * 1000;
    previous = value; at = now; ready = true;
    // Hardware counter units are 2^32 hashes. Reject resets/out-of-range bursts.
    return valid ? uint64_t(delta) << 32 : 0;
}
} // namespace bm1370
