#!/usr/bin/env python3
"""ESP32-S3 overlapped SHA kernel against a register model of an engine that
latches TEXT when a block starts, and of one that does not (boot fallback)."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
source = (root / 'src/hw_sha.cpp').read_text()
start = source.index('namespace hwsha {', source.index('#if SOC_SHA_SUPPORT_RESUME'))
body = source[start:source.index('#elif CONFIG_IDF_TARGET_ESP32')]
common = source[source.rindex('namespace hwsha {'):]
prefix = r'''
#include <atomic>
#include <cassert>
#include <cstdio>
#include <cstring>
#include <vector>
#include "hw_sha.h"
#include "core/work.h"
#define SOC_SHA_SUPPORT_RESUME 1
#define CONFIG_IDF_TARGET_ESP32S3 1
uint32_t textRegisters[16], digestRegisters[8];
size_t textWrites = 0, busyTextWrites = 0, digestWrites = 0;
int locks = 0, busyPolls = 0, fallbacks = 0;
bool lazyText = false;  // an engine reading TEXT when it finishes, not when it starts
bool pending = false, pendingStart = false;
uint32_t completedWord7 = 0;
int64_t ticks = 0;
int64_t esp_timer_get_time() { return ticks; }
void logf(const char *format, ...) { if (strstr(format, "failed")) ++fallbacks; }
constexpr int SHA2_256 = 2;  // esp_sha_type: SHA1 is 0
// ESP-IDF 5: acquiring the engine resets it (mode back to 0, SHA-1), and
// start/continue no longer write the mode: the firmware must select SHA-256.
uint32_t modeRegister = 0;
#define SHA_MODE_REG (uintptr_t(&modeRegister))
#define SHA_TEXT_BASE (uintptr_t(textRegisters))
#define SHA_H_BASE (uintptr_t(digestRegisters))
void writeRegister(volatile uint32_t *address, uint32_t value) {
    assert(locks > 0);
    uintptr_t addr = uintptr_t(address);
    if (addr == SHA_MODE_REG) { *address = value; return; }
    if (addr >= uintptr_t(textRegisters) && addr < uintptr_t(textRegisters + 16)) {
        ++textWrites;
        if (busyPolls) ++busyTextWrites;  // allowed: written while the engine computes
        ticks += busyPolls ? 0 : 1;       // overlapped with the computation
    } else {
        assert(addr >= uintptr_t(digestRegisters) && addr < uintptr_t(digestRegisters + 8));
        assert(busyPolls == 0);  // the state the engine is computing
        ++digestWrites;
        ++ticks;
    }
    *address = value;
}
#define REG_WRITE(address, value) writeRegister((volatile uint32_t *)(address), value)
uint32_t readRegister(volatile uint32_t *address) {
    assert(locks > 0 && busyPolls == 0);
    ++ticks;
    return *address;
}
#define REG_READ(address) readRegister(address)
void esp_sha_acquire_hardware() { ++locks; modeRegister = 0; }
void esp_sha_release_hardware() { assert(locks > 0); --locks; }
void compute(bool start) {
    uint32_t state[8];
    for (int i = 0; i < 8; ++i) state[i] = start ? sha::IV[i] : __builtin_bswap32(digestRegisters[i]);
    sha::transform(state, (const uint8_t *)textRegisters);
    if (modeRegister != SHA2_256) state[0] ^= 0x5a5a5a5a;  // another algorithm
    for (int i = 0; i < 8; ++i) digestRegisters[i] = __builtin_bswap32(state[i]);
}
bool sha_ll_busy() {
    if (!busyPolls) return false;
    if (--busyPolls == 0) {
        if (pending) {
            pending = false;
            digestRegisters[7] = completedWord7;
            compute(pendingStart);
        } else {
            digestRegisters[7] = completedWord7;
        }
    }
    ticks += 10;
    return true;
}
void sha_ll_fill_text_block(const void *data, size_t words) {
    for (size_t i = 0; i < words; ++i) writeRegister(textRegisters + i, ((const uint32_t *)data)[i]);
}
void sha_ll_read_digest(int, void *out, size_t words) {
    assert(locks > 0 && busyPolls == 0);
    ticks += words;
    memcpy(out, digestRegisters, words * 4);
}
void sha_ll_write_digest(int, void *data, size_t words) {
    for (size_t i = 0; i < words; ++i) writeRegister(digestRegisters + i, ((uint32_t *)data)[i]);
}
void trigger(bool start) {
    assert(locks > 0 && busyPolls == 0);
    if (lazyText) {
        completedWord7 = digestRegisters[7];
        pending = true;
        pendingStart = start;
    }
    else {
        compute(start);  // TEXT latched when the block starts
        completedWord7 = digestRegisters[7];
    }
    digestRegisters[7] = 0;  // the peripheral exposes no word 7 while BUSY
    busyPolls = 3;
}
void sha_ll_start_block(int) { trigger(true); }
void sha_ll_continue_block(int) { trigger(false); }
'''
tests = r'''
struct Hit { uint32_t nonce; uint8_t digest[32]; };
bool clobberOnHit = true;
void hit(uint32_t nonce, const uint8_t *digest, void *ctx) {
    Hit value{}; value.nonce = nonce; memcpy(value.digest, digest, 32);
    ((std::vector<Hit> *)ctx)->push_back(value);
    // A callback may use the engine (mbedTLS): the overlapped kernel writes
    // the whole next block after a hit. (The older kernels assume it does not.)
    if (clobberOnHit) for (auto &word : textRegisters) word = 0xdeadbeef;
}
int main() {
    uint32_t seed = 0x1b2c3d4e;
    for (int k = 0; k < 16; ++k) {
        uint8_t header[80];
        for (auto &byte : header) byte = (seed = seed * 1664525u + 1013904223u) >> 24;
        hwsha::Job job;
        hwsha::prepare(job, header);
        hwsha::overlappedTransfers = true;
        for (uint32_t limit : {UINT32_MAX, 0x7fffffffu, 0x1fffffffu, 0xffffu, 0u}) {
            uint32_t first = k % 2 ? UINT32_MAX - 5 : 100;
            constexpr uint32_t count = 19;
            for (auto &word : textRegisters) word = 0xa5a5a5a5;  // left by another SHA user
            textWrites = busyTextWrites = digestWrites = 0;
            std::vector<Hit> hits;
            assert(hwsha::scan(job, first, count, limit, hit, &hits) == count && locks == 0);
            // 16 to start; per nonce 10 for the second block and 10 overlapped
            // ones for the next header block; 16 again after each hit.
            assert(textWrites == 16 + 20 * count + 16 * hits.size());
            assert(busyTextWrites == 10 * count && digestWrites == 8 * count);
            size_t found = 0;
            for (uint32_t i = 0; i < count; ++i) {
                uint32_t nonce = first + i;
                for (int b = 0; b < 4; ++b) header[76 + b] = nonce >> (8 * b);
                uint8_t ref[32]; sha::sha256d(header, sizeof(header), ref);
                uint32_t top = uint32_t(ref[28]) | uint32_t(ref[29]) << 8 |
                               uint32_t(ref[30]) << 16 | uint32_t(ref[31]) << 24;
                if (top <= limit) {
                    assert(found < hits.size() && hits[found].nonce == nonce);
                    assert(!memcmp(hits[found++].digest, ref, 32));
                }
            }
            assert(found == hits.size());
        }
        hwsha::hold();
        std::vector<Hit> hits;
        assert(hwsha::scan(job, 0, 19, UINT32_MAX, hit, &hits) == 0 && hits.empty());
        hwsha::release(); assert(locks == 0);
    }
    auto boot = [] {
        hwsha::overlappedTransfers = hwsha::directTransfers = hwsha::sparseTextWrites = true;
        fallbacks = 0;
        return hwsha::selfTest();
    };
    // Latching engine: checked, timed against the others, kept when faster.
    assert(boot() && hwsha::overlappedTransfers && fallbacks == 0 && locks == 0);
    // An engine reading TEXT when it finishes gets the next header block in
    // the second hash: the self-test must catch it and keep a working kernel.
    clobberOnHit = false;
    lazyText = true;
    assert(boot() && !hwsha::overlappedTransfers && fallbacks == 1 && locks == 0);
    std::vector<Hit> hits;
    uint8_t header[80] = {1, 2, 3};
    hwsha::Job job; hwsha::prepare(job, header);
    assert(hwsha::scan(job, 0, 64, UINT32_MAX, hit, &hits) == 64);
    for (auto &h : hits) {
        for (int b = 0; b < 4; ++b) header[76 + b] = h.nonce >> (8 * b);
        uint8_t ref[32]; sha::sha256d(header, sizeof(header), ref);
        assert(!memcmp(h.digest, ref, 32));
    }
    lazyText = false;
    puts("ESP32-S3 overlapped SHA: exact digests and hits, writes during the computation, callbacks using the engine, boot check and fallback passed");
}
'''
with tempfile.TemporaryDirectory(prefix='bdl-sha-s3-') as temp:
    cpp, exe = Path(temp) / 'test.cpp', Path(temp) / 'test'
    cpp.write_text(prefix + body + common + tests)
    subprocess.run(['g++', '-std=c++17', '-O2', '-Wall', '-Isrc', str(cpp),
                    'src/core/sha256.cpp', 'src/core/work.cpp', '-o', str(exe)], cwd=root, check=True)
    subprocess.run([str(exe)], check=True)
