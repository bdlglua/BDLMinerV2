#include "settings.h"
#include "core/remote_data.h"

#include <Preferences.h>
#include <WiFi.h>
#include <WiFiManager.h>
#include <esp_wifi.h>
#include <freertos/semphr.h>
#include <atomic>
#include <algorithm>
#include <cmath>

#include "log.h"
#ifdef BOARD_GAMMA601
#include "core/bm1370.h"
#endif
#include "settings_store.h"
#include "core/pool_identity.h"
#include "core/display_config.h"

Settings settings;
static std::atomic<uint8_t> liveBrightness{80};
static SemaphoreHandle_t settingsSaveMutex = xSemaphoreCreateMutex();
static uint32_t settingsSequence = 0;

static const char *kNamespace = "bdlminer";

#define S(key, type, field, group, label) {key, SettingType::type, &settings.field, 0, group, label}
#define N(key, field, group, label) {key, SettingType::Number, &settings.field, 8 * sizeof(settings.field), group, label}

const SettingInfo kSettings[] = {
    N("coin", miningCoin, "Pool", "Cryptocurrency"),
    S("pool", Text, poolHost, "Pool", "Pool host"),
    N("port", poolPort, "Pool", "Pool port"),
    S("addr", Text, btcAddress, "Pool", "Payout address or pool account (optionally .worker)"),
    S("pass", Text, poolPassword, "Pool", "Pool password"),
    S("diff", Decimal, suggestedDifficulty, "Pool", "Suggested share difficulty"),
    N("proto", poolProtocol, "Pool", "Main pool protocol"),
    S("sv2key", Text, poolAuthority, "Pool", "Main pool Stratum V2 authority key"),
    S("uaauto", Bool, userAgentAuto, "Pool", "User agent mode"),
    S("ua", Text, userAgent, "Pool", "User agent"),
    S("pool2", Text, backupHost, "Pool", "Backup pool host (empty = none)"),
    N("port2", backupPort, "Pool", "Backup pool port"),
    N("proto2", backupProtocol, "Pool", "Backup pool protocol"),
    S("sv2key2", Text, backupAuthority, "Pool", "Backup pool Stratum V2 authority key"),
    N("rotateh", rotateHours, "Pool", "Alternate main / backup pool every N hours (0 = only on failure)"),
    N("bright", brightness, "Display", "Brightness (%)"),
    N("rot", rotation, "Display", "Rotation (0-3)"),
    S("autorot", Bool, autoRotate, "Display", "Rotate with the accelerometer (M5 devices)"),
    N("lang", language, "Display", "Language: 0 English, 1 French"),
    N("theme", theme, "Display", "Colour theme (0 orange, 1 green, 2 blue, 3 purple, 4 pink)"),
    N("cycle", cycleSeconds, "Display", "Next screen every N seconds (0 = off)"),
    S("customscreen", Bool, customScreen, "Display", "Custom mining screen"),
    N("screen0", screenWidget0, "Display", "First screen metric"),
    N("screen1", screenWidget1, "Display", "Second screen metric"),
    N("screen2", screenWidget2, "Display", "Third screen metric"),
    N("screen3", screenWidget3, "Display", "Fourth screen metric"),
    N("pages", screenPages, "Display", "Enabled screens"),
    N("nightfrom", nightFrom, "Display", "Night mode from hour (0-23, 255 = off)"),
    N("nightto", nightTo, "Display", "Night mode until hour (0-23)"),
    S("beep", Bool, beep, "Display", "Beep on a 32-bit share"),
    S("tz", Text, timezone, "Display", "Timezone (POSIX TZ, e.g. CET-1CEST,M3.5.0,M10.5.0/3 or UTC0)"),
    S("fiat", Text, currency, "Display", "Price currency (USD, EUR, GBP, CAD, CHF, AUD, JPY)"),
    S("ntfy", Text, ntfyTopic, "Notifications", "ntfy topic (empty = off)"),
    S("ntfysrv", Text, ntfyServer, "Notifications", "ntfy server"),
    S("notify32", Bool, notify32, "Notifications", "Also notify every 32-bit share"),
    S("hook", Secret, webhookUrl, "Notifications", "Webhook URL: JSON POST for Discord, Telegram... (empty = off, holds its token)"),
    N("alertpct", alertPercent, "Notifications", "Alert when hashrate stays below this % of normal (0 = off)"),
    N("alertmin", alertMinutes, "Notifications", "... for this many minutes"),
    N("summary", summaryHour, "Notifications", "Daily summary at this hour (0-23, 255 = off)"),
    N("maxtemp", maxTemp, "Safety", "Pause mining above this temperature in C (0 = off)"),
#ifdef BOARD_GAMMA601
    N("asicmhz", asicFrequency, "Safety", "Gamma ASIC frequency (MHz)"),
    N("asicmv", asicVoltage, "Safety", "Gamma ASIC core voltage (mV)"),
#endif
    N("power", miningPower, "Safety", "Mining power on USB and battery (0-100%, 0 = paused)"),
#ifdef BOARD_GAMMA601
    S("fanauto", Bool, fanAuto, "Safety", "Fan control"),
    N("fanpct", fanPercent, "Safety", "Manual fan speed (%)"),
#endif
    N("rebooth", rebootHours, "Safety", "Restart after N hours without an accepted share (0 = off)"),
    S("saver", Bool, batterySaver, "Safety", "Battery saver: dim and mine at half speed on battery"),
    S("wifi2", Text, wifi2Ssid, "Network", "Fallback WiFi 1 name"),
    S("wpass2", Secret, wifi2Pass, "Network", "Fallback WiFi 1 password"),
    S("wifi3", Text, wifi3Ssid, "Network", "Fallback WiFi 2 name"),
    S("wpass3", Secret, wifi3Pass, "Network", "Fallback WiFi 2 password"),
    S("mqtt", Text, mqttHost, "Network", "MQTT broker for Home Assistant (empty = off)"),
    N("mqttport", mqttPort, "Network", "MQTT port"),
    S("mqttuser", Text, mqttUser, "Network", "MQTT user"),
    S("mqttpass", Secret, mqttPass, "Network", "MQTT password"),
    S("peers", Text, peers, "Network", "Other miners on the dashboard (comma separated IPs or names)"),
    S("webpass", Secret, webPassword, "Network", "Web password (optional: protects settings and firmware update; empty = open)"),
    N("update", githubUpdate, "Network", "Updates from GitHub: 0 off, 1 check and offer, 2 install automatically"),
    N("channel", updateChannel, "Network", "Update channel"),
    S("count", Bool, usageCount, "Network",
      "Anonymous usage count: once a day, download a 2-byte file of this version's release (nothing is sent)"),
    S("sharehr", Bool, shareHashrate, "Network",
      "Share my hashrate anonymously: every 5 minutes, send the version, the board, the cryptocurrency and the hashrate"),
    S("sharepool", Bool, sharePool, "Network",
      "Share my pool anonymously: optional public pool group, only with hashrate sharing"),
    S("remote", Bool, remoteEnabled, "Network", "Private remote monitoring (optional, by payout address)"),
    S("remotekey", Secret, remoteKey, "Network", "Optional private remote access key (64 lowercase hexadecimal characters)"),
    S("rname", Text, remoteName, "Network", "Name in my remote dashboard"),
};
const size_t kSettingCount = sizeof(kSettings) / sizeof(kSettings[0]);

