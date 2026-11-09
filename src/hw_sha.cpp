#include "hw_sha.h"

#include <string.h>

#include <atomic>

#include "core/sha256.h"
#include "core/mining_safety.h"
#include "core/work.h"
#include "log.h"
#include "sdkconfig.h"
#include "soc/soc_caps.h"

#if SOC_SHA_SUPPORT_RESUME

#include "esp_timer.h"
#include "hal/sha_ll.h"
#include "sha/sha_dma.h"

namespace hwsha {

bool available() { return true; }

uint32_t nonceAt(uint32_t position) { return position; }

// While held, the miner starts no hardware batch (it sees `held` and hashes
// in software) and mbedtls takes the engine through its own lock, waiting at
// most for the batch in progress. hold() must not take that lock itself:
// ESP-IDF 5 made the SHA/AES lock non-recursive, so mbedtls, taking it again
// from the same task during the handshake, would wait forever (the TLS task,
// then every SHA user after it, such as the update's image check).
static std::atomic<bool> held{false};
static std::atomic<int> holds{0};  // hold() nests (TLS tasks may overlap)
static bool sparseTextWrites = true;  // selected by selfTest(), before workers start
static bool directTransfers = true;   // correctness and speed checked before workers start
#if CONFIG_IDF_TARGET_ESP32S3
static bool overlappedTransfers = true;  // S3 only: checked and timed at boot like the others
#endif

// ESP-IDF 5 resets the engine each time it is acquired, and its start and
// continue helpers no longer write the mode register (sha_ll_set_mode() does,
// separately): left at 0 after the reset, the engine computed SHA-1, every
// boot self-test failed and the S2, S3 and C3 mined in software only (2.5.0:
// 66 instead of 288 kH/s on an S3). Select SHA-256 after every acquire.
static inline void acquire() {
    esp_sha_acquire_hardware();
    REG_WRITE(SHA_MODE_REG, SHA2_256);
}

void hold() {
    holds++;
    held = true;
}

void release() {
    if (--holds == 0) held = false;
}

void idle() {}

static inline void waitIdle() {
    while (sha_ll_busy()) {
    }
}

static inline uint32_t waitHashWord7(volatile uint32_t *word7) {
    for (;;) {
        for (unsigned probe = 0; probe < 32; ++probe) {
            uint32_t value = *word7;
            if (value != 0) return value;
        }
        // A completed hash can legitimately have a zero most-significant word.
        if (!sha_ll_busy()) return *word7;
    }
}

void prepare(Job &job, const uint8_t header[80]) {
    // The engine works on raw message bytes, so the header is copied as-is.
    memcpy(job.tail, header + 64, 16);
    memset(&job.tail[4], 0, 48);
    ((uint8_t *)job.tail)[16] = 0x80;
    ((uint8_t *)job.tail)[62] = 0x02;  // 640 bits, big-endian length
    ((uint8_t *)job.tail)[63] = 0x80;

    acquire();
    sha_ll_fill_text_block(header, 16);
    sha_ll_start_block(SHA2_256);
    waitIdle();
    sha_ll_read_digest(SHA2_256, job.mid, 8);
    esp_sha_release_hardware();
}

bool fillAvailable() { return false; }
void prepareFill(Fill &, const uint8_t *) {}

// S2/S3/C3 expose digest and TEXT in the same byte order. Unlike the classic
// ESP32 these targets do not need the DPORT read workaround. Keep the SDK's
// volatile register accesses, but avoid an intermediate RAM digest and the
// out-of-line read-buffer call on S2/S3. Fixed-size transfers also avoid a
// pointer/count loop for every midstate and header tail.
template<unsigned words>
static inline __attribute__((always_inline)) void writeWords(volatile uint32_t *dest, const uint32_t *src) {
#pragma GCC unroll 16
    for (unsigned i = 0; i < words; ++i) REG_WRITE(dest + i, src[i]);
}

static inline __attribute__((always_inline)) void digestToText(volatile uint32_t *text,
                                                             volatile uint32_t *digest) {
#pragma GCC unroll 8
    for (unsigned i = 0; i < 8; ++i) REG_WRITE(text + i, REG_READ(digest + i));
}

template<bool sparse, bool direct = false>
static __attribute__((noinline)) uint32_t scanWithPadding(
    Job &job, uint32_t first, uint32_t count, uint32_t limit,
    void (*hit)(uint32_t, const uint8_t *, void *), void *ctx) {
    uint32_t second[8];  // only the first digest changes; padding stays in the peripheral
    volatile uint32_t *text = (volatile uint32_t *)SHA_TEXT_BASE;
    volatile uint32_t *digest = (volatile uint32_t *)SHA_H_BASE;
    volatile uint32_t *h7 = (volatile uint32_t *)(SHA_H_BASE + 28);

    if (held) return 0;
    acquire();
    // Words 9..14 are zero in both padded blocks. The peripheral reads TEXT
    // without changing it; initialize these once while owning the engine.
    // Reinitialize for every batch because TLS may have used it in between.
    if (sparse) for (int i = 9; i < 15; ++i) REG_WRITE(text + i, 0);
    uint32_t nonce = first;
    for (uint32_t i = 0; i < count; i++, nonce++) {
        job.tail[3] = nonce;
        if (direct) {
            writeWords<8>(digest, job.mid);
            writeWords<9>(text, job.tail);
        } else {
            sha_ll_write_digest(SHA2_256, job.mid, 8);
            sha_ll_fill_text_block(job.tail, sparse ? 9 : 16);
        }
        if (sparse) REG_WRITE(text + 15, job.tail[15]);
        sha_ll_continue_block(SHA2_256);
        waitIdle();
        if (direct) {
            digestToText(text, digest);
        } else {
            sha_ll_read_digest(SHA2_256, second, 8);
            sha_ll_fill_text_block(second, 8);
        }
        REG_WRITE(text + 8, 0x00000080);   // padding byte 0x80
        if (!sparse) for (int i = 9; i < 15; ++i) REG_WRITE(text + i, 0);
        REG_WRITE(text + 15, 0x00010000);  // 256 bits, big-endian length
        sha_ll_start_block(SHA2_256);
        waitIdle();
        // Bytes 28..31 of the digest, read as a little-endian word, are the
        // most significant 32 bits of the hash as a 256-bit number.
        if (*h7 <= limit) {
            uint8_t hash[32];
            sha_ll_read_digest(SHA2_256, hash, 8);
            hit(nonce, hash, ctx);
        }
    }
    esp_sha_release_hardware();
    return count;
}

#if CONFIG_IDF_TARGET_ESP32S3
// ESP32-S3: every register access crosses the slow peripheral bus, which
// costs more than the block computation itself. The engine latches TEXT when
// a block starts (measured on the chip by NerdMiner Modded, MIT, its
// PIPELINED_S3_MINING loop: ~740 instead of ~1080 cycles per nonce), so the
// next nonce's header block is written while the second hash computes, and
// only its words that the second block changed: 0..8 and 15 (9..14 stay 0).
// After a hit the whole block is written again, whatever the callback did.
// selfTest() checks every digest and the exact set of hits, then times it
// against the other kernels; any failure falls back to them.
static __attribute__((noinline)) uint32_t scanOverlapped(
    Job &job, uint32_t first, uint32_t count, uint32_t limit,
    void (*hit)(uint32_t, const uint8_t *, void *), void *ctx) {
    volatile uint32_t *text = (volatile uint32_t *)SHA_TEXT_BASE;
    volatile uint32_t *digest = (volatile uint32_t *)SHA_H_BASE;
    volatile uint32_t *h7 = (volatile uint32_t *)(SHA_H_BASE + 28);
    if (held || !count) return 0;
    acquire();
    uint32_t nonce = first;
    job.tail[3] = nonce;
    writeWords<16>(text, job.tail);
    for (uint32_t i = 0; i < count; i++, nonce++) {
        writeWords<8>(digest, job.mid);
        sha_ll_continue_block(SHA2_256);
        waitIdle();
        digestToText(text, digest);
        REG_WRITE(text + 8, 0x00000080);   // padding byte 0x80
        REG_WRITE(text + 15, 0x00010000);  // 256 bits, big-endian length
        sha_ll_start_block(SHA2_256);
        // While it computes: the next nonce's header block, changed words only.
        job.tail[3] = nonce + 1;
        writeWords<9>(text, job.tail);
        REG_WRITE(text + 15, job.tail[15]);
        waitIdle();
        if (*h7 <= limit) {
            uint8_t hash[32];
            sha_ll_read_digest(SHA2_256, hash, 8);
            hit(nonce, hash, ctx);
            writeWords<16>(text, job.tail);
        }
    }
    esp_sha_release_hardware();
    return count;
}
#endif

uint32_t scan(Job &job, uint32_t first, uint32_t count, uint32_t limit,
              void (*hit)(uint32_t, const uint8_t *, void *), void *ctx, Fill *) {
    // Specialize outside the nonce loop: no per-nonce mode checks. The old
    // sparse and complete-block paths remain available for boot fallback.
#if CONFIG_IDF_TARGET_ESP32S3
    if (overlappedTransfers) return scanOverlapped(job, first, count, limit, hit, ctx);
#endif
    if (directTransfers && sparseTextWrites)
        return scanWithPadding<true, true>(job, first, count, limit, hit, ctx);
    return sparseTextWrites ? scanWithPadding<true>(job, first, count, limit, hit, ctx)
                            : scanWithPadding<false>(job, first, count, limit, hit, ctx);
}

// A compiler/SDK/chip combination may make the buffered path faster. Compare
// both on the actual peripheral, including locks and busy waits, at boot only.
// Alternate order and use the best of three to reduce interrupt/cache noise.
// These short local scans are not pool work and never enter mining counters.
static bool preferDirectTransfers(Job &job) {
    constexpr uint32_t count = 4096;
    int64_t best[2] = {INT64_MAX, INT64_MAX};
    for (int run = 0; run < 3; ++run) {
        for (int turn = 0; turn < 2; ++turn) {
            int direct = (run + turn) & 1;
            int64_t start = esp_timer_get_time();
            auto discard = [](uint32_t, const uint8_t *, void *) {};
            uint32_t done = direct
                ? scanWithPadding<true, true>(job, 0, count, 0, discard, nullptr)
                : scanWithPadding<true>(job, 0, count, 0, discard, nullptr);
            int64_t elapsed = esp_timer_get_time() - start;
            if (done != count || elapsed <= 0) return false;
            if (elapsed < best[direct]) best[direct] = elapsed;
        }
    }
    bool faster = best[1] * 100 < best[0] * 99;  // require at least 1% improvement
    logf("[miner] resumable SHA transfers: %s (buffered %.1f, direct %.1f kH/s, boot scan)\n",
         faster ? "direct" : "buffered", count * 1000.0 / best[0], count * 1000.0 / best[1]);
    return faster;
}

#if CONFIG_IDF_TARGET_ESP32S3
// The overlapped kernel against the best other one, timed like
// preferDirectTransfers(): kept only when at least 1% faster.
static bool preferOverlapped(Job &job) {
    constexpr uint32_t count = 4096;
    int64_t best[2] = {INT64_MAX, INT64_MAX};
    auto discard = [](uint32_t, const uint8_t *, void *) {};
    for (int run = 0; run < 3; ++run) {
        for (int turn = 0; turn < 2; ++turn) {
            int overlapped = (run + turn) & 1;
            int64_t start = esp_timer_get_time();
            uint32_t done = overlapped ? scanOverlapped(job, 0, count, 0, discard, nullptr)
                : directTransfers && sparseTextWrites ? scanWithPadding<true, true>(job, 0, count, 0, discard, nullptr)
                : sparseTextWrites ? scanWithPadding<true>(job, 0, count, 0, discard, nullptr)
                                   : scanWithPadding<false>(job, 0, count, 0, discard, nullptr);
            int64_t elapsed = esp_timer_get_time() - start;
            if (done != count || elapsed <= 0) return false;
            if (elapsed < best[overlapped]) best[overlapped] = elapsed;
        }
    }
    bool faster = best[1] * 100 < best[0] * 99;
    logf("[miner] overlapped SHA transfers: %s (%.1f against %.1f kH/s, boot scan)\n",
         faster ? "kept" : "not faster", count * 1000.0 / best[1], count * 1000.0 / best[0]);
    return faster;
}
#endif

// With every nonce a hit, the kernels write whole blocks after each one. Here
// about one nonce in eight is a hit: the hashes between them come from the
// fast path, and the exact set of hits must match the software digests.
static bool verifyHitSet() {
    struct Check {
        alignas(4) uint8_t header[80];
        uint32_t hits;
        bool correct;
    } check{};
    constexpr uint32_t count = 256, limit = UINT32_MAX / 8;
    static constexpr uint32_t starts[] = {0, 0xff80, UINT32_MAX - 127};  // carries and wrap
    uint32_t seed = 0x3c5a7e91;
    for (uint32_t first : starts) {
        for (auto &byte : check.header) byte = (seed = seed * 1664525u + 1013904223u) >> 24;
        check.hits = 0;
        check.correct = true;
        Job job;
        prepare(job, check.header);
        uint32_t done = scan(job, first, count, limit,
            [](uint32_t nonce, const uint8_t *digest, void *ctx) {
                Check &check = *(Check *)ctx;
                uint8_t ref[32];
                memcpy(check.header + 76, &nonce, 4);
                sha::sha256d(check.header, 80, ref);
                if (memcmp(ref, digest, 32)) check.correct = false;
                ++check.hits;
            }, &check);
        uint32_t expected = 0;
        for (uint32_t i = 0; i < count; ++i) {
            uint32_t nonce = first + i, top;
            uint8_t ref[32];
            memcpy(check.header + 76, &nonce, 4);
            sha::sha256d(check.header, 80, ref);
            memcpy(&top, ref + 28, 4);
            if (top <= limit) ++expected;
        }
        if (done != count || !check.correct || check.hits != expected) return false;
    }
    return true;
}

// A single rare genesis candidate cannot detect every missed/misordered
// transfer. Also compare complete digests at easy targets, nonce carries and
// wrap, across different midstates, before timing either kernel.
static bool verifyResumeTransfers() {
    struct Check {
        alignas(4) uint8_t header[80];
        uint32_t next, hits;
        bool correct;
    } check{};
    static constexpr uint32_t starts[] = {0, 0xfffc, UINT32_MAX - 3};
    uint32_t seed = 0xa5b6c7d8;
    for (uint32_t first : starts) {
        for (auto &byte : check.header) byte = (seed = seed * 1664525u + 1013904223u) >> 24;
        check.next = first;
        check.hits = 0;
        check.correct = true;
        Job job;
        prepare(job, check.header);
        uint32_t done = scan(job, first, 8, UINT32_MAX,
            [](uint32_t nonce, const uint8_t *digest, void *ctx) {
                Check &check = *(Check *)ctx;
                uint8_t ref[32];
                memcpy(check.header + 76, &nonce, 4);
                sha::sha256d(check.header, 80, ref);
                if (nonce != check.next || memcmp(ref, digest, 32)) check.correct = false;
                ++check.next;
                ++check.hits;
            }, &check);
        if (done != 8 || check.hits != 8 || !check.correct) return false;
    }
    return true;
}

}  // namespace hwsha

