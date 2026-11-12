//! ESP32-C3: C functions that std and its dependencies refer to weakly and
//! that ESP-IDF's newlib does not provide. Undefined weak symbols are reached
//! through a GOT, and a .got section breaks ESP-IDF's flash layout ("The gap
//! between .flash.rodata and .eh_frame_hdr must not exist"); defined here,
//! inside the one LTO module, they are called directly.

use std::ffi::{c_char, c_int, c_void};

use esp_idf_svc::sys::{time_t, tm};

/// Never called back: the firmware never exits.
#[no_mangle]
extern "C" fn atexit(_f: Option<unsafe extern "C" fn()>) -> c_int {
    0
}

/// No fork on ESP-IDF.
#[no_mangle]
extern "C" fn pthread_atfork(
    _prepare: Option<unsafe extern "C" fn()>,
    _parent: Option<unsafe extern "C" fn()>,
    _child: Option<unsafe extern "C" fn()>,
) -> c_int {
    0
}

/// No symbolic links on ESP-IDF's file systems: `stat`.
#[no_mangle]
unsafe extern "C" fn lstat(path: *const c_char, buf: *mut c_void) -> c_int {
    esp_idf_svc::sys::stat(path, buf as *mut esp_idf_svc::sys::stat)
}

/// `tm` in UTC to seconds since 1970.
#[no_mangle]
unsafe extern "C" fn timegm(t: *mut tm) -> time_t {
    let t = &*t;
    let year = t.tm_year as i64 + 1900 + (t.tm_mon as i64).div_euclid(12);
    let month = (t.tm_mon as i64).rem_euclid(12) + 1;
    // Days from civil (Howard Hinnant).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + t.tm_mday as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    (days * 86_400 + t.tm_hour as i64 * 3600 + t.tm_min as i64 * 60 + t.tm_sec as i64) as time_t
}
