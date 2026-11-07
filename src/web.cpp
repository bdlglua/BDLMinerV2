#include "web.h"
#ifdef BOARD_GAMMA601
#include "gamma601.h"
#endif
#ifdef BOARD_NERDNOS
#include "nerdnos.h"
#endif

#include <ArduinoJson.h>
#include <ESPmDNS.h>
#include <Update.h>
#include <WebServer.h>
#include <WiFi.h>
#include <esp_ota_ops.h>
#include <esp_system.h>

#include <algorithm>
#include <atomic>
#include <cmath>
#include <lwip/sockets.h>

#include "board.h"
#include "core/address.h"
#include "ghupdate.h"
#include "log.h"
#include "market.h"
#include "miner.h"
#include "net.h"
#include "ota.h"
#include "settings.h"
#include "settings_apply.h"
#include "watch.h"
#include "runtime.h"
#include "tls.h"
#include "trial.h"
#include "core/pool_diagnostics.h"
#include "block_status.h"
#include "notify.h"
#include "mqtt.h"
#include "crashlog.h"
#include "web_validation.h"

#ifndef FW_ENV
#define FW_ENV "dev"
#endif

namespace {

// The library serves one connection at a time and waits up to 5 s for the
// request of a new one. Browsers open spare connections they use later or
// never: each stalled the page's other files for 5 s, and some gave up. New
// connections now wait here, and the first one to send its request is served.
class Server : public WebServer {
public:
    using WebServer::WebServer;
    void handleClient() override {
        if (_currentStatus == HC_NONE) {
            uint32_t now = millis();
            while (_server.hasClient()) {
                int slot = 0;  // a free slot, or the oldest connection
                for (int i = 0; i < kWaiting; i++) {
                    if (!waiting[i]) { slot = i; break; }
                    if ((int32_t)(since[i] - since[slot]) < 0) slot = i;
                }
                waiting[slot].stop();
#if defined(ESP_ARDUINO_VERSION_MAJOR) && ESP_ARDUINO_VERSION_MAJOR >= 3
                waiting[slot] = _server.accept();  // available() was renamed in 3.x
#else
                waiting[slot] = _server.available();
#endif
                since[slot] = now;
            }
            for (int i = 0; i < kWaiting; i++) {
                if (!waiting[i]) {  // closed by the browser: free its socket
                    waiting[i] = WiFiClient();
                    continue;
                }
                if (waiting[i].available()) {
                    _currentClient = waiting[i];
                    waiting[i] = WiFiClient();
                    _currentStatus = HC_WAIT_READ;
                    _statusChange = now;
                    break;
                }
                if (now - since[i] > kIdleMs) {
                    waiting[i].stop();
                    waiting[i] = WiFiClient();
                }
            }
            // Nothing to serve: the library would take the next new
            // connection itself and wait for its request.
            if (_currentStatus == HC_NONE) return;
        }
        WebServer::handleClient();
    }

private:
    static constexpr int kWaiting = 6;  // a browser opens up to 6 per site
    static constexpr uint32_t kIdleMs = 10000;
    WiFiClient waiting[kWaiting];
    uint32_t since[kWaiting] = {};
};

Server server(80);
String hostName;
std::atomic<uint32_t> restartAt{0};  // restart due at this millis(), 0 = none
std::atomic<const char *> restartWhy{""};
std::atomic<bool> settingsChanged{false};
HistoryPoint historyScratch[HISTORY_LEN]; // one HTTP worker; shared between its history handlers
uint32_t bootSession = 0;

// Generated from the maintainable web/ sources before every firmware build.
#include "web_assets.generated.h"

void sendPage(const char *title, const char *body, const char *script) {
    // Send one complete head and three fixed-length body segments instead of
    // framing every small tag as a separate HTTP chunk/TCP write.
    String head;
    head.reserve(1024);
    head = "<!doctype html><html lang=\"";
    head += settings.language == 1 ? "fr" : "en";
    head += "\"><head><title>"; head += title; head += "</title>";
    head += "<meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"color-scheme\" content=\"dark\">";
    // Installable on a phone's home screen (a full PWA install needs HTTPS).
    head += "<meta name=\"theme-color\" content=\"#0a0d12\"><meta name=\"mobile-web-app-capable\" content=\"yes\">"
            "<meta name=\"apple-mobile-web-app-capable\" content=\"yes\"><meta name=\"apple-mobile-web-app-status-bar-style\" content=\"black\">"
            "<link rel=\"manifest\" href=\"/manifest.webmanifest\"><link rel=\"icon\" type=\"image/png\" href=\"/assets/icon-192.png\">"
            "<link rel=\"apple-touch-icon\" href=\"/assets/icon-192.png\">";
    head += "<link rel=\"stylesheet\" href=\"/assets/style.css?v="; head += kWebVersion; head += "\">";
    head += "<script>const LANG='"; head += settings.language == 1 ? "fr" : "en"; head += "';</script>";
    head += "<script defer src=\"/assets/common.js?v="; head += kWebVersion; head += "\"></script>";
    head += "<script defer src=\"/assets/"; head += script; head += ".js?v="; head += kWebVersion; head += "\"></script></head><body>";
    static const char end[] = "</div></div></body></html>";
    server.sendHeader("Cache-Control", "no-store");
    server.setContentLength(head.length() + strlen_P(kHeader) + strlen_P(body) + sizeof(end) - 1);
    server.send(200, "text/html; charset=utf-8", head);
    server.sendContent_P(kHeader);
    server.sendContent_P(body);
    server.sendContent(end);
}

void sendAsset(const char *mime, const uint8_t *data, size_t size, bool gzip = true) {
    if (gzip) server.sendHeader("Content-Encoding", "gzip");
    server.sendHeader("Cache-Control", "public, max-age=31536000, immutable");
    server.send_P(200, mime, reinterpret_cast<PGM_P>(data), size);
}

// Whether the request comes from the miner's own pages: another site's page
// (or a name of another site leading here) cannot change it or read its secrets.
bool fromOwnPage() {
    String host = server.hostHeader();
    return webvalidation::hostAllowed(host.c_str(), hostName.c_str()) &&
           webvalidation::originAllowed(server.header("Origin").c_str(), host.c_str());
}

// Settings, backups, firmware and crash reports: from the miner's own pages
// only, then with the web password when one is set; without one (the
// default) they are open to the local network.
bool authorized() {
    if (!fromOwnPage()) {
        server.send(403, "text/plain", "Refused: open the miner's own page (http://its address/) to do this");
        return false;
    }
    if (settings.webPassword.isEmpty()) return true;
    if (!server.authenticate("admin", settings.webPassword.c_str())) {
        server.requestAuthentication(BASIC_AUTH, "BDLMiner", "Wrong password");
        return false;
    }
    return true;
}

void restartSoon(const char *why) {
    restartWhy = why;
    uint32_t due = millis() + 1500;
    restartAt = due ? due : 1; // zero is reserved for "no restart", including at millis() wrap
}

const char *accentCss() {
    static const char *kAccent[] = {"#f7931a", "#38d07a", "#3b9cf6", "#a66bf5", "#ec5aa0"};
    return kAccent[settings.theme % 5];
}

// The log names networks, pools, hosts and every error: protected like the
// settings (the web password when one is set, from the miner's own pages).
void handleLog() {
    if (!authorized()) return;
    server.sendHeader("Cache-Control", "no-store");
    server.send(200, "text/plain", logText());
}

void buildPoolDiagnostics(JsonObject out,const MinerStats &stats) {
    pool_diagnostics::Input input;
    input.connected=stats.poolConnected;input.work=stats.workAvailable;input.mask=stats.versionMask;
#ifdef BOARD_GAMMA601
    input.requiredBits=8;
#elif defined(BOARD_NERDNOS)
    input.requiredBits=2;
#endif
    input.difficulty=stats.poolDiff;input.hashrate=minerHashrate();input.acceptedWork=stats.acceptedWork;
    input.accepted=stats.accepted;input.seconds=minerUptimeSeconds();
    String status=netStatus();input.status=status.c_str();
    auto report=pool_diagnostics::diagnose(input);
    out["code"]=pool_diagnostics::code(report.issue);out["compatible"]=report.compatible;
    out["connected"]=input.connected;out["work_available"]=input.work;
    out["protocol"]=netProtocol();out["status"]=status;
    out["version_rolling_bits"]=report.rollingBits;out["required_rolling_bits"]=input.requiredBits;
    out["difficulty"]=input.difficulty;
    if(std::isfinite(report.expectedShareSeconds))out["expected_share_seconds"]=report.expectedShareSeconds;
    else out["expected_share_seconds"]=nullptr;
    if(std::isfinite(report.shareBasedHashrate))out["share_based_hashrate"]=report.shareBasedHashrate;
    else out["share_based_hashrate"]=nullptr;
    out["accepted_samples"]=input.accepted;out["estimate_window_s"]=uint32_t(input.seconds);
    out["checked_ms"]=millis();
}

void handlePoolDiagnostics() {
    JsonDocument j;buildPoolDiagnostics(j.to<JsonObject>(),minerStats());
    server.sendHeader("Cache-Control","no-store");
    String body;serializeJson(j,body);server.send(200,"application/json",body);
}

// What the dashboard shows; `history` adds the 24-hour chart.
void buildStats(JsonDocument &j, bool history) {
    const MinerStats stats = minerStats();
    j["board"] = board::modelName();
    j["api_version"] = 2;
    j["firmware"] = FW_VERSION;
    j["boot_id"] = String(bootSession, HEX);
    j["uptime_s"] = minerUptimeSeconds();
    j["hashrate"] = minerHashrate();
    j["hashrate_live"] = minerHashrateLive();
    j["firmware_env"] = FW_ENV;
    j["total_hashes"] = (double)minerTotalHashes();
    JsonArray engines = j["engines"].to<JsonArray>();
    for (int i = 0; i < stats.minerCount; i++) {
        JsonObject e = engines.add<JsonObject>();
        e["name"] = stats.engine[i];
        e["rate"] = minerEngineRate(i);
    }
    j["shares"]["accepted"] = stats.accepted;
    j["shares"]["rejected"] = stats.rejected;
    j["shares"]["work"] = stats.acceptedWork;
    for(unsigned i=0;i<unsigned(stratum::RejectReason::Count);++i)
        j["shares"]["rejected_reasons"][stratum::reasonName(stratum::RejectReason(i))]=stats.rejectedReasons[i];
    buildPoolDiagnostics(j["pool_diagnostics"].to<JsonObject>(),stats);
    j["best"]["session"] = stats.sessionBestDiff;
    j["best"]["ever"] = stats.bestDiff;
    j["blocks"] = stats.blocksFound;
    BlockCandidate candidate{};
    if (minerLastBlockCandidate(candidate)) {
        static const char *const states[] = {"queued", "submitted", "share_accepted", "share_rejected", "unanswered", "stale", "queue_full"};
        uint8_t id[32];
        for (unsigned i = 0; i < 32; ++i) id[i] = candidate.hash[31 - i];
        j["block_candidate"]["height"] = candidate.height;
        j["block_candidate"]["coin"] = coin::ticker(candidate.coin);
        j["block_candidate"]["hash"] = work::toHex(id, 32);
        j["block_candidate"]["status"] = states[candidate.state];
        j["block_candidate"]["previous_session"] = candidate.previousSession;
        BlockChainStatus chain{};
        bool checked = candidate.coin == coin::BTC && blockStatusGet(candidate.hash, chain);
        if (checked && chain.known && !chain.error) j["block_candidate"]["chain_confirmed"] = chain.inBestChain;
        else j["block_candidate"]["chain_confirmed"] = nullptr;
        j["block_candidate"]["chain_source"] = candidate.coin == coin::BTC ? "mempool.space" : "";
        j["block_candidate"]["confirmations"] = checked && chain.known && !chain.error ? chain.confirmations : 0;
        if (checked && chain.checkedMs) j["block_candidate"]["chain_checked_s"] = uint32_t(millis() - chain.checkedMs) / 1000;
        else j["block_candidate"]["chain_checked_s"] = -1;
        if (checked && chain.notFound) j["block_candidate"]["chain_not_found"] = true;
        if (checked && chain.error) j["block_candidate"]["chain_error"] = chain.error;
    }
    j["over32bits"] = stats.over32bits;
    j["over64bits"] = stats.over64bits;
    BestShare top[TOP_SHARES];
    int nt = minerTopShares(top, TOP_SHARES);
    JsonArray ta = j["top"].to<JsonArray>();
    for (int i = 0; i < nt; i++) {
        JsonObject o = ta.add<JsonObject>();
        o["d"] = top[i].difficulty;
        o["t"] = top[i].time;
    }
    j["pool"]["host"] = netPoolHost();
    j["pool"]["port"] = netPoolPort();
    j["pool"]["backup"] = netOnBackup();
    j["pool"]["connected"] = stats.poolConnected;
    j["pool"]["status"] = netStatus();
    j["pool"]["difficulty"] = stats.poolDiff;
    j["pool"]["pending_shares"] = netPendingShares();
    j["pool"]["user_agent"] = netUserAgent();
    j["pool"]["protocol"] = netProtocol();
    j["pool"]["user_agent_auto"] = settings.userAgentAuto;
    PoolUptime poolUp = minerPoolUptime();
    uint32_t uptimeMs = millis();
    j["pool"]["connected_percent"] = uptimeMs ? std::round(1000.0 * poolUp.connectedMs / uptimeMs) / 10 : 0;
    j["pool"]["sessions"] = poolUp.sessions;
    j["pool"]["failovers"] = netFailovers();
    ShareEntry sent[20];
    int sentCount = minerShareLog(sent, 20);
    if (sentCount) {
        static const char *const kStates[] = {"pending", "accepted", "rejected", "unanswered"};
        JsonArray log = j["shares_log"].to<JsonArray>();
        for (int i = 0; i < sentCount; ++i) {
            JsonObject e = log.add<JsonObject>();
            e["t"] = sent[i].time;
            e["d"] = sent[i].difficulty;
            e["s"] = kStates[sent[i].state & 3];
            if (sent[i].backup) e["b"] = true;
        }
    }
    PoolStats pv = poolStatsGet();
    j["pool_view"]["updated_s"] = pv.updatedMs ? (long)((millis() - pv.updatedMs) / 1000) : -1;
    j["pool_view"]["supported"] = pv.supported;
    j["pool_view"]["hashrate"] = pv.hashrate;
    j["pool_view"]["best"] = pv.bestEver;
    j["pool_view"]["shares"] = pv.shares;
    j["pool_view"]["workers"] = pv.workers;
    if (pv.source[0]) j["pool_view"]["source"] = pv.source;
    if (pv.error) j["pool_view"]["error"] = pv.error;
    j["network"]["height"] = stats.blockHeight;
    j["network"]["difficulty"] = stats.networkDiff;
    j["address"] = settings.btcAddress;
    j["coin"] = coin::ticker(settings.miningCoin);
    j["coin_name"] = coin::name(settings.miningCoin);
    j["algorithm"] = "SHA-256d";
    j["address_type"] = address::typeName(address::parse(settings.btcAddress.c_str(), settings.miningCoin).type);
    j["lifetime"]["hashes"] = (double)stats.lifetimeHashes;
    j["lifetime"]["shares"] = stats.lifetimeShares;
    j["lifetime"]["blocks"] = stats.lifetimeBlocks;
    j["lifetime"]["seconds"] = stats.lifetimeSeconds;
    j["history_revision"] = minerHistoryRevision();
    if (history) {
        int n = minerHistoryPoints(historyScratch, HISTORY_LEN);
        j["history"]["interval_s"] = HISTORY_INTERVAL_S;
        JsonArray hr = j["history"]["hashrate"].to<JsonArray>();
        for (int i = 0; i < n; i++) {
            const HistoryPoint &p = historyScratch[i];
            if (p.coveragePercent || p.hashrate) hr.add(double(p.hashrate));
            else hr.add(nullptr);
        }
    }
    j["heap"] = ESP.getFreeHeap();
    j["task_failures"] = runtime::failures().load();
    j["heap_max_block"] = ESP.getMaxAllocHeap();
    float temp = board::temperature();
    j["temperature_source"] = board::temperatureSource();
#ifdef BOARD_GAMMA601
    auto asic = gamma601::status();
    JsonObject a = j["asic"].to<JsonObject>();
    a["model"] = "BM1370"; a["experimental"] = true; a["running"] = asic.running; a["state"] = asic.state;
    a["frequency_mhz"] = asic.frequency; a["fan_rpm"] = asic.rpm;
    if (asic.fanPercent < 0) a["fan_percent"] = nullptr;
    else a["fan_percent"] = asic.fanPercent;
    a["fan_requested_percent"] = asic.requestedFanPercent; a["fan_auto"] = asic.fanAutomatic;
    a["fan_override"] = asic.fanOverride; a["fan_reason"] = asic.fanReason;
    a["input_v"] = asic.inputVoltage; a["core_v"] = asic.coreVoltage;
    a["current_a"] = asic.current; a["regulator_temp_c"] = asic.regulatorTemperature;
    a["regulator_status"] = asic.regulatorStatus;
    a["profile_mhz"] = asic.requestedMHz; a["stock_fallback"] = asic.stockFallback;
    a["fallback_reason"] = asic.fallbackReason;
    if (asic.hardwareErrorPercent < 0) a["hardware_error_percent"] = nullptr;
    else a["hardware_error_percent"] = asic.hardwareErrorPercent;
    char fault[192];
    uint32_t faultTime = gamma601::lastFault(fault, sizeof(fault));
    if (fault[0]) {
        a["last_fault"] = fault;
        a["last_fault_time"] = faultTime;
    }
#endif
#ifdef BOARD_NERDNOS
    auto asic = nerdnos::status();
    JsonObject a = j["asic"].to<JsonObject>();
    a["model"] = "BM1397"; a["experimental"] = true; a["running"] = asic.running; a["state"] = asic.state;
    a["frequency_mhz"] = asic.frequency; a["results"] = asic.results;
    if (asic.hardwareErrorPercent < 0) a["hardware_error_percent"] = nullptr;
    else a["hardware_error_percent"] = asic.hardwareErrorPercent;
#endif
    if (isnan(temp)) j["temp_c"] = nullptr;
    else j["temp_c"] = (int)lroundf(temp);
    j["throttle"] = minerThrottle() == Throttle::Paused ? "paused" : minerThrottle() == Throttle::Half ? "half" : "full";
    j["power_percent"] = minerPowerPercent();
    j["power_limit_percent"] = settings.miningPower;
    j["max_temp_c"] = settings.maxTemp;
    j["limitation_reason"] = watchLimitReason();
    if (minerThrottle() == Throttle::Reduced) j["throttle"] = "reduced";
    j["night"] = watchNight();
    j["boot_reason"] = bootReason();
    if (crashPresent() || crashHasHistory()) {
        if (crashPresent()) {
            j["crash"]["summary"] = crashSummary();
            j["crash"]["size"] = crashSize();
        }
        crashHistory(j["crash"]["history"].to<JsonArray>());
    }
    j["firmware_trial"] = trialPending();
    NotifyStatus ns = notifyStatus();
    j["integrations"]["mqtt"] = mqttState();
    if (!settings.mqttHost.isEmpty()) j["integrations"]["mqtt_host"] = settings.mqttHost + ":" + settings.mqttPort;
    j["integrations"]["notify"]["enabled"] = ns.enabled;
    if (ns.id) {
        j["integrations"]["notify"]["title"] = ns.title;
        j["integrations"]["notify"]["ntfy"] = ns.ntfy;
        j["integrations"]["notify"]["webhook"] = ns.webhook;
    }
    j["accent"] = accentCss();
    j["ip"] = WiFi.localIP().toString();
    j["ssid"] = WiFi.SSID();
    j["rssi"] = WiFi.RSSI();
    JsonArray peers = j["peers"].to<JsonArray>();
    for (int from = 0; from < (int)settings.peers.length();) {
        int comma = settings.peers.indexOf(',', from);
        if (comma < 0) comma = settings.peers.length();
        String p = settings.peers.substring(from, comma);
        p.trim();
        if (p.length()) peers.add(p);
        from = comma + 1;
    }

    MarketData m = marketGet();
    JsonObject mk = j["market"].to<JsonObject>();
    mk["updated_s"] = m.updatedMs ? (long)((millis() - m.updatedMs) / 1000) : -1;
    mk["currency"] = settings.currency;
    mk["price"] = m.price;
    JsonArray fees = mk["fees"].to<JsonArray>();
    fees.add(m.feeFast);
    fees.add(m.feeHalfHour);
    fees.add(m.feeHour);
    mk["hashrate"] = m.networkHashrate;
    mk["mempool_tx"] = m.mempoolTx;
    mk["retarget"]["change"] = m.retargetChange;
    mk["retarget"]["progress"] = m.retargetProgress;
    mk["retarget"]["blocks"] = m.retargetBlocks;
    // Latest blocks, with your best share while each was the one to find
    // (the first entry is the block being mined, not found yet).
    BlockBest bb[BLOCK_BESTS];
    int nb = minerBlockBests(bb, BLOCK_BESTS);
    auto bestFor = [&](uint32_t h, JsonObject o) {
        for (int i = 0; i < nb; i++)
            if (bb[i].height == h) o["best"] = bb[i].best;
    };
    JsonArray blocks = j["blocks_recent"].to<JsonArray>();
    if (stats.blockHeight) {
        JsonObject o = blocks.add<JsonObject>();
        o["h"] = stats.blockHeight;
        o["mining"] = true;
        bestFor(stats.blockHeight, o);
    }
    for (int i = 0; i < m.blockCount; i++) {
        JsonObject o = blocks.add<JsonObject>();
        o["h"] = m.blocks[i].height;
        o["t"] = m.blocks[i].time;
        o["pool"] = m.blocks[i].pool;
        o["tx"] = m.blocks[i].txs;
        bestFor(m.blocks[i].height, o);
    }
    j["lang"] = settings.language == 1 ? "fr" : "en";
    GithubUpdate gu = ghUpdateState();
    JsonObject up = j["github_update"].to<JsonObject>();
    up["available"] = gu.available;
    up["mode"] = settings.githubUpdate;
    up["channel"] = settings.updateChannel == 1 ? "beta" : "stable";
    up["checked"] = gu.checked;
    up["checking"] = gu.checking;
    up["request_id"] = gu.installRequest;
    up["version"] = gu.version;
    up["installing"] = gu.installing;
    up["progress"] = gu.progress;
    up["retry_in_s"] = gu.retrySeconds;
    up["supported"] = esp_ota_get_next_update_partition(nullptr) != nullptr;
    up["release_url"] = String("https://github.com/" GITHUB_REPO "/releases/") + (gu.version[0] ? String("tag/v") + gu.version : "latest");
    up["error"] = gu.error;
    up["notes"] = gu.installing ? String(gu.progress) + " %" : String(gu.error);
}

void handleStats() {
    JsonDocument j;
    buildStats(j, server.arg("history") != "0");
    String body;
    body.reserve(measureJson(j));
    serializeJson(j, body);
    server.sendHeader("Cache-Control", "no-store");
    server.sendHeader("Access-Control-Allow-Origin", "*");  // read by the other miners' dashboards
    server.send(200, "application/json", body);
}

// Live dashboards: /api/events keeps its connection and receives the
// statistics every 2 s (Server-Sent Events) instead of asking for them. The
// server only drops its own reference to an answered client, so a copy kept
// here holds the socket open.
constexpr int kMaxStreams = 2;
constexpr uint32_t kStreamMs = 2000;
WiFiClient streams[kMaxStreams];
bool streamDue = false;

void handleEvents() {
    int slot = -1;
    for (int i = 0; i < kMaxStreams && slot < 0; i++)
        if (!streams[i].connected()) slot = i;
    if (slot < 0) {
        server.send(503, "text/plain", "Too many live dashboards; polling instead");
        return;
    }
    WiFiClient client = server.client();
    client.print("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n"
                 "Connection: keep-alive\r\n\r\nretry: 5000\n\n");
    streams[slot] = client;
    streamDue = true;  // a first snapshot right away
}

// Never blocks: a dashboard that cannot keep up is dropped (its browser
// reconnects) rather than holding up the web server.
bool sendNow(WiFiClient &client, const String &data) {
    int fd = client.fd();
    const char *p = data.c_str();
    size_t left = data.length();
    while (left) {
        int n = lwip_send(fd, p, left, MSG_DONTWAIT);
        if (n <= 0) return false;
        p += n;
        left -= n;
    }
    return true;
}

void pushStreams() {
    static uint32_t last = 0;
    if (!streamDue && millis() - last < kStreamMs) return;
    streamDue = false;
    last = millis();
    bool any = false;
    for (WiFiClient &c : streams) any = any || c.connected();
    if (!any) return;
    JsonDocument j;
    buildStats(j, false);
    String json;
    serializeJson(j, json);  // replaces the string's content: the prefix goes in afterwards
    String message;
    message.reserve(json.length() + 8);
    message = "data: ";
    message += json;
    message += "\n\n";
    for (WiFiClient &c : streams)
        if (c.connected() && !sendNow(c, message)) c.stop();
}

void handleHistory() {
    uint32_t revision;
    int n = minerHistoryPoints(historyScratch, HISTORY_LEN, &revision);
    String body;
    body.reserve(n * 32 + 160);
    body = String("{\"boot_id\":\"") + String(bootSession, HEX) + "\",\"interval_s\":" + HISTORY_INTERVAL_S + ",\"revision\":" + revision;
    const char *keys[] = {"hashrate", "temperature_c", "power_percent", "coverage_percent"};
    for (int field = 0; field < 4; ++field) {
        body += String(",\"") + keys[field] + "\":[";
        for (int i = 0; i < n; ++i) {
            if (i) body += ',';
            const auto &p = historyScratch[i];
            // No observation at all (the miner was off): a gap, not 0 H/s.
            if (field == 0) body += p.coveragePercent || p.hashrate ? String(p.hashrate, 0) : String("null");
            else if (field == 1) body += p.temperatureDeciC == INT16_MIN ? String("null") : String(p.temperatureDeciC / 10.0f, 1);
            else if (field == 2) body += p.coveragePercent ? String(p.powerPercent) : String("null");
            else body += String(p.coveragePercent);
        }
        body += ']';
    }
    body += '}';
    server.sendHeader("Cache-Control", "no-store");
    server.send(200, "application/json", body);
}

void handleDiagnostics() {
    const MinerStats stats = minerStats();
    MinerProfile workers[MAX_MINERS];
    minerProfiles(workers, MAX_MINERS);
    TlsProfile tls = tlsProfile();
    JsonDocument j;
    j["uptime_s"] = minerUptimeSeconds();
    j["firmware"] = FW_VERSION;
    j["tls"]["handshakes"] = tls.handshakes;
    j["boot_id"] = String(bootSession, HEX);
    j["tls"]["failures"] = tls.failures;
    j["tls"]["total_ms"] = tls.handshakeMs;
    j["tls"]["last_ms"] = tls.lastMs;
    JsonArray engines = j["engines"].to<JsonArray>();
    for (int i = 0; i < stats.minerCount; ++i) {
        if (!strcmp(stats.engine[i], "fill")) continue; // fill shares its hardware worker's time
        JsonObject engine = engines.add<JsonObject>();
        engine["index"] = i;
        engine["name"] = stats.engine[i];
        engine["hashing_ms"] = workers[i].hashingUs / 1000;
        engine["rest_ms"] = workers[i].restMs;
        engine["waiting_ms"] = workers[i].waitingMs;
        engine["paused_ms"] = workers[i].pausedMs;
    }
    String body;
    serializeJson(j, body);
    server.sendHeader("Cache-Control", "no-store");
    server.send(200, "application/json", body);
}

// A set password or webhook as the pages see it: secrets never leave the
// miner (anyone on the network can open these pages), and a save that sends
// this back keeps the secret as it is.
const char kSecretMask[] = "\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022";

// Every setting's value, under its key; the secrets themselves only with
// `secrets` (a backup protected by the web password).
void settingValues(JsonObject values, bool secrets) {
    Settings visible = settings;
    visible.brightness = settingsBrightness();
    for (size_t i = 0; i < kSettingCount; i++) {
        const SettingInfo &s = kSettings[i];
        const void *value = settingValue(s, visible);
        switch (s.type) {
            case SettingType::Text: values[s.key] = *(const String *)value; break;
            case SettingType::Secret: {
                const String &secret = *(const String *)value;
                if (secrets || secret.isEmpty()) values[s.key] = secret;
                else values[s.key] = kSecretMask;
                break;
            }
            case SettingType::Number:
                values[s.key] = s.bits == 8 ? *(const uint8_t *)value : *(const uint16_t *)value;
                break;
            case SettingType::Decimal: values[s.key] = *(const double *)value; break;
            case SettingType::Bool: values[s.key] = *(const bool *)value; break;
        }
    }
}

// Stage, normalize and save known keys; -1 means a required field became empty.
int applySettings(JsonObjectConst values) {
    // Network tasks keep using immutable live strings until the reboot.
    Settings pending = settings;
    pending.brightness = settingsBrightness();
    int changed = settingsApply(pending, values);
    if (changed) {
        settingsSanitize(pending);
        if (!pending.poolHost.length() || !pending.btcAddress.length()) return -1;
        if (!settingsSave(pending, true)) return -2;
    }
    return changed;
}

// Drops the secrets sent back masked: they stay as they are.
void keepSecrets(JsonObject values) {
    for (size_t i = 0; i < kSettingCount; i++)
        if (kSettings[i].type == SettingType::Secret && values[kSettings[i].key] == kSecretMask)
            values.remove(kSettings[i].key);
}

void handleGetSettings() {
    if (!authorized()) return;
    static const char *kTypes[] = {"text", "secret", "number", "decimal", "bool"};
    JsonDocument j;
    j["capabilities"]["ota"] = esp_ota_get_next_update_partition(nullptr) != nullptr;
    j["capabilities"]["firmware_env"] = FW_ENV;
#ifdef BOARD_GAMMA601
    j["capabilities"]["display_pages"]=3;
#elif defined(HAS_DISPLAY)
    j["capabilities"]["display_pages"]=8;
#else
    j["capabilities"]["display_pages"]=0;
#endif
#ifdef BOARD_GAMMA601
    j["capabilities"]["fan_control"] = true;
#endif
#ifdef BOARD_ASIC
    j["capabilities"]["asic"] = true;
    j["capabilities"]["thermal_steps"] = false;
#else
    j["capabilities"]["thermal_steps"] = true;
#endif
    JsonArray schema = j["schema"].to<JsonArray>();
    JsonArray agents = j["capabilities"]["user_agents"].to<JsonArray>();
    agents.add(user_agent::kNerdMiner);
    agents.add(DEFAULT_USER_AGENT);
    for (size_t i = 0; i < kSettingCount; i++) {
        const SettingInfo &s = kSettings[i];
        if (!strcmp(s.key, "uaauto")) continue; // retained only in saved records for rollback
        JsonArray row = schema.add<JsonArray>();
        row.add(s.key);
        row.add(kTypes[(int)s.type]);
        row.add(s.group);
        row.add(s.label);
        JsonObject limits = row.add<JsonObject>();
        if (s.type == SettingType::Number) {
            SettingLimits range = settingLimits(s);
            limits["min"] = range.minimum;
            limits["max"] = range.maximum;
        } else if (s.type == SettingType::Decimal) limits["min"] = 0;
    }
    settingValues(j["values"].to<JsonObject>(), false);
    JsonArray live = j["live"].to<JsonArray>();
    for (size_t i = 0; i < kSettingCount; i++)
        if (settingIsLive(kSettings[i].key)) live.add(kSettings[i].key);
    String body;
    serializeJson(j, body);
    server.sendHeader("Cache-Control", "no-store");
    server.send(200, "application/json", body);
}

// Saves the settings present in the JSON body (the page's form). Numbers and
// switches take effect at once; anything else restarts, which applies it the
// same way as at boot. Answers {"restart": bool, "changed": n}.
void handlePostSettings() {
    if (!authorized()) return;
    if (restartAt || ota::busy()) {
        server.send(409, "text/plain", "Firmware update or restart pending; try again afterwards");
        return;
    }
    JsonDocument j;
    if (deserializeJson(j, server.arg("plain")) || !j.is<JsonObject>()) {
        server.send(400, "text/plain", "Not a settings object");
        return;
    }
    keepSecrets(j.as<JsonObject>());
    if (const char *error = settingsError(j.as<JsonObjectConst>())) {
        server.send(400, "text/plain", error);
        return;
    }
    Settings current = settings;
    current.brightness = settingsBrightness();
    Settings pending = current;
    if (!settingsApply(pending, j.as<JsonObjectConst>())) {
        server.send(400, "text/plain", "No settings to save");
        return;
    }
    settingsSanitize(pending);
    if (!pending.poolHost.length() || !pending.btcAddress.length()) {
        server.send(400, "text/plain", "Pool host and payout address cannot be empty");
        return;
    }
    SettingsChange change = settingsCompare(pending, current);
    bool restart = change.count && !change.live;
    if (change.count && !settingsSave(pending, restart)) {
        server.send(500, "text/plain", "Settings could not be saved. Previous configuration kept; try again.");
        return;
    }
    char body[48];
    snprintf(body, sizeof(body), "{\"restart\":%s,\"changed\":%d}", restart ? "true" : "false", change.count);
    server.send(200, "application/json", body);
    if (restart) {
        logf("[web] %d settings saved, restarting\n", change.count);
        restartSoon("settings changed from the web page");
    } else if (change.count) {
        settingsCopyLive(settings, pending);
#ifdef BOARD_GAMMA601
        gamma601::setFanControl(pending.fanAuto, pending.fanPercent, pending.maxTemp);
#endif
        if (pending.brightness != current.brightness) settingsSetBrightness(pending.brightness);
        settingsChanged = true;  // the loop applies the screen settings
        logf("[web] %d settings applied without restarting\n", change.count);
    }
}

// GET /api/backup: settings (passwords included when a web password protects
// them) and lifetime statistics.
void handleGetBackup() {
    if (!authorized()) return;
    JsonDocument j;
    j["bdlminer_backup"] = 1;
    j["board"] = board::modelName();
    j["firmware"] = FW_VERSION;
    time_t now = time(nullptr);
    j["date"] = now > 1700000000 ? (uint32_t)now : 0;
    // Without a web password, anyone on the network could download it: no secrets then.
    settingValues(j["settings"].to<JsonObject>(), !settings.webPassword.isEmpty());
    StatsSnapshot s;
    minerStatsSnapshot(s);
    JsonObject st = j["stats"].to<JsonObject>();
    st["best"] = s.bestDiff;
    st["hashes"] = s.hashes;
    st["shares"] = s.shares;
    st["blocks"] = s.blocks;
    st["seconds"] = s.seconds;
    JsonArray top = st["top"].to<JsonArray>();
    for (const BestShare &b : s.top) {
        if (!(b.difficulty > 0)) break;
        JsonObject o = top.add<JsonObject>();
        o["d"] = b.difficulty;
        o["t"] = b.time;
    }
    String body;
    serializeJson(j, body);
    char name[80];
    snprintf(name, sizeof(name), "attachment; filename=\"%s-backup.json\"", hostName.c_str());
    server.sendHeader("Content-Disposition", name);
    server.sendHeader("Cache-Control", "no-store");
    server.send(200, "application/json", body);
}

// POST /api/backup: a file from GET /api/backup (or a settings-only export
// of an earlier version); restores what it holds, then restarts.
void handlePostBackup() {
    if (!authorized()) return;
    if (restartAt || ota::busy()) {
        server.send(409, "text/plain", "Firmware update or restart pending; try again afterwards");
        return;
    }
    JsonDocument j;
    if (deserializeJson(j, server.arg("plain")) || !j.is<JsonObject>()) {
        server.send(400, "text/plain", "Not a backup file");
        return;
    }
    if (const char *error = webvalidation::backupError(j.as<JsonObjectConst>(), TOP_SHARES)) {
        server.send(400, "text/plain", error);
        return;
    }
    bool full = j["bdlminer_backup"].is<unsigned>();
    keepSecrets(full ? j["settings"].as<JsonObject>() : j.as<JsonObject>());
    settingsMigrateBackupCoin(full ? j["settings"].as<JsonObject>() : j.as<JsonObject>(), full);
    settingsMigrateBackupAgent(full ? j["settings"].as<JsonObject>() : j.as<JsonObject>());
    JsonObjectConst values = full ? j["settings"].as<JsonObjectConst>() : j.as<JsonObjectConst>();
    if (const char *error = settingsError(values)) {
        server.send(400, "text/plain", error);
        return;
    }
    int settingsCount = applySettings(values);
    if (settingsCount == -2) {
        server.send(500, "text/plain", "Settings could not be saved. Previous configuration kept; try again.");
        return;
    }
    if (settingsCount < 0) {
        server.send(400, "text/plain", "Pool host and payout address cannot be empty");
        return;
    }
    JsonObjectConst st = j["stats"];
    bool withStats = full && !st.isNull();
    if (withStats) {
        StatsSnapshot s{};
        s.bestDiff = st["best"] | 0.0;
        s.hashes = st["hashes"] | (uint64_t)0;
        s.shares = st["shares"] | 0u;
        s.blocks = st["blocks"] | 0u;
        s.seconds = st["seconds"] | 0u;
        int i = 0;
        for (JsonObjectConst o : st["top"].as<JsonArrayConst>()) {
            if (i == TOP_SHARES) break;
            s.top[i++] = {o["d"] | 0.0, o["t"] | 0u};
        }
        // backupError() tolerates the order JSON rounding may change, but
        // the miner inserts new records into a list it expects best first.
        std::stable_sort(s.top, s.top + i, [](const BestShare &a, const BestShare &b) {
            return a.difficulty > b.difficulty;
        });
        if (!minerStatsRestore(s)) {
            server.send(500, "text/plain", "Settings saved, but statistics could not be restored. Retry before restarting.");
            return;
        }
    }
    if (!settingsCount && !withStats) {
        server.send(400, "text/plain", "Nothing to restore in this file");
        return;
    }
    logf("[web] backup restored: %d settings%s, restarting\n", settingsCount, withStats ? " and statistics" : "");
    server.send(200, "text/plain", String("Restored ") + settingsCount + " settings" +
                                       (withStats ? " and the statistics" : "") + ", restarting… reload in 15 s");
    restartSoon("backup restored from the web page");
}

void handleNotifyTest() {
    if (!authorized()) return;
    if (!notifyStatus().enabled) {
        server.send(409, "text/plain", "Set an ntfy topic or a webhook URL, save, then try again");
        return;
    }
    uint32_t id = notifyTest();
    if (!id) {
        server.send(503, "text/plain", "Notifications are busy; try again shortly");
        return;
    }
    char body[32];
    snprintf(body, sizeof(body), "{\"id\":%u}", (unsigned)id);
    server.send(202, "application/json", body);
}

void handleNotifyResult() {
    String raw = server.arg("id");
    char *end;
    unsigned long long id = strtoull(raw.c_str(), &end, 10);
    if (raw.length() && (end == raw.c_str() || *end || id > UINT32_MAX)) {
        server.send(400, "text/plain", "Invalid notification test ID");
        return;
    }
    server.sendHeader("Cache-Control", "no-store");
    server.send(200, "application/json", notifyResult(uint32_t(id)));
}

// POST /update (multipart): one firmware.bin built for this board.
bool uploadStarted = false, uploadComplete = false;
bool updateAllowed = false, updateBusy = false, uploadRejected = false;
String uploadError;

void resetUpload() {
    uploadStarted = uploadComplete = updateAllowed = updateBusy = uploadRejected = false;
    uploadError = "";
}

void abortWebUpload() {
    if (uploadStarted && updateAllowed && !restartAt && ota::owns(ota::Owner::Web)) {
        if (Update.isRunning()) Update.abort();
        ota::release(ota::Owner::Web);
    }
}

void uploadFailure(int status, const String &message) {
    abortWebUpload();
    server.send(status, "text/plain", message);
    resetUpload();
}

void handleUpload() {
    HTTPUpload &up = server.upload();
    if (up.status == UPLOAD_FILE_WRITE) vTaskDelay(1);
    if (up.status == UPLOAD_FILE_ABORTED) {
        abortWebUpload();
        resetUpload();
        return;
    }
    if (up.status == UPLOAD_FILE_START) {
        if (uploadStarted) {
            uploadRejected = true;
            uploadComplete = false;
            uploadError = "Only one firmware file is allowed";
            abortWebUpload();
            return;
        }
        resetUpload();
        uploadStarted = true;
        updateAllowed = fromOwnPage() &&
                        (settings.webPassword.isEmpty() || server.authenticate("admin", settings.webPassword.c_str()));
        if (!updateAllowed) return;
        if (restartAt) { updateBusy = true; return; }
        String name = up.filename;
        name.toLowerCase();
        if (!name.endsWith(".bin") || name.endsWith("factory.bin")) {
            uploadRejected = true;
            uploadError = "Use a firmware .bin file, not a factory image";
            return;
        }
        if (!ota::acquire(ota::Owner::Web)) { updateBusy = true; return; }
        logf("[web] firmware update: %s\n", up.filename.c_str());
        if (!Update.begin(UPDATE_SIZE_UNKNOWN)) {
            uploadError = Update.errorString();
            ota::release(ota::Owner::Web);
        }
        return;
    }
    if (!updateAllowed || updateBusy || uploadRejected || !ota::owns(ota::Owner::Web)) return;
    if (up.status == UPLOAD_FILE_WRITE) {
        if (Update.write(up.buf, up.currentSize) != up.currentSize) {
            uploadError = Update.errorString();
            abortWebUpload();
        }
    } else if (up.status == UPLOAD_FILE_END) {
        if (!up.totalSize) {
            uploadRejected = true;
            uploadError = "Empty firmware file";
            abortWebUpload();
        } else {
            // Activate the new image only after the entire multipart request
            // is parsed and authenticated. A later part or abort can still fail.
            uploadComplete = true;
        }
    }
}

void handleUpdateDone() {
    // Authenticate this request even if it has no multipart callbacks. Never
    // reuse authorization or success flags from an earlier upload.
    if (!authorized()) { abortWebUpload(); resetUpload(); return; }
    if (restartAt || updateBusy) {
        uploadFailure(409, "Another firmware update or restart is already pending");
        return;
    }
    if (!uploadStarted) { uploadFailure(400, "No firmware file uploaded"); return; }
    if (uploadRejected) { uploadFailure(400, uploadError); return; }
    if (!uploadComplete || !uploadError.isEmpty() || !ota::owns(ota::Owner::Web)) {
        String why = uploadError.isEmpty() ? String("Incomplete firmware upload") : uploadError;
        uploadFailure(500, String("Update failed: ") + why);
        return;
    }
    if (!Update.end(true)) {
        String why = Update.errorString();
        uploadFailure(500, String("Update failed: ") + why +
                          " (this board may require a USB update)");
        return;
    }
    logf("[web] firmware validated, restarting\n");
    resetUpload();
    server.send(200, "text/plain", "Firmware updated, restarting… reload in 20 s");
    restartSoon("firmware updated from the web page");
    // Keep OTA ownership until restart so neither miners nor another updater
    // can modify the newly selected image in the response/reboot interval.
}

// Prometheus text format, for Grafana / Home Assistant scrapers.
void handleMetrics() {
    const MinerStats stats = minerStats();
    String out;
    out.reserve(1536);
    auto metric = [&out](const char *name, const char *type, double value, const char *labels = "") {
        char line[160];
        if (type) {
            snprintf(line, sizeof(line), "# TYPE %s %s\n", name, type);
            out += line;
        }
        snprintf(line, sizeof(line), "%s%s %.17g\n", name, labels, value);
        out += line;
    };
    metric("bdlminer_hashrate_hps", "gauge", minerHashrate());
    metric("bdlminer_hashes_total", "counter", (double)minerTotalHashes());
    metric("bdlminer_shares_accepted_total", "counter", stats.accepted);
    metric("bdlminer_shares_rejected_total", "counter", stats.rejected);
    metric("bdlminer_blocks_found_total", "counter", stats.blocksFound);
    metric("bdlminer_block_candidates_total", "counter", stats.blocksFound);
    metric("bdlminer_best_difficulty", "gauge", stats.sessionBestDiff);
    metric("bdlminer_best_difficulty_ever", "gauge", stats.bestDiff);
    metric("bdlminer_shares_32bit_total", "counter", stats.over32bits);
    metric("bdlminer_shares_64bit_total", "counter", stats.over64bits);
    metric("bdlminer_lifetime_hashes_total", "counter", (double)stats.lifetimeHashes);
    metric("bdlminer_lifetime_shares_total", "counter", stats.lifetimeShares);
    metric("bdlminer_lifetime_seconds_total", "counter", stats.lifetimeSeconds);
    metric("bdlminer_pool_connected", "gauge", stats.poolConnected ? 1 : 0);
    metric("bdlminer_pool_backup", "gauge", netOnBackup() ? 1 : 0);
    metric("bdlminer_pool_difficulty", "gauge", stats.poolDiff);
    metric("bdlminer_block_height", "gauge", stats.blockHeight);
    metric("bdlminer_network_difficulty", "gauge", stats.networkDiff);
    metric("bdlminer_uptime_seconds", "gauge", minerUptimeSeconds());
    if (!isnan(board::temperature())) metric("bdlminer_temperature_celsius", "gauge", board::temperature());
    metric("bdlminer_throttle", "gauge", (int)minerThrottle());
    metric("bdlminer_power_percent", "gauge", minerPowerPercent());
    metric("bdlminer_wifi_rssi_dbm", "gauge", WiFi.RSSI());
    metric("bdlminer_free_heap_bytes", "gauge", ESP.getFreeHeap());
    for (int i = 0; i < stats.minerCount; i++) {
        char labels[40];
        // Software fallback can run two engines with the same name.
        snprintf(labels, sizeof(labels), "{engine=\"%s\",index=\"%d\"}", stats.engine[i], i);
        // A metric family has one TYPE line, even with several engines.
        metric("bdlminer_engine_hashrate_hps", i == 0 ? "gauge" : nullptr, minerEngineRate(i), labels);
    }
    char labels[96];
    snprintf(labels, sizeof(labels), "{board=\"%s\",firmware=\"%s\"}", board::modelName(), FW_VERSION);
    metric("bdlminer_info", "gauge", 1, labels);
    server.sendHeader("Cache-Control", "no-store");
    server.send(200, "text/plain; version=0.0.4", out);
}

void webTask(void *) {
    for (;;) {
        server.handleClient();
        pushStreams();
        vTaskDelay(pdMS_TO_TICKS(10));
    }
}

}  // namespace