#elif CONFIG_IDF_TARGET_ESP32

#include <Arduino.h>
#include <stddef.h>

#include <xtensa/hal.h>

#include "core/fill.h"
#include "driver/periph_ctrl.h"
#include "esp_chip_info.h"
#include "esp_idf_version.h"
#include "sha/sha_parallel_engine.h"
#include "soc/hwcrypto_reg.h"

#define HWSHA_CLASSIC 1

namespace hwsha {

namespace {

// What the loops in hw_sha_classic.S work from (CTX_* offsets there); the
// fields from fillP on are only used by hw_sha_fill.S (CTX in
// tools/hw_fill/gen_hw_fill.py). Addresses are 32-bit on the ESP32.
struct Ctx {
    uint32_t hdr[20];       // header words as the engine takes them (big-endian)
    uint32_t nonce;         // in: first position; out: the next one
    uint32_t budget;        // in: positions to try; out: positions not tried
    volatile uint32_t run;  // read every nonce: clearing it stops the loop
    uint32_t pad;           // 0x80000000
    uint32_t chk;           // only used by the original's benchmark builds
    uint32_t fillP;         // software hash: current record
    uint32_t q;             // software hash: position of the nonce in progress
    uint32_t hitQ, hitF;    // software candidate: position, and set when there is one
    uint32_t end1, end2;    // last record of each hash
    uint32_t b1, b2, b1_61; // first record of each hash, record 61 of the first
    uint32_t iv7, c18;
    uint32_t mid[8];
};
static_assert(offsetof(Ctx, nonce) == 80 && offsetof(Ctx, budget) == 84 && offsetof(Ctx, run) == 88 &&
                  offsetof(Ctx, pad) == 92 && offsetof(Ctx, chk) == 96,
              "layout known to hw_sha_classic.S");
static_assert(offsetof(Ctx, fillP) == 100 && offsetof(Ctx, q) == 104 && offsetof(Ctx, hitQ) == 108 &&
                  offsetof(Ctx, hitF) == 112 && offsetof(Ctx, end1) == 116 && offsetof(Ctx, b1) == 124 &&
                  offsetof(Ctx, b1_61) == 132 && offsetof(Ctx, iv7) == 136 && offsetof(Ctx, c18) == 140 &&
                  offsetof(Ctx, mid) == 144,
              "layout known to hw_sha_fill.S");

// Returns 1 on a candidate (low 16 bits of the last digest word zero, i.e. the
// top 16 bits of the hash) at position ctx->nonce - 1, 0 when the budget is
// spent or ctx->run was cleared. The nonce word is written to the engine
// as-is, which is why positions are byte-swapped nonces.
typedef uint32_t (*Loop)(volatile uint32_t *text, Ctx *ctx);
extern "C" uint32_t hwsha_classic_fast(volatile uint32_t *text, Ctx *ctx);
extern "C" uint32_t hwsha_classic_margin(volatile uint32_t *text, Ctx *ctx);
extern "C" uint32_t hwsha_classic_wide(volatile uint32_t *text, Ctx *ctx);
// The fast loop with a software hash in its idle cycles (hw_sha_fill.S).
extern "C" uint32_t hwsha_classic_fill(volatile uint32_t *text, Ctx *ctx);

// Fastest first (247, 257 and 317 cycles per nonce); the slower ones only
// add timing margin, for chips on which the measured schedule fails.
const struct {
    Loop fn;
    const char *name;
} kLoops[] = {
    {hwsha_classic_fast, "fast"},
    {hwsha_classic_margin, "fast, +2 cycle margins"},
    {hwsha_classic_wide, "wide margins"},
};
constexpr int kLoopCount = sizeof(kLoops) / sizeof(kLoops[0]);
volatile int current = kLoopCount;  // selected loop, kLoopCount = none usable

volatile uint32_t *text() { return (volatile uint32_t *)SHA_TEXT_BASE; }

// Known answer, checked on a PC with the software kernel: for the header
// words 0x9E3779B9 * (i + 1), these are the only candidates among the 262144
// positions from 0x1000. One wrong hash that happens to be (or to hide) a
// candidate fails the test.
bool knownAnswer(Loop fn) {
    static const uint32_t expected[] = {0xa94c, 0xe496, 0x1f476, 0x302f0, 0x35af9, 0x3cf5c};
    const size_t n = sizeof(expected) / sizeof(expected[0]);
    Ctx c{};
    for (int i = 0; i < 19; i++) c.hdr[i] = 0x9E3779B9u * (i + 1);
    c.nonce = 0x1000;
    c.budget = 262144;
    c.run = 1;
    c.pad = 0x80000000u;
    size_t found = 0;
    while (c.budget > 0 && fn(text(), &c)) {
        if (found == n || c.nonce - 1 - 0x1000 != expected[found]) return false;
        found++;
    }
    return found == n && c.budget == 0;
}

// The ESP32's SHA-1, SHA-256 and SHA-384/512 engines are locked one by one
// but share the TEXT registers the loops write: all three are taken while a
// loop runs, so mbedtls (TLS) hashes in software meanwhile instead of writing
// TEXT under the loop (which made every TLS handshake fail). Always in this
// order: no deadlock with code that takes them one at a time.
const esp_sha_type kEngines[] = {SHA1, SHA2_256, SHA2_384};

bool tryLockEngines() {
    for (int i = 0; i < 3; i++)
        if (!esp_sha_try_lock_engine(kEngines[i])) {
            while (i--) esp_sha_unlock_engine(kEngines[i]);
            return false;
        }
    return true;
}

void lockEngines() {
    for (esp_sha_type e : kEngines) esp_sha_lock_engine(e);
}

void unlockEngines() {
    for (esp_sha_type e : kEngines) esp_sha_unlock_engine(e);
}

// ---------------------------------------------------------------------------
// Idle-cycle software hashing (hw_sha_fill.S)
// ---------------------------------------------------------------------------

fill::Buffers fillBuf;
volatile bool fillOk = false;

uint32_t addr(const void *p) { return (uint32_t)(uintptr_t)p; }

// Points the generated code at the buffers and resumes `fill`.
void fillLoad(Ctx &c, const Fill &f) {
    c.fillP = f.p;
    c.q = f.q;
    c.hitF = 0;
    c.end1 = addr(&fillBuf.b1[fill::kSteps * fill::kRecordWords]);
    c.end2 = addr(&fillBuf.b2[fill::kSteps * fill::kRecordWords]);
    c.b1 = addr(fillBuf.b1);
    c.b2 = addr(fillBuf.b2);
    c.b1_61 = addr(&fillBuf.b1[61 * fill::kRecordWords]);
    c.iv7 = sha::IV[7];
    c.c18 = fillBuf.c18;
    memcpy(c.mid, fillBuf.mid, sizeof(c.mid));
}

void fillStart(Fill &f, const uint8_t header[80], uint32_t first) {
    fill::prepare(fillBuf, header);
    sha::prepare(f.check, header);
    // As after a second hash: the first iteration starts nonce `first`.
    f.p = addr(&fillBuf.b2[fill::kSteps * fill::kRecordWords]);
    f.q = first - 1;
    f.ready = true;
}

void kat(uint32_t words[20], uint8_t header[80]) {
    for (int i = 0; i < 20; i++) {
        uint32_t w = i < 19 ? 0x9E3779B9u * (i + 1) : 0;
        if (words) words[i] = w;
        if (header) {
            header[4 * i] = w >> 24;
            header[4 * i + 1] = w >> 16;
            header[4 * i + 2] = w >> 8;
            header[4 * i + 3] = w;
        }
    }
}

// The hardware part of hw_sha_fill.S is the fast loop instruction for
// instruction, so knownAnswer() must pass with the software hash running,
// which must meanwhile find the one candidate of its range.
bool fillSelfTest() {
    static const uint32_t expected[] = {0xa94c, 0xe496, 0x1f476, 0x302f0, 0x35af9, 0x3cf5c};
    uint8_t header[80];
    Ctx c{};
    kat(c.hdr, header);
    Fill f;
    fillStart(f, header, 0x1000 + 0xa94c - 3);
    fillLoad(c, f);
    c.nonce = 0x1000;
    c.budget = 262144;
    c.run = 1;
    c.pad = 0x80000000u;
    size_t found = 0;
    int fillHits = 0, wrongHw = 0;
    uint32_t firstHit = 0, q0 = c.q;
    while (c.budget > 0) {
        bool hw = hwsha_classic_fill(text(), &c);
        if (c.hitF) {
            if (!fillHits++) firstHit = c.hitQ;
            c.hitF = 0;
        }
        if (!hw) break;
        if (found >= 6 || c.nonce - 1 - 0x1000 != expected[found]) wrongHw++;
        found++;
    }
    bool ok = !wrongHw && found == 6 && c.budget == 0 && fillHits == 1 && firstHit == 0x1000 + 0xa94c;
    if (!ok)
        logf("[fill] self-test: hardware %u candidates (%d wrong, want 6), budget left %u; software %u nonces, "
             "%d candidates (first %08x, want 0000b94c)\n",
             (unsigned)found, wrongHw, (unsigned)c.budget, (unsigned)(c.q - q0), fillHits, (unsigned)firstHit);
    return ok;
}

// After a failed self-test: runs the software hash for exactly one nonce (64
// hardware nonces) and compares every round with a reference computed here.
void fillDiagnose() {
    auto rotr = [](uint32_t x, int n) { return (x >> n) | (x << (32 - n)); };
    auto rounds = [&](const uint32_t st[8], const uint32_t w[64], uint32_t A[65], uint32_t E[65]) {
        uint32_t a = st[0], b = st[1], c = st[2], d = st[3], e = st[4], f = st[5], g = st[6], h = st[7];
        for (int i = 0; i <= 64; i++) {
            A[i] = a;
            E[i] = e;
            if (i == 64) break;
            uint32_t t1 = h + (rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)) + (g ^ (e & (f ^ g))) + sha::K[i] + w[i];
            uint32_t t2 = (rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)) + ((a & b) | (c & (a | b)));
            h = g; g = f; f = e; e = d + t1;
            d = c; c = b; b = a; a = t1 + t2;
        }
    };
    auto expand = [&](uint32_t w[64]) {
        for (int i = 16; i < 64; i++)
            w[i] = (rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >> 10)) + w[i - 7] +
                   (rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >> 3)) + w[i - 16];
    };

    uint8_t header[80];
    Ctx c{};
    kat(c.hdr, header);
    const uint32_t q = 0x1000 + 0xa94c;  // a candidate
    Fill fl;
    fillStart(fl, header, q);
    fillLoad(c, fl);
    c.nonce = 0x40000000u;
    c.budget = 64;  // transition B, 31 iterations, transition A, 31 iterations
    c.run = 1;
    c.pad = 0x80000000u;
    while (c.budget > 0 && hwsha_classic_fill(text(), &c)) {
    }
    logf("[fill] after 64 iterations: q %08x (want %08x), P %d records from the end of hash 2 (want 0)\n",
         (unsigned)c.q, (unsigned)q, ((int)c.fillP - (int)c.end2) / fill::kRecordBytes);

    sha::MidState ms;
    sha::prepare(ms, header);
    uint32_t w[64] = {ms.w[0], ms.w[1], ms.w[2], q, 0x80000000};
    w[15] = 640;
    expand(w);
    uint32_t A[65], E[65];
    rounds(ms.mid, w, A, E);
    const uint32_t *b1 = fillBuf.b1;
    int bad = -1;
    for (int k = 3; k < 64 && bad < 0; k++)
        if (b1[k * 5 + 2] != w[k]) bad = k;
    logf("[fill] hash 1 message words: %s %d\n", bad < 0 ? "all right" : "first wrong at", bad);
    bad = -1;
    for (int k = 4; k <= 64 && bad < 0; k++)
        if (b1[k * 5] != A[k] || b1[k * 5 + 1] != E[k]) bad = k;
    if (bad >= 0)
        logf("[fill] hash 1 state: first wrong before round %d: a %08x/%08x e %08x/%08x\n", bad,
             (unsigned)b1[bad * 5], (unsigned)A[bad], (unsigned)b1[bad * 5 + 1], (unsigned)E[bad]);
    else
        logf("[fill] hash 1 state: all right\n");

    uint32_t h1[8] = {A[64], A[63], A[62], A[61], E[64], E[63], E[62], E[61]};
    uint32_t w2[64] = {0};
    for (int k = 0; k < 8; k++) w2[k] = ms.mid[k] + h1[k];
    w2[8] = 0x80000000;
    w2[15] = 256;
    expand(w2);
    uint32_t A2[65], E2[65];
    rounds(sha::IV, w2, A2, E2);
    const uint32_t *b2 = fillBuf.b2;  // record k at k + 3
    bad = -1;
    for (int k = 0; k < 62 && bad < 0; k++)
        if (b2[(k + 3) * 5 + 2] != w2[k]) bad = k;
    logf("[fill] hash 2 message words: %s %d\n", bad < 0 ? "all right" : "first wrong at", bad);
    bad = -1;
    for (int k = 1; k <= 62 && bad < 0; k++)
        if (b2[(k + 3) * 5] != A2[k] || b2[(k + 3) * 5 + 1] != E2[k]) bad = k;
    if (bad >= 0)
        logf("[fill] hash 2 state: first wrong before round %d: a %08x/%08x e %08x/%08x\n", bad,
             (unsigned)b2[(bad + 3) * 5], (unsigned)A2[bad], (unsigned)b2[(bad + 3) * 5 + 1], (unsigned)E2[bad]);
    else
        logf("[fill] hash 2 state: all right (candidate: %s)\n",
             ((E2[61] + sha::IV[7]) & 0xFFFF) == 0 ? "yes" : "no");
}

