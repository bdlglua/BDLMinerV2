#include "miner.h"
#ifdef BOARD_GAMMA601
#include "gamma601.h"
#endif
#ifdef BOARD_NERDNOS
#include "nerdnos.h"
#endif

#include <Preferences.h>
#include <esp_partition.h>
#include <esp_rom_crc.h>
#include <ctime>
#include <stddef.h>
#include <freertos/queue.h>
#include <freertos/semphr.h>
#include <atomic>

#include "core/mining_safety.h"
#include "core/coin.h"
#include "hw_sha.h"
#include "log.h"
#include "ota.h"
#include "runtime.h"

#if CONFIG_IDF_TARGET_ESP32 || CONFIG_IDF_TARGET_ESP32S3
// Hand-scheduled assembly of sha::scan() up to its early exit, in IRAM
// (sw_sha_prefilter.S): true exactly when the top 16 bits of the hash are
// zero. ~5300 instructions against ~7250 for the compiled C kernel.
extern "C" bool sw_sha256d_prefilter(const uint32_t *mid, const uint8_t *tail, const uint32_t *bake);
#define HAVE_SW_PREFILTER 1
#endif

MinerStats stats;

namespace {
uint8_t activeMiningCoin = coin::BTC; // fixed before worker tasks start

// Hashes done between two looks at the job board (and two updates of the
// hash counter): ~0.1 s of software work, ~67 ms of the original ESP32's
// hardware loop. Both divide 2^32, so a work's nonce space always ends on a
// batch boundary.
constexpr uint32_t kBatchSw = 4096;
constexpr uint32_t kBatchHw = 65536;

// The hardware miner owns the last core of dual-core chips: above every task
// that could share it (everything else, Arduino loop included, runs on core
// 0). Stealing its CPU leaves the SHA engine idle, so it only pauses every
// 10 s, for the idle task; miners sharing a core pause after every batch.
constexpr UBaseType_t kHwMinerPriority = 19;
constexpr uint32_t kOwnCorePauseMs = 10000;

// Candidates under this top-32-bit value (diff >= ~1.5e-5) are fully
// evaluated, which is what feeds the "best difficulty" statistic.
constexpr uint32_t kBestDiffFilter = 0x0000FFFF;

SemaphoreHandle_t jobMutex;
work::Job board;
std::atomic<bool> boardValid{false};
std::atomic<uint32_t> boardSeq{0};
std::atomic<uint32_t> workGeneration{0};  // only changes when earlier jobs become invalid
std::atomic<uint32_t> lastJobTime{0};
uint64_t extranonce2Counter = 0;

struct QueuedShare {
    stratum::Share share;
    uint32_t generation;
};
QueueHandle_t shareQueue;
portMUX_TYPE statsMux = portMUX_INITIALIZER_UNLOCKED;
// Reserved, bounded storage: ordinary shares must never displace a block
// candidate. Consumers check this before the regular queue (waits <= 20 ms).
QueuedShare blockQueue[2];
unsigned blockQueueCount = 0;
struct CandidateRecord {
    BlockCandidate info;
    uint32_t submissionId;
};
CandidateRecord candidates[4];
unsigned candidateHead = 0, candidateCount = 0;
BlockCandidate previousCandidate{};
bool havePreviousCandidate = false;
uint32_t candidateRevision = 0, savedCandidateRevision = 0;
bool bestDirty = false;
BestShare top[TOP_SHARES];  // best first, difficulty 0 = empty
BlockBest blockBests[BLOCK_BESTS];  // ring, blockHead = the height mined now
int blockHead = 0;
mining::SafetyLease safety;
std::atomic<bool> batteryLimited{false};

#ifdef BOARD_ASIC
using HashCount = uint64_t;
#else
using HashCount = uint32_t;
#endif
HashCount lastHashes[MAX_MINERS];
uint64_t totalHashes = 0;
// Hash rates are averages over the last 60 one-second ticks (like a pool's
// "1m" rate): the second or two the hardware loop pauses for a TLS handshake
// then costs a few percent for a minute instead of a dip to two thirds.
constexpr int kRateWindow = 60;
struct RateSample {
    uint32_t ms;
    HashCount hashes[MAX_MINERS];
};
RateSample rateRing[kRateWindow];
int rateCount = 0, rateHead = 0;
double hashrate = 0, hashrateLive = 0;
constexpr int kLiveWindow = 10;
double engineRate[MAX_MINERS];
uint32_t lastTickMs = 0;
uint64_t sessionMs = 0;

// Lifetime totals as loaded at boot; this session is added on top.
uint64_t baseHashes = 0;
uint32_t baseShares = 0, baseBlocks = 0, baseSeconds = 0;
uint32_t savedBlocks = 0;

// Ring buffer of HISTORY_INTERVAL_S averages.
HistoryPoint history[HISTORY_LEN];
int historyLen = 0, historyHead = 0;
uint32_t historyRevision = 0;
uint64_t historyHashes = 0;
uint32_t historyMs = 0;
float temperatureSum = 0;
uint32_t temperatureSamples = 0, powerSum = 0, observationSamples = 0;
MinerProfile profiles[MAX_MINERS];

#ifdef BOARD_ASIC
using MiningWork = AsicWork;
#else
struct MiningWork : work::Work {
    uint32_t generation;
    uint32_t height;
};
#endif

struct Miner {
    uint8_t index;
    bool hardware;
    bool ownsCore;  // alone on its core at kHwMinerPriority
    MiningWork work;
    hwsha::Job hwJob;
    // Hardware miner on the original ESP32: a second work hashed in software
    // in the idle cycles of the hardware loop (hwsha::Fill).
    bool useFill;
    MiningWork work2;
    hwsha::Fill fill;
};

Miner miners[MAX_MINERS];
int fillIndex = -1;  // stats.hashes slot of the idle-cycle hashing
bool useAsm = false;  // sw_sha256d_prefilter() passed its self-test

// `ctx` is the MiningWork the nonce belongs to.
void onCandidate(uint32_t nonce, const uint8_t hash[32], void *ctx) {
    const MiningWork &w = *(const MiningWork *)ctx;
    double d = work::hashDifficulty(hash);
    uint32_t when = 0;
    portENTER_CRITICAL(&statsMux);
    bool topCandidate = d > top[TOP_SHARES - 1].difficulty;
    portEXIT_CRITICAL(&statsMux);
    if (topCandidate) {
        time_t now = time(nullptr);
        if (now > 1700000000) when = (uint32_t)now;  // clock set by NTP
    }

    portENTER_CRITICAL(&statsMux);
    if (d > stats.sessionBestDiff) stats.sessionBestDiff = d;
    if (d > stats.periodBestDiff) stats.periodBestDiff = d;
    if (d > stats.bestDiff) {
        stats.bestDiff = d;
        bestDirty = true;
    }
    if (d >= 1.0) stats.over32bits = stats.over32bits + 1;
    // Bitcoin interprets the digest as little-endian. Inspect the exact bits
    // of fully verified candidates, avoiding floating-point boundary rounding.
    if (!(hash[24] | hash[25] | hash[26] | hash[27] | hash[28] | hash[29] | hash[30] | hash[31]))
        stats.over64bits = stats.over64bits + 1;
    for (auto &b : blockBests) {
        if (b.height == w.height && w.height && d > b.best) b.best = d;
    }
    if (d > top[TOP_SHARES - 1].difficulty) {
        int i = TOP_SHARES - 1;
        for (; i > 0 && top[i - 1].difficulty < d; i--) top[i] = top[i - 1];
        top[i] = {d, when};  // saved with the lifetime totals (flash wear)
    }
    portEXIT_CRITICAL(&statsMux);

    bool block = work::meetsTarget(hash, w.networkTarget);
    if ((w.exactShareTarget ? work::meetsTarget(hash, w.shareTarget) : d >= w.shareDifficulty) || block) {
        stratum::Share s{};
        strlcpy(s.jobId, w.jobId.c_str(), sizeof(s.jobId));
        strlcpy(s.extranonce2, w.extranonce2Hex.c_str(), sizeof(s.extranonce2));
        s.ntime = w.ntime;
        s.nonce = nonce;
        s.difficulty = d;
        s.poolDifficulty = w.shareDifficulty;
        s.versionMask = w.versionMask;
        s.version = uint32_t(w.header[0]) | uint32_t(w.header[1]) << 8 | uint32_t(w.header[2]) << 16 | uint32_t(w.header[3]) << 24;
        memcpy(s.hash, hash, 32);
        QueuedShare queued{s, w.generation};
        if (block) {
            // Serialize the rare block path with clean jobs/disconnects. A
            // batch finishing on obsolete work must not announce a new block.
            if (!jobMutex || !shareQueue) return;
            xSemaphoreTake(jobMutex, portMAX_DELAY);
            if (!boardValid || w.generation != workGeneration) {
                xSemaphoreGive(jobMutex);
                return;
            }
            bool duplicate = false, enqueued = false;
            portENTER_CRITICAL(&statsMux);
            for (unsigned i = 0; i < candidateCount; ++i)
                if (!memcmp(candidates[i].info.hash, hash, 32)) duplicate = true;
            if (!duplicate) {
                enqueued = blockQueueCount < 2;
                if (enqueued) blockQueue[blockQueueCount++] = queued;
                CandidateRecord &record = candidates[candidateHead];
                record = {};
                stats.blocksFound = stats.blocksFound + 1;
                record.info.number = stats.blocksFound;
                record.info.height = w.height;
                record.info.coin = activeMiningCoin;
                memcpy(record.info.hash, hash, 32);
                record.info.state = enqueued ? BlockQueued : BlockQueueFull;
                ++candidateRevision;
                candidateHead = (candidateHead + 1) % 4;
                if (candidateCount < 4) ++candidateCount;
                stats.lifetimeBlocks = baseBlocks + stats.blocksFound;
                if (enqueued) stats.sharesFound = stats.sharesFound + 1;
            }
            portEXIT_CRITICAL(&statsMux);
            xSemaphoreGive(jobMutex);
            if (!duplicate) {
                uint8_t id[32];
                for (unsigned i = 0; i < 32; ++i) id[i] = hash[31 - i];
                logf("[miner] BLOCK CANDIDATE height=%u hash=%s job=%s %s; chain confirmation unknown\n",
                     (unsigned)w.height, work::toHex(id, 32).c_str(), w.jobId.c_str(),
                     enqueued ? "queued with priority" : "NOT QUEUED: candidate queue full");
            }
        } else if (shareQueue && xQueueSend(shareQueue, &queued, 0) == pdTRUE) {
            portENTER_CRITICAL(&statsMux);
            stats.sharesFound = stats.sharesFound + 1;
            portEXIT_CRITICAL(&statsMux);
        }
    }
}

// Takes the latest job from the board and builds fresh work with a unique
// extranonce2. Returns false if there is no job.
bool fetchWork(Miner &m, uint32_t &seq) {
    xSemaphoreTake(jobMutex, portMAX_DELAY);
    bool ok = boardValid;
    if (ok) {
        seq = boardSeq;
        work::build(board, extranonce2Counter++, m.work);
        m.work.generation = workGeneration;
        m.work.height = board.blockHeight(activeMiningCoin);
        if (m.useFill) {
            work::build(board, extranonce2Counter++, m.work2);
            m.work2.generation = workGeneration;
            m.work2.height = m.work.height;
        }
    }
    xSemaphoreGive(jobMutex);
    if (ok && m.hardware) hwsha::prepare(m.hwJob, m.work.header);
    if (ok && m.useFill) {
        hwsha::prepareFill(m.fill, m.work2.header);
        m.fill.ctx = &m.work2;
    }
    return ok;
}

// Software kernel over positions [first, first + count). A hardware miner
// whose engine is busy (TLS) keeps to the engine's nonce order so that no
// nonce is hashed twice.
void softwareBatch(Miner &m, uint32_t first, uint32_t count, uint32_t limit) {
    uint8_t hash[32];
    const sha::MidState &mid = m.work.mid;
#ifdef HAVE_SW_PREFILTER
    if (useAsm && mining::fixedFilterSupports(limit)) {
        // For easier pool targets the fixed 16-bit filter would lose shares;
        // use the general C kernel instead.
        uint8_t tail[16];
        memcpy(tail, m.work.header + 64, 16);
        for (uint32_t p = first, end = first + count; p != end; p++) {
            uint32_t n = m.hardware ? hwsha::nonceAt(p) : p;
            memcpy(tail + 12, &n, 4);  // little-endian, as in the header
            if (sw_sha256d_prefilter(mid.mid, tail, mid.bake) && sha::scan(mid, n, limit, hash))
                onCandidate(n, hash, &m.work);
        }
        return;
    }
#endif
    for (uint32_t p = first, end = first + count; p != end; p++) {
        uint32_t n = m.hardware ? hwsha::nonceAt(p) : p;
        if (sha::scan(mid, n, limit, hash)) onCandidate(n, hash, &m.work);
    }
}

#ifdef HAVE_SW_PREFILTER
// Compares the assembly with sha::scan() on 6 known candidates (the header and
// nonces of hwsha's known-answer test) and the 512 nonces after each.
bool prefilterSelfTest() {
    static const uint32_t kCandidates[] = {0xa94c, 0xe496, 0x1f476, 0x302f0, 0x35af9, 0x3cf5c};
    uint8_t header[80] = {0};
    for (int i = 0; i < 19; i++) {
        uint32_t w = 0x9E3779B9u * (i + 1);
        header[4 * i] = w >> 24;
        header[4 * i + 1] = w >> 16;
        header[4 * i + 2] = w >> 8;
        header[4 * i + 3] = w;
    }
    sha::MidState ms;
    sha::prepare(ms, header);
    uint8_t tail[16], hash[32];
    memcpy(tail, header + 64, 16);
    int hits = 0;
    for (uint32_t c : kCandidates) {
        uint32_t first = __builtin_bswap32(0x1000 + c);
        for (uint32_t n = first; n != first + 513; n++) {
            memcpy(tail + 12, &n, 4);
            bool fast = sw_sha256d_prefilter(ms.mid, tail, ms.bake);
            if (fast != sha::scan(ms, n, 0x0000FFFF, hash)) return false;
            hits += fast;
        }
    }
    return hits >= 6;
}
#endif

void minerTask(void *arg) {
    Miner &m = *(Miner *)arg;
    uint32_t seq = 0;
    uint32_t nonce = 0;
    bool haveWork = false;
    uint32_t lastPause = millis();

    for (;;) {
        if (!boardValid) {
            haveWork = false;
            if (m.hardware) hwsha::idle();
            uint32_t started = millis();
            vTaskDelay(pdMS_TO_TICKS(100));
            portENTER_CRITICAL(&statsMux);
            profiles[m.index].waitingMs += uint32_t(millis() - started);
            portEXIT_CRITICAL(&statsMux);
            continue;
        }
        uint8_t power = minerPowerPercent();
        if (!power) {
            if (m.hardware) hwsha::idle();
            uint32_t started = millis();
            vTaskDelay(pdMS_TO_TICKS(200));
            portENTER_CRITICAL(&statsMux);
            profiles[m.index].pausedMs += uint32_t(millis() - started);
            portEXIT_CRITICAL(&statsMux);
            continue;
        }
        uint32_t batchStart = millis();
        uint32_t batchStartUs = micros();
        // New job, or the whole 32-bit nonce space of this work is done.
        if (!haveWork || seq != boardSeq) {
            haveWork = fetchWork(m, seq);
            nonce = 0;
            if (!haveWork) {
                if (m.hardware) hwsha::idle();
                continue;
            }
        }

        if (!minerPowerPercent()) continue;
        uint32_t limit = max(m.work.quickLimit, kBestDiffFilter);
        // Never run past the end of the nonce space (after software batches
        // `nonce` is only a multiple of kBatchSw).
        uint32_t left = 0u - nonce;
        uint32_t hwBatch = left && left < kBatchHw ? left : kBatchHw;
        uint32_t done = m.hardware ? hwsha::scan(m.hwJob, nonce, hwBatch, limit, onCandidate, &m.work,
                                                 m.useFill ? &m.fill : nullptr)
                                   : 0;
        if (m.useFill && m.fill.hashes) {
            portENTER_CRITICAL(&statsMux);
            stats.hashes[fillIndex] += m.fill.hashes;
            portEXIT_CRITICAL(&statsMux);
            m.fill.hashes = 0;
        }
        if (!done) {
            softwareBatch(m, nonce, kBatchSw, limit);
            done = kBatchSw;
        }
        portENTER_CRITICAL(&statsMux);
        stats.hashes[m.index] += done;
        profiles[m.index].hashingUs += uint32_t(micros() - batchStartUs);
        portEXIT_CRITICAL(&statsMux);
        nonce += done;
        if (nonce == 0) haveWork = false;  // wrapped: 2^32 nonces exhausted

        if (power < 100) {
            uint32_t restStarted = millis();
            if (m.hardware) hwsha::idle();
            uint32_t rest = mining::restMs(millis() - batchStart, power);
            // Release hardware before sleeping; revisit controls frequently.
            while (rest) {
                uint32_t part = min<uint32_t>(rest, 100);
                vTaskDelay(max<TickType_t>(1, pdMS_TO_TICKS(part)));
                rest -= part;
                if (minerPowerPercent() != power || seq != boardSeq || !boardValid) break;
            }
            lastPause = millis();
            portENTER_CRITICAL(&statsMux);
            profiles[m.index].restMs += uint32_t(millis() - restStarted);
            portEXIT_CRITICAL(&statsMux);
        }
        // Let the idle task and lower-priority tasks run.
        if (!m.ownsCore || millis() - lastPause >= kOwnCorePauseMs) {
            uint32_t restStarted = millis();
            vTaskDelay(1);
            portENTER_CRITICAL(&statsMux);
            profiles[m.index].restMs += uint32_t(millis() - restStarted);
            portEXIT_CRITICAL(&statsMux);
            lastPause = millis();
        }
    }
}

}  // namespace

