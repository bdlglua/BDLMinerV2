// Bitaxe Gamma 601 wire protocol. Derived from bitaxeorg/ESP-Miner (GPL-3.0).
// Upstream revision and hardware references: docs/hardware/bitaxe-gamma-601.md.
#pragma once
#include "work.h"
#include <array>
#include <cstddef>
#include <cstdint>

namespace bm1370 {
constexpr uint32_t kVersionMask = 0x1fffe000;
constexpr size_t kSlots = 16;
// Profiles: 525 to 800 MHz in 25 MHz steps, and 690 (ESP-Miner's highest
// BM1370 preset; above it, the overclocking range AxeOS leaves to custom
// values, up to the 800 MHz the PLL table covers). A higher frequency needs a
// higher core voltage: ESP-Miner's BM1370 values (stock 1150 mV), and 1300 mV
// for the top profiles. recommendedMv() pairs each profile with its voltage.
constexpr uint16_t kStockMHz = 525, kMaxMHz = 800, kPllMaxMHz = 800, kEspMinerMaxMHz = 690;
inline bool validProfile(unsigned mhz) {
    return mhz == kEspMinerMaxMHz || (mhz >= kStockMHz && mhz <= kMaxMHz && (mhz - kStockMHz) % 25 == 0);
}
constexpr uint16_t kStockMv = 1150, kMinMv = 1000, kMaxMv = 1300;
constexpr uint16_t kVoltageOptions[] = {1000, 1060, 1100, 1150, 1200, 1250, 1300};
inline bool validVoltage(unsigned mv) {
    for (uint16_t option : kVoltageOptions) if (mv == option) return true;
    return false;
}
// The core voltage that goes with a profile, as Gamma 601 overclocking
// guides pair them: stock up to 600 MHz, then one 50 mV step per range.
inline uint16_t recommendedMv(unsigned mhz) {
    return mhz <= 600 ? kStockMv : mhz <= 675 ? 1200 : mhz <= 725 ? 1250 : 1300;
}
// Compare cumulative ASIC error/total counters in ten-second windows.
struct ErrorMonitor {
    uint32_t errors = 0, total = 0, at = 0;
    uint8_t badWindows = 0;
    bool ready = false;
    float percent = -1;
    void reset() { ready = false; badWindows = 0; percent = -1; }
    bool update(uint32_t errorValue, uint32_t totalValue, uint32_t now);
};
struct Packet { std::array<uint8_t, 128> bytes{}; size_t size = 0; };
uint8_t crc5(const uint8_t *data, size_t length);
Packet command(uint8_t header, const uint8_t *data, size_t length);
Packet reg(uint8_t address, uint32_t value, bool broadcast = true);
Packet job(uint8_t slot, const uint8_t header[80]);
Packet versionMask(uint32_t mask);
Packet ticketMask(uint8_t power);
Packet frequency(float mhz, float *actual = nullptr);

struct Response {
    bool job = false;
    uint8_t slot = 0, address = 0, reg = 0;
    uint32_t nonce = 0, versionBits = 0, value = 0;
};
// Incremental resynchronization and CRC validation, including fragmented UART reads.
class Receiver {
    std::array<uint8_t, 11> bytes{};
    size_t length = 0;
public:
    bool push(uint8_t byte, Response &out);
    void clear() { length = 0; }
};
float linear11(uint16_t word);
uint16_t encodeLinear11(float value);
// A whole number with the smallest exponent from 0 up, as ESP-Miner writes
// temperatures and timings (0 -> 0x0000, 105 -> 0x0069).
uint16_t encodeLinear11Int(unsigned value);
bool voltageWord(float volts, uint8_t mode, uint16_t &out);
float diodeTemperature(uint8_t msb, uint8_t lsb);
uint16_t fanRpm(uint16_t tach);
// TPS546 STATUS_WORD bits that stop the ASIC, as ESP-Miner: OFF (while
// powered), VOUT_OV, IOUT_OC, VIN_UV and TEMP. The other bits also report
// warnings, and some stay latched (OTHER), while the measured limits below
// already cover what they would tell.
// ASIC diode: the hard stop, ESP-Miner's (THROTTLE_TEMP, 75 C). And the
// regulator's, ESP-Miner's TPS546_THROTTLE_TEMP (the regulator's own OT warn
// limit, programmed at 105 C too; it shuts down at 145 C).
constexpr float kAsicMaxC = 75, kRegulatorMaxC = 105;
// Highest user thermal limit: that pause must come before the hard stop,
// which latches until a restart.
constexpr uint8_t kMaxUserTempC = uint8_t(kAsicMaxC) - 2;
constexpr uint16_t kStatusOff = 0x0040, kStatusVinUv = 0x0008;
constexpr uint16_t kStatusIoutOc = 0x0010;
constexpr uint16_t kStatusFaults = 0x0020 | kStatusIoutOc | 0x0004;  // VOUT_OV, IOUT_OC, TEMP
struct Health {
    float temperature, regulatorTemperature, inputVoltage, coreVoltage, current;
    uint16_t rpm, status;
    // The core voltage asked of the regulator; 0 (left out): the stock 1150 mV.
    // No default member initializer: the firmware's C++11 keeps Health an aggregate.
    float coreTarget;
    // A fault stops the ASIC until restart (null: none).
    const char *fault(bool powered, bool requireDiode = true) const;
    // Before power-on: why the ASIC cannot start yet (null: ready). Waiting,
    // not a fault: a Gamma on USB only has neither 5 V nor a turning fan.
    const char *notReady() const;
};
struct Counter {
    uint32_t previous = 0, at = 0;
    bool ready = false;
    void reset() { ready = false; }
    uint64_t update(uint32_t value, uint32_t now);
};
} // namespace bm1370