// Cycles for `n` hardware nonces with `fn`, the best of two runs.
uint32_t timeLoop(Loop fn, uint32_t n, uint32_t *fillNonces) {
    uint32_t best = UINT32_MAX;
    for (int run = 0; run < 2; run++) {
        uint8_t header[80];
        Ctx c{};
        kat(c.hdr, header);
        Fill f;
        fillStart(f, header, 0x80000000u);
        fillLoad(c, f);
        c.nonce = 0x40000000u;  // a candidate only returns early: the loop goes on
        c.budget = n;
        c.run = 1;
        c.pad = 0x80000000u;
        uint32_t t0 = xthal_get_ccount();
        while (c.budget > 0 && fn(text(), &c)) {
        }
        uint32_t t = xthal_get_ccount() - t0;
        if (t < best) best = t;
        if (fillNonces) *fillNonces = c.q - (0x80000000u - 1);
    }
    return best;
}

// Uses idle-cycle hashing only when it is correct and makes the total faster.
void pickFill() {
    if (!fillSelfTest() && !fillSelfTest()) {
        logf("[miner] idle-cycle software hashing failed its self-test, disabled\n");
        fillDiagnose();
        return;
    }
    const uint32_t n = 131072;
    uint32_t fillNonces = 0;
    uint32_t plain = timeLoop(hwsha_classic_fast, n, nullptr);
    uint32_t mixed = timeLoop(hwsha_classic_fill, n, &fillNonces);
    double mhz = getCpuFrequencyMhz();
    double hwPlain = n * mhz * 1e3 / plain, hwMixed = n * mhz * 1e3 / mixed;  // kH/s
    double extra = fillNonces * mhz * 1e3 / mixed;
    fillOk = hwMixed + extra > hwPlain * 1.002;
    logf("[miner] idle-cycle software hashing: %s (hardware %.1f -> %.1f kH/s, +%.1f kH/s software)\n",
         fillOk ? "on" : "off, no gain", hwPlain, hwMixed, extra);
}