// Called with jobMutex held, taking locks in the same order as onCandidate.
void invalidateBlockQueue() {
    portENTER_CRITICAL(&statsMux);
    blockQueueCount = 0;
    for (unsigned i = 0; i < candidateCount; ++i)
        if (candidates[i].info.state == BlockQueued) {
            candidates[i].info.state = BlockStale;
            ++candidateRevision;
        }
    portEXIT_CRITICAL(&statsMux);
}

void minerPublishJob(const work::Job &job, bool clean) {
    if (!jobMutex || !shareQueue) return;
    if (!coin::jobCompatible(activeMiningCoin, job.version, job.versionMask)) { minerInvalidate(); return; }
    xSemaphoreTake(jobMutex, portMAX_DELAY);
    if (clean) {
        workGeneration++;
        xQueueReset(shareQueue);
        invalidateBlockQueue();
    }
    board = job;
    lastJobTime = job.ntime;
    double networkDiff = work::difficultyFromBits(job.nbits);
    portENTER_CRITICAL(&statsMux);
    stats.blockHeight = job.blockHeight(activeMiningCoin);
    if (stats.blockHeight && stats.blockHeight != blockBests[blockHead].height) {
        blockHead = (blockHead + 1) % BLOCK_BESTS;
        blockBests[blockHead] = {stats.blockHeight, 0};
    }
    stats.networkDiff = networkDiff;
    stats.poolDiff = job.difficulty;
    stats.workAvailable=true;
    stats.versionMask=job.versionMask;
    portEXIT_CRITICAL(&statsMux);
    boardValid = true;
    boardSeq++;
    xSemaphoreGive(jobMutex);
}

