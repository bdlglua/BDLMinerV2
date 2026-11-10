#!/usr/bin/env python3
"""Anonymous hashrate sharing: the report's content and its schedule."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
source = (root / 'src/share.cpp').read_text()
body = source[source.index('namespace {'):]
prefix = r'''
#include <cassert>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include "core/public_pool.h"
#include "core/coin.h"
#define FW_VERSION "1.9.0"
#define BOARD_NAME "M5StickC-Plus2"
namespace board { const char *model = "M5StickC-Plus2"; const char *modelName() { return model; } }
#define FW_ENV "m5stickc-plus2"
const char *statsUrl = "https://stats.example/report";
#define STATS_URL statsUrl
using std::isfinite;
struct String : std::string {
    using std::string::string;
    String(const std::string &s) : std::string(s) {}
    String(int n) : std::string(std::to_string(n)) {}
    int indexOf(const char *s, int from = 0) const { auto i = find(s, from); return i == npos ? -1 : (int)i; }
    int indexOf(char c, int from = 0) const { auto i = find(c, from); return i == npos ? -1 : (int)i; }
    String substring(int a, int b) const { return substr(a, b - a); }
    size_t length() const { return size(); }
};
void logf(const char *, ...) {}
uint32_t nowMs = 0;
uint32_t millis() { return nowMs; }
struct { bool connected = true; bool isConnected() const { return connected; } } WiFi;
bool memoryOk = true;
bool tlsMemoryOk() { return memoryOk; }
bool tlsClockSet() { return true; }
double rate = 1.0234e6;
double minerHashrate() { return rate; }
struct { bool shareHashrate = true, sharePool = true; unsigned miningCoin = 0; } settings;
struct { bool poolConnected = true; } stats;
auto minerStats() { return stats; }
String activePool = "public-pool.io";
const String &netPoolHost() { return activePool; }
struct WiFiClientSecure {};
std::vector<std::string> posted, hosts;
std::vector<uint16_t> ports;
int answer = 204;
bool tlsConnect(WiFiClientSecure &, const char *host, uint16_t port = 443, uint8_t = 12) { hosts.push_back(host); ports.push_back(port); return true; }
struct HTTPClient {
    std::string url;
    void setTimeout(int) {}
    bool begin(WiFiClientSecure &, const String &u) { url = u; return true; }
    void setUserAgent(const char *) {}
    void addHeader(const char *, const char *) {}
    int POST(uint8_t *data, size_t n) { posted.emplace_back((char *)data, n); return answer; }
    void end() {}
};
size_t shareBody(char *out, size_t size, double hashrate, const char *model = nullptr, const char *poolHost = nullptr, unsigned selectedCoin = 0);
bool sameName(const char *a, const char *b);
void sharePoll();
'''
tests = r'''
int main() {
    char endpoint[254]; uint16_t endpointPort;
    assert(shareEndpoint("https://api.bdl-pool.com:36018/report", endpoint, sizeof(endpoint), endpointPort));
    assert(!strcmp(endpoint,"api.bdl-pool.com") && endpointPort==36018);
    assert(shareEndpoint("https://stats.example/report", endpoint, sizeof(endpoint), endpointPort) && endpointPort==443);
    for(const char *bad:{"http://stats.example/report","https:///report","https://user:pass@stats.example/report",
                        "https://stats.example:0/report","https://stats.example:65536/report","https://stats.example:/report",
                        "https://stats.example:443x/report","https://stats.example:999999999999/report"})
        assert(!shareEndpoint(bad,endpoint,sizeof(endpoint),endpointPort));
    char out[256];
    assert(shareBody(out, sizeof(out), 1.0234e6));
    assert(!strcmp(out, R"({"v":"1.9.0","board":"m5stickc-plus2","hashrate":1.02e+06,"every":300,"coin":"BTC"})"));
    shareBody(out, sizeof(out), 987654321.0); assert(strstr(out, "\"hashrate\":9.88e+08,"));
    shareBody(out, sizeof(out), 0); assert(strstr(out, "\"hashrate\":0,"));
    shareBody(out, sizeof(out), NAN); assert(strstr(out, "\"hashrate\":0,"));
    assert(shareBody(out, 20, 1e6) == 0);
    shareBody(out, sizeof(out), 1e6, nullptr, "bch.example.org", 1);
    assert(strstr(out, R"("coin":"BCH")"));
    for(unsigned id=2;id<coin::Count;++id) {
        shareBody(out,sizeof(out),1e6,nullptr,nullptr,id);
        assert(std::string(out).find(std::string("\"coin\":\"")+coin::ticker(id)+"\"")!=std::string::npos);
    }
    assert(!shareBody(out,sizeof(out),1e6,nullptr,nullptr,coin::Count));
    assert(sameName("M5StickC Plus2", "M5StickC-Plus2") && sameName("abc", "A.B-C") && !sameName("M5Stack Core2", "M5Stack-ESP32") && !sameName("ab", "abc"));
    // The exact model, cleaned, only when given.
    shareBody(out, sizeof(out), 1e6, "M5Stack Core2");
    assert(!strcmp(out, R"({"v":"1.9.0","board":"m5stickc-plus2","hashrate":1e+06,"every":300,"coin":"BTC","model":"M5Stack Core2"})"));
    shareBody(out, sizeof(out), 1e6, "Bad\"name\\x"); assert(strstr(out, "\"model\":\"Badnamex\"}"));

    // A finite public group leaves the device, never the raw endpoint.
    for (const char *host : {"public-pool.io", "PUBLIC-POOL.IO.", "regional.public-pool.io"})
        assert(!strcmp(publicPoolGroup(host), "public-pool.io"));
    assert(!strcmp(publicPoolGroup("pool.nerdminers.org"), "nerdminers.org"));
    assert(!strcmp(publicPoolGroup("pool.nerdminer.io"), "nerdminer.io"));
    for (const char *domain : {"ckpool.org", "ocean.xyz", "braiins.com", "viabtc.com", "f2pool.com", "antpool.com"})
        assert(!strcmp(publicPoolGroup(domain), domain));
    for (const char *host : {"", "192.168.1.40", "miner.local", "pool.home.example", "notpublic-pool.io",
                             "public-pool.io.evil.test", "https://secret@public-pool.io", "public-pool.io:3333",
                             ".public-pool.io", "x..public-pool.io", "x/public-pool.io", "[fd00::1]"}) {
        assert(!strcmp(publicPoolGroup(host), "other"));
        shareBody(out, sizeof(out), 1e6, nullptr, host);
        assert(!strcmp(out, R"({"v":"1.9.0","board":"m5stickc-plus2","hashrate":1e+06,"every":300,"coin":"BTC","pool":"other"})"));
    }
    shareBody(out, sizeof(out), 1e6, "M5Stack Core2", "my-worker.public-pool.io");
    assert(strstr(out, "\"model\":\"M5Stack Core2\",\"pool\":\"public-pool.io\"}"));
    assert(!strstr(out, "my-worker"));
    assert(!shareBody(out, 25, 1e6, "M5Stack Core2", "public-pool.io"));

    assert(settings.sharePool);
    nowMs = kFirstMs; sharePoll();
    assert(posted.size() == 1 && posted.back().find(R"("pool":"public-pool.io")") != std::string::npos);
    posted.clear(); hosts.clear(); nowMs = 0; next = kFirstMs;
    settings.sharePool = false; // an owner can disable just the optional pool field
    sharePoll(); assert(posted.empty());                       // not before 3 minutes
    nowMs = kFirstMs; sharePoll(); assert(posted.size() == 1);
    assert(hosts.back() == "stats.example");
    assert(ports.back() == 443);
    assert(posted[0] == R"({"v":"1.9.0","board":"m5stickc-plus2","hashrate":1.02e+06,"every":300,"coin":"BTC"})");
    assert(kEveryMs == kEverySeconds * 1000 && kEverySeconds == 300);
    nowMs += kEveryMs - 1; sharePoll(); assert(posted.size() == 1);
    nowMs += 1; answer = 500; sharePoll(); assert(posted.size() == 2);
    nowMs += 60000; sharePoll(); assert(posted.size() == 2);   // a failure waits for the next report
    nowMs = next; rate = 0; sharePoll(); assert(posted.size() == 2);  // paused: nothing sent
    nowMs += kWaitMs; rate = 1e6; memoryOk = false; sharePoll(); assert(posted.size() == 2);
    nowMs += kWaitMs; memoryOk = true; settings.shareHashrate = false; sharePoll(); assert(posted.size() == 2);
    settings.shareHashrate = true; sharePoll(); assert(posted.size() == 3);
    assert(posted[2].find("model") == std::string::npos);   // the firmware's own name: not sent
    board::model = "M5StickC Plus2"; nowMs += kEveryMs; sharePoll();   // the same as BOARD_NAME: not sent
    assert(posted.size() == 4 && posted[3].find("model") == std::string::npos);
    board::model = "M5Stack Core2"; nowMs += kEveryMs; sharePoll();
    assert(posted.size() == 5 && posted[4].find(R"("model":"M5Stack Core2")") != std::string::npos);
    assert(posted.back().find("pool") == std::string::npos); // the saved opt-out is still respected
    settings.sharePool = true; nowMs += kEveryMs; sharePoll();
    assert(posted.size() == 6 && posted.back().find(R"("pool":"public-pool.io")") != std::string::npos);
    activePool = "pool.nerdminers.org"; nowMs += kEveryMs; sharePoll();
    assert(posted.size() == 7 && posted.back().find(R"("pool":"nerdminers.org")") != std::string::npos); // current backup
    activePool = "192.168.1.40"; nowMs += kEveryMs; sharePoll();
    assert(posted.size() == 8 && posted.back().find(R"("pool":"other")") != std::string::npos);
    assert(posted.back().find("192.168") == std::string::npos);
    stats.poolConnected = false; nowMs += kEveryMs; sharePoll();
    assert(posted.size() == 9 && posted.back().find("pool") == std::string::npos);
    settings.shareHashrate = false; nowMs += kEveryMs; sharePoll();
    assert(posted.size() == 9); // pool sharing cannot bypass the main opt-out
    settings.shareHashrate = true; settings.sharePool = false; stats.poolConnected = true; sharePoll();
    assert(posted.size() == 10 && posted.back().find("pool") == std::string::npos);
    statsUrl="https://api.bdl-pool.com:36018/report";
    nowMs += kEveryMs; sharePoll();
    assert(hosts.back()=="api.bdl-pool.com" && ports.back()==36018);
    const auto count=posted.size();statsUrl="http://api.bdl-pool.com:36018/report";
    nowMs += kEveryMs;sharePoll();assert(posted.size()==count);
    puts("share: OK (verified HTTPS on default and explicit ports)");
}
'''
with tempfile.TemporaryDirectory() as tmp:
    src = Path(tmp) / 'share_test.cpp'
    src.write_text(prefix + body + tests)
    exe = Path(tmp) / 'share_test'
    subprocess.run(['g++', '-std=c++17', '-Wall', '-Wno-unused-function', '-I'+str(root/'src'), str(src), '-o', str(exe)], check=True)
    subprocess.run([str(exe)], check=True)