// Picks the fastest loop this chip runs correctly.
bool pickLoop() {
    // Before revision 3, a raw DPORT read on one core (the loops read the
    // digest that way) can corrupt APB reads on the other one: WiFi and UART
    // die within seconds, which no self-test can see.
    esp_chip_info_t chip;
    esp_chip_info(&chip);
    // Major * 100 + minor: `full_revision` in ESP-IDF 4, `revision` since 5.
#if ESP_IDF_VERSION_MAJOR >= 5
    unsigned revision = chip.revision;
#else
    unsigned revision = chip.full_revision;
#endif
    if (revision / 100 < 3) {
        logf("[miner] ESP32 revision %u.%u: the hardware SHA loop needs revision 3\n",
             revision / 100, revision % 100);
        return false;
    }

    lockEngines();
    periph_module_reset(PERIPH_SHA_MODULE);
    int pick = 0;
    // Best of three: one stray misread must not cost the fast loop for good.
    while (pick < kLoopCount && !knownAnswer(kLoops[pick].fn) && !knownAnswer(kLoops[pick].fn) &&
           !knownAnswer(kLoops[pick].fn)) {
        logf("[miner] hardware SHA loop '%s' failed its self-test\n", kLoops[pick].name);
        pick++;
    }
    current = pick;
    if (pick < kLoopCount) logf("[miner] hardware SHA loop: %s\n", kLoops[pick].name);
    if (pick == 0) pickFill();  // built on the fast loop only
    unlockEngines();
    return pick < kLoopCount;
}