uint32_t minerJobTime() { return lastJobTime.load(); }

void minerInvalidate() {
    if (!jobMutex || !shareQueue) return;
    xSemaphoreTake(jobMutex, portMAX_DELAY);
    boardValid = false;
    portENTER_CRITICAL(&statsMux);
    stats.workAvailable=false;
    stats.versionMask=0;
    portEXIT_CRITICAL(&statsMux);
    workGeneration++;
    xQueueReset(shareQueue);
    invalidateBlockQueue();
    xSemaphoreGive(jobMutex);
}

bool minerNextShare(stratum::Share &share, uint32_t waitMs) {
    if (!shareQueue) return false;
    QueuedShare queued;
    TickType_t wait = pdMS_TO_TICKS(min<uint32_t>(waitMs, 20));
    for (;;) {
        portENTER_CRITICAL(&statsMux);
        bool priority = blockQueueCount != 0;
        if (priority) {
            queued = blockQueue[0];
            if (--blockQueueCount) blockQueue[0] = blockQueue[1];
        }
        portEXIT_CRITICAL(&statsMux);
        if (!priority && xQueueReceive(shareQueue, &queued, wait) != pdTRUE) {
            if (!wait) return false;
            wait = 0; // check candidates that arrived during the queue wait
            continue;
        }
        // A batch already running during a clean job or disconnect can enqueue
        // its result after the queue reset. It must not reach the next session.
        if (queued.generation == workGeneration && boardValid) {
            share = queued.share;
            return true;
        }
        wait = 0;
    }
}

