// C interface to the hardware SHA-256 engine (src/hw_sha.h) for the Rust
// firmware. See firmware-rs/esp/src/hwsha.rs.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void (*bdl_hwsha_hit_fn)(uint32_t nonce, const uint8_t *hash, void *ctx);
typedef void (*bdl_hwsha_log_fn)(const char *line);

void bdl_hwsha_set_log(bdl_hwsha_log_fn fn);
bool bdl_hwsha_available(void);
bool bdl_hwsha_self_test(void);
bool bdl_hwsha_fill_available(void);
uint32_t bdl_hwsha_nonce_at(uint32_t position);

// hwsha::Job and hwsha::Fill, in caller-owned buffers of these sizes
// (aligned to 8 bytes).
size_t bdl_hwsha_job_size(void);
size_t bdl_hwsha_fill_size(void);
void bdl_hwsha_job_init(void *job);
void bdl_hwsha_fill_init(void *fill);
void bdl_hwsha_prepare(void *job, const uint8_t *header);
void bdl_hwsha_prepare_fill(void *fill, const uint8_t *header, void *ctx);
// Returns and resets the nonces the fill work did.
uint32_t bdl_hwsha_fill_take_hashes(void *fill);
uint32_t bdl_hwsha_scan(void *job, uint32_t first, uint32_t count, uint32_t limit, bdl_hwsha_hit_fn hit,
                        void *ctx, void *fill);

void bdl_hwsha_hold(void);
void bdl_hwsha_release(void);
void bdl_hwsha_idle(void);

// The assembly software prefilter (ESP32 and ESP32-S3 only): true exactly
// when the top 16 bits of the double hash are zero.
bool bdl_sw_prefilter_available(void);
bool bdl_sw_prefilter(const uint32_t *mid, const uint8_t *tail, const uint32_t *bake);

#ifdef __cplusplus
}
#endif
