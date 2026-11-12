// src/gamma601.cpp for the Rust firmware: its settings and C entry points.
#include "bdl_asic.h"

#include <Arduino.h>

#include "gamma601.h"
#include "settings.h"

Settings settings;

namespace {
void copy(const gamma601::Status &s, bdl_asic_status *out) {
    *out = {};
    out->temperature = s.temperature;
    out->regulator_temperature = s.regulatorTemperature;
    out->input_voltage = s.inputVoltage;
    out->core_voltage = s.coreVoltage;
    out->current = s.current;
    out->frequency = s.frequency;
    out->rpm = s.rpm;
    out->regulator_status = s.regulatorStatus;
    out->running = s.running;
    out->state = s.state;
    out->requested_mhz = s.requestedMHz;
    out->stock_fallback = s.stockFallback;
    out->fallback_reason = s.fallbackReason;
    out->hardware_error_percent = s.hardwareErrorPercent;
    out->fan_percent = s.fanPercent;
    out->requested_fan_percent = s.requestedFanPercent;
    out->fan_automatic = s.fanAutomatic;
    out->fan_override = s.fanOverride;
    out->fan_reason = s.fanReason;
}
}  // namespace

extern "C" {

void bdl_gamma_settings(uint16_t mhz, uint16_t mv, bool fan_auto, uint8_t fan_percent, uint8_t max_temp) {
    settings.asicFrequency = mhz;
    settings.asicVoltage = mv;
    settings.fanAuto = fan_auto;
    settings.fanPercent = fan_percent;
    settings.maxTemp = max_temp;
}

// As board.cpp does first thing: the ASIC held in reset (GPIO1 low).
void bdl_gamma_hold_reset(void) {
    pinMode(1, OUTPUT);
    digitalWrite(1, LOW);
}

void bdl_gamma_begin(void) { gamma601::begin(); }
bool bdl_gamma_start(void) { return gamma601::start(); }
void bdl_gamma_fan(bool automatic, uint8_t percent, uint8_t thermal_limit) {
    gamma601::setFanControl(automatic, percent, thermal_limit);
}
void bdl_gamma_status(bdl_asic_status *out) { copy(gamma601::status(), out); }
float bdl_gamma_temperature(void) { return gamma601::temperature(); }
const char *bdl_gamma_temperature_source(void) { return gamma601::temperatureSource(); }
uint32_t bdl_gamma_last_fault(char *out, size_t size) { return gamma601::lastFault(out, size); }

}  // extern "C"