// Every candidate is recomputed in software, so the share of candidates that
// software does not confirm measures how many hashes the loop gets wrong. It
// is 0 when healthy; otherwise fall back to the next, slower loop, and to
// software alone after the last one.
void health(bool confirmed) {
    static uint32_t hits = 0, wrong = 0;
    hits++;
    if (!confirmed) wrong++;
    if (hits < 256) return;
    if (wrong >= 8) {
        int next = current + 1;
        logf("[miner] %u of %u hardware SHA candidates were wrong, switching to: %s\n", (unsigned)wrong,
             (unsigned)hits, next < kLoopCount ? kLoops[next].name : "software");
        current = next;
    }
    hits = wrong = 0;
}

}  // namespace

bool available() { return true; }

uint32_t nonceAt(uint32_t position) { return __builtin_bswap32(position); }

// The loops hold all three SHA engines (see lockEngines()), so mbedtls hashes
// in software and a TLS handshake runs alongside them. But scan() used to
// free them after every batch, and mbedtls, retrying at each digest, would
// sooner or later get one in that gap and keep it for the whole handshake
// (its transcript hash): the hardware loop then stopped for 1.5 s, and the
// WiFi task, waiting on the TLS task meanwhile, lent it priority 23, which
// froze everything else on core 0 (screen, web server). Between hold() and
// release(), scan() keeps the engines from one batch to the next.
static std::atomic<int> holds{0};  // TLS tasks and an update download may overlap
static bool minerHolds = false;     // only touched by the mining task

