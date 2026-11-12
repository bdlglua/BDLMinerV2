// Arduino calls of the ASIC drivers (stub/Arduino.h, stub/Preferences.h) on
// ESP-IDF: GPIO, UART, the ADC with its calibration, and NVS.
#include "Arduino.h"
#include "Preferences.h"

#include <driver/gpio.h>
#include <driver/uart.h>
#include <esp_adc/adc_cali.h>
#include <esp_adc/adc_cali_scheme.h>
#include <esp_adc/adc_oneshot.h>
#include <esp_timer.h>
#include <nvs.h>

uint32_t millis() { return uint32_t(esp_timer_get_time() / 1000); }

void delay(uint32_t ms) { vTaskDelay(pdMS_TO_TICKS(ms ? ms : 1)); }

void pinMode(uint8_t pin, uint8_t mode) {
    gpio_config_t config = {};
    config.pin_bit_mask = 1ULL << pin;
    config.mode = mode == OUTPUT ? GPIO_MODE_OUTPUT : GPIO_MODE_INPUT;
    gpio_config(&config);
}

void digitalWrite(uint8_t pin, uint8_t value) { gpio_set_level(gpio_num_t(pin), value ? 1 : 0); }

namespace {
adc_oneshot_unit_handle_t adc_unit = nullptr;
adc_cali_handle_t adc_cali = nullptr;
adc_atten_t atten_of[64] = {};
}  // namespace

void analogSetPinAttenuation(uint8_t pin, int attenuation) {
    if (pin < 64) atten_of[pin] = adc_atten_t(attenuation);
}

// ADC1 only (the NerdNOS NTC is on GPIO2): calibrated millivolts, as
// Arduino's analogReadMilliVolts.
uint32_t analogReadMilliVolts(uint8_t pin) {
    adc_unit_t unit;
    adc_channel_t channel;
    if (adc_oneshot_io_to_channel(pin, &unit, &channel) != ESP_OK || unit != ADC_UNIT_1) return 0;
    adc_atten_t atten = pin < 64 ? atten_of[pin] : ADC_ATTEN_DB_12;
    if (!adc_unit) {
        adc_oneshot_unit_init_cfg_t init = {};
        init.unit_id = ADC_UNIT_1;
        if (adc_oneshot_new_unit(&init, &adc_unit) != ESP_OK) return 0;
        adc_cali_curve_fitting_config_t cali = {};
        cali.unit_id = ADC_UNIT_1;
        cali.atten = atten;
        cali.bitwidth = ADC_BITWIDTH_DEFAULT;
        if (adc_cali_create_scheme_curve_fitting(&cali, &adc_cali) != ESP_OK) adc_cali = nullptr;
    }
    adc_oneshot_chan_cfg_t chan = {};
    chan.atten = atten;
    chan.bitwidth = ADC_BITWIDTH_DEFAULT;
    if (adc_oneshot_config_channel(adc_unit, channel, &chan) != ESP_OK) return 0;
    int raw = 0, mv = 0;
    if (adc_oneshot_read(adc_unit, channel, &raw) != ESP_OK) return 0;
    if (adc_cali && adc_cali_raw_to_voltage(adc_cali, raw, &mv) == ESP_OK) return uint32_t(mv);
    return uint32_t(raw) * 3100 / 4095;  // uncalibrated: 12 dB range
}

void HardwareSerial::begin(unsigned long baud, uint32_t, int8_t rx, int8_t tx) {
    end();
    uart_config_t config = {};
    config.baud_rate = int(baud);
    config.data_bits = UART_DATA_8_BITS;
    config.parity = UART_PARITY_DISABLE;
    config.stop_bits = UART_STOP_BITS_1;
    config.flow_ctrl = UART_HW_FLOWCTRL_DISABLE;
    config.source_clk = UART_SCLK_DEFAULT;
    size_t rx_size = rx_size_ < 256 ? 256 : rx_size_;
    if (uart_driver_install(uart_port_t(port_), int(rx_size), 0, 0, nullptr, 0) != ESP_OK) return;
    uart_param_config(uart_port_t(port_), &config);
    uart_set_pin(uart_port_t(port_), tx, rx, UART_PIN_NO_CHANGE, UART_PIN_NO_CHANGE);
    started_ = true;
}

void HardwareSerial::end() {
    if (started_) uart_driver_delete(uart_port_t(port_));
    started_ = false;
}

int HardwareSerial::available() {
    size_t n = 0;
    if (!started_ || uart_get_buffered_data_len(uart_port_t(port_), &n) != ESP_OK) return 0;
    return int(n);
}

int HardwareSerial::read() {
    uint8_t byte;
    if (!started_ || uart_read_bytes(uart_port_t(port_), &byte, 1, 0) != 1) return -1;
    return byte;
}

size_t HardwareSerial::write(const uint8_t *data, size_t size) {
    if (!started_) return 0;
    int n = uart_write_bytes(uart_port_t(port_), data, size);
    return n < 0 ? 0 : size_t(n);
}

void HardwareSerial::flush() {
    if (started_) uart_wait_tx_done(uart_port_t(port_), portMAX_DELAY);
}

void HardwareSerial::updateBaudRate(unsigned long baud) {
    if (started_) uart_set_baudrate(uart_port_t(port_), uint32_t(baud));
}

bool Preferences::begin(const char *name, bool read_only) {
    nvs_handle_t h;
    open_ = nvs_open(name, read_only ? NVS_READONLY : NVS_READWRITE, &h) == ESP_OK;
    handle_ = open_ ? h : 0;
    return open_;
}

void Preferences::end() {
    if (open_) {
        nvs_commit(handle_);
        nvs_close(handle_);
    }
    open_ = false;
}

size_t Preferences::putBytes(const char *key, const void *value, size_t size) {
    return open_ && nvs_set_blob(handle_, key, value, size) == ESP_OK ? size : 0;
}

size_t Preferences::getBytesLength(const char *key) {
    size_t size = 0;
    return open_ && nvs_get_blob(handle_, key, nullptr, &size) == ESP_OK ? size : 0;
}

size_t Preferences::getBytes(const char *key, void *buffer, size_t size) {
    size_t n = size;
    return open_ && nvs_get_blob(handle_, key, buffer, &n) == ESP_OK ? n : 0;
}