void minerStart(unsigned selectedCoin) {
    activeMiningCoin = coin::valid(selectedCoin) ? selectedCoin : coin::BTC;
    jobMutex = xSemaphoreCreateMutex();
    shareQueue = xQueueCreate(16, sizeof(QueuedShare));
    if (!runtime::started(jobMutex && shareQueue, runtime::Miner, "miner resources")) return;
    stats.startMillis = millis();
    lastTickMs = stats.startMillis;

#ifdef BOARD_GAMMA601
    stats.engine[0] = "BM1370";
    stats.minerCount = gamma601::start() ? 1 : 0;
    return;
#endif
#ifdef BOARD_NERDNOS
    stats.engine[0] = "BM1397";
    stats.minerCount = nerdnos::start() ? 1 : 0;
    return;
#endif
    bool hw = hwsha::selfTest();
    // The classic ESP32 self-test can leave the engines reserved between
    // batches. Release them before task creation, including a failed start.
    hwsha::idle();
    logf("[miner] hardware SHA: %s\n",
         hwsha::available() ? (hw ? "self-test OK" : "self-test FAILED, disabled") : "not supported");
#ifdef HAVE_SW_PREFILTER
    useAsm = prefilterSelfTest();
    logf("[miner] software SHA: %s\n", useAsm ? "assembly (self-test OK)" : "C (assembly self-test FAILED)");
#endif

    // One miner per core. With a hardware engine it gets its own core (the
    // last one, away from WiFi on dual-core chips) and software runs on the rest.
    int cores = portNUM_PROCESSORS;
    int tasks = min(cores, MAX_MINERS);
    bool canFill = hw && hwsha::fillAvailable() && tasks < MAX_MINERS;
    bool hardwareStarted = false;
    fillIndex = -1;
    stats.minerCount = 0;
    for (int core = cores - 1; core >= 0 && stats.minerCount < tasks; core--) {
        Miner &m = miners[stats.minerCount];
        m.index = stats.minerCount;
        m.hardware = hw && core == cores - 1;
        m.useFill = m.hardware && canFill;
        stats.engine[m.index] = m.hardware ? "hw" : "sw";
        char name[12];
        snprintf(name, sizeof(name), "miner%d", m.index);
        // On a single core the network needs the CPU more than any miner.
        m.ownsCore = m.hardware && cores > 1;
        if (!runtime::started(xTaskCreatePinnedToCore(minerTask, name, 6144, &m, m.ownsCore ? kHwMinerPriority : 1, nullptr, core) == pdPASS,
                              runtime::Miner, name)) continue;
        if (m.hardware) hardwareStarted = true;
        logf("[miner] %s engine=%s core=%d\n", name, stats.engine[m.index], core);
        stats.minerCount++;
    }
    if (hardwareStarted && canFill) {
        fillIndex = stats.minerCount;
        stats.engine[fillIndex] = "fill";
        stats.minerCount++;
        logf("[miner] engine=fill: a second work in the idle cycles of miner0\n");
    }
}

MinerStats minerStats() {
    portENTER_CRITICAL(&statsMux);
    MinerStats copy = stats;
    portEXIT_CRITICAL(&statsMux);
    return copy;
}

// Time connected to a pool since boot, and the sessions that make it up.
uint32_t poolConnectedMs = 0, poolSince = 0, poolSessions = 0;

constexpr int kShareLog = 20;
ShareEntry shareLog[kShareLog];
uint32_t shareLogIds[kShareLog];
int shareLogHead = 0, shareLogCount = 0;

void minerShareSent(double difficulty, bool backup, uint32_t submissionId, const uint8_t *hash) {
    time_t now = time(nullptr);
    portENTER_CRITICAL(&statsMux);
    shareLog[shareLogHead] = ShareEntry{now > 1700000000 ? uint32_t(now) : 0, float(difficulty), SharePending, backup};
    shareLogIds[shareLogHead] = submissionId;
    shareLogHead = (shareLogHead + 1) % kShareLog;
    if (shareLogCount < kShareLog) shareLogCount++;
    if (hash) for (unsigned i = 0; i < candidateCount; ++i) {
        CandidateRecord &record = candidates[i];
        if (record.info.state == BlockQueued && !memcmp(record.info.hash, hash, 32)) {
            record.submissionId = submissionId;
            record.info.state = BlockSubmitted;
            ++candidateRevision;
        }
    }
    portEXIT_CRITICAL(&statsMux);
}

// Called with statsMux held.
void settleShare(uint32_t submissionId, uint8_t state) {
    // Track candidates independently of the short ordinary-share log.
    for (unsigned i = 0; i < candidateCount; ++i) {
        CandidateRecord &record = candidates[i];
        if (record.info.state == BlockSubmitted && record.submissionId == submissionId) {
            record.info.state = state == ShareAccepted ? BlockShareAccepted : BlockShareRejected;
            ++candidateRevision;
        }
    }
    for (int i = shareLogCount; i > 0; --i) {
        int index = (shareLogHead - i + kShareLog) % kShareLog;
        ShareEntry &e = shareLog[index];
        if (e.state == SharePending && shareLogIds[index] == submissionId) {
            e.state = state;
            return;
        }
    }
}

bool minerLastBlockCandidate(BlockCandidate &out) {
    portENTER_CRITICAL(&statsMux);
    bool found = candidateCount != 0 || havePreviousCandidate;
    if (candidateCount) out = candidates[(candidateHead + 3) % 4].info;
    else if (found) out = previousCandidate;
    portEXIT_CRITICAL(&statsMux);
    return found;
}

int minerShareLog(ShareEntry *out, int capacity) {
    portENTER_CRITICAL(&statsMux);
    int n = min(capacity, shareLogCount);
    for (int i = 0; i < n; ++i) out[i] = shareLog[(shareLogHead - 1 - i + kShareLog) % kShareLog];
    portEXIT_CRITICAL(&statsMux);
    return n;
}