void hold() { holds++; }
void release() { holds--; }

void idle() {
    if (minerHolds) {
        unlockEngines();
        minerHolds = false;
    }
}

bool fillAvailable() { return fillOk; }

void prepareFill(Fill &f, const uint8_t header[80]) { fillStart(f, header, 0); }

void prepare(Job &job, const uint8_t header[80]) {
    for (int i = 0; i < 20; i++)
        job.words[i] = (uint32_t)header[4 * i] << 24 | (uint32_t)header[4 * i + 1] << 16 |
                       (uint32_t)header[4 * i + 2] << 8 | header[4 * i + 3];
    sha::prepare(job.check, header);
}

// Hands a software candidate of the idle-cycle hash over, once confirmed.
void fillHit(Ctx &c, Fill &f, uint32_t limit, void (*hit)(uint32_t, const uint8_t *, void *)) {
    if (!c.hitF) return;
    c.hitF = 0;
    uint32_t nonce = nonceAt(c.hitQ);
    uint8_t hash[32];
    if (!sha::scan(f.check, nonce, 0x0000FFFF, hash)) {
        fillOk = false;
        logf("[miner] idle-cycle software hashing reported a wrong candidate, disabled\n");
        return;
    }
    uint32_t top = (uint32_t)hash[28] | (uint32_t)hash[29] << 8 | (uint32_t)hash[30] << 16 | (uint32_t)hash[31] << 24;
    if (top <= limit) hit(nonce, hash, f.ctx);
}

