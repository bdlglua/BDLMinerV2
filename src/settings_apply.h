// Apply web/backup values to a staged copy; live network strings stay intact.
#pragma once

#include <ArduinoJson.h>
#include <cmath>
#include <cctype>
#include <cstring>
#include <utility>
#include "settings.h"
#ifdef BOARD_GAMMA601
#include "core/bm1370.h"
#endif
#include "core/sv2_noise.h"
#include "core/pool_identity.h"
#include "core/display_config.h"
#include "core/remote_data.h"

struct SettingLimits { long minimum, maximum; };
// Old complete backups predate coin selection and describe a BTC configuration.
// Partial settings-only restores keep the current chain unless they include a
// complete pool identity. Never reinterpret an old BTC backup as BCH.
inline void settingsMigrateBackupCoin(JsonObject values, bool full) {
    if (values["coin"].isUnbound() && (full || (!values["pool"].isUnbound() && !values["addr"].isUnbound())))
        values["coin"] = unsigned(coin::BTC);
}
inline void settingsMigrateBackupAgent(JsonObject values) {
    if (values["ua"].is<const char *>()) values["ua"] = user_agent::normalize(values["ua"].as<const char *>(), DEFAULT_USER_AGENT);
    if (values["uaauto"].is<bool>()) values["uaauto"] = false;
}
inline SettingLimits settingLimits(const SettingInfo &s) {
    SettingLimits limits{0, s.bits == 8 ? 255 : 65535};
    if (!strcmp(s.key, "port") || !strcmp(s.key, "port2") || !strcmp(s.key, "mqttport")) limits.minimum = 1;
    if (!strcmp(s.key, "bright") || !strcmp(s.key, "power") || !strcmp(s.key, "alertpct")) limits.maximum = 100;
    if (!strcmp(s.key, "alertmin")) { limits.minimum = 1; limits.maximum = 240; }
    if (!strcmp(s.key, "rot")) limits.maximum = 3;
    if (!strcmp(s.key, "lang")) limits.maximum = 1;
    if (!strcmp(s.key, "coin")) limits.maximum = coin::Count - 1;
    if (!strcmp(s.key, "theme")) limits.maximum = 4;
    if (!strcmp(s.key, "proto") || !strcmp(s.key, "proto2")) limits.maximum = 1;
    if (!strcmp(s.key, "update")) limits.maximum = 2;
    if(!strcmp(s.key,"channel"))limits.maximum=1;
    if(!strncmp(s.key,"screen",6)&&strlen(s.key)==7){limits.minimum=1;limits.maximum=display_config::kWidgets;}
    if(!strcmp(s.key,"pages"))limits.minimum=1;
#ifdef BOARD_GAMMA601
    if (!strcmp(s.key, "asicmhz")) { limits.minimum=bm1370::kStockMHz; limits.maximum=bm1370::kMaxMHz; }
    if (!strcmp(s.key, "asicmv")) { limits.minimum=bm1370::kMinMv; limits.maximum=bm1370::kMaxMv; }
    if (!strcmp(s.key, "fanpct")) { limits.minimum = 30; limits.maximum = 100; }
    // The ASIC diode's hard stop at 75 C latches until a restart: the user
    // limit must pause (and cool) before it, never above it.
    if (!strcmp(s.key, "maxtemp")) limits.maximum = bm1370::kMaxUserTempC;
#endif
    return limits;
}

