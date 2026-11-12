// Panels of the third-party boards with a screen (M5GFX / LovyanGFX device
// definitions), shared by the C++ firmware (board.cpp) and the Rust
// firmware's renderer component (firmware-rs/esp/components/bdl_ui).
#pragma once

#include <M5GFX.h>
#include <sdkconfig.h>
#include <lgfx/v1/panel/Panel_LCD.hpp>
#include <lgfx/v1/panel/Panel_SSD1306.hpp>
#include <lgfx/v1/panel/Panel_ST7789.hpp>

namespace display_panels {

// Bitaxe Gamma 601: SSD1306 128x32 on I2C (SDA 47, SCL 48).
class Gamma601 : public lgfx::LGFX_Device {
    lgfx::Panel_SSD1306 panel_;
    lgfx::Bus_I2C bus_;
public:
    Gamma601() {
        auto b = bus_.config(); b.i2c_port = 0; b.pin_sda = 47; b.pin_scl = 48; b.i2c_addr = 0x3c;
        b.freq_write = b.freq_read = 400000; bus_.config(b); panel_.setBus(&bus_);
        auto p = panel_.config(); p.memory_width = p.panel_width = 128;
        p.memory_height = p.panel_height = 32; p.offset_x = p.offset_y = 0; panel_.config(p);
        panel_.setComPins(0x02); setPanel(&panel_);
    }
};

#if CONFIG_IDF_TARGET_ESP32
// LilyGO TTGO T-Display (ESP32): ST7789 135x240 on VSPI (SPI3).
class TDisplay : public lgfx::LGFX_Device {
    lgfx::Panel_ST7789 panel_;
    lgfx::Bus_SPI bus_;
    lgfx::Light_PWM light_;

public:
    TDisplay() {
        auto b = bus_.config();
        b.spi_host = SPI3_HOST;  // VSPI
        b.spi_mode = 0;
        b.freq_write = 40000000;
        b.pin_sclk = 18;
        b.pin_mosi = 19;
        b.pin_miso = -1;
        b.pin_dc = 16;
        bus_.config(b);
        panel_.setBus(&bus_);

        auto p = panel_.config();
        p.pin_cs = 5;
        p.pin_rst = 23;
        p.panel_width = 135;
        p.panel_height = 240;
        p.offset_x = 52;
        p.offset_y = 40;
        p.invert = true;
        p.readable = false;
        panel_.config(p);

        auto l = light_.config();
        l.pin_bl = 4;
        l.pwm_channel = 7;
        light_.config(l);
        panel_.setLight(&light_);
        setPanel(&panel_);
    }
};
#endif

#if CONFIG_IDF_TARGET_ESP32S3
// LilyGO T-Display-S3 (also on the NerdNOS): ST7789 170x320, 8-bit parallel.
class TDisplayS3 : public lgfx::LGFX_Device {
    lgfx::Panel_ST7789 panel_;
    lgfx::Bus_Parallel8 bus_;
    lgfx::Light_PWM light_;

public:
    TDisplayS3() {
        auto b = bus_.config();
        b.freq_write = 20000000;
        b.pin_wr = 8;
        b.pin_rd = 9;
        b.pin_rs = 7;
        const int8_t data[8] = {39, 40, 41, 42, 45, 46, 47, 48};
        for (int i = 0; i < 8; i++) b.pin_data[i] = data[i];
        bus_.config(b);
        panel_.setBus(&bus_);

        auto p = panel_.config();
        p.pin_cs = 6;
        p.pin_rst = 5;
        p.panel_width = 170;
        p.panel_height = 320;
        p.offset_x = 35;
        p.offset_y = 0;
        p.invert = true;
        p.readable = false;
        panel_.config(p);

        auto l = light_.config();
        l.pin_bl = 38;
        l.pwm_channel = 7;
        light_.config(l);
        panel_.setLight(&light_);
        setPanel(&panel_);
    }
};
#endif

#if CONFIG_IDF_TARGET_ESP32
// M5GFX only ships the ILI9342C (M5Stack) driver, so the ILI9341 init
// sequence (same as Adafruit / LovyanGFX) is provided here.
struct Panel_ILI9341 : public lgfx::Panel_LCD {
    Panel_ILI9341() {
        _cfg.memory_width = _cfg.panel_width = 240;
        _cfg.memory_height = _cfg.panel_height = 320;
    }

protected:
    const uint8_t *getInitCommands(uint8_t listno) const override {
        static constexpr uint8_t list0[] = {
            0xEF, 3, 0x03, 0x80, 0x02,
            0xCF, 3, 0x00, 0xC1, 0x30,
            0xED, 4, 0x64, 0x03, 0x12, 0x81,
            0xE8, 3, 0x85, 0x00, 0x78,
            0xCB, 5, 0x39, 0x2C, 0x00, 0x34, 0x02,
            0xF7, 1, 0x20,
            0xEA, 2, 0x00, 0x00,
            0xC0, 1, 0x23,        // power control 1
            0xC1, 1, 0x10,        // power control 2
            0xC5, 2, 0x3E, 0x28,  // VCOM control 1
            0xC7, 1, 0x86,        // VCOM control 2
            0xB1, 2, 0x00, 0x18,  // frame rate
            0xB6, 3, 0x08, 0x82, 0x27,
            0xF2, 1, 0x00,        // 3-gamma off
            0x26, 1, 0x01,        // gamma curve 1
            0xE0, 15, 0x0F, 0x31, 0x2B, 0x0C, 0x0E, 0x08, 0x4E, 0xF1, 0x37, 0x07, 0x10, 0x03, 0x0E, 0x09, 0x00,
            0xE1, 15, 0x00, 0x0E, 0x14, 0x03, 0x11, 0x07, 0x31, 0xC1, 0x48, 0x08, 0x0F, 0x0C, 0x31, 0x36, 0x0F,
            CMD_SLPOUT, 0 + CMD_INIT_DELAY, 120,
            CMD_IDMOFF, 0,
            CMD_DISPON, 0 + CMD_INIT_DELAY, 20,
            0xFF, 0xFF,
        };
        return listno == 0 ? list0 : nullptr;
    }
};

// ESP32-2432S028R "Cheap Yellow Display": ILI9341 320x240 on HSPI (SPI2).
class Cyd : public lgfx::LGFX_Device {
    Panel_ILI9341 panel_;
    lgfx::Bus_SPI bus_;
    lgfx::Light_PWM light_;

public:
    Cyd() {
        auto b = bus_.config();
        b.spi_host = SPI2_HOST;  // HSPI
        b.spi_mode = 0;
        b.freq_write = 40000000;
        b.freq_read = 6000000;  // the panel ID read below: both chips read at 6.6 MHz at most
        b.pin_sclk = 14;
        b.pin_mosi = 13;
        b.pin_miso = 12;
        b.pin_dc = 2;
        bus_.config(b);
        panel_.setBus(&bus_);

        auto p = panel_.config();
        p.pin_cs = 15;
        p.pin_rst = -1;
        p.panel_width = 240;
        p.panel_height = 320;
        panel_.config(p);

        auto l = light_.config();
        l.pin_bl = 21;
        l.pwm_channel = 7;
        light_.config(l);
        panel_.setLight(&light_);
        setPanel(&panel_);
    }
};
#endif

}  // namespace display_panels