#undef S
#undef N

// Currencies quoted by mempool.space/api/v1/prices.
static const char *const kCurrencies[] = {"USD", "EUR", "GBP", "CAD", "CHF", "AUD", "JPY"};

static String normalizeCurrency(String c) {
    c.trim();
    c.toUpperCase();
    for (const char *k : kCurrencies)
        if (c == k) return c;
    return DEFAULT_CURRENCY;
}

// "stratum+tcp://pool.example.com:3333/" -> host "pool.example.com", port 3333
// (with `port`; without it a ":port" stays in `host`). Pool addresses are
// often pasted that way.
static void cleanHost(String &host, uint16_t *port) {
    host.trim();
    int scheme = host.indexOf("://");
    if (scheme >= 0) host.remove(0, scheme + 3);
    int slash = host.indexOf('/');
    if (slash >= 0) host.remove(slash);
    int colon = host.lastIndexOf(':');
    if (port && colon > 0 && host.indexOf(':') == colon) {  // not an IPv6 address
        long p = host.substring(colon + 1).toInt();
        if (port && p > 0 && p < 65536) *port = p;
        host.remove(colon);
    }
}

static void poolURL(String &host, uint8_t &protocol, String &authority) {
    auto identity = stratum::poolIdentity(host.c_str(), protocol, authority.c_str());
    protocol = identity.protocol;
    authority = identity.authority.c_str();
}