void minerPoolConnected(bool connected) {
    uint32_t now = millis();
    portENTER_CRITICAL(&statsMux);
    if (connected && !stats.poolConnected) {
        poolSince = now;
        poolSessions++;
    } else if (!connected && stats.poolConnected) {
        poolConnectedMs += now - poolSince;
    }
    if (!connected) { // a lost connection takes its pending answers with it
        for (int i = 0; i < shareLogCount; ++i)
            if (shareLog[i].state == SharePending) shareLog[i].state = ShareUnanswered;
        for (unsigned i = 0; i < candidateCount; ++i)
            if (candidates[i].info.state == BlockSubmitted) {
                candidates[i].info.state = BlockUnanswered;
                ++candidateRevision;
            }
    }
    stats.poolConnected = connected;
    portEXIT_CRITICAL(&statsMux);
}

PoolUptime minerPoolUptime() {
    uint32_t now = millis();
    PoolUptime u;
    portENTER_CRITICAL(&statsMux);
    u.connectedMs = poolConnectedMs + (stats.poolConnected ? now - poolSince : 0);
    u.sessions = poolSessions;
    portEXIT_CRITICAL(&statsMux);
    return u;
}

void minerPoolDifficulty(double difficulty) {
    portENTER_CRITICAL(&statsMux);
    stats.poolDiff = difficulty;
    portEXIT_CRITICAL(&statsMux);
}

void minerShareResult(bool accepted, const stratum::ShareAck &ack) {
    portENTER_CRITICAL(&statsMux);
    settleShare(ack.id, accepted ? ShareAccepted : ShareRejected);
    if (accepted) {
        stats.accepted = stats.accepted + 1;
        // A delayed reply may arrive after vardiff or a new job. Credit the
        // submitted target, never the candidate's luck or the latest target.
        stats.acceptedWork += ack.difficulty;
    } else {
        stats.rejected = stats.rejected + 1;
        unsigned reason=unsigned(ack.reason);
        stats.rejectedReasons[reason<unsigned(stratum::RejectReason::Count)?reason:0]++;
    }
    portEXIT_CRITICAL(&statsMux);
}

uint32_t minerUptimeSeconds() {
    portENTER_CRITICAL(&statsMux);
    uint32_t value = sessionMs / 1000;
    portEXIT_CRITICAL(&statsMux);
    return value;
}

uint64_t minerTotalHashes() {
    portENTER_CRITICAL(&statsMux);
    uint64_t value = totalHashes;
    portEXIT_CRITICAL(&statsMux);
    return value;
}

double minerHashrate() {
    portENTER_CRITICAL(&statsMux);
    double value = hashrate;
    portEXIT_CRITICAL(&statsMux);
    return value;
}

double minerHashrateLive() {
    portENTER_CRITICAL(&statsMux);
    double value = hashrateLive;
    portEXIT_CRITICAL(&statsMux);
    return value;
}

double minerEngineRate(int i) {
    portENTER_CRITICAL(&statsMux);
    double value = i >= 0 && i < stats.minerCount ? engineRate[i] : 0;
    portEXIT_CRITICAL(&statsMux);
    return value;
}

void minerTick() {
    uint32_t now = millis();
    portENTER_CRITICAL(&statsMux);
    sessionMs += uint32_t(now - lastTickMs);
    HashCount delta = 0, engineDelta[MAX_MINERS];
    for (int i = 0; i < stats.minerCount; i++) {
        HashCount h = stats.hashes[i];
        engineDelta[i] = h - lastHashes[i];
        delta += engineDelta[i];
        lastHashes[i] = h;
    }
    totalHashes += delta;
    if (lastTickMs) {
        RateSample &r = rateRing[rateHead];
        r.ms = now - lastTickMs;
        for (int i = 0; i < MAX_MINERS; i++) r.hashes[i] = i < stats.minerCount ? engineDelta[i] : 0;
        rateHead = (rateHead + 1) % kRateWindow;
        if (rateCount < kRateWindow) rateCount++;
        uint32_t ms = 0;
        uint64_t sum[MAX_MINERS] = {0};
        for (int k = 0; k < rateCount; k++) {
            ms += rateRing[k].ms;
            for (int i = 0; i < MAX_MINERS; i++) sum[i] += rateRing[k].hashes[i];
        }
        if (ms) {
            double total = 0;
            for (int i = 0; i < MAX_MINERS; i++) {
                engineRate[i] = sum[i] * 1000.0 / ms;
                total += engineRate[i];
            }
            hashrate = total;
        }
        uint32_t liveMs = 0;
        uint64_t liveSum = 0;
        for (int k = 1; k <= min(rateCount, kLiveWindow); k++) {
            const RateSample &s = rateRing[(rateHead - k + kRateWindow) % kRateWindow];
            liveMs += s.ms;
            for (int i = 0; i < MAX_MINERS; i++) liveSum += s.hashes[i];
        }
        if (liveMs) hashrateLive = liveSum * 1000.0 / liveMs;
    }
    lastTickMs = now;

    stats.lifetimeHashes = baseHashes + totalHashes;
    stats.lifetimeShares = baseShares + stats.accepted;
    stats.lifetimeBlocks = baseBlocks + stats.blocksFound;
    stats.lifetimeSeconds = baseSeconds + sessionMs / 1000;

    if (!historyMs) {
        historyMs = now;
        historyHashes = totalHashes;
    } else if (now - historyMs >= HISTORY_INTERVAL_S * 1000UL) {
        HistoryPoint &point = history[historyHead];
        point.hashrate = (totalHashes - historyHashes) * 1000.0 / (now - historyMs);
        point.temperatureDeciC = temperatureSamples ? int16_t(lroundf(temperatureSum * 10 / temperatureSamples)) : INT16_MIN;
        point.powerPercent = observationSamples ? uint8_t(powerSum / observationSamples) : 0;
        point.coveragePercent = min<uint32_t>(100, uint64_t(observationSamples) * 100000 / (now - historyMs));
        temperatureSum = 0;
        temperatureSamples = powerSum = observationSamples = 0;
        historyHead = (historyHead + 1) % HISTORY_LEN;
        ++historyRevision;
        if (historyLen < HISTORY_LEN) historyLen++;
        historyMs = now;
        historyHashes = totalHashes;
    }

    // Flash wear: a found block is saved at once, a new best difficulty at
    // most once a minute, the lifetime totals every 10 minutes.
    static uint32_t lastSave = 0, retrySaveAt = 0;
    bool save = int32_t(now - retrySaveAt) >= 0 &&
                (stats.blocksFound != savedBlocks || candidateRevision != savedCandidateRevision ||
                 (bestDirty && now - lastSave > 60000) || now - lastSave > 600000);
    portEXIT_CRITICAL(&statsMux);
    if (save) {
        if (minerSaveStats(true)) { lastSave = now; retrySaveAt = now; }
        else retrySaveAt = now + 30000;
    }
}

