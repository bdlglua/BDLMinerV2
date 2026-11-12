// The C++ firmware's ASIC drivers for the Rust firmware (src/asic.rs).
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Implemented in Rust (src/asic.rs): the miner the drivers report to.
void bdl_asic_rs_log(const char *line);
uint8_t bdl_asic_rs_power_percent(void);
bool bdl_asic_rs_ready(void);
// Fills the 80-byte header (its rolling bits cleared) and the work's fields.
bool bdl_asic_rs_work(uint8_t *header, uint32_t *version_mask, double *share_difficulty, uint32_t *generation,
                      uint32_t *height);
bool bdl_asic_rs_current(uint32_t generation);
// Difficulty of the result, or -1 for stale or unknown work.
double bdl_asic_rs_candidate(const uint8_t *header, uint32_t generation, uint32_t nonce, uint32_t version_bits,
                             double minimum);
void bdl_asic_rs_hashes(uint64_t hashes);

// What the drivers show (gamma601::Status / nerdnos::Status).
typedef struct {
    float temperature, regulator_temperature, input_voltage, core_voltage, current, frequency;
    uint16_t rpm, regulator_status;
    bool running;
    const char *state;
    uint16_t requested_mhz;
    bool stock_fallback;
    const char *fallback_reason;
    float hardware_error_percent;
    int16_t fan_percent;
    uint8_t requested_fan_percent;
    bool fan_automatic, fan_override;
    const char *fan_reason;
    uint32_t results;
} bdl_asic_status;

// Bitaxe Gamma 601 (src/gamma601.cpp). Settings first; begin() with the
// shared I2C bus up (the SSD1306 started).
void bdl_gamma_settings(uint16_t mhz, uint16_t mv, bool fan_auto, uint8_t fan_percent, uint8_t max_temp);
void bdl_gamma_hold_reset(void);
void bdl_gamma_begin(void);
bool bdl_gamma_start(void);
void bdl_gamma_fan(bool automatic, uint8_t percent, uint8_t thermal_limit);
void bdl_gamma_status(bdl_asic_status *out);
float bdl_gamma_temperature(void);
const char *bdl_gamma_temperature_source(void);
uint32_t bdl_gamma_last_fault(char *out, size_t size);

// NerdNOS (src/nerdnos.cpp).
void bdl_nerdnos_begin(void);
bool bdl_nerdnos_start(void);
void bdl_nerdnos_status(bdl_asic_status *out);
float bdl_nerdnos_temperature(void);

#ifdef __cplusplus
}
#endif