uint32_t webBootId() { return bootSession; }

void webStart() {
    bootSession = esp_random();
    hostName = deviceHostName();
    const char *name = hostName.c_str();
    if (MDNS.begin(name)) MDNS.addService("http", "tcp", 80);

    server.on("/assets/style.css", HTTP_GET, []() { sendAsset("text/css; charset=utf-8", kWebStyle, sizeof(kWebStyle)); });
    server.on("/assets/common.js", HTTP_GET, []() { sendAsset("text/javascript; charset=utf-8", kWebCommon, sizeof(kWebCommon)); });
    server.on("/assets/dashboard.js", HTTP_GET, []() { sendAsset("text/javascript; charset=utf-8", kWebDashboard, sizeof(kWebDashboard)); });
    server.on("/assets/settings.js", HTTP_GET, []() { sendAsset("text/javascript; charset=utf-8", kWebSettings, sizeof(kWebSettings)); });
    server.on("/assets/log.js", HTTP_GET, []() { sendAsset("text/javascript; charset=utf-8", kWebLog, sizeof(kWebLog)); });
    server.on("/assets/icon-192.png", HTTP_GET, []() { sendAsset("image/png", kWebIcon192, sizeof(kWebIcon192), false); });
    server.on("/assets/icon-512.png", HTTP_GET, []() { sendAsset("image/png", kWebIcon512, sizeof(kWebIcon512), false); });
    server.on("/favicon.ico", HTTP_GET, []() { sendAsset("image/png", kWebIcon192, sizeof(kWebIcon192), false); });
    server.on("/manifest.webmanifest", HTTP_GET, []() {
        server.sendHeader("Content-Encoding", "gzip");
        server.sendHeader("Cache-Control", "no-cache");  // fixed URL: revalidate after an update
        server.send_P(200, "application/manifest+json", reinterpret_cast<PGM_P>(kWebManifest), sizeof(kWebManifest));
    });
    server.on("/", HTTP_GET, []() { sendPage("BDLMiner V2", kDashboard, "dashboard"); });
    server.on("/settings", HTTP_GET, []() { sendPage("BDLMiner V2 settings", kSettingsPage, "settings"); });
    server.on("/log", HTTP_GET, []() { sendPage("BDLMiner V2 log", kLogPage, "log"); });
    server.on("/api/stats", HTTP_GET, handleStats);
    server.on("/api/events", HTTP_GET, handleEvents);
    server.on("/api/history", HTTP_GET, handleHistory);
    server.on("/api/diagnostics", HTTP_GET, handleDiagnostics);
    server.on("/api/pool/diagnostics", HTTP_GET, handlePoolDiagnostics);
    server.on("/api/log", HTTP_GET, handleLog);
    server.on("/api/settings", HTTP_GET, handleGetSettings);
    server.on("/api/settings", HTTP_POST, handlePostSettings);
    server.on("/api/backup", HTTP_GET, handleGetBackup);
    server.on("/api/backup", HTTP_POST, handlePostBackup);
    server.on("/api/notify-test", HTTP_POST, handleNotifyTest);
    server.on("/api/notify-test", HTTP_GET, handleNotifyResult);
    server.on("/api/restart", HTTP_POST, []() {
        if (!authorized()) return;
        if (restartAt || ota::busy()) {
            server.send(409, "text/plain", "Firmware update or restart pending; try again afterwards");
            return;
        }
        server.send(202, "text/plain", "Restarting");
        restartSoon("restart requested from the web page");
    });
    server.on("/update", HTTP_POST, handleUpdateDone, handleUpload);
    // Only looks for a newer release, so no password: the installation asks for
    // it. One requested check per 30 s at most, so that nobody on the network
    // can keep the miner busy with TLS handshakes.
    server.on("/api/github-check", HTTP_POST, []() {
        static uint32_t lastRequest = 0;
        static bool requested = false;
        if (!fromOwnPage()) {
            server.send(403, "text/plain", "Refused: open the miner's own page to do this");
            return;
        }
        if (!settings.githubUpdate) {
            server.send(409, "text/plain", "GitHub updates are disabled in settings");
            return;
        }
        if (requested && millis() - lastRequest < 30000) {
            server.send(202, "text/plain", "Check requested");
            return;
        }
        if (ghUpdateCheck()) {
            requested = true;
            lastRequest = millis();
            server.send(202, "text/plain", "Check requested");
            return;
        }
        GithubUpdate state = ghUpdateState();
        // A check already queued or running answers this request as well.
        if (state.installing || ota::busy()) server.send(409, "text/plain", "An update is being installed");
        else server.send(202, "text/plain", "Check requested");
    });
    server.on("/api/github-update", HTTP_POST, []() {
        // Only installs the latest release, and only when it carries the
        // release key's signature.
        if (!authorized()) return;
        if (ghUpdateInstall()) server.send(200, "text/plain", "Installing, the miner restarts by itself");
        else server.send(409, "text/plain", "No update to install");
    });
    // The crash report is a memory image, which can hold passwords: password only.
    server.on("/api/coredump", HTTP_GET, []() {
        if (!authorized()) return;
        size_t size = crashSize();
        if (!size) {
            server.send(404, "text/plain", "No crash report");
            return;
        }
        server.sendHeader("Content-Disposition", "attachment; filename=\"bdlminer-coredump.elf\"");
        server.sendHeader("Cache-Control", "no-store");
        server.setContentLength(size);
        server.send(200, "application/octet-stream", "");
        static uint8_t chunk[1024];
        for (size_t offset = 0; offset < size;) {
            size_t n = min(sizeof(chunk), size - offset);
            if (!crashRead(offset, chunk, n)) break;
            server.sendContent((const char *)chunk, n);
            offset += n;
        }
    });
    server.on("/api/coredump/erase", HTTP_POST, []() {
        if (!authorized()) return;
        if (crashErase()) server.send(200, "text/plain", "Crash report erased");
        else server.send(500, "text/plain", "The crash report could not be erased");
    });
    server.on("/metrics", HTTP_GET, handleMetrics);
    server.onNotFound([]() { server.send(404, "text/plain", "Not found"); });
    static const char *kHeaders[] = {"Origin"};  // for fromOwnPage(); Host is always kept
    server.collectHeaders(kHeaders, 1);
    server.begin();
    // Slow HTTP clients must never stop the loop that owns the I2C sensors,
    // thermal control, buttons and screen. The worker uses cached snapshots.
    // Share core 0 with its software miner rather than preempting it for an
    // entire HTTP response. The hardware miner stays alone on core 1.
    if (!runtime::started(xTaskCreatePinnedToCore(webTask, "web", 6144, nullptr, 1, nullptr, 0) == pdPASS,
                          runtime::Web, "web")) {
        server.stop();
        logf("[web] not enough memory to start the web task\n");
        return;
    }
    logf("[web] dashboard: http://%s/ or http://%s.local/\n", WiFi.localIP().toString().c_str(), name);
}

void webHandle() {
    if (restartAt && (int32_t)(millis() - restartAt) >= 0) {
        // A validated web upload deliberately retains its OTA ownership.
        // Other reboots must reserve the slot without interrupting GitHub OTA.
        if (!ota::owns(ota::Owner::Web) && !ota::acquire(ota::Owner::Restart)) return;
        trialConfirm(restartWhy.load());  // the dashboard was usable
        minerSaveStats();  // does nothing after a restored backup
        setRestartReason(restartWhy.load());
        ESP.restart();
    }
}

const String &webHostName() { return hostName; }

void webRequestRestart(const char *why) {
    if (!restartAt) restartSoon(why);
}

bool webTakeSettingsChange() { return settingsChanged.exchange(false); }
