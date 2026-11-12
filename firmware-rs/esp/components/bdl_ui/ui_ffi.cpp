// C functions over ui_view::View (src/ui_view.h) for the Rust firmware. The
// panel is M5GFX's own board detection on M5Stack devices (as the C++
// firmware gets it through M5Unified), or one of the C++ firmware's panels
// for the third-party boards (src/display_panels.h).
#include "bdl_ui.h"

#include <M5GFX.h>
#include <driver/gpio.h>
#include <esp_log.h>
#include <esp_rom_sys.h>
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>

#include <cmath>
#include <memory>
#include <new>
#include <string>
#include <vector>

#include "display_panels.h"
#include "ui_view.h"

namespace {

// MPU6886 on the internal I2C bus.
constexpr int kImuPort = 0;
constexpr int kImuAddress = 0x68;

lgfx::LGFX_Device *lcd = nullptr;
int panel_kind = BDL_PANEL_M5;
std::unique_ptr<ui_view::View> view;

std::string str(const char *s) { return s ? s : ""; }

// M5GFX's detection; a panel that has only just been powered (the board
// started on USB, and the power hold switched its supply on) may not answer
// yet: try again.
lgfx::LGFX_Device *m5_panel() {
    for (int attempt = 1; attempt <= 3; attempt++) {
        auto *m5 = new (std::nothrow) M5GFX();
        if (!m5) return nullptr;
        bool ok = m5->init();
        if (ok && m5->getBoard() != lgfx::boards::board_unknown && m5->width() > 0) return m5;
        ESP_LOGW("ui", "attempt %d: M5GFX %s, board %d", attempt, ok ? "started" : "failed", int(m5->getBoard()));
        if (attempt == 3 && ok && m5->width() > 0) return m5;  // draw on what M5GFX found
        delete m5;
        vTaskDelay(pdMS_TO_TICKS(200));
    }
    return nullptr;
}

#if CONFIG_IDF_TARGET_ESP32
// XPT2046 resistive touch of the ESP32-2432S028R, on its own pins, as in the
// C++ firmware (board.cpp): only "pressed or not", from its pressure (Z1)
// measurement, with no calibration and the display's SPI bus left alone.
constexpr gpio_num_t kTouchClk = GPIO_NUM_25, kTouchMosi = GPIO_NUM_32, kTouchMiso = GPIO_NUM_39,
                      kTouchCs = GPIO_NUM_33;
bool touch_ready = false;

uint16_t xpt2046(uint8_t command) {
    gpio_set_level(kTouchCs, 0);
    for (int bit = 7; bit >= 0; --bit) {
        gpio_set_level(kTouchMosi, (command >> bit) & 1);
        gpio_set_level(kTouchClk, 1);
        gpio_set_level(kTouchClk, 0);
    }
    uint16_t value = 0;
    for (int bit = 0; bit < 16; ++bit) {
        gpio_set_level(kTouchClk, 1);
        value = value << 1 | gpio_get_level(kTouchMiso);
        gpio_set_level(kTouchClk, 0);
    }
    gpio_set_level(kTouchCs, 1);
    return (value >> 3) & 0xFFF;
}

bool cyd_touched() {
    if (!touch_ready) {
        gpio_config_t out = {};
        out.pin_bit_mask = (1ULL << kTouchCs) | (1ULL << kTouchClk) | (1ULL << kTouchMosi);
        out.mode = GPIO_MODE_OUTPUT;
        gpio_set_level(kTouchCs, 1);
        gpio_config(&out);
        gpio_set_level(kTouchCs, 1);
        gpio_config_t in = {};
        in.pin_bit_mask = 1ULL << kTouchMiso;
        in.mode = GPIO_MODE_INPUT;
        gpio_config(&in);
        touch_ready = true;
    }
    return xpt2046(0xB0) > 200;  // Z1, 12 bits, pen interrupt left enabled
}
#endif

}  // namespace