uint32_t scan(Job &job, uint32_t first, uint32_t count, uint32_t limit,
              void (*hit)(uint32_t, const uint8_t *, void *), void *ctx, Fill *fill) {
    if (!mining::fixedFilterSupports(limit)) {
        idle();
        return 0;
    }
    int loop = current;
    if (loop >= kLoopCount) {
        // health() gave up on every loop: software mines from now on, so
        // the engines this task may still hold go back to TLS.
        idle();
        return 0;
    }
    // An open TLS connection can hold the engine for seconds (mbedtls then
    // keeps it for the whole digest); never wait for it.
    if (!minerHolds && !tryLockEngines()) return 0;
    minerHolds = true;
    // Start from a clean peripheral, whatever used it last.
    periph_module_reset(PERIPH_SHA_MODULE);

    Ctx c;
    memcpy(c.hdr, job.words, sizeof(c.hdr));
    c.nonce = first;
    c.budget = count;
    c.run = 1;
    c.pad = 0x80000000u;
    bool useFill = fill && fill->ready && fillOk && loop == 0;
    Loop fn = useFill ? hwsha_classic_fill : kLoops[loop].fn;
    uint32_t q0 = 0;
    if (useFill) {
        fillLoad(c, *fill);
        q0 = c.q;
    }
    while (c.budget > 0 && fn(text(), &c)) {
        if (useFill) fillHit(c, *fill, limit, hit);
        // The loop only looked at 16 bits of the digest: recompute the hash.
        uint32_t nonce = nonceAt(c.nonce - 1);
        uint8_t hash[32];
        bool confirmed = sha::scan(job.check, nonce, 0x0000FFFF, hash);
        health(confirmed);
        if (confirmed) {
            uint32_t top = (uint32_t)hash[28] | (uint32_t)hash[29] << 8 | (uint32_t)hash[30] << 16 |
                           (uint32_t)hash[31] << 24;
            if (top <= limit) hit(nonce, hash, ctx);
        }
    }
    if (useFill) {
        fillHit(c, *fill, limit, hit);
        fill->p = c.fillP;
        fill->q = c.q;
        fill->hashes += c.q - q0;
    }
    if (!holds) idle();
    return count - c.budget;
}

}  // namespace hwsha

