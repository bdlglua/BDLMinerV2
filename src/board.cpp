#include "board.h"
#ifdef BOARD_GAMMA601
#include "gamma601.h"
#endif
#ifdef BOARD_NERDNOS
#include "nerdnos.h"
#endif
#include <atomic>
#include "core/mining_safety.h"
#include "log.h"

#define LONG_PRESS_MS 5000

// ---------------------------------------------------------------------------
// Displays for third-party boards (M5GFX device definitions)
// ---------------------------------------------------------------------------

#if defined(DISPLAY_LGFX_GAMMA601) || defined(DISPLAY_LGFX_T_DISPLAY) || defined(DISPLAY_LGFX_T_DISPLAY_S3) || defined(DISPLAY_LGFX_CYD)
#include "display_panels.h"
#endif
#if defined(DISPLAY_LGFX_GAMMA601)
using LGFX = display_panels::Gamma601;
#elif defined(DISPLAY_LGFX_T_DISPLAY)
using LGFX = display_panels::TDisplay;
#elif defined(DISPLAY_LGFX_T_DISPLAY_S3)
using LGFX = display_panels::TDisplayS3;
#elif defined(DISPLAY_LGFX_CYD)
using LGFX = display_panels::Cyd;
#endif

// ---------------------------------------------------------------------------
// Buttons for non-M5 boards
// ---------------------------------------------------------------------------

#ifndef DISPLAY_M5UNIFIED
namespace {

struct Button {
    int pin = -1;
    bool down = false;
    bool longFired = false;
    uint32_t downAt = 0;

    void begin(int p) {
        pin = p;
        if (pin >= 0) pinMode(pin, INPUT_PULLUP);  // GPIO34-39 rely on external pull-ups
    }
    bool held() const { return pin >= 0 && digitalRead(pin) == LOW; }

    // 1 = click, 2 = long press (fired once while still held)
    int poll() {
        if (pin < 0) return 0;
        bool now = held();
        uint32_t t = millis();
        int ev = 0;
        if (now && !down) {
            downAt = t;
            longFired = false;
        } else if (now && down && !longFired && t - downAt >= LONG_PRESS_MS) {
            longFired = true;
            ev = 2;
        } else if (!now && down && !longFired && t - downAt >= 30) {
            ev = 1;
        }
        down = now;
        return ev;
    }
};

Button btnA, btnB;

}  // namespace
#endif

// A tap on the screen shows the next page, a long press changes brightness
// (touch panels: M5Stack Core2 / CoreS3 through M5Unified, CYD below).
namespace {
struct TouchGesture {
    bool down = false, held = false;
    uint32_t downAt = 0;
    // 1 = tap, 2 = long press (once, while still pressed)
    int update(bool now) {
        uint32_t t = millis();
        int ev = 0;
        if (now && !down) {
            downAt = t;
            held = false;
        } else if (now && down && !held && t - downAt >= 1000) {
            held = true;
            ev = 2;
        } else if (!now && down && !held && t - downAt >= 30) {
            ev = 1;
        }
        down = now;
        return ev;
    }
};
TouchGesture touchGesture;

#ifdef DISPLAY_LGFX_CYD
// XPT2046 resistive controller of the ESP32-2432S028R, on its own pins. Only
// "pressed or not" is needed, from its pressure (Z1) measurement: no
// calibration, and the display's SPI bus is left alone.
constexpr int kTouchClk = 25, kTouchMosi = 32, kTouchMiso = 39, kTouchCs = 33;
bool touchReady = false;

uint16_t xpt2046(uint8_t command) {
    digitalWrite(kTouchCs, LOW);
    for (int bit = 7; bit >= 0; --bit) {
        digitalWrite(kTouchMosi, (command >> bit) & 1);
        digitalWrite(kTouchClk, HIGH);
        digitalWrite(kTouchClk, LOW);
    }
    uint16_t value = 0;
    for (int bit = 0; bit < 16; ++bit) {
        digitalWrite(kTouchClk, HIGH);
        value = value << 1 | digitalRead(kTouchMiso);
        digitalWrite(kTouchClk, LOW);
    }
    digitalWrite(kTouchCs, HIGH);
    return (value >> 3) & 0xFFF;
}

bool touchPressed() {
    if (!touchReady) {
        pinMode(kTouchCs, OUTPUT);
        digitalWrite(kTouchCs, HIGH);
        pinMode(kTouchClk, OUTPUT);
        pinMode(kTouchMosi, OUTPUT);
        pinMode(kTouchMiso, INPUT);
        touchReady = true;
    }
    return xpt2046(0xB0) > 200;  // Z1, 12 bits, pen interrupt left enabled
}
#endif
}  // namespace

#ifdef DISPLAY_M5UNIFIED
#include <M5Unified.h>
#endif

