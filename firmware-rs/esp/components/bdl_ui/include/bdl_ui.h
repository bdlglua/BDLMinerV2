// M5GFX's panels for the Rust firmware's M5Stack image (see ui_ffi.cpp and
// firmware-rs/esp/src/display.rs). Call from one task only.
#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Detects the board and starts its panel. False without one.
bool bdl_m5_begin(uint8_t rotation);
int bdl_m5_width(void);
int bdl_m5_height(void);
// w * h RGB565 pixels, row-major, in the CPU's byte order.
void bdl_m5_push(int x, int y, int w, int h, const uint16_t *pixels);
void bdl_m5_fill(int x, int y, int w, int h, uint16_t color);
// M5GFX's board (lgfx::boards::board_t), 0 when unknown.
int bdl_m5_board(void);
// The board's buttons (GPIO, -1 for none; active low) and whether it has a
// touch panel.
void bdl_m5_controls(int *button_a, int *button_b, bool *touch);
bool bdl_m5_touched(void);
// 0..255.
void bdl_m5_brightness(uint8_t value);
void bdl_m5_rotation(uint8_t rotation);

#ifdef __cplusplus
}
#endif
