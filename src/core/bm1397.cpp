#include "bm1397.h"
#include <cmath>
#include <cstring>
#include "bm1370.h"  // the same CRC5
#include "sha256.h"

namespace bm1397 {
namespace {
constexpr uint8_t kJob = 0x21, kWrite = 0x51, kRead = 0x52, kInactive = 0x53, kSetAddress = 0x40;
const uint8_t kChipId[] = {0xaa, 0x55, 0x13, 0x97, 0x18, 0x00};
}  // namespace

Packet command(uint8_t header, const uint8_t *data, size_t length) {
    Packet p;
    bool job = (header & 0x20) != 0;
    size_t extra = job ? 6 : 5;
    if (length > p.bytes.size() - extra) return p;
    p.size = length + extra;
    p.bytes[0] = 0x55; p.bytes[1] = 0xaa;
    p.bytes[2] = header; p.bytes[3] = length + (job ? 4 : 3);
    if (length) std::memcpy(p.bytes.data() + 4, data, length);
    if (job) {
        uint16_t crc = 0xffff;  // CRC16-CCITT-FALSE over header, length and data
        for (size_t i = 2; i < length + 4; ++i) {
            crc ^= uint16_t(p.bytes[i]) << 8;
            for (int bit = 0; bit < 8; ++bit) crc = (crc << 1) ^ ((crc & 0x8000) ? 0x1021 : 0);
        }
        p.bytes[length + 4] = crc >> 8; p.bytes[length + 5] = crc;
    } else p.bytes[length + 4] = bm1370::crc5(p.bytes.data() + 2, length + 2);
    return p;
}
Packet reg(uint8_t address, uint32_t value) {
    uint8_t data[] = {0, address, uint8_t(value >> 24), uint8_t(value >> 16), uint8_t(value >> 8), uint8_t(value)};
    return command(kWrite, data, sizeof(data));
}
Packet readRegister(uint8_t address) { uint8_t data[] = {0, address}; return command(kRead, data, 2); }
Packet chainInactive() { uint8_t data[] = {0, 0}; return command(kInactive, data, 2); }
Packet setAddress(uint8_t address) { uint8_t data[] = {address, 0}; return command(kSetAddress, data, 2); }
Packet ticketMask(uint8_t power) {
    if (power > 31) return {};
    // The chip reads each byte of the mask bit-reversed.
    uint32_t mask = power ? (uint32_t(1) << power) - 1 : 0, reversed = 0;
    for (int byte = 0; byte < 4; ++byte)
        for (int bit = 0; bit < 8; ++bit) reversed |= ((mask >> (byte * 8 + bit)) & 1) << (byte * 8 + 7 - bit);
    return reg(0x14, reversed);
}
// misc_control: UART divider 1 (bits 8..12), with the bits the stock value keeps.
Packet maxBaud() { return reg(0x18, 0x00006131); }
Packet pllDivider() { return reg(0x70, 0x0f0f0f00); }
Packet frequency(float mhz, float *actual) {
    if (!std::isfinite(mhz)) return {};
    float f = mhz < 50 ? 50 : mhz > 650 ? 650 : mhz;
    // f * fb * fc1 * fc2 lands in 2500..6500, rounded up to a 25 MHz step.
    int fb = f >= 500 ? 1 : 2, fc1 = f <= 150 ? 3 : f <= 250 ? 2 : 1, fc2 = 5;
    float base = 25.0f * std::ceil(f * fb * fc1 * fc2 / 25.0f);
    int fa = int(base / 25.0f);
    if (fa < 0x10 || fa > 0x104) return {};
    if (actual) *actual = base / (fb * fc1 * fc2);
    return reg(0x08, uint32_t(0x40 + (fa >> 8)) << 24 | uint32_t(fa & 0xff) << 16 | uint32_t(fb) << 8 |
                         uint32_t((fc1 & 7) << 4 | (fc2 & 7)));
}

uint32_t incrementMask(uint32_t value, uint32_t mask) {
    while (mask) {
        uint32_t sum = (value & mask) + (mask & (~mask + 1));
        uint32_t overflow = sum & ~mask;
        value = (value & ~mask) | (sum & mask);
        if (!overflow) break;
        mask = overflow << 1;  // carry into the bits above
    }
    return value;
}

void midstate(const uint8_t header[80], uint32_t version, uint8_t out[32]) {
    uint8_t block[64];
    std::memcpy(block, header, 64);
    std::memcpy(block, &version, 4);
    uint32_t state[8];
    std::memcpy(state, sha::IV, sizeof(state));
    sha::transform(state, block);
    for (int w = 0; w < 8; ++w) {
        uint32_t word = state[7 - w];
        for (int b = 0; b < 4; ++b) out[4 * w + b] = uint8_t(word >> (8 * b));
    }
}

Packet job(uint8_t slot, const uint8_t header[80], uint32_t versionMask, uint32_t versions[kMidstates], uint8_t &count) {
    count = 0;
    if (slot >= kSlots) return {};
    uint32_t version;
    std::memcpy(&version, header, 4);
    count = versionMask ? kMidstates : 1;
    uint8_t payload[18 + 32 * kMidstates]{};
    payload[0] = slot * 4;  // the chip reports it back with the midstate index in bits 0..1
    payload[1] = count;
    // payload[2..5]: starting nonce 0
    std::memcpy(payload + 6, header + 72, 4);   // nbits
    std::memcpy(payload + 10, header + 68, 4);  // ntime
    std::memcpy(payload + 14, header + 64, 4);  // last 4 merkle-root bytes
    for (uint8_t i = 0; i < count; ++i) {
        versions[i] = version;
        midstate(header, version, payload + 18 + 32 * i);
        version = incrementMask(version, versionMask);
    }
    return command(kJob, payload, 18 + 32 * count);
}

bool Receiver::push(uint8_t byte, Response &out) {
    if (length == 0 && byte != 0xaa) return false;
    if (length == 1 && byte != 0x55) {
        length = byte == 0xaa ? 1 : 0;
        return false;
    }
    bytes[length++] = byte;
    if (length < bytes.size()) return false;
    length = 0;
    Response r;
    if (!std::memcmp(bytes.data(), kChipId, sizeof(kChipId))) {
        r.chipId = true;
    } else {
        // Job ids are 0, 4 ... 124. With bit 7 set (a corrupted byte: the CRC
        // is not checked) the slot would be 32..63, past the backend's table.
        if (bytes[7] & 0x80) return false;
        std::memcpy(&r.nonce, bytes.data() + 2, 4);
        r.slot = (bytes[7] & 0xfc) >> 2;
        r.midstate = bytes[7] & 0x03;
    }
    out = r;
    return true;
}
}  // namespace bm1397