namespace board {

#if defined(DISPLAY_LGFX_T_DISPLAY) || defined(DISPLAY_LGFX_T_DISPLAY_S3) || defined(DISPLAY_LGFX_CYD) || defined(DISPLAY_LGFX_GAMMA601)
static LGFX lcd;
#endif

void begin() {
#ifdef BOARD_GAMMA601
    pinMode(1, OUTPUT); digitalWrite(1, LOW);
#endif
#ifdef POWER_ON_PIN
    pinMode(POWER_ON_PIN, OUTPUT);  // T-Display-S3: keeps the panel powered on battery
    digitalWrite(POWER_ON_PIN, HIGH);
#endif
#ifdef LED_PIN
    pinMode(LED_PIN, OUTPUT);
    setLed(false);
#endif

#ifdef DISPLAY_M5UNIFIED
    auto cfg = M5.config();
    cfg.serial_baudrate = 0;  // Serial is started by main
    cfg.clear_display = true;
    cfg.internal_imu = true;  // its die temperature is the board's thermometer
    cfg.internal_mic = false;
    cfg.internal_spk = true;  // StickC Plus2: buzzer on GPIO2
    M5.begin(cfg);  // also latches the power-hold pin on the StickC Plus2
    M5.Speaker.setVolume(160);
#else
#ifdef BUTTON_A_PIN
    btnA.begin(BUTTON_A_PIN);
#endif
#ifdef BUTTON_B_PIN
    btnB.begin(BUTTON_B_PIN);
#endif
#endif

#if defined(HAS_DISPLAY) && !defined(DISPLAY_M5UNIFIED)
    lcd.init();
#ifdef DISPLAY_LGFX_CYD
    // The CYD with two USB ports carries an ST7789 instead of the ILI9341: the
    // same commands drive it, but its colours come out inverted. Its ID (RDDID,
    // 04h: 85 85 52) tells them apart.
    uint32_t id = lcd.getPanel()->readCommand(0x04, 0, 3);
    if ((id & 0xFF) == 0x85 && (id >> 16 & 0xFF) == 0x52) {
        lcd.invertDisplay(true);
        logf("[board] ST7789 display (CYD with two USB ports): colours inverted\n");
    }
#endif
#ifdef BOARD_GAMMA601
    gamma601::begin();
#endif
#ifdef BOARD_NERDNOS
    nerdnos::begin();
#endif
#endif
}

#ifdef HAS_DISPLAY
lgfx::LovyanGFX *display() {
#ifdef DISPLAY_M5UNIFIED
    // Atom Lite/Matrix, Stamp... report a zero sized display.
    return M5.Display.width() > 0 ? &M5.Display : nullptr;
#else
    return &lcd;
#endif
}
#endif

const char *modelName() {
#ifdef DISPLAY_M5UNIFIED
    switch (M5.getBoard()) {
        case m5::board_t::board_M5StickCPlus2: return "M5StickC Plus2";
        case m5::board_t::board_M5StickCPlus: return "M5StickC Plus";
        case m5::board_t::board_M5StickC: return "M5StickC";
        case m5::board_t::board_M5Stack: return "M5Stack Core";
        case m5::board_t::board_M5StackCore2: return "M5Stack Core2";
        case m5::board_t::board_M5Tough: return "M5Tough";
        case m5::board_t::board_M5StackCoreS3: return "M5Stack CoreS3";
        case m5::board_t::board_M5StackCoreS3SE: return "M5Stack CoreS3 SE";
        case m5::board_t::board_M5AtomS3: return "M5AtomS3";
        case m5::board_t::board_M5AtomS3R: return "M5AtomS3R";
        case m5::board_t::board_M5AtomS3Lite: return "M5AtomS3 Lite";
        case m5::board_t::board_M5Cardputer: return "M5Cardputer";
        case m5::board_t::board_M5CardputerADV: return "M5Cardputer ADV";
        case m5::board_t::board_M5Dial: return "M5Dial";
        case m5::board_t::board_M5DinMeter: return "M5DinMeter";
        case m5::board_t::board_M5StickS3: return "M5StickS3";
        case m5::board_t::board_M5AtomLite: return "M5Atom Lite";
        case m5::board_t::board_M5AtomMatrix: return "M5Atom Matrix";
        case m5::board_t::board_M5StampPico: return "M5Stamp Pico";
        case m5::board_t::board_M5StampS3: return "M5StampS3";
        default: return BOARD_NAME;
    }
#else
    return BOARD_NAME;
#endif
}

bool mainButtonHeld() {
#ifdef DISPLAY_M5UNIFIED
    M5.update();
    return M5.BtnA.isPressed();
#else
    return btnA.held();
#endif
}

Input poll() {
#ifdef DISPLAY_M5UNIFIED
    M5.update();
    static bool resetFired = false;
    if (M5.BtnA.pressedFor(LONG_PRESS_MS)) {
        if (!resetFired) {
            resetFired = true;
            return Input::FactoryReset;
        }
    } else if (M5.BtnA.wasReleased()) {
        bool wasLong = resetFired;
        resetFired = false;
        if (!wasLong) return Input::Next;
    }
    if (M5.BtnB.wasClicked() || M5.BtnPWR.wasClicked()) return Input::Brightness;
    if (M5.Touch.isEnabled()) {
        // Core2's touch buttons under the panel are BtnA/B/C already.
        auto touch = M5.Touch.getDetail();
        bool onScreen = touch.isPressed() && touch.y >= 0 && touch.y < M5.Display.height();
        int gesture = touchGesture.update(onScreen || (touchGesture.down && touch.isPressed()));
        if (gesture == 1) return Input::Next;
        if (gesture == 2) return Input::Brightness;
    }
    return Input::None;
#else
    int a = btnA.poll();
    if (a == 2) return Input::FactoryReset;
    if (a == 1) return Input::Next;
    if (btnB.poll() == 1) return Input::Brightness;
#ifdef DISPLAY_LGFX_CYD
    int gesture = touchGesture.update(touchPressed());
    if (gesture == 1) return Input::Next;
    if (gesture == 2) return Input::Brightness;
#endif
    return Input::None;
#endif
}

void setBrightness(uint8_t percent) {
    uint8_t v = percent * 255 / 100;
#if defined(DISPLAY_M5UNIFIED)
    M5.Display.setBrightness(v);
#elif defined(HAS_DISPLAY)
    lcd.setBrightness(v);
#else
    (void)v;
#endif
}

int batteryLevel() {
#ifdef DISPLAY_M5UNIFIED
    int level = M5.Power.getBatteryLevel();
    return level >= 0 && level <= 100 ? level : -1;
#else
    return -1;
#endif
}

void setLed(bool on) {
#ifdef LED_PIN
#ifdef LED_ACTIVE_LOW
    on = !on;
#endif
    digitalWrite(LED_PIN, on ? HIGH : LOW);
#else
    (void)on;
#endif
}

int onBattery() {
#ifdef DISPLAY_M5UNIFIED
    int vbus = M5.Power.getVBUSVoltage();  // power chips only, else negative
    if (vbus >= 0) return vbus < 4000 ? 1 : 0;
    int mv = M5.Power.getBatteryVoltage();
    if (mv <= 0) return -1;
    return mv < 4050 ? 1 : 0;
#else
    return -1;
#endif
}

bool accel(float &x, float &y) {
#ifdef DISPLAY_M5UNIFIED
    float z;
    return M5.Imu.isEnabled() && M5.Imu.getAccel(&x, &y, &z);
#else
    (void)x;
    (void)y;
    return false;
#endif
}

void flashLed(bool on) {
#if defined(DISPLAY_M5UNIFIED)
    M5.Power.setLed(on ? 255 : 0);
#else
    (void)on;  // LED_PIN already shows the pool state
#endif
}

void beep() {
#if defined(DISPLAY_M5UNIFIED)
    M5.Speaker.tone(4000, 150);
#elif defined(BUZZER_PIN)
    tone(BUZZER_PIN, 4000, 150);
#endif
}

static std::atomic<float> lastTemperature{NAN};
static mining::Thermometer thermometer;
static std::atomic<mining::TemperatureSource> temperatureSensor{mining::TemperatureSource::Unknown};

void sampleTemperature() {
#ifdef BOARD_GAMMA601
    lastTemperature = gamma601::temperature();
    return;
#endif
#ifdef BOARD_NERDNOS
    lastTemperature = nerdnos::temperature();
    return;
#endif
    float imu = NAN, chip = NAN;
#ifdef DISPLAY_M5UNIFIED
    if (thermometer.source != mining::TemperatureSource::Chip && M5.Imu.isEnabled()) {
        if (!M5.Imu.getTemp(&imu)) imu = NAN;
    }
#endif
    if (thermometer.source != mining::TemperatureSource::Imu && !isfinite(imu)) {
        chip = temperatureRead();
#if CONFIG_IDF_TARGET_ESP32
        // The original ESP32's sensor is undocumented and missing on most
        // chips, which then always read a raw 128, i.e. 53.3 C.
        if (fabsf(chip - (128 - 32) / 1.8f) < 0.01f) chip = NAN;
#endif
    }
    lastTemperature = thermometer.sample(imu, chip);
    temperatureSensor = thermometer.source;
}

float temperature() { return lastTemperature; }

const char *temperatureSource() {
#ifdef BOARD_GAMMA601
    return gamma601::temperatureSource();
#elif defined(BOARD_ASIC)
    return "asic";
#endif
    switch (temperatureSensor.load()) {
        case mining::TemperatureSource::Imu: return "imu";
        case mining::TemperatureSource::Chip: return "chip";
        default: return "unavailable";
    }
}

}  // namespace board
