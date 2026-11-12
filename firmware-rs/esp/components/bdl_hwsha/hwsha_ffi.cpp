// C functions over hwsha (src/hw_sha.h) for the Rust firmware, and the
// logf() that src/hw_sha.cpp logs through.
#include "bdl_hwsha.h"

#include <stdarg.h>
#include <stdio.h>

#include <new>

#include "hw_sha.h"
#include "log.h"
#include "sdkconfig.h"

namespace {
bdl_hwsha_log_fn logFn = nullptr;

hwsha::Job &job(void *p) { return *static_cast<hwsha::Job *>(p); }
hwsha::Fill &fill(void *p) { return *static_cast<hwsha::Fill *>(p); }
}  // namespace

void logf(const char *fmt, ...) {
    char line[192];
    va_list args;
    va_start(args, fmt);
    vsnprintf(line, sizeof(line), fmt, args);
    va_end(args);
    // The C++ lines end with a newline; Rust's logger adds its own.
    size_t n = strlen(line);
    if (n && line[n - 1] == '\n') line[n - 1] = 0;
    if (logFn) logFn(line);
    else printf("%s\n", line);
}

#if defined(CONFIG_IDF_TARGET_ESP32) || defined(CONFIG_IDF_TARGET_ESP32S3)
extern "C" bool sw_sha256d_prefilter(const uint32_t *mid, const uint8_t *tail, const uint32_t *bake);
#endif

extern "C" {

void bdl_hwsha_set_log(bdl_hwsha_log_fn fn) { logFn = fn; }
bool bdl_hwsha_available(void) { return hwsha::available(); }
bool bdl_hwsha_self_test(void) { return hwsha::selfTest(); }
bool bdl_hwsha_fill_available(void) { return hwsha::fillAvailable(); }
uint32_t bdl_hwsha_nonce_at(uint32_t position) { return hwsha::nonceAt(position); }

size_t bdl_hwsha_job_size(void) { return sizeof(hwsha::Job); }
size_t bdl_hwsha_fill_size(void) { return sizeof(hwsha::Fill); }
void bdl_hwsha_job_init(void *p) { new (p) hwsha::Job(); }
void bdl_hwsha_fill_init(void *p) { new (p) hwsha::Fill(); }

void bdl_hwsha_prepare(void *p, const uint8_t *header) { hwsha::prepare(job(p), header); }

void bdl_hwsha_prepare_fill(void *p, const uint8_t *header, void *ctx) {
    hwsha::prepareFill(fill(p), header);
    fill(p).ctx = ctx;
}

uint32_t bdl_hwsha_fill_take_hashes(void *p) {
    uint32_t n = fill(p).hashes;
    fill(p).hashes = 0;
    return n;
}

uint32_t bdl_hwsha_scan(void *p, uint32_t first, uint32_t count, uint32_t limit, bdl_hwsha_hit_fn hit, void *ctx,
                        void *f) {
    return hwsha::scan(job(p), first, count, limit, hit, ctx, f ? &fill(f) : nullptr);
}

void bdl_hwsha_hold(void) { hwsha::hold(); }
void bdl_hwsha_release(void) { hwsha::release(); }
void bdl_hwsha_idle(void) { hwsha::idle(); }

#if defined(CONFIG_IDF_TARGET_ESP32) || defined(CONFIG_IDF_TARGET_ESP32S3)
bool bdl_sw_prefilter_available(void) { return true; }
bool bdl_sw_prefilter(const uint32_t *mid, const uint8_t *tail, const uint32_t *bake) {
    return sw_sha256d_prefilter(mid, tail, bake);
}
#else
bool bdl_sw_prefilter_available(void) { return false; }
bool bdl_sw_prefilter(const uint32_t *, const uint8_t *, const uint32_t *) { return false; }
#endif

}  // extern "C"