// Validate the complete request before staging or persisting any field. Null,
// strings masquerading as numbers and blank numeric inputs must not disable
// safety settings or silently become zero.
inline const char *settingsError(JsonObjectConst values) {
    for (size_t i = 0; i < kSettingCount; i++) {
        const SettingInfo &s = kSettings[i];
        JsonVariantConst v = values[s.key];
        if (v.isUnbound()) continue;
        switch (s.type) {
            case SettingType::Text:
            case SettingType::Secret:
                if (!v.is<const char *>()) return "Invalid text setting";
                if (!strcmp(s.key, "ua") && strlen(v.as<const char *>()) > 64) return "User agent must be at most 64 characters";
                if (!strcmp(s.key, "remotekey") && *v.as<const char *>() && !remote_data::validKey(v.as<const char *>()))
                    return "The remote access key must contain 64 lowercase hexadecimal characters";
                if (!strcmp(s.key, "rname") && strlen(v.as<const char *>()) > 48)
                    return "The remote miner name must be at most 48 bytes";
                if (!strcmp(s.key, "hook")) {
                    const char *url = v.as<const char *>();
                    while (*url && std::isspace((unsigned char)*url)) ++url;
                    if (*url && strncmp(url, "http://", 7) && strncmp(url, "https://", 8))
                        return "The webhook URL must start with http:// or https://";
                }
                if (!strcmp(s.key, "pool") || !strcmp(s.key, "addr")) {
                    const char *text = v.as<const char *>();
                    while (*text && std::isspace((unsigned char)*text)) ++text;
                    if (!*text) return "Pool host and payout address cannot be empty";
                }
                break;
            case SettingType::Number: {
                SettingLimits limits = settingLimits(s);
                if (!v.is<long>() || v.as<long>() < limits.minimum || v.as<long>() > limits.maximum)
                    return "Setting outside its allowed range";
                long n = v.as<long>();
                if(!strcmp(s.key,"pages")&&!(n&1))return "The mining screen must remain enabled";
#ifdef BOARD_GAMMA601
                if (!strcmp(s.key, "asicmhz") && !bm1370::validProfile(unsigned(n)))
                    return "Gamma frequency must be 525-800 MHz in 25 MHz steps, or 690 MHz";
                if (!strcmp(s.key, "asicmv") && !bm1370::validVoltage(unsigned(n)))
                    return "Gamma core voltage must be 1000, 1060, 1100, 1150, 1200, 1250 or 1300 mV";
#endif
                if (!strcmp(s.key, "maxtemp") && n && n < 30) return "Temperature must be 0 or at least 30 C";
                if ((!strcmp(s.key, "nightfrom") || !strcmp(s.key, "nightto")) && n > 23 && n != 255)
                    return "Night hours must be 0-23 or 255";
                if (!strcmp(s.key, "summary") && n > 23 && n != 255) return "The summary hour must be 0-23 or 255";
                break;
            }
            case SettingType::Decimal:
                if (!v.is<double>() || !std::isfinite(v.as<double>()) || v.as<double>() <= 0)
                    return "Difficulty must be a finite positive number";
                break;
            case SettingType::Bool:
                if (!v.is<bool>()) return "Invalid checkbox setting";
                break;
        }
    }
    if ((values["remote"] | settings.remoteEnabled) &&
        !remote_data::canReport(values["remotekey"] | settings.remoteKey.c_str(), values["addr"] | settings.btcAddress.c_str()))
        return "Set a payout address before enabling remote monitoring";
    unsigned selectedCoin = values["coin"] | unsigned(settings.miningCoin);
    const char *user = values["addr"] | settings.btcAddress.c_str();
    if (!coin::addressCompatible(selectedCoin, stratum::trimIdentity(user))) return "Use an address for the selected cryptocurrency";
    const char *agent = values["ua"] | settings.userAgent.c_str();
    if (!user_agent::valid(agent, DEFAULT_USER_AGENT)) return "Choose one of the available user agents";
    for (const auto &fields : {std::pair<const char *, const char *>{"proto", "sv2key"}, {"proto2", "sv2key2"}}) {
        unsigned protocol = values[fields.first] | unsigned(!strcmp(fields.first, "proto") ? settings.poolProtocol : settings.backupProtocol);
        const char *key = values[fields.second] | (!strcmp(fields.first, "proto") ? settings.poolAuthority.c_str() : settings.backupAuthority.c_str());
        const bool main = !strcmp(fields.first, "proto");
        const char *host = values[main ? "pool" : "pool2"] | (main ? settings.poolHost.c_str() : settings.backupHost.c_str());
        auto identity = stratum::poolIdentity(host, protocol, key);
        if (main || !stratum::trimIdentity(host).empty())
            if (const char *error = coin::poolError(selectedCoin, host, identity.protocol)) return error;
        uint8_t authority[32];
        if (identity.protocol == 1 && !sv2::authorityKey(identity.authority.c_str(), authority)) return "A valid Stratum V2 authority key is required";
    }
    return nullptr;
}

