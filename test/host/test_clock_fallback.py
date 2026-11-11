#!/usr/bin/env python3
"""Execute the production clock fallback: the pool's job time when NTP is blocked."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
source = (root / 'src/market.cpp').read_text()
fallback = source[source.index('// Some networks block NTP.'):source.index('void marketTask(')]
prefix = r'''
#include <cassert>
#include <cstdint>
#include <cstdio>
#include <ctime>
#include <initializer_list>
#include "core/build_time.h"
uint32_t uptime = 0, jobTime = 0;
time_t clock_ = 0;
int sets = 0;
bool settable = true;
uint32_t millis() { return uptime; }
uint32_t minerJobTime() { return jobTime; }
bool tlsClockSet() { return clock_ >= 1700000000; }
int settimeofday(const timeval *tv, const void *) { if (!settable) return -1; clock_ = tv->tv_sec; ++sets; return 0; }
void logf(const char *, ...) {}
'''
tests = r'''
int main() {
    // Times well after any build of this test: 4000000000 is in 2096.
    const uint32_t later = 4000000000u;
    // NTP gets its first minute; no job yet: nothing to set the clock from.
    uptime = 30000; jobTime = later; clockFromPool(); assert(sets == 0);
    uptime = 61000; jobTime = 0; clockFromPool(); assert(sets == 0);
    // NTP answered meanwhile: its time stands.
    clock_ = time_t(later) + 500; jobTime = later; clockFromPool(); assert(sets == 0 && clock_ == time_t(later) + 500);
    // NTP blocked, but a job time from before this firmware was built is not the time:
    // it would make an expired certificate look valid.
    clock_ = 5;
    const uint32_t built = build_time::epoch(__DATE__);
    assert(built > 1790000000);
    for (uint32_t old : {1700000000u, 1750000000u, built - 3 * 86400u}) {
        jobTime = old; clockFromPool(); assert(sets == 0 && clock_ == 5);
    }
    // NTP blocked: the pool's job time, once.
    jobTime = later; settable = false; clockFromPool(); assert(sets == 0);
    settable = true; clockFromPool(); assert(sets == 1 && clock_ == time_t(later));
    clock_ = 5; jobTime = later + 900; clockFromPool(); assert(sets == 1);
    puts("Clock fallback: NTP first, then the pool's job time once, never over a set clock or from before the build passed");
}
'''
with tempfile.TemporaryDirectory(prefix='bdl-clock-') as tmp:
    test = Path(tmp) / 'test.cpp'
    binary = Path(tmp) / 'test'
    test.write_text(prefix + fallback + tests)
    subprocess.run(['g++', '-std=c++17', '-O2', '-Wall', '-I' + str(root / 'src'), str(test), '-o', str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
