// C functions over ui_view::View (src/ui_view.h) for the Rust firmware, on
// the M5StickC Plus2 panel. M5GFX's board detection sets it up (ST7789V2
// 135x240 on SPI2, PWM backlight on GPIO27, power hold on GPIO4), exactly as
// for the C++ firmware through M5Unified.
#include "bdl_ui.h"

#include <M5GFX.h>
#include <esp_log.h>
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>

#include <cmath>
#include <memory>
#include <new>
#include <string>
#include <vector>

#include "ui_view.h"

namespace {

// MPU6886 on the internal I2C bus.
constexpr int kImuPort = 0;
constexpr int kImuAddress = 0x68;

M5GFX *lcd = nullptr;
std::unique_ptr<ui_view::View> view;

std::string str(const char *s) { return s ? s : ""; }

}  // namespace

extern "C" {

bool bdl_ui_begin(uint8_t rotation, uint32_t accent, uint8_t language) {
    if (view) return true;
    // A panel that has only just been powered (the board started on USB, and
    // the power hold switched its supply on) may not answer yet: try again.
    for (int attempt = 1; attempt <= 3; attempt++) {
        lcd = new (std::nothrow) M5GFX();
        if (!lcd) return false;
        bool ok = lcd->init();
        if (ok && lcd->getBoard() == lgfx::boards::board_M5StickCPlus2) break;
        ESP_LOGW("ui", "attempt %d: M5GFX %s, board %d", attempt, ok ? "started" : "failed", int(lcd->getBoard()));
        if (attempt == 3) {
            if (ok) break;  // another board: draw on what M5GFX found
            delete lcd;
            lcd = nullptr;
            return false;
        }
        delete lcd;
        lcd = nullptr;
        vTaskDelay(pdMS_TO_TICKS(200));
    }
    lcd->setRotation(rotation);
    view.reset(new (std::nothrow) ui_view::View(lcd, true, accent, language));
    return view != nullptr;
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
