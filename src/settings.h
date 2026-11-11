// Persistent user settings (NVS), the WiFiManager configuration portal and
// the table that describes every setting to the web page (export / import).
#pragma once

#include <Arduino.h>
#include <cstring>
#include "core/coin.h"
#include "core/user_agent.h"

inline bool legacyManualAgent(const char *agent) {
    return agent && *agent && strcmp(agent, "NerdMinerV2") &&
        strncmp(agent, "BDLMinerV1/", 11) && strncmp(agent, "BDLMinerV2/", 11);
}

#define DEFAULT_POOL_HOST "public-pool.io"
#ifdef BOARD_ASIC
#define DEFAULT_POOL_PORT 3333
#define DEFAULT_DIFFICULTY 1024.0
#else
#define DEFAULT_POOL_PORT 21496
#define DEFAULT_DIFFICULTY 0.0002
#endif
#define DEFAULT_POOL_PASSWORD "x"
#define DEFAULT_BACKUP_PORT 3333
#define PORTAL_AP_PASSWORD "MineYourCoins"
#define PORTAL_TIMEOUT_S 180
// POSIX TZ string (Central European Time with daylight saving by default).
#define DEFAULT_TIMEZONE "CET-1CEST,M3.5.0,M10.5.0/3"
#define DEFAULT_CURRENCY "USD"
#ifndef FW_VERSION
#define FW_VERSION "dev"
#endif
#define DEFAULT_USER_AGENT "BDLMinerV2/" FW_VERSION
#define DEFAULT_NTFY_SERVER "ntfy.sh"
// Firmware updates come from the releases of this GitHub repository.
#ifndef GITHUB_REPO
#define GITHUB_REPO "bdlglua/BDLMinerV2"
#endif
#define HOUR_OFF 255  // nightFrom / nightTo: no night mode

// Default thermal limit of the boards that read the chip's own sensor (2.5.3;
// 65 C before, see raiseChipThermalDefaultOnce() in settings.cpp).
constexpr uint8_t kChipSensorMaxTemp = 75, kOldMaxTempDefault = 65;

struct Settings {
    // Pool
    uint8_t miningCoin = coin::BTC; // absent from older records: retain Bitcoin
    String poolHost = DEFAULT_POOL_HOST;
    uint16_t poolPort = DEFAULT_POOL_PORT;
    String backupHost;  // used when the main pool keeps failing, empty = none
    uint16_t backupPort = DEFAULT_BACKUP_PORT;
    uint8_t rotateHours = 0;  // > 0: alternate main and backup pool every N hours
    String btcAddress;  // Stratum username/address; legacy member/key kept for backups
    String poolPassword = DEFAULT_POOL_PASSWORD;
    double suggestedDifficulty = DEFAULT_DIFFICULTY;
    bool userAgentAuto = false; // compatibility key for backups/OTA rollback; never an automatic mode
    String userAgent = DEFAULT_USER_AGENT;
    uint8_t poolProtocol = 0, backupProtocol = 0; // 0 Stratum V1, 1 Stratum V2 + Noise
    uint8_t updateChannel=0; // 0 stable, including migrated settings; 1 beta
    String poolAuthority, backupAuthority; // SV2 Base58Check authority keys, or 32-byte hex
    // Display and sound
    uint8_t brightness = 80;  // percent
    uint8_t rotation = 1;     // 0..3, display orientation
    bool autoRotate = false;  // follow the accelerometer (M5 devices with an IMU)
    uint8_t theme = 0;        // accent colour, see ui.cpp
    uint8_t language = 0;     // 0 English, 1 French (screen and dashboard)
    uint8_t cycleSeconds = 0; // > 0: next screen every N seconds
    bool customScreen=false;
    uint8_t screenWidget0=1,screenWidget1=2,screenWidget2=4,screenWidget3=3;
    uint8_t screenPages=255;
    uint8_t nightFrom = HOUR_OFF, nightTo = HOUR_OFF;  // screen off and no beep
    bool beep = true;  // beep when a 32-bit share (difficulty >= 1) is found
    String timezone = DEFAULT_TIMEZONE;
    String currency = DEFAULT_CURRENCY;  // fiat for the BTC price (USD, EUR...)
    // Notifications (ntfy.sh or a self-hosted ntfy server)
    String ntfyTopic;  // empty = off
    String ntfyServer = DEFAULT_NTFY_SERVER;
    bool notify32 = false;  // also every 32-bit share
    String webhookUrl;       // POST JSON to it too (Discord, Telegram...), empty = off
    uint8_t alertPercent = 50;  // notify when hashrate stays below this % of normal, 0 = off
    uint8_t alertMinutes = 15;  // ... for that long
    uint8_t summaryHour = 20;   // daily summary notification at this local hour, HOUR_OFF = none
#ifdef BOARD_GAMMA601
    uint16_t asicFrequency = 525; // requested Gamma profile (stock, as ESP-Miner); automatic stock fallback
    uint16_t asicVoltage = 1150;  // Gamma core voltage (mV), one of ESP-Miner's BM1370 values
    bool fanAuto = false;       // preserve full cooling until the owner changes it
    uint8_t fanPercent = 100;    // manual EMC2101 command, 30..100%
#endif
    // Safety
#ifdef BOARD_NERDNOS
    uint8_t maxTemp = 75;    // NTC next to the BM1397; the stock firmware cuts power at 80 C
#elif defined(BOARD_ASIC) || defined(DISPLAY_M5UNIFIED)
    uint8_t maxTemp = 65;    // 0 disables thermal limiting; M5 devices may read their IMU, next to the battery
#else
    // The chip's own sensor (T-Display, T-Display-S3, CYD, boards without a
    // screen) reads the die, which runs warmer than the board: at 65 C, an
    // ESP32-S3 at full speed sat on the 50 % step from 55 C (176 instead of
    // 351 kH/s on a T-Display-S3). Reduction now starts at 65 C.
    uint8_t maxTemp = kChipSensorMaxTemp;
#endif
    uint8_t miningPower = 100; // full available speed; saved user limits are preserved
    uint8_t rebootHours = 0; // > 0: restart after N hours without an accepted share
    bool batterySaver = false;  // on battery: dim the screen, mine at half speed
    // Network
    String wifi2Ssid, wifi2Pass, wifi3Ssid, wifi3Pass;  // fallbacks to the portal's network
    String mqttHost;  // empty = no MQTT / Home Assistant
    uint16_t mqttPort = 1883;
    String mqttUser, mqttPass;
    String peers;  // other miners for the dashboard: "192.168.1.41, miner2.local"
    String webPassword;  // empty = settings and firmware can be changed from the web without a password
    uint8_t githubUpdate =
#ifdef BOARD_ASIC
        1; // experimental hardware: offer updates for review
#else
        2;
#endif  // 0 off, 1 check and offer, 2 install by itself
    bool usageCount = true;    // daily download of this version's ping-<board>.txt (ghupdate.h)
    bool shareHashrate = true;  // anonymous hashrate report every 5 minutes (share.h)
    bool sharePool = true;      // public pool group only; saved opt-outs are preserved
    bool remoteEnabled = false; // personal monitoring is separate and opt-in
    String remoteKey, remoteName;
};