inline int settingsApply(Settings &pending, JsonObjectConst values) {
    int changed = 0;
    for (size_t i = 0; i < kSettingCount; i++) {
        const SettingInfo &s = kSettings[i];
        JsonVariantConst v = values[s.key];
        if (v.isNull()) continue;
        void *value = settingValue(s, pending);
        switch (s.type) {
            case SettingType::Text:
            case SettingType::Secret: *(String *)value = v.as<String>(); break;
            case SettingType::Number: {
                long n = v.as<long>();
                long maximum = s.bits == 8 ? 255 : 65535;
                n = n < 0 ? 0 : n > maximum ? maximum : n;
                if (s.bits == 8) *(uint8_t *)value = n;
                else *(uint16_t *)value = n;
                break;
            }
            case SettingType::Decimal: *(double *)value = v.as<double>(); break;
            case SettingType::Bool: *(bool *)value = v.as<bool>(); break;
        }
        changed++;
    }
    settingsNormalizeAgent(pending);
    return changed;
}

// Settings applied while the miner runs: numbers and switches that are read
// where they are used (supervision, screen, alerts, updates). Text settings,
// the pool, WiFi and MQTT rebuild connections or tasks: they need a restart,
// and other tasks read those strings without a lock.
inline bool settingIsLive(const char *key) {
    static const char *const kLive[] = {"bright", "rot",      "autorot",  "lang",     "theme",  "cycle",
                                        "nightfrom", "nightto", "beep",   "notify32", "alertpct", "alertmin", "summary",
                                        "maxtemp", "power",    "rebooth",  "saver",    "update", "count", "sharehr", "sharepool",
                                        "fanauto", "fanpct", "customscreen", "screen0", "screen1", "screen2", "screen3", "pages", "channel", "remote"};
    for (const char *k : kLive)
        if (!strcmp(k, key)) return true;
    return false;
}

inline bool settingDiffers(const SettingInfo &s, const Settings &a, const Settings &b) {
    const void *x = settingValue(s, a), *y = settingValue(s, b);
    switch (s.type) {
        case SettingType::Text:
        case SettingType::Secret: return *(const String *)x != *(const String *)y;
        case SettingType::Number:
            return s.bits == 8 ? *(const uint8_t *)x != *(const uint8_t *)y : *(const uint16_t *)x != *(const uint16_t *)y;
        case SettingType::Decimal: return *(const double *)x != *(const double *)y;
        case SettingType::Bool: return *(const bool *)x != *(const bool *)y;
    }
    return false;
}

// How `pending` differs from `current`: the number of changed settings, and
// whether all of them can be applied without a restart.
struct SettingsChange { int count = 0; bool live = true; };
inline SettingsChange settingsCompare(const Settings &pending, const Settings &current) {
    SettingsChange change;
    for (size_t i = 0; i < kSettingCount; i++) {
        if (!settingDiffers(kSettings[i], pending, current)) continue;
        change.count++;
        if (!settingIsLive(kSettings[i].key)) change.live = false;
    }
    return change;
}

// Copies the live settings that changed (numbers and switches only).
inline void settingsCopyLive(Settings &live, const Settings &pending) {
    for (size_t i = 0; i < kSettingCount; i++) {
        const SettingInfo &s = kSettings[i];
        if (!settingIsLive(s.key) || s.type == SettingType::Text || s.type == SettingType::Secret) continue;
        size_t size = s.type == SettingType::Number ? s.bits / 8 : s.type == SettingType::Decimal ? sizeof(double) : sizeof(bool);
        memcpy(settingValue(s, live), settingValue(s, pending), size);
    }
}