void settingsSanitize(Settings &settings) {
    if (!remote_data::canReport(settings.remoteKey.c_str(), settings.btcAddress.c_str())) settings.remoteEnabled = false;
    if (settings.remoteName.length() > 48) settings.remoteName.remove(48);
    if (!coin::valid(settings.miningCoin)) settings.miningCoin = coin::BTC;
    for (size_t i = 0; i < kSettingCount; i++)
        if (kSettings[i].type == SettingType::Text) ((String *)settingValue(kSettings[i], settings))->trim();
    poolURL(settings.poolHost, settings.poolProtocol, settings.poolAuthority);
    poolURL(settings.backupHost, settings.backupProtocol, settings.backupAuthority);
    cleanHost(settings.poolHost, &settings.poolPort);
    cleanHost(settings.backupHost, &settings.backupPort);
    cleanHost(settings.ntfyServer, nullptr);
    cleanHost(settings.mqttHost, &settings.mqttPort);
    if (!settings.poolPort) settings.poolPort = DEFAULT_POOL_PORT;
    if (!settings.backupPort) settings.backupPort = DEFAULT_BACKUP_PORT;
    if (!(settings.suggestedDifficulty > 0) || !std::isfinite(settings.suggestedDifficulty)) settings.suggestedDifficulty = DEFAULT_DIFFICULTY;
    settingsNormalizeAgent(settings);
    if (settings.poolProtocol > 1) settings.poolProtocol = 0;
    if (settings.backupProtocol > 1) settings.backupProtocol = 0;
    if (settings.timezone.isEmpty()) settings.timezone = DEFAULT_TIMEZONE;
    if (settings.ntfyServer.isEmpty()) settings.ntfyServer = DEFAULT_NTFY_SERVER;
    settings.currency = normalizeCurrency(settings.currency);
    if (settings.brightness > 100) settings.brightness = 100;
    settings.rotation &= 3;
    if (settings.language > 1) settings.language = 0;
    if (settings.githubUpdate > 2) settings.githubUpdate = 2;
    if(settings.updateChannel>1)settings.updateChannel=0;
    if(!display_config::validWidget(settings.screenWidget0))settings.screenWidget0=1;
    if(!display_config::validWidget(settings.screenWidget1))settings.screenWidget1=2;
    if(!display_config::validWidget(settings.screenWidget2))settings.screenWidget2=4;
    if(!display_config::validWidget(settings.screenWidget3))settings.screenWidget3=3;
    settings.screenPages|=1;
    if (settings.maxTemp && settings.maxTemp < 30) settings.maxTemp = 30;  // resumes 5 C below
#ifdef BOARD_GAMMA601
    if (!bm1370::validProfile(settings.asicFrequency)) settings.asicFrequency = bm1370::kStockMHz;
    if (!bm1370::validVoltage(settings.asicVoltage)) settings.asicVoltage = bm1370::kStockMv;
    if (settings.maxTemp > bm1370::kMaxUserTempC) settings.maxTemp = bm1370::kMaxUserTempC;  // saved before the limit
    settings.fanPercent = std::max<uint8_t>(30, std::min<uint8_t>(100, settings.fanPercent));
#endif
    if (settings.miningPower > 100) settings.miningPower = 100;
    if (settings.alertPercent > 100) settings.alertPercent = 100;
    if (!settings.alertMinutes) settings.alertMinutes = 15;
    if (settings.summaryHour > 23 && settings.summaryHour != HOUR_OFF) settings.summaryHour = 20;
    if (settings.nightFrom > 23) settings.nightFrom = HOUR_OFF;
    if (settings.nightTo > 23) settings.nightTo = HOUR_OFF;
}

