// The part of Arduino-ESP32 that src/gamma601.cpp and src/nerdnos.cpp use,
// on ESP-IDF (implemented in arduino_shim.cpp), plus a String for the
// headers they include (settings.h, log.h).
#pragma once

#include <freertos/FreeRTOS.h>
#include <freertos/task.h>

#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <ctime>
#include <string>

class String : public std::string {
public:
    using std::string::string;
    using std::string::operator=;
    String() = default;
    String(const std::string &s) : std::string(s) {}
};

#define HIGH 1
#define LOW 0
#define INPUT 0x01
#define OUTPUT 0x03
#define SERIAL_8N1 0x800001c
#define ADC_11db 3

uint32_t millis();
void delay(uint32_t ms);
void pinMode(uint8_t pin, uint8_t mode);
void digitalWrite(uint8_t pin, uint8_t value);
void analogSetPinAttenuation(uint8_t pin, int attenuation);
uint32_t analogReadMilliVolts(uint8_t pin);

class HardwareSerial {
public:
    explicit HardwareSerial(int port) : port_(port) {}
    void begin(unsigned long baud, uint32_t config, int8_t rx, int8_t tx);
    void end();
    void setRxBufferSize(size_t size) { rx_size_ = size; }
    int available();
    int read();
    size_t write(const uint8_t *data, size_t size);
    void flush();
    void updateBaudRate(unsigned long baud);

private:
    int port_;
    size_t rx_size_ = 256;
    bool started_ = false;
};
