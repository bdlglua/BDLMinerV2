// The miner interface of the C++ firmware (miner.h, log.h) that the ASIC
// drivers call, answered by the Rust miner (bdl_asic_rs_*).
#include "bdl_asic.h"

#include <cstdarg>
#include <cstdio>
#include <cstring>

#include "log.h"
#include "miner.h"

void logf(const char *fmt, ...) {
    char line[256];
    va_list args;
    va_start(args, fmt);
    vsnprintf(line, sizeof(line), fmt, args);
    va_end(args);
    size_t n = strlen(line);
    while (n && (line[n - 1] == '\n' || line[n - 1] == '\r')) line[--n] = 0;
    bdl_asic_rs_log(line);
}

uint8_t minerPowerPercent() { return bdl_asic_rs_power_percent(); }

bool minerAsicReady() { return bdl_asic_rs_ready(); }

bool minerAsicWork(AsicWork &out) {
    uint32_t version_mask = 0, generation = 0, height = 0;
    double share_difficulty = 1.0;
    if (!bdl_asic_rs_work(out.header, &version_mask, &share_difficulty, &generation, &height)) return false;
    out.versionMask = version_mask;
    out.shareDifficulty = share_difficulty;
    out.generation = generation;
    out.height = height;
    return true;
}

bool minerAsicCurrent(uint32_t generation) { return bdl_asic_rs_current(generation); }

double minerAsicCandidate(AsicWork &work, uint32_t nonce, uint32_t versionBits, double minimumDifficulty) {
    return bdl_asic_rs_candidate(work.header, work.generation, nonce, versionBits, minimumDifficulty);
}

void minerAsicHashes(uint64_t hashes) { bdl_asic_rs_hashes(hashes); }