// 2.5.3 raised the default thermal limit from 65 to 75 C on the boards that
// read the chip's own sensor. Every save stores every value, so a miner still
// at the old default is raised once ("tmp75" marks it done, after a
// successful save); any other saved choice, 0 included, is kept.
static void raiseChipThermalDefaultOnce() {
#if !defined(BOARD_ASIC) && !defined(DISPLAY_M5UNIFIED)
    Preferences p;
    if (!p.begin(kNamespace, true)) return;
    bool done = p.isKey("tmp75");
    p.end();
    if (done) return;
    if (settings.maxTemp == kOldMaxTempDefault) {
        Settings copy = settings;
        copy.maxTemp = kChipSensorMaxTemp;
        if (!settingsSave(copy)) return;  // tried again at the next start
        settings.maxTemp = kChipSensorMaxTemp;
        logf("[cfg] thermal limit raised to the new default: %u C (chip sensor)\n", (unsigned)kChipSensorMaxTemp);
    }
    if (p.begin(kNamespace, false)) {
        p.putBool("tmp75", true);
        p.end();
    }
#endif
}

// From 1.9.3 the dashboard asks for no password unless its owner sets one: a
// password saved by an earlier version is removed once ("pwopen" marks it
// done, after a successful save), so that no sign-in is ever asked after the
// update. One set afterwards is kept.
static void openDashboardOnce() {
    Preferences p;
    if (!p.begin(kNamespace, true)) return;
    bool done = p.isKey("pwopen");
    p.end();
    if (done) return;
    if (!settings.webPassword.isEmpty()) {
        Settings copy = settings;
        copy.webPassword = "";
        if (!settingsSave(copy)) return;  // tried again at the next start
        settings.webPassword = "";
        logf("[cfg] web password removed: the dashboard no longer asks for one\n");
    }
    if (p.begin(kNamespace, false)) {
        p.putBool("pwopen", true);
        p.end();
    }
}

void settingsLoad() {
    Preferences p;
    p.begin(kNamespace, true);
    if (settingsstore::load(p, settings, settingsSequence)) {
        p.end();
        settingsSanitize();
        liveBrightness = settings.brightness;
        openDashboardOnce();
        raiseChipThermalDefaultOnce();
        return;
    }
    for (size_t i = 0; i < kSettingCount; i++) {
        const SettingInfo &s = kSettings[i];
        switch (s.type) {
            case SettingType::Text:
            case SettingType::Secret: {
                String *v = (String *)s.value;
                *v = p.getString(s.key, *v);
                break;
            }
            case SettingType::Number:
                if (s.bits == 8) *(uint8_t *)s.value = p.getUChar(s.key, *(uint8_t *)s.value);
                else *(uint16_t *)s.value = p.getUShort(s.key, *(uint16_t *)s.value);
                break;
            case SettingType::Decimal: *(double *)s.value = p.getDouble(s.key, *(double *)s.value); break;
            case SettingType::Bool: *(bool *)s.value = p.getBool(s.key, *(bool *)s.value); break;
        }
    }
    // From 1.0.7 every miner installs updates by itself: settings saved before
    // (every save stored the value) switch to 2; those saved since carry
    // "updv" and keep their choice.
    if (!p.isKey("updv")) settings.githubUpdate = 2;
    p.end();
    settingsSanitize();
    liveBrightness = settings.brightness;
    openDashboardOnce();
    raiseChipThermalDefaultOnce();
}

