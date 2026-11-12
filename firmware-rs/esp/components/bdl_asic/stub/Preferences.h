// Arduino's Preferences on ESP-IDF's NVS (arduino_shim.cpp): the calls
// src/gamma601.cpp makes to keep its last fault across restarts.
#pragma once

#include <cstddef>
#include <cstdint>

class Preferences {
public:
    bool begin(const char *name, bool read_only = false);
    void end();
    size_t putBytes(const char *key, const void *value, size_t size);
    size_t getBytes(const char *key, void *buffer, size_t size);
    size_t getBytesLength(const char *key);

private:
    uint32_t handle_ = 0;
    bool open_ = false;
};