int minerBlockBests(BlockBest *out, int capacity) {
    int n = 0;
    portENTER_CRITICAL(&statsMux);
    for (int i = 0; i < BLOCK_BESTS && n < capacity; i++) {
        const BlockBest &b = blockBests[(blockHead - i + BLOCK_BESTS) % BLOCK_BESTS];
        if (b.height) out[n++] = b;
    }
    portEXIT_CRITICAL(&statsMux);
    return n;
}

int minerTopShares(BestShare *out, int capacity) {
    int n = 0;
    portENTER_CRITICAL(&statsMux);
    for (int i = 0; i < TOP_SHARES && n < capacity && top[i].difficulty > 0; i++) out[n++] = top[i];
    portEXIT_CRITICAL(&statsMux);
    return n;
}

void minerSetPower(uint8_t percent, bool batterySaver) {
    batteryLimited = batterySaver;
    safety.update(millis(), percent);
}

void minerRenewSafety() { safety.renew(millis()); }

uint8_t minerPowerPercent() { return ota::busy() ? 0 : safety.power(millis()); }

Throttle minerThrottle() {
    uint8_t power = minerPowerPercent();
    return !power ? Throttle::Paused : power == 100 ? Throttle::Full
                               : batteryLimited ? Throttle::Half : Throttle::Reduced;
}

int minerHistory(float *out, int capacity) {
    portENTER_CRITICAL(&statsMux);
    int n = min(historyLen, capacity);
    for (int i = 0; i < n; i++) out[i] = history[(historyHead - n + i + HISTORY_LEN) % HISTORY_LEN].hashrate;
    portEXIT_CRITICAL(&statsMux);
    return n;
}

int minerHistoryPoints(HistoryPoint *out, int capacity, uint32_t *revision) {
    portENTER_CRITICAL(&statsMux);
    int n = min(historyLen, capacity);
    for (int i = 0; i < n; ++i) out[i] = history[(historyHead - n + i + HISTORY_LEN) % HISTORY_LEN];
    if (revision) *revision = historyRevision;
    portEXIT_CRITICAL(&statsMux);
    return n;
}

double minerTakePeriodBest() {
    portENTER_CRITICAL(&statsMux);
    double best = stats.periodBestDiff;
    stats.periodBestDiff = 0;
    portEXIT_CRITICAL(&statsMux);
    return best;
}

uint32_t minerHistoryRevision() {
    portENTER_CRITICAL(&statsMux);
    uint32_t revision = historyRevision;
    portEXIT_CRITICAL(&statsMux);
    return revision;
}

void minerObserve(float temperature, uint8_t effectivePower) {
    portENTER_CRITICAL(&statsMux);
    if (std::isfinite(temperature) && temperature > -100 && temperature < 200) {
        temperatureSum += temperature;
        ++temperatureSamples;
    }
    powerSum += effectivePower > 100 ? 100 : effectivePower;
    ++observationSamples;
    portEXIT_CRITICAL(&statsMux);
}

void minerProfiles(MinerProfile *out, int capacity) {
    portENTER_CRITICAL(&statsMux);
    for (int i = 0; i < min(capacity, MAX_MINERS); ++i) out[i] = profiles[i];
    portEXIT_CRITICAL(&statsMux);
}

namespace {

// One saved record, written in turn to the slots "statA" and "statB" (the
// scheme of NerdMiner Modded): at boot the newest slot with a good CRC wins.
constexpr uint32_t kStatsMagic = 0x314C4442;  // "BDL1"
struct StatsRecord {
    uint32_t magic;
    uint32_t seq;
    StatsSnapshot s;
    uint32_t crc;  // over every byte before it
};

void findHistory();  // below, with the history store

uint32_t statsSeq = 0;
bool statsLoaded = false;  // never save before the saved stats were read back
bool statsFrozen = false;  // a restored backup waits for the restart
SemaphoreHandle_t saveMutex = xSemaphoreCreateMutex();

// Independent extension: keep the original statA/statB layout readable by
// rollback firmware. Never persist RTOS/session IDs or replay old submissions.
constexpr uint32_t kCandidateMagic = 0x31434c42; // BLC1
constexpr uint32_t kMultiCoinCandidateMagic = 0x32434c42; // BLC2: older firmware ignores non-BTC proofs
struct CandidateSave {
    uint32_t magic, seq, height;
    uint8_t hash[32];
    uint8_t state, coin, reserved[2];
    uint32_t crc;
};
uint32_t candidateSaveSeq = 0;
uint32_t candidateCrc(const CandidateSave &r) {
    return esp_rom_crc32_le(0, (const uint8_t *)&r, offsetof(CandidateSave, crc));
}
bool readCandidateSlot(Preferences &p, const char *key, CandidateSave &r) {
    return p.getBytesLength(key) == sizeof(r) && p.getBytes(key, &r, sizeof(r)) == sizeof(r) &&
           ((r.magic == kCandidateMagic && r.coin == coin::BTC) ||
            (r.magic == kMultiCoinCandidateMagic && r.coin != coin::BTC && coin::valid(r.coin))) &&
           r.state <= BlockQueueFull && r.crc == candidateCrc(r);
}
void loadCandidate() {
    Preferences p;
    if (!p.begin("stats", true)) return;
    CandidateSave a{}, b{};
    bool okA = readCandidateSlot(p, "blockA", a), okB = readCandidateSlot(p, "blockB", b);
    p.end();
    if (!okA && !okB) return;
    const auto &r = okA && (!okB || int32_t(a.seq - b.seq) > 0) ? a : b;
    candidateSaveSeq = r.seq;
    previousCandidate = {};
    previousCandidate.height = r.height;
    previousCandidate.coin = r.coin;
    memcpy(previousCandidate.hash, r.hash, 32);
    previousCandidate.state = r.state == BlockQueued ? BlockStale : r.state == BlockSubmitted ? BlockUnanswered : r.state;
    previousCandidate.previousSession = true;
    havePreviousCandidate = true;
}
bool saveCandidate() {
    CandidateSave r{};
    uint32_t revision;
    portENTER_CRITICAL(&statsMux);
    revision = candidateRevision;
    bool needed = candidateCount && revision != savedCandidateRevision;
    if (needed) {
        const auto &info = candidates[(candidateHead + 3) % 4].info;
        r.height = info.height; r.state = info.state; r.coin = info.coin;
        memcpy(r.hash, info.hash, 32);
    }
    portEXIT_CRITICAL(&statsMux);
    if (!needed) return true;
    r.magic = r.coin == coin::BTC ? kCandidateMagic : kMultiCoinCandidateMagic;
    r.seq = candidateSaveSeq + 1; r.crc = candidateCrc(r);
    Preferences p;
    if (!p.begin("stats", false)) return false;
    const char *key = r.seq & 1 ? "blockB" : "blockA";
    CandidateSave verified{};
    bool ok = p.putBytes(key, &r, sizeof(r)) == sizeof(r) && readCandidateSlot(p, key, verified) && verified.seq == r.seq;
    p.end();
    if (ok) {
        candidateSaveSeq = r.seq;
        portENTER_CRITICAL(&statsMux);
        savedCandidateRevision = revision; // a later change stays dirty
        portEXIT_CRITICAL(&statsMux);
    } else logf("[stats] candidate save failed; previous proof retained\n");
    return ok;
}

uint32_t recordCrc(const StatsRecord &r) {
    return esp_rom_crc32_le(0, (const uint8_t *)&r, offsetof(StatsRecord, crc));
}

bool readSlot(Preferences &p, const char *key, StatsRecord &r) {
    if (p.getBytesLength(key) != sizeof(r)) return false;
    if (p.getBytes(key, &r, sizeof(r)) != sizeof(r)) return false;
    return r.magic == kStatsMagic && r.crc == recordCrc(r);
}

bool writeRecord(const StatsSnapshot &s) {
    StatsRecord r;
    memset(&r, 0, sizeof(r));
    r.magic = kStatsMagic;
    r.seq = statsSeq + 1;
    r.s = s;
    r.crc = recordCrc(r);
    Preferences p;
    if (!p.begin("stats", false)) return false;
    const char *key = r.seq & 1 ? "statB" : "statA";
    StatsRecord verified;
    bool ok = p.putBytes(key, &r, sizeof(r)) == sizeof(r) && readSlot(p, key, verified) && verified.seq == r.seq;
    p.end();
    if (!ok) logf("[stats] save failed; previous record kept\n");
    else statsSeq = r.seq;
    return ok;
}

}  // namespace

