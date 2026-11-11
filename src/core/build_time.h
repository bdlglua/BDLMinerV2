// The day this firmware was built, as a lower bound for a clock that comes
// from the network.
#pragma once

#include <cstdint>
#include <cstring>

namespace build_time {

// The compiler's __DATE__ ("Nov  9 2026") as the Unix time of that day at
// 00:00 UTC; 0 when it does not read as a date.
inline uint32_t epoch(const char *date) {
    static const char kMonths[] = "JanFebMarAprMayJunJulAugSepOctNovDec";
    if (!date || std::strlen(date) != 11 || date[3] != ' ' || date[6] != ' ') return 0;
    int month = -1;
    for (int m = 0; m < 12; ++m)
        if (!std::strncmp(date, kMonths + 3 * m, 3)) month = m + 1;
    auto digit = [](char c) { return c >= '0' && c <= '9'; };
    if (month < 0 || !(date[4] == ' ' || digit(date[4])) || !digit(date[5])) return 0;
    int day = (date[4] == ' ' ? 0 : date[4] - '0') * 10 + (date[5] - '0'), year = 0;
    for (int i = 7; i < 11; ++i) {
        if (!digit(date[i])) return 0;
        year = year * 10 + (date[i] - '0');
    }
    if (day < 1 || day > 31 || year < 1970 || year > 2105) return 0;
    // Days since 1970-01-01 (Howard Hinnant's days_from_civil).
    int y = month <= 2 ? year - 1 : year;
    int era = y / 400, yoe = y - era * 400;
    int doy = (153 * (month + (month > 2 ? -3 : 9)) + 2) / 5 + day - 1;
    int64_t days = int64_t(era) * 146097 + (yoe * 365 + yoe / 4 - yoe / 100 + doy) - 719468;
    int64_t seconds = days * 86400;
    return seconds > 0 && seconds <= UINT32_MAX ? uint32_t(seconds) : 0;
}

// The oldest time a network source may set the clock to: no real "now" is
// before the build. Two days of margin cover the build machine's time zone
// and a block's timestamp, which may trail the clock by a couple of hours.
// Never below `minimum`, the firmware's own "clock is set" threshold.
inline uint32_t oldestPlausible(const char *date, uint32_t minimum) {
    uint32_t built = epoch(date);
    uint32_t floor = built > 2 * 86400u ? built - 2 * 86400u : 0;
    return floor > minimum ? floor : minimum;
}

}  // namespace build_time
