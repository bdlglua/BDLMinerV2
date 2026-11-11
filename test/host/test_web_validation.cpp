#include <assert.h>
#include <limits>
#include <stdio.h>

#include "web_validation.h"

static const char *kBackup = R"({"bdlminer_backup":1,"settings":{"pool":"example.org"},
    "stats":{"best":12.5,"hashes":18446744073709551615,"shares":3,"blocks":0,
    "seconds":100,"top":[{"d":12.5,"t":0},{"d":1,"t":1700000000}]}})";

static bool valid(JsonDocument &j) {
    return webvalidation::backupError(j.as<JsonObjectConst>(), 10) == nullptr;
}

int main() {
    JsonDocument j;
    assert(!deserializeJson(j, kBackup));
    assert(valid(j));
    assert(j["stats"]["hashes"].as<uint64_t>() == UINT64_MAX);

    // The old settings-only format remains accepted.
    assert(!deserializeJson(j, R"({"pool":"example.org","port":3333})"));
    assert(valid(j));

    for (const char *version : {"null", "0", "2", "true", "\"1\""}) {
        assert(!deserializeJson(j, kBackup));
        JsonDocument v;
        assert(!deserializeJson(v, version));
        j["bdlminer_backup"].set(v.as<JsonVariantConst>());
        assert(!valid(j));
    }
    for (const char *key : {"settings", "stats"}) {
        assert(!deserializeJson(j, kBackup));
        j.remove(key);
        assert(!valid(j));
    }
    // A truncated stats object must never silently reset missing totals to 0.
    for (const char *key : {"best", "hashes", "shares", "blocks", "seconds", "top"}) {
        assert(!deserializeJson(j, kBackup));
        j["stats"].remove(key);
        assert(!valid(j));
    }
    for (const char *key : {"hashes", "shares", "blocks", "seconds"}) {
        for (const char *value : {"-1", "1.5", "\"12\"", "true", "null"}) {
            assert(!deserializeJson(j, kBackup));
            JsonDocument v;
            assert(!deserializeJson(v, value));
            j["stats"][key].set(v.as<JsonVariantConst>());
            assert(!valid(j));
        }
    }
    assert(!deserializeJson(j, kBackup));
    j["stats"]["shares"] = (uint64_t)UINT32_MAX + 1;
    assert(!valid(j));
    for (double value : {-1.0, std::numeric_limits<double>::infinity(),
                          std::numeric_limits<double>::quiet_NaN()}) {
        assert(!deserializeJson(j, kBackup));
        j["stats"]["best"] = value;
        assert(!valid(j));
    }
    for (const char *top : {"{}", "[null]", "[{\"d\":1}]", "[{\"d\":0,\"t\":0}]"}) {
        assert(!deserializeJson(j, kBackup));
        JsonDocument v;
        assert(!deserializeJson(v, top));
        j["stats"]["top"].set(v.as<JsonVariantConst>());
        assert(!valid(j));
    }
    assert(!deserializeJson(j, kBackup));
    for (int i = 0; i < 9; i++) {
        JsonObject entry = j["stats"]["top"].as<JsonArray>().add<JsonObject>();
        entry["d"] = 1;
        entry["t"] = 0;
    }
    assert(!valid(j));
    assert(!deserializeJson(j, kBackup));
    j["stats"]["best"] = 0;
    j["stats"]["top"].to<JsonArray>();
    assert(valid(j));

    // ArduinoJson's float/double serialization can reverse the order of
    // very close difficulties. A real exported backup must remain readable.
    assert(!deserializeJson(j, kBackup));
    j["stats"]["best"] = 0.056910570710897453;
    j["stats"]["top"][0]["d"] = 0.056910570710897453;
    j["stats"]["top"][1]["d"] = 0.056910570710897446;
    std::string exported;
    serializeJson(j, exported);
    assert(!deserializeJson(j, exported));
    assert(valid(j));
    puts("Backup validation tests passed");

    using webvalidation::hostAllowed;
    using webvalidation::originAllowed;
    for (const char *host : {"", "192.168.1.40", "192.168.1.40:80", "[fe80::1]", "bdlminer-f4e1", "BDLMiner-F4E1.local",
                             "bdlminer-f4e1.local.", "bdlminer-f4e1.box.example", "miner.lan", "miner.home.arpa", "router"})
        assert(hostAllowed(host, "bdlminer-f4e1"));
    for (const char *host : {"evil.example", "bdlminer-f4e1x.example", "192.168.1.40.nip.io", "local.example",
                             "bdlminer.example:80"})
        assert(!hostAllowed(host, "bdlminer-f4e1"));
    assert(!hostAllowed("x.example", ""));
    assert(originAllowed(nullptr, "192.168.1.40") && originAllowed("", "192.168.1.40"));
    assert(originAllowed("http://192.168.1.40", "192.168.1.40"));
    assert(originAllowed("http://BDLMINER-f4e1.local", "bdlminer-f4e1.local"));
    for (const char *origin : {"null", "https://evil.example", "http://evil.example", "http://192.168.1.40.evil",
                               "https://192.168.1.40", "http://192.168.1.4"})
        assert(!originAllowed(origin, "192.168.1.40"));
    puts("Request origin tests passed");

    using webvalidation::HostKind;
    using webvalidation::hostKind;
    assert(hostKind("192.168.1.40", "bdlminer-f4e1") == HostKind::Local);
    assert(hostKind("bdlminer-f4e1.local", "bdlminer-f4e1") == HostKind::Local);
    assert(hostKind("bdlminer-f4e1.fritz.box", "bdlminer-f4e1") == HostKind::OwnNamed);
    assert(hostKind("bdlminer-0000.evil.example", "bdlminer-f4e1") == HostKind::Refused);
    {
        // Guessing the miner's name: a few refused names close the foreign-domain name.
        webvalidation::RebindingGuard guard;
        const char *own = "bdlminer-f4e1", *named = "bdlminer-f4e1.fritz.box";
        uint32_t now = 4000000000u;  // millis() wraps during the test
        assert(guard.allowed(named, own, now));
        assert(!guard.allowed("bdlminer-0000.evil.example", own, now));
        assert(!guard.allowed("bdlminer-0001.evil.example", own, now));
        assert(guard.allowed(named, own, now));  // two mistakes are not an attack
        assert(!guard.allowed("bdlminer-0002.evil.example", own, now += 1000));
        assert(!guard.allowed(named, own, now));
        assert(!guard.allowed("bdlminer-f4e1.evil.example", own, now));  // the right guess comes too late
        assert(guard.allowed("192.168.1.40", own, now) && guard.allowed("bdlminer-f4e1.local", own, now) &&
               guard.allowed("bdlminer-f4e1", own, now) && guard.allowed("", own, now));
        // Still closed just before the hour, and each refused name starts it again.
        assert(!guard.allowed(named, own, now += webvalidation::RebindingGuard::kLockMs - 1));
        assert(!guard.allowed("bdlminer-0003.evil.example", own, now));
        assert(!guard.allowed(named, own, now += webvalidation::RebindingGuard::kLockMs - 1));
        assert(guard.allowed(named, own, now += 1) && guard.misses == 0);
        // Mistakes more than an hour apart never add up.
        for (int i = 0; i < 6; i++) {
            assert(!guard.allowed("evil.example", own, now += webvalidation::RebindingGuard::kLockMs));
            assert(guard.allowed(named, own, now));
        }
    }
    puts("DNS rebinding guard tests passed");

    using webvalidation::originLocal;
    for (const char *origin : {"http://192.168.1.41", "http://192.168.1.41:8080", "http://10.0.0.7", "http://172.16.0.1",
                               "http://172.31.255.254", "http://169.254.3.4", "http://127.0.0.1", "http://100.64.0.1",
                               "http://100.127.9.9", "http://bdlminer-0a1b.local", "http://BDLMINER-0A1B.LAN",
                               "http://homeassistant", "http://nas.home.arpa:8123", "http://bdlminer-0a1b.local."})
        assert(originLocal(origin));
    for (const char *origin : {"", "null", "https://192.168.1.41", "http://8.8.8.8", "http://172.32.0.1", "http://172.15.0.1",
                               "http://100.128.0.1", "http://192.169.1.1", "http://192.168.1", "http://192.168.1.1.1",
                               "http://192.168.1.256", "http://192.168..1", "http://1921.168.1.1", "http://evil.example",
                               "http://bdlminer-0a1b.evil.example", "http://192.168.1.41.evil.example", "http://local.example",
                               "http://[::1]", "http://", "http://:80", "ftp://192.168.1.41"})
        assert(!originLocal(origin));
    assert(!originLocal(nullptr));
    puts("Local origin tests passed");
}