uint8_t settingsBrightness() { return liveBrightness.load(); }

void settingsSetBrightness(uint8_t percent) {
    liveBrightness = percent > 100 ? 100 : percent;
    Settings copy = settings;
    copy.brightness = liveBrightness;
    settingsSave(copy);
}

bool settingsSave(const Settings &values, bool forRestart) {
    if (!settingsSaveMutex) return false;
    xSemaphoreTake(settingsSaveMutex, portMAX_DELAY);
    static bool stagedForRestart = false;
    // A brightness button during the reboot delay must not overwrite a
    // staged web/backup configuration with the old live settings.
    if (stagedForRestart && !forRestart) {
        xSemaphoreGive(settingsSaveMutex);
        return false;
    }
    Preferences p;
    if (!p.begin(kNamespace, false)) {
        xSemaphoreGive(settingsSaveMutex);
        logf("[cfg] cannot open settings storage\n");
        return false;
    }
    bool saved = settingsstore::save(p, values, settingsSequence);
    p.end();
    if (saved && forRestart) stagedForRestart = true;
    xSemaphoreGive(settingsSaveMutex);
    if (!saved) logf("[cfg] settings save failed; previous configuration kept\n");
    return saved;
}

String portalApName() {
    uint64_t mac = ESP.getEfuseMac();
    char buf[24];
    snprintf(buf, sizeof(buf), "BDLMinerV2-%04X", (unsigned)((mac >> 32) & 0xFFFF));
    return buf;
}

String deviceHostName() {
    char name[24];
    snprintf(name, sizeof(name), "bdlminer-%04x", (unsigned)((ESP.getEfuseMac() >> 32) & 0xFFFF));
    return name;
}

static bool shouldSave = false;

// The portal's network, read once connected: fallbacks are joined without
// saving them, so this is how to get back to it.
static String mainSsid, mainPass;

static bool waitConnected(uint32_t ms) {
    for (uint32_t t0 = millis(); millis() - t0 < ms;) {
        if (WiFi.isConnected()) return true;
        delay(100);
    }
    return WiFi.isConnected();
}

bool wifiTryFallbacks() {
    const String *nets[][2] = {{&mainSsid, &mainPass},
                               {&settings.wifi2Ssid, &settings.wifi2Pass},
                               {&settings.wifi3Ssid, &settings.wifi3Pass}};
    WiFi.persistent(false);  // never replace the portal's network in flash
    bool ok = false;
    for (auto &n : nets) {
        if (n[0]->isEmpty()) continue;
        logf("[wifi] trying \"%s\"\n", n[0]->c_str());
        WiFi.disconnect();
        WiFi.begin(n[0]->c_str(), n[1]->c_str());
        if ((ok = waitConnected(15000))) break;
    }
    WiFi.persistent(true);
    return ok;
}