extern "C" {

bool bdl_ui_begin(int panel, uint8_t rotation, uint32_t accent, uint8_t language) {
    if (view) return true;
    panel_kind = panel;
    switch (panel) {
#if CONFIG_IDF_TARGET_ESP32
    case BDL_PANEL_T_DISPLAY: lcd = new (std::nothrow) display_panels::TDisplay(); break;
    case BDL_PANEL_CYD: lcd = new (std::nothrow) display_panels::Cyd(); break;
#endif
#if CONFIG_IDF_TARGET_ESP32S3
    case BDL_PANEL_T_DISPLAY_S3: lcd = new (std::nothrow) display_panels::TDisplayS3(); break;
#endif
    case BDL_PANEL_GAMMA601: lcd = new (std::nothrow) display_panels::Gamma601(); break;
    case BDL_PANEL_M5: lcd = m5_panel(); break;
    default: lcd = nullptr; break;
    }
    if (!lcd) return false;
    if (panel != BDL_PANEL_M5 && !lcd->init()) return false;
#if CONFIG_IDF_TARGET_ESP32
    if (panel == BDL_PANEL_CYD) {
        // The CYD with two USB ports carries an ST7789 instead of the
        // ILI9341: the same commands drive it, but its colours come out
        // inverted. Its ID (RDDID, 04h: 85 85 52) tells them apart.
        uint32_t id = lcd->getPanel()->readCommand(0x04, 0, 3);
        if ((id & 0xFF) == 0x85 && (id >> 16 & 0xFF) == 0x52) {
            lcd->invertDisplay(true);
            ESP_LOGI("ui", "ST7789 display (CYD with two USB ports): colours inverted");
        }
    }
#endif
    lcd->setRotation(rotation);
    view.reset(new (std::nothrow) ui_view::View(lcd, true, accent, language));
    return view != nullptr;
}

int bdl_ui_board(void) { return lcd ? int(lcd->getBoard()) : 0; }

void bdl_ui_controls(int *button_a, int *button_b, bool *touch) {
    *button_a = *button_b = -1;
    *touch = false;
    switch (panel_kind) {
    case BDL_PANEL_T_DISPLAY: *button_a = 35; *button_b = 0; return;
    case BDL_PANEL_T_DISPLAY_S3: *button_a = 14; *button_b = 0; return;
    case BDL_PANEL_CYD: *button_a = 0; *touch = true; return;
    case BDL_PANEL_GAMMA601: *button_a = 0; return;
    default: break;
    }
    switch (lcd ? lcd->getBoard() : lgfx::boards::board_unknown) {
    case lgfx::boards::board_M5StickC:
    case lgfx::boards::board_M5StickCPlus:
    case lgfx::boards::board_M5StickCPlus2: *button_a = 37; *button_b = 39; break;
    case lgfx::boards::board_M5Stack: *button_a = 39; *button_b = 38; break;
    case lgfx::boards::board_M5AtomS3:
    case lgfx::boards::board_M5AtomS3R: *button_a = 41; break;
    case lgfx::boards::board_M5StackCore2:
    case lgfx::boards::board_M5Tough:
    case lgfx::boards::board_M5StackCoreS3:
    case lgfx::boards::board_M5StackCoreS3SE: *touch = true; break;
    default: break;
    }
}

bool bdl_ui_touched(void) {
#if CONFIG_IDF_TARGET_ESP32
    if (panel_kind == BDL_PANEL_CYD) return cyd_touched();
#endif
    if (!lcd) return false;
    lgfx::touch_point_t point;
    return lcd->getTouch(&point, 1) > 0;
}

void bdl_ui_brightness(uint8_t percent) {
    if (lcd) lcd->setBrightness(percent > 100 ? 255 : percent * 255 / 100);
}

void bdl_ui_rotation(uint8_t rotation) {
    if (!lcd || lcd->getRotation() == rotation) return;
    lcd->setRotation(rotation);
    if (view) view->invalidate();
}

void bdl_ui_invalidate(void) {
    if (view) view->invalidate();
}

void bdl_ui_render(const bdl_ui_model *m) {
    if (!view || !m) return;
    static std::vector<ui_view::Point> points;
    ui_view::Model model;
    model.page = m->page;
    model.language = m->language;
    model.accent = m->accent;
    model.statusColor = m->status_color;
    model.title = str(m->title);
    model.status = str(m->status);
    model.battery = str(m->battery);
    model.footer = str(m->footer);
    for (int i = 0; i < 6; i++) {
        model.metrics[i].label = str(m->metrics[i].label);
        model.metrics[i].value = str(m->metrics[i].value);
        model.metrics[i].detail = str(m->metrics[i].detail);
        model.metrics[i].color = m->metrics[i].color;
    }
    points.resize(m->point_count > 0 ? m->point_count : 0);
    for (size_t i = 0; i < points.size(); i++) points[i] = {m->points[i].ms, m->points[i].rate};
    model.points = points.data();
    model.pointCount = int(points.size());
    model.graphRevision = m->graph_revision;
    view->render(model);
}

void bdl_ui_message(const char *title, const char *line1, const char *line2) {
    if (view) view->message(title, line1, line2);
}

void bdl_ui_portal(const char *title, const char *ssid, const char *password, const char *qr) {
    if (view) view->portal(title, ssid, password, qr);
}

void bdl_ui_alert(const char *title, const char *value, const char *line) {
    if (view) view->alert(title, value, line);
}

int bdl_imu_begin(void) {
    if (lgfx::i2c::init(kImuPort, 21, 22).has_error()) return -1;
    auto id = lgfx::i2c::readRegister8(kImuPort, kImuAddress, 0x75);  // WHO_AM_I
    if (id.has_error()) return -1;
    // Out of sleep, internal clock.
    if (id.value() == 0x19 && lgfx::i2c::writeRegister8(kImuPort, kImuAddress, 0x6B, 0x00).has_error()) return -1;
    return id.value();
}

float bdl_imu_temperature(void) {
    uint8_t raw[2];
    if (lgfx::i2c::readRegister(kImuPort, kImuAddress, 0x41, raw, 2).has_error()) return NAN;  // TEMP_OUT_H
    return int16_t(uint16_t(raw[0]) << 8 | raw[1]) / 326.8f + 25.0f;
}

}  // extern "C"