inline void settingsNormalizeAgent(Settings &values) {
    values.userAgent = user_agent::normalize(values.userAgent.c_str(), DEFAULT_USER_AGENT);
    values.userAgentAuto = false;
}

extern Settings settings;

void settingsLoad();
bool settingsSave(const Settings &values = settings, bool forRestart = false);
uint8_t settingsBrightness();
void settingsSetBrightness(uint8_t percent);
// Trims texts and brings out-of-range values back to defaults.
void settingsSanitize(Settings &values = settings);

// Every setting the web page can show and change (also the export format).
enum class SettingType { Text, Secret, Number, Decimal, Bool };
struct SettingInfo {
    const char *key;    // NVS key and JSON name
    SettingType type;
    void *value;        // String, uint8_t / uint16_t (by `bits`), double or bool
    uint8_t bits;       // 8 or 16 for Number
    const char *group;
    const char *label;
};
extern const SettingInfo kSettings[];
extern const size_t kSettingCount;
// Locate a table entry in a staged copy instead of mutating live strings.
inline void *settingValue(const SettingInfo &info, Settings &values) {
    size_t offset = (uintptr_t)info.value - (uintptr_t)&settings;
    return (char *)&values + offset;
}
inline const void *settingValue(const SettingInfo &info, const Settings &values) {
    size_t offset = (uintptr_t)info.value - (uintptr_t)&settings;
    return (const char *)&values + offset;
}

// Name of the access point opened by the configuration portal.
String portalApName();
// Its password. PORTAL_AP_PASSWORD, which the guides print, until the miner
// has a payout address: nothing is there to protect yet. Afterwards a password
// of this miner alone (shown on its screen and in the dashboard's settings),
// so that nobody nearby can reconfigure it while its network is down.
String portalPassword();
// "bdlminer-xxxx": mDNS name (bdlminer-xxxx.local) and DHCP host name.
String deviceHostName();

// Connects to WiFi using stored credentials, then the fallback networks.
// Opens the configuration portal when nothing is stored, when `forcePortal`
// is set, or when no network can be joined. `onPortal` is called right before
// the portal starts (to update the screen). Restarts the board on timeout.
void wifiSetup(bool forcePortal, void (*onPortal)());

// Tries the fallback networks (settings.wifi2/wifi3) once; true if connected.
bool wifiTryFallbacks();

// Erase WiFi credentials and settings, then reboot into the portal.
void settingsFactoryReset();
