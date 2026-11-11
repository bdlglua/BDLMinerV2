#include "market.h"

#include <ArduinoJson.h>
#include <HTTPClient.h>
#include <WiFi.h>
#include <Preferences.h>
#include <WiFiClientSecure.h>
#include <cctype>
#include <cmath>
#include <cstdlib>
#include <sys/time.h>

#include "ghupdate.h"
#include "log.h"
#include "miner.h"
#include "net.h"
#include "notify.h"
#include "settings.h"
#include "share.h"
#include "remote.h"
#include "tls.h"
#include "runtime.h"
#include "block_status.h"
#include "core/build_time.h"

namespace {

constexpr const char *kHost = "mempool.space";
constexpr const char *kApi = "https://mempool.space/api";
// Each refresh costs a TLS handshake: ~1.5 s of core 0.
constexpr uint32_t kRefreshMs = 10 * 60 * 1000;
constexpr uint32_t kRetryMs = 30 * 1000;
// A TLS session needs ~45 KB; never starve the miner and the pool socket.

constexpr uint32_t kPoolStatsMs = 30 * 60 * 1000;
constexpr uint32_t kPoolStatsRetryMs = 10 * 60 * 1000;  // unreachable API (often a pool outage): sooner
constexpr uint32_t kApiSearchMs = 6 * 60 * 60 * 1000;   // no API found: search again after 6 h

uint32_t endpointAt[6] = {};
bool endpointValid[6] = {};

MarketData data;
PoolStats pool;
bool poolBackup = false;  // identifies the endpoint that produced `pool`
portMUX_TYPE dataMux = portMUX_INITIALIZER_UNLOCKED;

// GET `path` on the TLS connection shared by one refresh and parse the JSON
// body through `filter` (keeps RAM low: large arrays such as the mempool fee
// histogram are skipped). The bodies are a few hundred bytes.
bool fetch(int endpoint, WiFiClientSecure &tls, HTTPClient &http, const char *path, JsonDocument &doc, const JsonDocument &filter) {
    String url = String(kApi) + path;
    if (!http.begin(tls, url)) {
        tls.stop();
        return false;
    }
    // begin() can close an existing socket when it initializes the host.
    // Connect afterwards so every TLS handshake uses the miner-safe helper.
    if (!tls.connected() && !tlsConnect(tls, kHost)) {
        logf("[market] %s: cannot connect\n", path);
        http.end();
        return false;
    }
    int code = http.GET();
    if (code < 0) {
        // The shared connection was lost (the server closed it, or right
        // after the handshake, seen on mempool.space): one more go on a new one.
        http.end();
        tls.stop();
        if (http.begin(tls, url) && tlsConnect(tls, kHost)) code = http.GET();
    }
    bool ok = false;
    if (code == HTTP_CODE_OK) {
        // Parsed as it arrives when the length is known (the block list is
        // ~30 KB, several TLS records); a chunked body is read whole first.
        TlsBodyReader body(tls);
        DeserializationError err =
            http.getSize() > 0 ? deserializeJson(doc, body, DeserializationOption::Filter(filter))
                               : deserializeJson(doc, http.getString(), DeserializationOption::Filter(filter));
        ok = !err;
        if (err) logf("[market] %s: %s\n", path, err.c_str());
    } else {
        logf("[market] %s: HTTP %d\n", path, code);
    }
    http.end();  // keeps the connection open when the server allows it
    if (ok) { endpointAt[endpoint] = millis(); endpointValid[endpoint] = true; }
    return ok;
}

// One refresh of every endpoint. Fields that fail keep their previous value.
bool refresh() {
    if (!tlsMemoryOk()) {
        logf("[market] skipped, low memory (%u bytes free, largest block %u)\n", (unsigned)ESP.getFreeHeap(),
             (unsigned)ESP.getMaxAllocHeap());
        return false;
    }
    uint32_t t0 = millis();
    WiFiClientSecure tls;
    HTTPClient http;    // one for the refresh: destroying it closes the connection
    http.setReuse(true);
    http.setTimeout(10000);
    MarketData d = marketGet();
    int ok = 0, attempted = 0;
    auto needed = [&](int endpoint) {
        bool due = !endpointValid[endpoint] || millis() - endpointAt[endpoint] >= kRefreshMs;
        if (due) attempted++;
        return due;
    };

    if (needed(0)) {
        JsonDocument filter, doc;
        filter[settings.currency.c_str()] = true;
        if (fetch(0, tls, http, "/v1/prices", doc, filter)) {
            d.price = doc[settings.currency.c_str()] | 0.0;
            ok++;
        }
    }
    if (needed(1)) {
        JsonDocument filter, doc;
        filter["fastestFee"] = true;
        filter["halfHourFee"] = true;
        filter["hourFee"] = true;
        if (fetch(1, tls, http, "/v1/fees/recommended", doc, filter)) {
            d.feeFast = doc["fastestFee"] | 0;
            d.feeHalfHour = doc["halfHourFee"] | 0;
            d.feeHour = doc["hourFee"] | 0;
            ok++;
        }
    }
    if (needed(2)) {
        JsonDocument filter, doc;
        filter["currentHashrate"] = true;
        if (fetch(2, tls, http, "/v1/mining/hashrate/3d", doc, filter)) {
            d.networkHashrate = doc["currentHashrate"] | 0.0;
            ok++;
        }
    }
    if (needed(3)) {
        JsonDocument filter, doc;
        filter["progressPercent"] = true;
        filter["difficultyChange"] = true;
        filter["remainingBlocks"] = true;
        if (fetch(3, tls, http, "/v1/difficulty-adjustment", doc, filter)) {
            d.retargetProgress = doc["progressPercent"] | 0.0;
            d.retargetChange = doc["difficultyChange"] | 0.0;
            d.retargetBlocks = doc["remainingBlocks"] | 0;
            ok++;
        }
    }
    if (needed(4)) {
        JsonDocument filter, doc;
        filter["count"] = true;
        if (fetch(4, tls, http, "/mempool", doc, filter)) {
            d.mempoolTx = doc["count"] | 0;
            ok++;
        }
    }
    if (needed(5)) {
        JsonDocument filter, doc;
        JsonObject b = filter.add<JsonObject>();
        b["height"] = true;
        b["timestamp"] = true;
        b["tx_count"] = true;
        b["extras"]["pool"]["name"] = true;
        if (fetch(5, tls, http, "/v1/blocks", doc, filter)) {
            d.blockCount = 0;
            for (JsonObject o : doc.as<JsonArray>()) {
                if (d.blockCount == sizeof(d.blocks) / sizeof(d.blocks[0])) break;
                MarketData::Block &k = d.blocks[d.blockCount++];
                k.height = o["height"] | 0;
                k.time = o["timestamp"] | 0;
                k.txs = o["tx_count"] | 0;
                strlcpy(k.pool, o["extras"]["pool"]["name"] | "", sizeof(k.pool));
            }
            ok++;
        }
    }

    if (ok) d.updatedMs = millis();
    portENTER_CRITICAL(&dataMux);
    data = d;
    portEXIT_CRITICAL(&dataMux);
    logf("[market] %d/%d ok in %u ms | 1 BTC = %.0f %s | fees %u/%u/%u | net %.3g H/s | retarget %+.2f%% in %u\n",
         ok, attempted, (unsigned)(millis() - t0), d.price, settings.currency.c_str(), d.feeFast, d.feeHalfHour, d.feeHour,
         d.networkHashrate, d.retargetChange, (unsigned)d.retargetBlocks);
    return ok == attempted;
}

// Pool APIs may serialize database numbers as either JSON numbers or text.
// Missing/malformed values must not erase the last successfully read record.
double poolNumber(JsonVariantConst value, double previous = 0) {
    double number;
    if (value.is<double>()) number = value.as<double>();
    else if (value.is<const char *>()) {
        const char *text = value.as<const char *>(); char *end;
        number = std::strtod(text, &end);
        if (end == text) return previous;
        while (std::isspace(static_cast<unsigned char>(*end))) ++end;
        if (*end) return previous;
    } else return previous;
    return std::isfinite(number) && number >= 0 ? number : previous;
}

// "856K" (ckpool hash rates) -> 856000
double parseRate(const char *s, double previous = 0) {
    if (!s) return previous;
    char *end;
    double v = strtod(s, &end);
    if (end == s || !std::isfinite(v) || v < 0) return previous;
    while (std::isspace(static_cast<unsigned char>(*end))) ++end;
    const char *units = "KMGTPE";
    if (*end) {
        const char *u = strchr(units, std::toupper(static_cast<unsigned char>(*end)));
        if (!u) return previous;
        v *= pow(1000.0, u - units + 1);
        ++end;
        while (std::isspace(static_cast<unsigned char>(*end))) ++end;
        if (*end) return previous;
    }
    return std::isfinite(v) ? v : previous;
}

// Why the last fetchJson() failed, for the log.
char fetchFailure[112] = "";  // "TLS: " and the TLS error
// GET http(s)://host:port/path, the body as JSON through `filter`. Returns the
// HTTP code or a negative error.
int fetchJson(bool secure, const String &host, uint16_t port, const String &path, JsonDocument &doc,
              const JsonDocument &filter, uint8_t timeoutSeconds = 12) {
    WiFiClientSecure tls;
    WiFiClient tcp;
    HTTPClient http;
    http.setTimeout(timeoutSeconds * 1000UL);
    http.setConnectTimeout(timeoutSeconds < 5 ? timeoutSeconds * 1000 : 5000);
    String url = String(secure ? "https://" : "http://") + host + ":" + port + path;
    fetchFailure[0] = 0;
    if (!(secure ? http.begin(tls, url) : http.begin(tcp, url))) {
        snprintf(fetchFailure, sizeof(fetchFailure), "invalid URL");
        return -2;
    }
    if (secure && !tlsConnect(tls, host.c_str(), port, timeoutSeconds)) {
        snprintf(fetchFailure, sizeof(fetchFailure), "TLS: %s", tlsLastError());
        return -4;
    }
    int code = http.GET();
    if (code == HTTP_CODE_OK && deserializeJson(doc, http.getString(), DeserializationOption::Filter(filter))) {
        snprintf(fetchFailure, sizeof(fetchFailure), "HTTP 200, not JSON");
        code = -3;
    } else if (code < 0) {
        snprintf(fetchFailure, sizeof(fetchFailure), "%s", HTTPClient::errorToString(code).c_str());
    } else if (code != HTTP_CODE_OK) {
        snprintf(fetchFailure, sizeof(fetchFailure), "HTTP %d", code);
    }
    http.end();
    return code;
}

// Pool statistics APIs, found by trying them in turn: ckpool's /users/<address>
// (also served as /api/miner?address=<address>, e.g. nerdminer.io) and
// public-pool's /api/client/<address>, on the pool's host, on its parent
// domain (eu.pool.example -> pool.example) and on the ports these servers use
// by default. Whatever the site, the JSON tells which kind answered.
struct PoolApi {
    bool secure, parent;
    uint16_t port;
    const char *path;
};
const PoolApi kPoolApis[] = {
    {true, false, 443, "/users/"},          {true, false, 40557, "/api/client/"}, {true, false, 443, "/api/client/"},
    {true, true, 443, "/users/"},           {true, true, 40557, "/api/client/"},  {true, true, 443, "/api/client/"},
    {false, false, 3334, "/api/client/"},   {false, false, 80, "/users/"},  // self-hosted, local network
    // Appended, never inserted: the index found is saved on each miner.
    {true, false, 443, "/api/miner?address="}, {true, true, 443, "/api/miner?address="},
};
constexpr int kPoolApiCount = sizeof(kPoolApis) / sizeof(kPoolApis[0]);
int apiIndex[2] = {-1, -1};  // per main / backup pool: the API that answered, -1 = none yet
String apiHost[2];
uint32_t searchAfter[2] = {0, 0};
// A pool usually creates an address's page with its first accepted share: a
// share after a fruitless search is worth a new one, without waiting 6 hours.
uint32_t sharesAtSearch[2] = {0, 0};
// Only when a probe reached the API and the address was not known (HTTP 404):
// a server refusing the connection (TLS, timeout) is not cured by a share,
// and a miner finding shares every minute would search again at every pass.
bool retryOnShare[2] = {true, true};

// "eu.pool.example" -> "pool.example"; empty for two-label names and IP addresses.
String parentDomain(const String &host) {
    int dot = host.indexOf('.');
    if (dot < 0 || host.indexOf('.', dot + 1) < 0) return "";
    bool ip = true;
    for (char c : host) ip = ip && (isdigit((unsigned char)c) || c == '.');
    return ip ? String() : host.substring(dot + 1);
}

void poolFilter(JsonDocument &filter) {
    filter["bestDifficulty"] = true;
    filter["workersCount"] = true;
    filter["workers"][0]["hashRate"] = true;
    filter["accounting"]["totalAcceptedShares"] = true;
    for (const char *k : {"hashrate1hr", "hashrate5m", "bestever", "shares", "workers"}) filter[k] = true;
}

// Reads either API's answer into `p`; false for JSON neither of them sends.
bool parseStats(JsonDocument &doc, PoolStats &p) {
    if (!doc["bestDifficulty"].isNull() || !doc["workersCount"].isNull()) {  // public-pool
        p.bestEver = poolNumber(doc["bestDifficulty"], p.bestEver);
        p.workers = doc["workersCount"] | 0;
        p.shares = poolNumber(doc["accounting"]["totalAcceptedShares"], p.shares);
        p.hashrate = 0;
        for (JsonVariant w : doc["workers"].as<JsonArray>()) p.hashrate += poolNumber(w["hashRate"]);
        return true;
    }
    if (!doc["bestever"].isNull() || !doc["hashrate1hr"].isNull() || !doc["hashrate5m"].isNull()) {  // ckpool
        p.hashrate = parseRate(doc["hashrate1hr"] | doc["hashrate5m"] | "", p.hashrate);
        p.bestEver = poolNumber(doc["bestever"], p.bestEver);
        p.shares = poolNumber(doc["shares"], p.shares);
        p.workers = doc["workers"] | 0;
        return true;
    }
    return false;
}
// End of the pool stats parsing.

constexpr int kUnknownJson = -5;
// A probe gives up sooner: a port that does not speak TLS would hold the
// handshake lock for the whole timeout (18 s seen at 12 s).
constexpr uint8_t kProbeSeconds = 6;
int queryApi(int i, const String &host, const String &addr, PoolStats &p, bool probe = false) {
    const PoolApi &api = kPoolApis[i];
    String target = api.parent ? parentDomain(host) : host;
    if (target.isEmpty()) { fetchFailure[0] = 0; return -1; }
    JsonDocument filter, doc;
    poolFilter(filter);
    int code = fetchJson(api.secure, target, api.port, String(api.path) + addr, doc, filter, probe ? kProbeSeconds : 12);
    if (code == HTTP_CODE_OK && !parseStats(doc, p)) code = kUnknownJson;
    if (code == HTTP_CODE_OK)
        snprintf(p.source, sizeof(p.source), "%s%s:%u%s", api.secure ? "" : "http://", target.c_str(), api.port, api.path);
    return code;
}

// The API found for a pool is kept across restarts ("host|index").
const char *apiKey(bool backup) { return backup ? "backup" : "main"; }
int loadApi(bool backup, const String &host) {
    Preferences prefs;
    if (!prefs.begin("poolapi", true)) return -1;
    String saved = prefs.getString(apiKey(backup), "");
    prefs.end();
    int bar = saved.lastIndexOf('|');
    if (bar < 0 || saved.substring(0, bar) != host) return -1;
    int i = saved.substring(bar + 1).toInt();
    return i >= 0 && i < kPoolApiCount ? i : -1;
}
void saveApi(bool backup, const String &host, int i) {
    Preferences prefs;
    if (!prefs.begin("poolapi", false)) return;
    if (i < 0) prefs.remove(apiKey(backup));
    else prefs.putString(apiKey(backup), host + "|" + i);
    prefs.end();
}

void storePool(const PoolStats &p, bool backup) {
    portENTER_CRITICAL(&dataMux);
    pool = p;
    poolBackup = backup;
    portEXIT_CRITICAL(&dataMux);
}

// Refreshes PoolStats for the pool in use; false when it should be tried again soon.
bool refreshPoolStats() {
    if (!tlsMemoryOk()) return false;
    bool backup = netOnBackup();
    String host = backup ? settings.backupHost : settings.poolHost;
    host.toLowerCase();
    String addr = settings.btcAddress.substring(0, settings.btcAddress.indexOf('.') < 0 ? settings.btcAddress.length()
                                                                                       : settings.btcAddress.indexOf('.'));
    portENTER_CRITICAL(&dataMux);
    PoolStats p = poolBackup == backup ? pool : PoolStats{};
    portEXIT_CRITICAL(&dataMux);
    if (apiHost[backup] != host) {
        apiHost[backup] = host;
        apiIndex[backup] = loadApi(backup, host);
        searchAfter[backup] = 0;
    }

    int code = -1;
    if (apiIndex[backup] >= 0) {
        code = queryApi(apiIndex[backup], host, addr, p);
        if (code == HTTP_CODE_NOT_FOUND || code == kUnknownJson) {  // the API moved: search again
            logf("[market] pool stats API of %s stopped answering (%d)\n", host.c_str(), code);
            apiIndex[backup] = -1;
            saveApi(backup, host, -1);
        }
    }
    if (apiIndex[backup] < 0) {
        bool newShare = retryOnShare[backup] && minerStats().accepted != sharesAtSearch[backup];
        if (searchAfter[backup] && (int32_t)(millis() - searchAfter[backup]) < 0 && !newShare) {
            p.supported = false;
            storePool(p, backup);
            return true;
        }
        bool addressUnknown = false, refusedTls12 = false;
        for (int i = 0; i < kPoolApiCount && code != HTTP_CODE_OK; i++) {
            if (!tlsMemoryOk()) return false;  // continue at the next attempt
            code = queryApi(i, host, addr, p, true);
            addressUnknown |= code == HTTP_CODE_NOT_FOUND;
            refusedTls12 |= code == -4 && tlsLastErrorCode() == kTlsPeerAlert;
            if (code != HTTP_CODE_OK && (code == kUnknownJson || fetchFailure[0])) {  // nothing to say when there was no parent domain
                const PoolApi &api = kPoolApis[i];
                String target = api.parent ? parentDomain(host) : host;
                logf("[market] pool stats probe %s%s:%u%s: %s\n", api.secure ? "" : "http://", target.c_str(),
                     api.port, api.path, code == kUnknownJson ? "JSON of an unknown API" : fetchFailure);
            }
            if (code == HTTP_CODE_OK) {
                apiIndex[backup] = i;
                saveApi(backup, host, i);
                logf("[market] pool stats API found: %s\n", p.source);
            }
        }
        if (code != HTTP_CODE_OK) {
            sharesAtSearch[backup] = minerStats().accepted;
            retryOnShare[backup] = addressUnknown;
            searchAfter[backup] = millis() + kApiSearchMs;
            if (!searchAfter[backup]) searchAfter[backup] = 1;
            p.supported = false;
            storePool(p, backup);
            if (refusedTls12)
                logf("[market] %s refused the TLS handshake (alert from the server): its stats API asks for TLS "
                     "settings this firmware does not offer\n", host.c_str());
            logf("[market] no pool stats API found for %s; searched again %s\n", host.c_str(),
                 addressUnknown ? "after its next accepted share, or in 6 h" : "in 6 h");
            return true;
        }
    }
    p.supported = true;
    p.error = code == HTTP_CODE_OK ? 0 : code;
    if (code == HTTP_CODE_OK) p.updatedMs = millis();
    storePool(p, backup);
    if (code == HTTP_CODE_OK)
        logf("[market] pool sees %.3g H/s, best %.4g, %u worker(s)\n", p.hashrate, p.bestEver, (unsigned)p.workers);
    else
        logf("[market] pool stats from %s: error %d\n", p.source[0] ? p.source : host.c_str(), code);
    return code == HTTP_CODE_OK;
}

// Some networks block NTP. Certificates are checked against the clock, so no
// notification, market or update request could ever be made: after a minute
// without NTP, the pool's job time (within minutes of the real time) sets
// it. NTP keeps trying and corrects it whenever it answers.
//
// The job time is the pool's word, over a connection that Stratum V1 does not
// protect: a time before this firmware was built is never the real one, and
// would let a certificate that has since expired pass as valid.
void clockFromPool() {
    static bool done = false;
    static const uint32_t kOldest = build_time::oldestPlausible(__DATE__, 1700000000);
    if (done || tlsClockSet() || millis() < 60000) return;
    uint32_t jobTime = minerJobTime();
    if (jobTime < kOldest) {
        static bool refused = false;
        if (jobTime && !refused) {
            refused = true;
            logf("[time] the pool's job time is older than this firmware: not used to set the clock\n");
        }
        return;
    }
    timeval now{time_t(jobTime), 0};
    if (settimeofday(&now, nullptr) != 0) return;
    done = true;
    logf("[time] NTP not answering: clock set from the pool's job time\n");
}

void marketTask(void *) {
    uint32_t nextMarket = 0, nextPool = 60000;  // pool stats once mining for a minute
    bool lastPoolBackup = netOnBackup();
    uint32_t retry = kRetryMs;
    for (;;) {
        clockFromPool();
        ghUpdatePoll();
        notifyPoll();
        sharePoll();
        remotePoll();
        blockStatusPoll();
        bool backup = netOnBackup();
        if (backup != lastPoolBackup) {
            lastPoolBackup = backup;
            nextPool = millis();  // do not wait up to an hour after failover
        }
        // Certificates are checked against the clock: no HTTPS before NTP.
        bool online = WiFi.isConnected() && tlsClockSet();
        // Said once: without it a miner waiting for the clock shows no market
        // data and logs nothing about why.
        static bool waitLogged = false;
        if (!online && !waitLogged && millis() > 90000) {
            waitLogged = true;
            logf("[market] waiting for %s\n", WiFi.isConnected() ? "the clock (NTP has not answered yet)" : "WiFi");
        }
        if (coin::bitcoinData(settings.miningCoin) && online && (int32_t)(millis() - nextMarket) >= 0) {
            bool ok = refresh();
            nextMarket = millis() + (ok ? kRefreshMs : retry);
            retry = ok ? kRetryMs : min<uint32_t>(retry * 2, kRefreshMs);
        }
        if (coin::bitcoinData(settings.miningCoin) && online && minerStats().poolConnected && (int32_t)(millis() - nextPool) >= 0) {
            bool ok = refreshPoolStats();
            nextPool = millis() + (ok ? kPoolStatsMs : kPoolStatsRetryMs);
        }
        vTaskDelay(pdMS_TO_TICKS(5000));
    }
}

}  // namespace

void marketStart() {
    configTzTime(settings.timezone.c_str(), "pool.ntp.org", "time.cloudflare.com");
    // 12 KB: the GitHub update check and install (ghUpdatePoll()) run here too.
    runtime::started(xTaskCreatePinnedToCore(marketTask, "market", 12288, nullptr, 2, nullptr, 0) == pdPASS,
                     runtime::Market, "market and GitHub updates");
}

PoolStats poolStatsGet() {
    if (!coin::bitcoinData(settings.miningCoin)) { PoolStats unavailable; unavailable.supported = false; return unavailable; }
    portENTER_CRITICAL(&dataMux);
    PoolStats p = pool;
    bool backup = poolBackup;
    portEXIT_CRITICAL(&dataMux);
    return backup == netOnBackup() ? p : PoolStats{};
}

MarketData marketGet() {
    if (!coin::bitcoinData(settings.miningCoin)) return MarketData{};
    portENTER_CRITICAL(&dataMux);
    MarketData d = data;
    portEXIT_CRITICAL(&dataMux);
    return d;
}

bool clockNow(struct tm &out) {
    time_t now = time(nullptr);
    if (now < 1700000000) return false;  // not synchronised yet
    localtime_r(&now, &out);
    return true;
}
