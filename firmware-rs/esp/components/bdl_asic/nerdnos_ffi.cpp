// src/nerdnos.cpp for the Rust firmware: its C entry points.
#include "bdl_asic.h"

#include "nerdnos.h"

extern "C" {

void bdl_nerdnos_begin(void) { nerdnos::begin(); }
bool bdl_nerdnos_start(void) { return nerdnos::start(); }

void bdl_nerdnos_status(bdl_asic_status *out) {
    nerdnos::Status s = nerdnos::status();
    *out = {};
    out->temperature = s.temperature;
    out->frequency = s.frequency;
    out->running = s.running;
    out->state = s.state;
    out->hardware_error_percent = s.hardwareErrorPercent;
    out->results = s.results;
    out->regulator_temperature = out->input_voltage = out->core_voltage = out->current = -1;
    out->fan_percent = -1;
}

float bdl_nerdnos_temperature(void) { return nerdnos::temperature(); }

}  // extern "C"
