// C interface to the display renderer (src/ui_view.h) for the Rust firmware.
// See firmware-rs/esp/src/display.rs. Call from one task only.
#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    const char *label, *value, *detail;
    uint32_t color;
} bdl_ui_metric;

typedef struct {
    uint32_t ms;
    float rate;
} bdl_ui_point;

// ui_view::Model.
typedef struct {
    int page;
    uint8_t language;
    uint32_t accent, status_color;
    const char *title, *status, *battery, *footer;
    bdl_ui_metric metrics[6];
    const bdl_ui_point *points;
    int point_count;
    uint32_t graph_revision;
} bdl_ui_model;

// Starts the M5StickC Plus2 panel. False when it could not be set up.
bool bdl_ui_begin(uint8_t rotation, uint32_t accent, uint8_t language);
void bdl_ui_brightness(uint8_t percent);
void bdl_ui_rotation(uint8_t rotation);
void bdl_ui_invalidate(void);
void bdl_ui_render(const bdl_ui_model *model);
void bdl_ui_message(const char *title, const char *line1, const char *line2);
void bdl_ui_portal(const char *title, const char *ssid, const char *password, const char *qr);
void bdl_ui_alert(const char *title, const char *value, const char *line);

// The MPU6886 IMU on the internal bus (SDA 21, SCL 22), read through M5GFX's
// I2C driver: ESP-IDF aborts at boot when its legacy driver (esp-idf-hal's
// I2cDriver) is linked next to the new one M5GFX uses.
// WHO_AM_I (0x19 for an MPU6886), or -1 when the bus or the chip fails.
int bdl_imu_begin(void);
// Die temperature in degrees Celsius, NaN on a bus error.
float bdl_imu_temperature(void);

#ifdef __cplusplus
}
#endif