void minerLoadStats() {
    Preferences p;
    p.begin("stats", true);
    StatsRecord a, b;
    bool okA = readSlot(p, "statA", a), okB = readSlot(p, "statB", b);
    StatsSnapshot s{};
    if (okA || okB) {
        const StatsRecord &r = okA && (!okB || (int32_t)(a.seq - b.seq) > 0) ? a : b;
        s = r.s;
        statsSeq = r.seq;
        logf("[stats] restored save #%u%s\n", (unsigned)r.seq, okA && okB ? "" : " (one slot only)");
    } else if (p.isKey("hashes")) {
        // The separate keys of earlier versions, read once.
        s.bestDiff = p.getDouble("best", 0);
        s.hashes = p.getULong64("hashes", 0);
        s.shares = p.getULong("shares", 0);
        s.blocks = p.getULong("blocks", 0);
        s.seconds = p.getULong("seconds", 0);
        p.getBytes("top", s.top, sizeof(s.top));
        logf("[stats] restored from the previous format\n");
    }
    p.end();

    stats.bestDiff = s.bestDiff;
    baseHashes = s.hashes;
    baseShares = s.shares;
    baseBlocks = s.blocks;
    baseSeconds = s.seconds;
    memcpy(top, s.top, sizeof(top));
    stats.lifetimeHashes = baseHashes;
    stats.lifetimeShares = baseShares;
    stats.lifetimeBlocks = baseBlocks;
    stats.lifetimeSeconds = baseSeconds;
    statsLoaded = true;
    loadCandidate();
    findHistory();
}

void minerStatsSnapshot(StatsSnapshot &out) {
    memset(&out, 0, sizeof(out));
    portENTER_CRITICAL(&statsMux);
    out.hashes = stats.lifetimeHashes;
    out.shares = stats.lifetimeShares;
    out.blocks = stats.lifetimeBlocks;
    out.seconds = stats.lifetimeSeconds;
    out.bestDiff = stats.bestDiff;
    memcpy(out.top, top, sizeof(out.top));
    portEXIT_CRITICAL(&statsMux);
}

namespace {

// The 24-hour hashrate history, kept in the "spiffs" data partition (unused
// otherwise) so that a restart does not empty the chart. Each save goes to the
// next of up to 64 sectors: the header is written last, so a save cut short
// leaves the previous record the newest valid one.
constexpr uint32_t kHistoryMagic = 0x48444C42;  // "BLDH"
constexpr uint32_t kSector = 4096, kPointsAt = 32, kMaxSectors = 64;
constexpr uint32_t kHistorySaveMs = 30 * 60 * 1000;
struct HistoryHeader {
    uint32_t magic, seq;
    uint32_t lastPointAt;  // Unix time of the newest point
    uint32_t count;
    uint32_t crc;          // over the points, then the fields above
};
static_assert(kPointsAt + HISTORY_LEN * sizeof(HistoryPoint) <= kSector, "one record per sector");
const esp_partition_t *historyPart = nullptr;
uint32_t historySectors = 0, historySeq = 0, historyNext = 0, historySavedAt = 0, historySavedRevision = 0;
// The record found at boot, merged once the clock is set.
bool restorePending = false;
uint32_t restoreSector = 0, restoreCount = 0, restoreLastAt = 0;

uint32_t headerCrc(const HistoryHeader &h, uint32_t crc) {
    return esp_rom_crc32_le(crc, (const uint8_t *)&h, offsetof(HistoryHeader, crc));
}

// Reads `count` points of a record from `first` on, in small pieces.
bool readPoints(uint32_t sector, uint32_t first, uint32_t count, HistoryPoint *out, uint32_t *crc) {
    HistoryPoint chunk[32];
    for (uint32_t done = 0; done < count;) {
        uint32_t n = std::min<uint32_t>(32, count - done);
        if (esp_partition_read(historyPart, sector * kSector + kPointsAt + (first + done) * sizeof(HistoryPoint), chunk,
                               n * sizeof(HistoryPoint)) != ESP_OK)
            return false;
        if (crc) *crc = esp_rom_crc32_le(*crc, (const uint8_t *)chunk, n * sizeof(HistoryPoint));
        if (out) memcpy(out + done, chunk, n * sizeof(HistoryPoint));
        done += n;
    }
    return true;
}

void findHistory() {
    historyPart = esp_partition_find_first(ESP_PARTITION_TYPE_DATA, ESP_PARTITION_SUBTYPE_DATA_SPIFFS, nullptr);
    if (!historyPart) return;
    historySectors = std::min<uint32_t>(kMaxSectors, historyPart->size / kSector);
    bool found = false;
    for (uint32_t i = 0; i < historySectors; ++i) {
        HistoryHeader h;
        if (esp_partition_read(historyPart, i * kSector, &h, sizeof(h)) != ESP_OK || h.magic != kHistoryMagic ||
            h.count == 0 || h.count > HISTORY_LEN || (found && int32_t(h.seq - historySeq) <= 0))
            continue;
        uint32_t crc = 0;
        if (!readPoints(i, 0, h.count, nullptr, &crc) || headerCrc(h, crc) != h.crc) continue;
        found = true;
        historySeq = h.seq;
        historyNext = (i + 1) % historySectors;
        restoreSector = i;
        restoreCount = h.count;
        restoreLastAt = h.lastPointAt;
    }
    restorePending = found;
}

// Writes the history as it is now; false if it changed meanwhile or could not be written.
bool saveHistory() {
    time_t now = time(nullptr);
    if (!historyPart || !historySectors || now < 1700000000) return false;  // points need a date
    portENTER_CRITICAL(&statsMux);
    uint32_t count = historyLen, head = historyHead, revision = historyRevision;
    uint32_t lastAt = uint32_t(now) - (millis() - historyMs) / 1000;
    portEXIT_CRITICAL(&statsMux);
    if (!count) return false;
    uint32_t sector = historyNext;
    if (esp_partition_erase_range(historyPart, sector * kSector, kSector) != ESP_OK) return false;
    uint32_t crc = 0;
    HistoryPoint chunk[32];
    for (uint32_t done = 0; done < count;) {
        uint32_t n = std::min<uint32_t>(32, count - done);
        bool same = true;
        portENTER_CRITICAL(&statsMux);
        same = historyRevision == revision;
        for (uint32_t i = 0; same && i < n; ++i)
            chunk[i] = history[(head - count + done + i + HISTORY_LEN) % HISTORY_LEN];
        portEXIT_CRITICAL(&statsMux);
        if (!same) return false;  // a new point arrived: next time
        crc = esp_rom_crc32_le(crc, (const uint8_t *)chunk, n * sizeof(HistoryPoint));
        if (esp_partition_write(historyPart, sector * kSector + kPointsAt + done * sizeof(HistoryPoint), chunk,
                                n * sizeof(HistoryPoint)) != ESP_OK)
            return false;
        done += n;
    }
    HistoryHeader h{kHistoryMagic, historySeq + 1, lastAt, count, 0};
    h.crc = headerCrc(h, crc);
    if (esp_partition_write(historyPart, sector * kSector, &h, sizeof(h)) != ESP_OK) return false;
    historySeq = h.seq;
    historyNext = (sector + 1) % historySectors;
    historySavedAt = millis();
    historySavedRevision = revision;
    logf("[stats] hashrate history saved: %u points\n", (unsigned)count);
    return true;
}

}  // namespace

