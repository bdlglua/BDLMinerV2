//! The short beep of a 32-bit share or a block candidate (setting `beep`),
//! as the C++ firmware's board::beep(): 4 kHz for 150 ms on the boards with
//! a buzzer or speaker driven by a pin (M5StickC Plus2: GPIO2; M5Stack
//! Core: the speaker on GPIO25). Does nothing elsewhere.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use esp_idf_svc::sys::*;

/// LEDC timer 2, channel 6: the backlight has timer 3, channel 7.
const TIMER: u32 = 2;
const CHANNEL: u32 = 6;

static READY: OnceLock<bool> = OnceLock::new();
static SOUNDING: AtomicBool = AtomicBool::new(false);

fn pin() -> Option<i32> {
    if cfg!(feature = "m5stickc-plus2") {
        return Some(2);
    }
    #[cfg(all(feature = "m5stack", not(esp32s3)))]
    {
        // M5GFX's board_M5Stack (the Core): a speaker on GPIO25. The Core2
        // and CoreS3 drive theirs over I2S.
        if crate::display::m5_board() == 2 {
            return Some(25);
        }
    }
    None
}

fn setup() -> bool {
    let Some(pin) = pin() else { return false };
    let timer = ledc_timer_config_t {
        speed_mode: ledc_mode_t_LEDC_LOW_SPEED_MODE,
        duty_resolution: ledc_timer_bit_t_LEDC_TIMER_8_BIT,
        timer_num: TIMER,
        freq_hz: 4000,
        ..unsafe { std::mem::zeroed() }
    };
    let channel = ledc_channel_config_t {
        gpio_num: pin,
        speed_mode: ledc_mode_t_LEDC_LOW_SPEED_MODE,
        channel: CHANNEL,
        timer_sel: TIMER,
        duty: 0,
        hpoint: 0,
        ..unsafe { std::mem::zeroed() }
    };
    unsafe { ledc_timer_config(&timer) == ESP_OK && ledc_channel_config(&channel) == ESP_OK }
}

fn duty(value: u32) {
    unsafe {
        ledc_set_duty(ledc_mode_t_LEDC_LOW_SPEED_MODE, CHANNEL, value);
        ledc_update_duty(ledc_mode_t_LEDC_LOW_SPEED_MODE, CHANNEL);
    }
}

/// Starts a beep; returns at once (a short thread ends it).
pub fn beep() {
    if !*READY.get_or_init(setup) || SOUNDING.swap(true, Ordering::Relaxed) {
        return;
    }
    duty(128);
    let ended = std::thread::Builder::new().stack_size(2048).spawn(|| {
        std::thread::sleep(Duration::from_millis(150));
        duty(0);
        SOUNDING.store(false, Ordering::Relaxed);
    });
    if ended.is_err() {
        duty(0);
        SOUNDING.store(false, Ordering::Relaxed);
    }
}