void wifiSetup(bool forcePortal, void (*onPortal)()) {
    // Before the station starts: the router then lists (and its local DNS
    // often names) the miner bdlminer-xxxx, a name the dashboard accepts
    // under any domain (bdlminer-xxxx.fritz.box), instead of esp32-xxxxxx.
    WiFi.setHostname(deviceHostName().c_str());
    WiFi.mode(WIFI_STA);
    WiFi.setSleep(false);

    // Without a payout address there is nothing useful to mine for.
    if (settings.btcAddress.isEmpty()) forcePortal = true;

    // The network saved by the portal, also when a fallback ends up joined.
    wifi_config_t saved;
    if (esp_wifi_get_config(WIFI_IF_STA, &saved) == ESP_OK && saved.sta.ssid[0]) {
        char ssid[33] = {0}, pass[65] = {0};
        memcpy(ssid, saved.sta.ssid, sizeof(saved.sta.ssid));
        memcpy(pass, saved.sta.password, sizeof(saved.sta.password));
        mainSsid = ssid;
        mainPass = pass;
    }

    if (!forcePortal) {
        WiFi.begin();  // the network saved by the portal
        if (waitConnected(15000) || wifiTryFallbacks()) return;
    }

    WiFiManager wm;
    wm.setDebugOutput(false);
    wm.setTitle("BDLMiner V2");
    wm.setConfigPortalTimeout(PORTAL_TIMEOUT_S);
    wm.setConnectTimeout(20);
    wm.setSaveConfigCallback([]() { shouldSave = true; });
    wm.setAPCallback([onPortal](WiFiManager *) {
        if (onPortal) onPortal();
    });

    char port[8], port2[8], diff[16], beep[2] = {settings.beep ? '1' : '0', 0}, lang[2] = {char('0' + settings.language), 0};
    snprintf(port, sizeof(port), "%u", settings.poolPort);
    snprintf(port2, sizeof(port2), "%u", settings.backupPort);
    snprintf(diff, sizeof(diff), "%g", settings.suggestedDifficulty);
    char proto[2] = {char('0' + settings.poolProtocol), 0}, proto2[2] = {char('0' + settings.backupProtocol), 0};
    String portalAgent = settings.userAgent;
    const std::string agentHtml = user_agent::portalSelect(portalAgent.c_str(), DEFAULT_USER_AGENT);
    wm.setSaveParamsCallback([&]() {
        if (wm.server && wm.server->hasArg("ua")) portalAgent = wm.server->arg("ua");
    });

    char selectedCoin[2] = {char('0' + settings.miningCoin), 0};
    WiFiManagerParameter pCoin("coin", "SHA-256d: 0 BTC, 1 BCH, 2 BSV, 3 XEC, 4 DGB, 5 PPC (PoW). Use matching pools/address; other coins require Stratum V1. DGB: SHA-256d port only.", selectedCoin, 1, "type='number' min='0' max='5'");
    WiFiManagerParameter pHost("pool", "Pool host", settings.poolHost.c_str(), 80);
    WiFiManagerParameter pPort("port", "Pool port", port, 6, "type='number' min='1' max='65535'");
    WiFiManagerParameter pHost2("pool2", "Backup pool host (optional, e.g. solo.ckpool.org)",
                                settings.backupHost.c_str(), 80);
    WiFiManagerParameter pPort2("port2", "Backup pool port", port2, 6, "type='number' min='1' max='65535'");
    WiFiManagerParameter pAddr("addr", "Payout address or pool account (optionally .worker)",
                               settings.btcAddress.c_str(), 100);
    WiFiManagerParameter pPass("pass", "Pool password", settings.poolPassword.c_str(), 40);
    WiFiManagerParameter pDiff("diff", "Suggested share difficulty", diff, 15);
    WiFiManagerParameter pProto("proto", "Main protocol: 0 Stratum V1, 1 Stratum V2 (Noise)", proto, 1, "type='number' min='0' max='1'");
    WiFiManagerParameter pKey("sv2key", "Main SV2 authority key (required for V2)", settings.poolAuthority.c_str(), 64);
    WiFiManagerParameter pProto2("proto2", "Backup protocol: 0 Stratum V1, 1 Stratum V2 (Noise)", proto2, 1, "type='number' min='0' max='1'");
    WiFiManagerParameter pKey2("sv2key2", "Backup SV2 authority key (required for V2)", settings.backupAuthority.c_str(), 64);
    WiFiManagerParameter pUa(agentHtml.c_str());
    WiFiManagerParameter pBeep("beep", "Beep on a 32-bit share (1 = on, 0 = off)", beep, 1,
                               "type='number' min='0' max='1'");
    WiFiManagerParameter pTz("tz", "Timezone (POSIX TZ, e.g. CET-1CEST,M3.5.0,M10.5.0/3 or UTC0)",
                             settings.timezone.c_str(), 48);
    WiFiManagerParameter pFiat("fiat", "Price currency (USD, EUR, GBP, CAD, CHF, AUD, JPY)",
                               settings.currency.c_str(), 4);
    WiFiManagerParameter pLang("lang", "Language: 0 English, 1 French (Francais)", lang, 1,
                               "type='number' min='0' max='1'");
    // Never sent back to the portal: its access point has a published
    // password, and the page would hand the web password (in its HTML) to
    // anyone nearby while the miner cannot join its network. Left empty, the
    // one already set is kept; the dashboard's settings remove it.
    WiFiManagerParameter pWeb("webpass",
                              "Web password, optional: only to protect the dashboard's settings and firmware "
                              "update on a shared network. Leave empty to keep the current setting",
                              "", 40, "type='password'");
    wm.addParameter(&pCoin);
    wm.addParameter(&pHost);
    wm.addParameter(&pPort);
    wm.addParameter(&pProto);
    wm.addParameter(&pKey);
    wm.addParameter(&pHost2);
    wm.addParameter(&pPort2);
    wm.addParameter(&pProto2);
    wm.addParameter(&pKey2);
    wm.addParameter(&pAddr);
    wm.addParameter(&pPass);
    wm.addParameter(&pDiff);
    wm.addParameter(&pUa);
    wm.addParameter(&pBeep);
    wm.addParameter(&pTz);
    wm.addParameter(&pFiat);
    wm.addParameter(&pLang);
    wm.addParameter(&pWeb);

    String ap = portalApName();
    // The portal runs in this loop, which pauses between two passes, rather
    // than in WiFiManager's, which never does: a Gamma 601 restarted twice on
    // the task watchdog during its setup.
    wm.setConfigPortalBlocking(false);
    // After a failed attempt (a mistyped password, a network out of reach),
    // WiFiManager's own loop turns the station off so that the access point
    // stays steady; its non-blocking mode does not, and the miner kept trying
    // the wrong network while the phone could drop off the portal. Done here
    // the same way; the next attempt turns the station back on.
    bool attempted = false;
    wm.setPreSaveConfigCallback([&attempted]() { attempted = true; });
    bool connected = forcePortal ? wm.startConfigPortal(ap.c_str(), PORTAL_AP_PASSWORD)
                                 : wm.autoConnect(ap.c_str(), PORTAL_AP_PASSWORD);
    while (!connected && wm.getConfigPortalActive()) {
        connected = wm.process();  // false once the portal times out
        if (attempted && !connected) {
            attempted = false;
            WiFi.disconnect();
            WiFi.enableSTA(false);
        }
        delay(10);
    }

    if (shouldSave) {
        settings.miningCoin = atoi(pCoin.getValue());
        settings.poolHost = pHost.getValue();
        settings.poolPort = atoi(pPort.getValue());
        settings.backupHost = pHost2.getValue();
        settings.backupPort = atoi(pPort2.getValue());
        settings.btcAddress = pAddr.getValue();
        settings.poolPassword = pPass.getValue();
        settings.suggestedDifficulty = atof(pDiff.getValue());
        settings.poolProtocol = atoi(pProto.getValue()); settings.backupProtocol = atoi(pProto2.getValue());
        settings.poolAuthority = pKey.getValue(); settings.backupAuthority = pKey2.getValue();
        settings.userAgent = portalAgent;
        settings.beep = atoi(pBeep.getValue()) != 0;
        settings.timezone = pTz.getValue();
        settings.currency = pFiat.getValue();
        if (pWeb.getValue()[0]) settings.webPassword = pWeb.getValue();
        settings.language = atoi(pLang.getValue());
        settingsSanitize();
        logf(settingsSave() ? "[cfg] settings saved\n" : "[cfg] settings could not be saved\n");
    }

    if (!connected || settings.btcAddress.isEmpty()) {
        logf("[wifi] not configured, restarting\n");
        delay(1000);
        ESP.restart();
    }
    mainSsid = WiFi.SSID();
    mainPass = WiFi.psk();
}

void settingsFactoryReset() {
    Preferences p;
    p.begin(kNamespace, false);
    p.clear();
    p.end();
    WiFi.disconnect(true, true);  // also erases stored credentials
    delay(500);
    ESP.restart();
}
