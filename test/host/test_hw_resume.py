#!/usr/bin/env python3
"""Run the production resumable SHA loop against a register-level host model."""
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
uint32_t textRegisters[16], digestRegisters[8];
uint32_t pendingDigest[8];
size_t textWrites = 0, digestWrites = 0, directReads = 0, bufferedReads = 0;
int locks = 0;
int busyPolls = 0;
bool clobberText = false, badDirectRead = false, badEngine = false, frozenTimer = false;
bool badDirectAtFive = false;
int fallbacks = 0;
int64_t ticks = 0;
int directReadCost = 1, bufferedOverhead = 40;
int64_t esp_timer_get_time() { return frozenTimer ? 0 : ticks; }
void logf(const char *format, ...) { if (strstr(format, "failed")) ++fallbacks; }
constexpr int SHA2_256 = 2;  // esp_sha_type: SHA1 is 0
// ESP-IDF 5: acquiring the engine resets it (mode back to 0, SHA-1), and
// start/continue no longer write the mode: the firmware must select SHA-256.
uint32_t modeRegister = 0;
#define SHA_MODE_REG (uintptr_t(&modeRegister))
#define SHA_TEXT_BASE (uintptr_t(textRegisters))
#define SHA_H_BASE (uintptr_t(digestRegisters))
void writeRegister(volatile uint32_t *address, uint32_t value) {
    assert(locks > 0 && busyPolls == 0);
    uintptr_t addr = uintptr_t(address);
    if (addr == SHA_MODE_REG) { *address = value; return; }
    if (addr >= uintptr_t(textRegisters) && addr < uintptr_t(textRegisters + 16)) ++textWrites;
    else {
        assert(addr >= uintptr_t(digestRegisters) && addr < uintptr_t(digestRegisters + 8));
        ++digestWrites;
    }
    *address = value;
    ++ticks;
}
#define REG_WRITE(address, value) writeRegister((volatile uint32_t *)(address), value)
uint32_t readRegister(volatile uint32_t *address) {
    assert(locks > 0 && busyPolls == 0);
    assert(address >= digestRegisters && address < digestRegisters + 8);
    ++directReads;
    ticks += directReadCost;
    return *address ^ uint32_t(badDirectRead || (badDirectAtFive && textRegisters[3] == 5));
}
#define REG_READ(address) readRegister(address)
void esp_sha_acquire_hardware() { ++locks; modeRegister = 0; }
void esp_sha_release_hardware() { assert(locks > 0); --locks; }
bool sha_ll_busy() {
    if (!busyPolls) return false;
    if (--busyPolls) return true;
    memcpy(digestRegisters, pendingDigest, sizeof(digestRegisters));
    return false;
}
void sha_ll_fill_text_block(const void *data, size_t words) {
    for (size_t i = 0; i < words; ++i) writeRegister(textRegisters + i, ((const uint32_t *)data)[i]);
}
void sha_ll_read_digest(int, void *out, size_t words) {
    assert(locks > 0 && busyPolls == 0);
    ++bufferedReads;
    ticks += words + bufferedOverhead;  // controllable clock fixture, not a hardware speed model
    memcpy(out, digestRegisters, words * 4);
}
void sha_ll_write_digest(int, void *data, size_t words) {
    for (size_t i = 0; i < words; ++i) writeRegister(digestRegisters + i, ((uint32_t *)data)[i]);
}
void compress(bool start) {
    assert(locks > 0 && busyPolls == 0);
    uint32_t state[8];
    for (int i = 0; i < 8; ++i) state[i] = start ? sha::IV[i] : __builtin_bswap32(digestRegisters[i]);
    sha::transform(state, (const uint8_t *)textRegisters);
    if (badEngine) state[0] ^= 1;
    if (modeRegister != SHA2_256) state[0] ^= 0x5a5a5a5a;  // another algorithm
    for (int i = 0; i < 8; ++i) pendingDigest[i] = __builtin_bswap32(state[i]);
    digestRegisters[7] = 0;  // the peripheral exposes no word 7 while BUSY
    if (clobberText) for (int i = 9; i < 15; ++i) textRegisters[i] = 0xdeadbeef;
    busyPolls = 1;
}
void sha_ll_start_block(int) { compress(true); }
void sha_ll_continue_block(int) { compress(false); }
'''
tests = r'''
struct Hit { uint32_t nonce; uint8_t digest[32]; };
void hit(uint32_t nonce, const uint8_t *digest, void *ctx) {
    Hit value{}; value.nonce = nonce; memcpy(value.digest, digest, 32);
    ((std::vector<Hit> *)ctx)->push_back(value);
}
int main() {
    uint32_t seed = 0xa5b6c7d8;
    for (int k = 0; k < 16; ++k) {
        uint8_t header[80];
        for (auto &byte : header) byte = (seed = seed * 1664525u + 1013904223u) >> 24;
        hwsha::Job job;
        hwsha::prepare(job, header);
        assert(locks == 0);
        for (int mode = 0; mode < 3; ++mode) {
          hwsha::directTransfers = mode == 0;
          hwsha::sparseTextWrites = mode != 2;
          for (uint32_t limit : {UINT32_MAX, 0x7fffffffu, 0xffffu, 0u}) {
            uint32_t first = k % 2 ? UINT32_MAX - 5 : 100;
            constexpr uint32_t count = 19;
            // A different SHA user may leave nonzero padding between batches.
            for (auto &word : textRegisters) word = 0xa5a5a5a5;
            textWrites = digestWrites = directReads = bufferedReads = 0;
            std::vector<Hit> hits;
            assert(hwsha::scan(job, first, count, limit, hit, &hits) == count);
            assert(textWrites == (mode == 2 ? 32 * count : 6 + 20 * count) && locks == 0);
            assert(digestWrites == 8 * count && directReads == (mode == 0 ? 8 * count : 0));
            assert(bufferedReads == hits.size() + (mode == 0 ? 0 : count));
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
        }
        hwsha::hold();
        hwsha::hold();  // nested TLS/update reservation
        size_t writesBefore = textWrites;
        std::vector<Hit> hits;
        assert(hwsha::scan(job, 0, 19, UINT32_MAX, hit, &hits) == 0);
        assert(hits.empty() && textWrites == writesBefore);
        hwsha::release();
        assert(hwsha::scan(job, 0, 19, UINT32_MAX, hit, &hits) == 0);
        hwsha::release(); assert(locks == 0);
    }
    auto boot = [] {
        hwsha::directTransfers = hwsha::sparseTextWrites = true;
        fallbacks = 0;
        return hwsha::selfTest();
    };
    assert(boot() && hwsha::directTransfers && hwsha::sparseTextWrites && fallbacks == 0);
    // Timing fixtures check selection, without asserting a real-world speedup.
    directReadCost = 10;
    bufferedOverhead = 0;
    assert(boot() && !hwsha::directTransfers && hwsha::sparseTextWrites && fallbacks == 0);
    directReadCost = 1;  // equal modeled duration: stay buffered without >1% gain
    assert(boot() && !hwsha::directTransfers && hwsha::sparseTextWrites);
    bufferedOverhead = 40;
    frozenTimer = true;
    assert(boot() && !hwsha::directTransfers);
    frozenTimer = false;
    badDirectRead = true;
    assert(boot() && !hwsha::directTransfers && hwsha::sparseTextWrites && fallbacks == 1);
    badDirectRead = false;
    // A nonce-specific defect outside the genesis scan must also fall back.
    badDirectAtFive = true;
    assert(boot() && !hwsha::directTransfers && hwsha::sparseTextWrites && fallbacks == 1);
    badDirectAtFive = false;
    // Model a peripheral which overwrites its TEXT registers during hashing.
    // The real boot self-test must recover the existing hardware mode.
    clobberText = true;
    assert(boot() && !hwsha::directTransfers && !hwsha::sparseTextWrites && fallbacks == 2);
    assert(hwsha::selfTest() && locks == 0 && fallbacks == 2);
    clobberText = false;
    badEngine = true;
    assert(!boot() && !hwsha::directTransfers && !hwsha::sparseTextWrites && locks == 0);
    puts("Resumable SHA: three kernels, exact digests/filters, nonce wrap, busy/lock ownership, boot speed selection and correctness fallbacks passed");
}
'''
with tempfile.TemporaryDirectory(prefix='bdl-sha-resume-') as temp:
    cpp, exe = Path(temp) / 'test.cpp', Path(temp) / 'test'
    cpp.write_text(prefix + body + common + tests)
    subprocess.run(['g++', '-std=c++17', '-O2', '-Wall', '-Isrc', str(cpp),
                    'src/core/sha256.cpp', 'src/core/work.cpp', '-o', str(exe)], cwd=root, check=True)
    subprocess.run([str(exe)], check=True)