void minerHistoryPoll() {
    if (!restorePending) return;
    uint32_t up = millis();
    time_t now = time(nullptr);
    // The first new point comes after five minutes: merging stops being simple then.
    if (up > (HISTORY_INTERVAL_S - 20) * 1000UL) {
        restorePending = false;
        logf("[stats] hashrate history not restored: clock not set in time\n");
        return;
    }
    if (now < 1700000000) return;
    restorePending = false;
    portENTER_CRITICAL(&statsMux);
    bool empty = historyLen == 0;
    portEXIT_CRITICAL(&statsMux);
    if (!empty) return;
    int64_t off = int64_t(now) - up / 1000 - restoreLastAt;  // seconds between the last point and this boot
    uint32_t missing = off > 0 ? uint32_t(off / HISTORY_INTERVAL_S) : 0;
    if (missing >= HISTORY_LEN) return;  // all older than 24 hours
    uint32_t keep = std::min<uint32_t>(restoreCount, HISTORY_LEN - missing);
    // Only this task adds points, and none exists yet: readers see historyLen 0 until the end.
    if (!readPoints(restoreSector, restoreCount - keep, keep, history, nullptr)) return;
    for (uint32_t i = 0; i < missing; ++i) history[keep + i] = HistoryPoint{0, INT16_MIN, 0, 0};  // off: no observation
    portENTER_CRITICAL(&statsMux);
    historyLen = keep + missing;
    historyHead = historyLen % HISTORY_LEN;
    ++historyRevision;
    portEXIT_CRITICAL(&statsMux);
    logf("[stats] hashrate history restored: %u points, %u while off\n", (unsigned)keep, (unsigned)missing);
}

bool minerSaveStats(bool periodic) {
    if (!saveMutex) return false;
    xSemaphoreTake(saveMutex, portMAX_DELAY);
    if (!statsLoaded || statsFrozen) {
        xSemaphoreGive(saveMutex);
        return true;
    }
    // Restarts save the history too; the periodic save, every 30 minutes.
    if ((!periodic || millis() - historySavedAt >= kHistorySaveMs) && historyRevision != historySavedRevision &&
        !restorePending && !saveHistory())
        historySavedAt = millis() - kHistorySaveMs + 60000;  // try again in a minute
    StatsSnapshot s;
    minerStatsSnapshot(s);
    bool ok = writeRecord(s);
    if (ok) {
        portENTER_CRITICAL(&statsMux);
        bestDirty = stats.bestDiff > s.bestDiff;
        savedBlocks = s.blocks - baseBlocks;
        portEXIT_CRITICAL(&statsMux);
    }
    bool saved = ok && saveCandidate();
    xSemaphoreGive(saveMutex);
    return saved;
}

bool minerStatsRestore(const StatsSnapshot &s) {
    if (!saveMutex) return false;
    xSemaphoreTake(saveMutex, portMAX_DELAY);
    bool ok = writeRecord(s);
    if (ok) statsFrozen = true;
    xSemaphoreGive(saveMutex);
    return ok;
}

#ifdef BOARD_ASIC
bool minerAsicWork(AsicWork &out) {
    if (!jobMutex || xSemaphoreTake(jobMutex, pdMS_TO_TICKS(20)) != pdTRUE) return false;
    bool valid = boardValid;
    if (valid) {
        work::build(board, extranonce2Counter++, out);
        out.generation = workGeneration; out.height = board.blockHeight(activeMiningCoin);
        // The ASIC ORs its rolling bits into the job version. Clear only negotiated bits.
        uint32_t version = board.version & ~out.versionMask;
        memcpy(out.header, &version, 4);
    }
    xSemaphoreGive(jobMutex);
    return valid;
}
bool minerAsicCurrent(uint32_t generation) { return boardValid && generation == workGeneration.load(); }
bool minerAsicReady() { return boardValid; }
double minerAsicCandidate(AsicWork &w, uint32_t nonce, uint32_t bits, double minimumDifficulty) {
    if (!minerAsicCurrent(w.generation) || (bits & ~w.versionMask)) return -1;
    AsicWork candidate = w;
    uint32_t version; memcpy(&version, candidate.header, 4); version |= bits;
    memcpy(candidate.header, &version, 4); memcpy(candidate.header + 76, &nonce, 4);
    uint8_t hash[32]; sha::sha256d(candidate.header, 80, hash);
    // Independently reject UART corruption/hardware errors before counting a share.
    double difficulty = work::hashDifficulty(hash);
    if (!minerAsicCurrent(w.generation)) return -1;
    if (difficulty >= std::max(1.0, minimumDifficulty)) onCandidate(nonce, hash, &candidate);
    return difficulty;
}
void minerAsicHashes(uint64_t hashes) {
    portENTER_CRITICAL(&statsMux); stats.hashes[0] += hashes; portEXIT_CRITICAL(&statsMux);
}
#endif