#else  // no usable SHA peripheral

namespace hwsha {

bool available() { return false; }
uint32_t nonceAt(uint32_t position) { return position; }
void hold() {}
void release() {}
void idle() {}
void prepare(Job &, const uint8_t *) {}
bool fillAvailable() { return false; }
void prepareFill(Fill &, const uint8_t *) {}
uint32_t scan(Job &, uint32_t, uint32_t, uint32_t, void (*)(uint32_t, const uint8_t *, void *), void *, Fill *) {
    return 0;
}

}  // namespace hwsha

#endif

namespace hwsha {

namespace {
struct TestCtx {
    uint32_t nonce;
    uint8_t hash[32];
    int hits;
};
}  // namespace

bool selfTest() {
    if (!available()) return false;
#ifdef HWSHA_CLASSIC
    if (!pickLoop()) return false;
#endif

    static const char *kGenesis =
        "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e"
        "67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c";
    const uint32_t kNonce = 2083236893;
    uint8_t header[80];
    work::fromHex(kGenesis, header, 80);

    Job job;
    prepare(job, header);
    TestCtx t{};
    // nonceAt() is its own inverse: this is the genesis nonce's position.
    uint32_t pos = nonceAt(kNonce);
    scan(job, pos - 1000, 2000, work::quickLimitFor(1.0),
         [](uint32_t nonce, const uint8_t hash[32], void *ctx) {
             TestCtx *t = (TestCtx *)ctx;
             t->nonce = nonce;
             memcpy(t->hash, hash, 32);
             t->hits++;
         },
         &t);
    uint8_t ref[32];
    sha::sha256d(header, 80, ref);
    bool correct = t.hits == 1 && t.nonce == kNonce && memcmp(ref, t.hash, 32) == 0;
#if SOC_SHA_SUPPORT_RESUME
    if (correct) correct = verifyResumeTransfers() && verifyHitSet();
#if CONFIG_IDF_TARGET_ESP32S3
    if (!correct && overlappedTransfers) {
        overlappedTransfers = false;
        logf("[miner] overlapped SHA transfers failed their self-test; using the other kernels\n");
        return selfTest();
    }
#endif
    if (!correct && directTransfers) {
        directTransfers = false;
        logf("[miner] direct SHA transfers failed their self-test; retrying buffered transfers\n");
        return selfTest();
    }
    if (!correct && sparseTextWrites) {
        // A chip/SDK which does not retain TEXT padding keeps its working
        // hardware path: retry the known answer with complete block writes.
        sparseTextWrites = false;
        logf("[miner] SHA padding reuse failed its self-test; retrying full block writes\n");
        return selfTest();
    }
#if CONFIG_IDF_TARGET_ESP32S3
    // Time the others first (their choice is the fallback), then this one.
    if (correct && overlappedTransfers) {
        overlappedTransfers = false;
        bool others = selfTest();
        overlappedTransfers = true;
        if (!others) return true;  // this one passed every check on its own
        if (preferOverlapped(job)) return true;
        overlappedTransfers = false;
        return true;  // the other kernels were just validated
    }
#endif
    if (correct && directTransfers && !preferDirectTransfers(job)) {
        directTransfers = false;
        return selfTest();  // validate the selected buffered path as well
    }
#endif
    return correct;
}

}  // namespace hwsha
