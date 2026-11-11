// The firmware's build day as a floor for a clock set from the network.
#include <cassert>
#include <cstdio>
#include <initializer_list>

#include "core/build_time.h"

int main() {
    using build_time::epoch;
    using build_time::oldestPlausible;
    assert(epoch("Jan  1 1970") == 0);  // not after the epoch: unusable
    assert(epoch("Jan  2 1970") == 86400);
    assert(epoch("Nov 14 2023") == 1699920000);
    assert(epoch("Feb 29 2024") == 1709164800 && epoch("Mar  1 2024") == 1709251200);
    assert(epoch("Dec 31 2026") == 1798675200 && epoch("Jan  1 2027") == 1798761600);
    assert(epoch("Nov  9 2026") == 1794182400 && epoch("Nov 11 2026") == 1794355200);
    assert(epoch("Jan 19 2038") == 2147472000 && epoch("Feb  7 2106") == 0);  // past 32 bits
    for (const char *bad : {"", "Nov 11 26", "Nev 11 2026", "Nov 1a 2026", "Nov 00 2026", "Nov 32 2026", "Nov-11-2026",
                            "Nov 11 20x6", "nov 11 2026", "Nov 11 2026 ", "Nov 11  2026"})
        assert(epoch(bad) == 0);
    assert(epoch(nullptr) == 0);
    assert(epoch(__DATE__) > 1790000000);  // this very build

    // Two days before the build, never under the firmware's own threshold.
    assert(oldestPlausible("Nov 11 2026", 1700000000) == 1794355200 - 172800);
    assert(oldestPlausible("Nov 14 2023", 1700000000) == 1700000000);
    assert(oldestPlausible("garbage", 1700000000) == 1700000000 && oldestPlausible(nullptr, 5) == 5);
    puts("Build time: __DATE__ to Unix time, leap years, unreadable dates and the network clock floor passed");
}
