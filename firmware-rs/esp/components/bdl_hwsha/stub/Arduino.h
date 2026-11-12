// The little of Arduino that src/log.h and src/hw_sha.cpp use, so they build
// unchanged under plain ESP-IDF for the Rust firmware.
#pragma once

#include <math.h>
#include <stdint.h>
#include <string.h>

#include "esp_attr.h"
#include "esp_private/esp_clk.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

class String;  // only named by declarations in log.h that are never called

inline uint32_t getCpuFrequencyMhz() { return (uint32_t)(esp_clk_cpu_freq() / 1000000); }
