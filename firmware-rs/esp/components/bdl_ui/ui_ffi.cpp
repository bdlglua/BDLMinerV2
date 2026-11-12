// M5GFX's board detection and panel drivers for the Rust firmware's M5Stack
// image (feature `m5stack`: Core/Basic/Gray/Fire, Core2, Tough on the ESP32;
// CoreS3, AtomS3... on the ESP32-S3). The renderer is Rust
// (bdlminer_app::ui_view); this only moves pixels, as the panels of
// src/lcd.rs do on the other boards.
#include "bdl_ui.h"

#include <M5GFX.h>
#include <esp_log.h>
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>

#include <new>

namespace {

M5GFX *lcd = nullptr;

// M5GFX's detection; a panel that has only just been powered (the board
// started on USB, and the power hold switched its supply on) may not answer
// yet: try again.
M5GFX *m5_panel() {
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

}  // namespace

extern "C" {

bool bdl_m5_begin(uint8_t rotation) {
    if (!lcd) lcd = m5_panel();
    if (!lcd) return false;
    lcd->setRotation(rotation);
    lcd->fillScreen(TFT_BLACK);
    return true;
}

int bdl_m5_width(void) { return lcd ? lcd->width() : 0; }
int bdl_m5_height(void) { return lcd ? lcd->height() : 0; }

void bdl_m5_push(int x, int y, int w, int h, const uint16_t *pixels) {
    if (!lcd) return;
    // RGB565 in the CPU's byte order.
    lcd->pushImage(x, y, w, h, (const lgfx::rgb565_t *)pixels);
}

void bdl_m5_fill(int x, int y, int w, int h, uint16_t color) {
    if (lcd) lcd->fillRect(x, y, w, h, lgfx::rgb565_t(color));
}

int bdl_m5_board(void) { return lcd ? int(lcd->getBoard()) : 0; }

void bdl_m5_controls(int *button_a, int *button_b, bool *touch) {
    *button_a = *button_b = -1;
    *touch = false;
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

bool bdl_m5_touched(void) {
    if (!lcd) return false;
    lgfx::touch_point_t point;
    return lcd->getTouch(&point, 1) > 0;
}

void bdl_m5_brightness(uint8_t value) {
    if (lcd) lcd->setBrightness(value);
}

void bdl_m5_rotation(uint8_t rotation) {
    if (lcd) lcd->setRotation(rotation);
}

}  // extern "C"
