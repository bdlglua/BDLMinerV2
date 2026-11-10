#include "remote.h"
#include <ArduinoJson.h>
#include <HTTPClient.h>
#include <WiFi.h>
#include <cmath>
#include <ctime>
#include "board.h"
#include "core/public_pool.h"
#include "core/remote_data.h"
#include "explorer_roots.h"
#include "log.h"
#include "miner.h"
#include "net.h"
#include "settings.h"
#include "tls.h"
#include "trial.h"
#include "web.h"

namespace {
constexpr char kRemoteHost[] = "api.bdl-pool.com";
constexpr uint16_t kRemotePort = 36018;
constexpr char kRemoteURL[] = "https://api.bdl-pool.com:36018/api/remote/report";
constexpr uint32_t kRemoteEveryMs = 300000;
uint32_t remoteNext = 60000;
}

void remotePoll() {
    // A private key keeps its secret space; otherwise the payout address names it.
    const bool keyed = remote_data::validKey(settings.remoteKey.c_str());
    char address[101];
    if (!settings.remoteEnabled || (!keyed && !remote_data::payoutAddress(settings.btcAddress.c_str(), address)) ||
        int32_t(millis() - remoteNext) < 0) return;
    // Verified TLS needs a synchronized clock. Missing trust/time/heap never
    // downgrades transport and does not stop the miner or its public reporting.
    if (!WiFi.isConnected() || !tlsMemoryOk() || time(nullptr) < 1700000000) {
        remoteNext = millis() + 60000;
        return;
    }
    remoteNext = millis() + kRemoteEveryMs;
    char body[1024];
    {
        const MinerStats stats = minerStats();
        char id[17];
        if (keyed) remote_data::deviceId(settings.remoteKey.c_str(), ESP.getEfuseMac(), id);
        else remote_data::addressDeviceId(address, ESP.getEfuseMac(), id);
        JsonDocument doc;
        doc["id"] = id;
        doc["name"] = settings.remoteName.length() ? settings.remoteName.c_str() : board::modelName();
        doc["v"] = FW_VERSION;
        doc["board"] = FW_ENV;
        doc["coin"] = coin::ticker(settings.miningCoin);
        doc["boot_id"] = String(webBootId(), HEX);
        doc["uptime_s"] = minerUptimeSeconds();
        doc["total_hashes"] = double(minerTotalHashes());
        const double rate = minerHashrate();
        doc["hashrate"] = std::isfinite(rate) && rate > 0 ? rate : 0;
        const float temperature = board::temperature();
        if (std::isfinite(temperature)) doc["temp_c"] = temperature;
        else doc["temp_c"] = nullptr;
        doc["accepted"] = stats.accepted;
        doc["rejected"] = stats.rejected;
        doc["best"] = std::isfinite(stats.bestDiff) && stats.bestDiff >= 0 ? stats.bestDiff : 0;
        doc["power_percent"] = minerPowerPercent();
        doc["pool_connected"] = stats.poolConnected;
        doc["pool"] = stats.poolConnected ? publicPoolGroup(netPoolHost().c_str()) : "unknown";
        doc["firmware_trial"] = trialPending();
        if (measureJson(doc) >= sizeof(body)) return;
        serializeJson(doc, body, sizeof(body));
    } // Free the JSON tree before allocating TLS record buffers.
    WiFiClientSecure tls;
    HTTPClient http;
    http.setTimeout(10000);
    http.setFollowRedirects(HTTPC_DISABLE_FOLLOW_REDIRECTS);
    int code = -1;
    if (http.begin(tls, kRemoteURL) && tlsConnectVerified(tls, kRemoteHost, kExplorerRootCA, kRemotePort)) {
        http.setUserAgent(DEFAULT_USER_AGENT);
        http.addHeader("Content-Type", "application/json");
        http.addHeader("Authorization", keyed ? String("Bearer ") + settings.remoteKey : String("Address ") + address);
        code = http.POST(reinterpret_cast<uint8_t *>(body), strlen(body));
    }
    http.end();
    // Never print the key, address, report body, server response or miner pseudonym.
    if (code == 204) logf("[remote] readings sent (%s)\n", keyed ? "private key" : "payout address");
    else logf("[remote] readings not sent (%d)\n", code);
}
