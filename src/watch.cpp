#include "watch.h"

#include <Arduino.h>
#include <Preferences.h>

#include <algorithm>
#include <memory>
#include <new>

#include "board.h"
#include "core/mining_safety.h"
#include "log.h"
#include "market.h"
#include "miner.h"
#include "net.h"
#include "notify.h"
#include "ota.h"
#include "settings.h"
#include "ui.h"

namespace {

constexpr uint32_t kNoPoolRestartMs = 30 * 60 * 1000;  // not connected that long: restart
constexpr uint32_t kOfflineNotifyMs = 10 * 60 * 1000;
constexpr uint32_t kBestNotifyMs = 10 * 60 * 1000;   // at most one record notification per 10 min
constexpr uint32_t kAlertWarmupMs = 5 * 60 * 1000;   // the minute average needs time after boot
constexpr int kNormalPoints = 6;                     // 30 minutes of full-speed history at least

std::atomic<bool> night{false};
std::atomic<const char *> limitReason{"supervision"};
mining::ThermalPolicy thermal = [] {
    mining::ThermalPolicy policy;
#ifdef BOARD_ASIC
    policy.steps = false;     // each power change restarts the ASIC
    policy.holdMs = 120000;   // its sensor is silent while it is off
#endif
    return policy;
}();
bool awake = false;
uint32_t wakeStarted = 0;  // night mode suspended for 30 s after a button press

void restart(const char *why) {
    ota::Guard reboot(ota::Owner::Restart);
    if (!reboot) return; // defer automatic restarts until the active OTA finishes
    logf("[watch] restarting: %s\n", why);
    minerSaveStats();  // what was mined since the last periodic save
    setRestartReason(why);
    delay(500);
    ESP.restart();
}

bool nightNow() {
    if (settings.nightFrom > 23 || settings.nightTo > 23 || settings.nightFrom == settings.nightTo) return false;
    struct tm t;
    if (!clockNow(t)) return false;
    int h = t.tm_hour;
    return settings.nightFrom < settings.nightTo ? h >= settings.nightFrom && h < settings.nightTo
                                                 : h >= settings.nightFrom || h < settings.nightTo;
}

void throttle() {
    board::sampleTemperature();
    float t = board::temperature();
    bool wasHot = thermal.hot, wasFault = thermal.sensorFault;
    bool saver = settings.batterySaver && board::onBattery() == 1;
    uint8_t power = thermal.power(t, settings.maxTemp, settings.miningPower, saver, millis());
    const char *reason = "full";
    if (!settings.miningPower) reason = "user_pause";
    else if (thermal.sensorFault) reason = "sensor";
    else if (thermal.hot) reason = "hot";
    else if (settings.maxTemp && std::isfinite(t) && t >= settings.maxTemp - 10 && power < settings.miningPower) reason = "thermal";
    else if (saver && settings.miningPower > 50) reason = "battery";
    else if (settings.miningPower < 100) reason = "user";
    limitReason = reason;
    if (thermal.hot != wasHot) {
        if (thermal.hot) {
            char msg[64];
            snprintf(msg, sizeof(msg), "%.0f C, mining paused until %u C", t, (unsigned)(settings.maxTemp - 5));
            logf("[watch] too hot: %s\n", msg);
            notify("Miner too hot", msg, 4, "thermometer");
        } else {
            logf("[watch] cooled down to %.0f C, mining again\n", t);
            notify("Miner cooled down", "Mining again", 2, "thermometer");
        }
    }
    if (thermal.sensorFault && !wasFault) {
        logf("[watch] temperature unavailable: mining %s\n", power ? "limited to 25%" : "paused");
        notify("Temperature unavailable", power ? "No usable sensor: mining limited to 25%" : "Mining paused until a valid temperature returns", 4, "thermometer");
    }
    minerSetPower(power, saver);
    minerObserve(t, minerPowerPercent());

    uint8_t b = settingsBrightness();
    if (awake && millis() - wakeStarted >= 30000) awake = false;
    if (night && !awake) b = 0;
    else if (saver && b > 20) b = 20;
    ui::setBacklight(b);
}

void alerts(uint32_t now, const MinerStats &stats);
void summary(uint32_t now, const MinerStats &stats);

void blockAlert() {
    static uint32_t lastBlocks = 0; // session count: include candidates before the first watch tick
    BlockCandidate candidate{};
    if (!minerLastBlockCandidate(candidate) || candidate.previousSession || candidate.number == lastBlocks) return;
    lastBlocks = candidate.number;
    char msg[160];
    snprintf(msg, sizeof(msg), "%s block candidate at height %u by %s. Check pool submission and chain confirmation.",
             coin::ticker(candidate.coin), (unsigned)candidate.height, board::modelName());
    notify("BLOCK CANDIDATE", msg, 5, "tada");
    logf("[miner] block candidate alert at height %u; chain confirmation unknown\n", (unsigned)candidate.height);
    watchWake();
    ui::blockFound(candidate.height);
    if (settings.beep) board::beep();
}

void events(uint32_t now) {
    const MinerStats stats = minerStats();
    static bool first = true;
    static uint32_t lastOver32, lastAccepted, acceptedAt, lastConnected;
    static double lastBest;
    static uint32_t bestAt = 0;
    static bool offlineSent = false, bootSent = false;
    if (first) {
        first = false;
        lastOver32 = stats.over32bits;
        lastAccepted = stats.accepted;
        acceptedAt = lastConnected = now;
        lastBest = stats.bestDiff;
    }
    char msg[160];

    blockAlert();

    // The red LED lights for a second on each 32-bit share (not at night).
    // An ASIC finds several every second (its own filter is above difficulty
    // 1): no log line, beep or notification for each there, they would drown
    // the log (1.5 minutes of it on a Gamma 601) and spam the phone.
    bool led = false;
#ifdef BOARD_ASIC
    lastOver32 = stats.over32bits;
#endif
    if (stats.over32bits != lastOver32) {
        lastOver32 = stats.over32bits;
        logf("[miner] 32-bit share found (%u so far)\n", (unsigned)lastOver32);
        if (settings.beep && !night) board::beep();
        led = !night;
        if (settings.notify32) {
            snprintf(msg, sizeof(msg), "%u so far", (unsigned)lastOver32);
            notify("32-bit share", msg, 2, "sparkles");
        }
    }

    // A new all-time best of at least difficulty 1, at most every 10 minutes.
    if (stats.bestDiff > lastBest && stats.bestDiff >= 1.0 && (!bestAt || now - bestAt > kBestNotifyMs)) {
        lastBest = stats.bestDiff;
        bestAt = now;
        snprintf(msg, sizeof(msg), "New best difficulty ever: %.4g", stats.bestDiff);
        notify("New record", msg, 3, "trophy");
    }

    if (stats.accepted != lastAccepted) {
        lastAccepted = stats.accepted;
        acceptedAt = now;
        ui::flashShare();
    }
    // A paused miner (power 0 %, too hot, firmware update) finds no share by
    // design: that time must not count towards the restart below.
    if (!minerPowerPercent()) acceptedAt = now;
    board::flashLed(led);

    if (stats.poolConnected) {
        lastConnected = now;
        if (offlineSent) {
            offlineSent = false;
            notify("Miner back online", netStatus().c_str(), 2, "white_check_mark");
        }
        if (!bootSent) {
            bootSent = true;
            // Not after a power-up, the reset button, or a restart asked for
            // from the web page.
            if (strcmp(bootReason(), "power on") && strcmp(bootReason(), "reset pin") &&
                !strstr(bootReason(), "web page")) {
                snprintf(msg, sizeof(msg), "Last reset: %s", bootReason());
                notify("Miner restarted", msg, 3, "warning");
            }
        }
    } else if (!offlineSent && now - lastConnected > kOfflineNotifyMs) {
        offlineSent = true;
        snprintf(msg, sizeof(msg), "Not mining for 10 minutes: %s", netStatus().c_str());
        notify("Miner offline", msg, 4, "warning");
    }

    alerts(now, stats);
    summary(now, stats);

    if (now - lastConnected > kNoPoolRestartMs) restart("no pool connection for 30 minutes");
    if (settings.rebootHours && now - acceptedAt > settings.rebootHours * 3600000UL) {
        snprintf(msg, sizeof(msg), "no accepted share for %u hours", settings.rebootHours);
        restart(msg);
    }
}

// Median of the full-speed history points: what this board normally does.
// Recomputed when a point is added (every 5 minutes), 0 while unknown.
double normalHashrate() {
    static double normal = 0;
    static uint32_t seen = UINT32_MAX;
    uint32_t revision = minerHistoryRevision();
    if (revision == seen) return normal;
    std::unique_ptr<HistoryPoint[]> points(new (std::nothrow) HistoryPoint[HISTORY_LEN]);
    std::unique_ptr<float[]> rates(new (std::nothrow) float[HISTORY_LEN]);
    if (!points || !rates) return normal;  // try again next second
    seen = revision;
    int count = minerHistoryPoints(points.get(), HISTORY_LEN), n = 0;
    for (int i = 0; i < count; i++) {
        const HistoryPoint &p = points[i];
        if (p.powerPercent >= 100 && p.coveragePercent >= 90 && p.hashrate > 0) rates[n++] = p.hashrate;
    }
    if (n < kNormalPoints) return normal = 0;
    std::nth_element(rates.get(), rates.get() + n / 2, rates.get() + n);
    return normal = rates[n / 2];
}

// Hashrate well below normal for settings.alertMinutes, and many rejected
// shares over the same span: both usually mean something to look at.
void alerts(uint32_t now, const MinerStats &stats) {
    static uint32_t lowSince = 0, windowAt = 0, accepted0 = 0, rejected0 = 0;
    static bool lowSent = false, started = false;
    uint32_t span = settings.alertMinutes * 60000UL;
    char msg[120];

    double normal = settings.alertPercent ? normalHashrate() : 0;
    bool comparable = normal > 0 && now >= kAlertWarmupMs && stats.poolConnected && settings.miningPower >= 100 &&
                      minerPowerPercent() >= 100 && !ota::busy();
    double rate = minerHashrate();
    if (!comparable) {
        lowSince = 0;  // paused, throttled or offline: start over
    } else if (rate < normal * settings.alertPercent / 100) {
        if (!lowSince) lowSince = now;
        if (!lowSent && now - lowSince >= span) {
            lowSent = true;
            snprintf(msg, sizeof(msg), "%.0f H/s for %u minutes, normally %.0f H/s", rate, settings.alertMinutes, normal);
            logf("[watch] hashrate low: %s\n", msg);
            notify("Hashrate low", msg, 4, "chart_with_downwards_trend");
        }
    } else {
        lowSince = 0;
        if (lowSent) {
            lowSent = false;
            snprintf(msg, sizeof(msg), "%.0f H/s, normally %.0f H/s", rate, normal);
            notify("Hashrate back to normal", msg, 2, "chart_with_upwards_trend");
        }
    }

    if (!started || now - windowAt >= span) {
        uint32_t rejected = stats.rejected - rejected0, accepted = stats.accepted - accepted0;
        if (started && rejected >= 5 && rejected * 10 >= rejected + accepted) {
            snprintf(msg, sizeof(msg), "%u of %u shares rejected in %u minutes", (unsigned)rejected,
                     (unsigned)(rejected + accepted), settings.alertMinutes);
            logf("[watch] %s\n", msg);
            notify("Shares rejected", msg, 4, "x");
        }
        started = true;
        windowAt = now;
        accepted0 = stats.accepted;
        rejected0 = stats.rejected;
    }
}

String rateText(double rate) {
    char out[24];
    if (rate >= 1e9) snprintf(out, sizeof(out), "%.2f GH/s", rate / 1e9);
    else if (rate >= 1e6) snprintf(out, sizeof(out), "%.2f MH/s", rate / 1e6);
    else snprintf(out, sizeof(out), "%.1f kH/s", rate / 1e3);
    return out;
}

// Once a day at settings.summaryHour (within its first 10 minutes): what the
// miner did since the previous summary, or since it started.
void summary(uint32_t now, const MinerStats &stats) {
    static bool started = false;
    static uint32_t fromMs, accepted0, rejected0, blocks0, connected0, sentDay;
    static uint64_t hashes0;
    if (!started) {
        started = true;
        fromMs = now;
        hashes0 = minerTotalHashes();
        accepted0 = stats.accepted;
        rejected0 = stats.rejected;
        blocks0 = stats.blocksFound;
        connected0 = minerPoolUptime().connectedMs;
        Preferences p;  // survives restarts: one summary a day
        p.begin("summary", true);
        sentDay = p.getUInt("day", 0);
        p.end();
    }
    struct tm t;
    if (settings.summaryHour > 23 || !clockNow(t) || t.tm_hour != settings.summaryHour || t.tm_min >= 10) return;
    uint32_t day = uint32_t(t.tm_year) * 1000 + t.tm_yday + 1;
    if (day == sentDay) return;
    sentDay = day;
    Preferences p;
    if (p.begin("summary", false)) {
        p.putUInt("day", day);
        p.end();
    }
    uint32_t ms = now - fromMs;
    // Every counter below restarts here, sent or not: so does the period's best.
    double best = minerTakePeriodBest();
    if (ms >= 30 * 60 * 1000) {  // nothing worth a message just after a start
        PoolUptime pool = minerPoolUptime();
        double rate = (minerTotalHashes() - hashes0) * 1000.0 / ms;
        uint32_t blocks = stats.blocksFound - blocks0;
        char msg[192], since[32];
        if (ms >= 23 * 3600000UL) snprintf(since, sizeof(since), "24 h");
        else snprintf(since, sizeof(since), "Since start %u h ago", (unsigned)((ms + 1800000) / 3600000));
        snprintf(msg, sizeof(msg), "%s%s: %s average, best %.4g (record %.4g), %u accepted / %u rejected, pool connected %.1f %%",
                 blocks ? "BLOCK CANDIDATE! " : "", since, rateText(rate).c_str(), best, stats.bestDiff,
                 (unsigned)(stats.accepted - accepted0), (unsigned)(stats.rejected - rejected0),
                 100.0 * (pool.connectedMs - connected0) / ms);
        logf("[watch] daily summary: %s\n", msg);
        notify("Daily summary", msg, 2, "bar_chart");
    }
    fromMs = now;
    hashes0 = minerTotalHashes();
    accepted0 = stats.accepted;
    rejected0 = stats.rejected;
    blocks0 = stats.blocksFound;
    connected0 = minerPoolUptime().connectedMs;
}

void screens() {
    if (settings.cycleSeconds && ui::screenAge() >= settings.cycleSeconds * 1000UL) ui::nextScreen();
    if (settings.autoRotate) {
        // Landscape either way up, from the gravity along the screen's width.
        float x, y;
        if (board::accel(x, y)) {
            if (x > 0.5f) ui::setRotation(3);
            else if (x < -0.5f) ui::setRotation(1);
        }
    }
}

}  // namespace

void watchTick() {
    uint32_t now = millis();
    night = nightNow();
    throttle();
    events(now);
    screens();
}

bool watchNight() { return night; }

const char *watchLimitReason() {
    if (ota::busy()) return "ota";
    const char *reason = limitReason.load();
    if (!minerPowerPercent() && strcmp(reason, "hot") && strcmp(reason, "sensor") && strcmp(reason, "user_pause"))
        return "supervision";
    return reason;
}

void watchWake() {
    wakeStarted = millis();
    awake = true;
}
