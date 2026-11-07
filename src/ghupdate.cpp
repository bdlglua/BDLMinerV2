#include "ghupdate.h"

#include <ArduinoJson.h>
#include <HTTPClient.h>
#include <Update.h>
#include <WiFi.h>
#include <esp_ota_ops.h>
#include <mbedtls/pk.h>
#include <mbedtls/sha256.h>
#include <mbedtls/version.h>

// mbedTLS 3 (ESP-IDF 5) renamed the *_ret SHA-256 calls of mbedTLS 2.
#if MBEDTLS_VERSION_MAJOR >= 3
#define sha256Starts mbedtls_sha256_starts
#define sha256Update mbedtls_sha256_update
#define sha256Finish mbedtls_sha256_finish
#else
#define sha256Starts mbedtls_sha256_starts_ret
#define sha256Update mbedtls_sha256_update_ret
#define sha256Finish mbedtls_sha256_finish_ret
#endif

#include "hw_sha.h"
#include "log.h"
#include "miner.h"
#include "notify.h"
#include "ota.h"
#include "settings.h"
#include "tls.h"
#include "trial.h"
#include "update_key.h"
#include "core/release_version.h"

#ifndef FW_ENV
#define FW_ENV "dev"
#endif

namespace {

constexpr uint32_t kFirstCheckMs = 30 * 1000;
constexpr uint32_t kCheckMs = 3600 * 1000UL;
constexpr uint32_t kRetryMs = 5 * 60 * 1000;
constexpr uint32_t kFirstPingMs = 5 * 60 * 1000;
constexpr uint32_t kPingMs = 24 * 3600 * 1000UL;
constexpr uint32_t kMinHeap = kTlsMinFree;

GithubUpdate state;
String firmwareUrl, signatureUrl;
portMUX_TYPE stateMux = portMUX_INITIALIZER_UNLOCKED;
std::atomic<bool> installRequested{false};
std::atomic<bool> checkRequested{false};
std::atomic<uint32_t> installRetryAt{0};
bool retryInstall = false;
uint8_t installFailures = 0;
uint32_t nextCheck = kFirstCheckMs;  // millis()
// A release whose installation was refused (bad signature, no room for a
// second firmware) gives the same answer every time: it is not installed
// automatically again, only when asked for from the dashboard.
char refusedVersion[24] = "";
char refusedError[80] = "";
uint32_t nextPing = kFirstPingMs;

void setError(const char *fmt, const char *arg = "") {
    portENTER_CRITICAL(&stateMux);
    snprintf(state.error, sizeof(state.error), fmt, arg);
    portEXIT_CRITICAL(&stateMux);
    logf("[update] %s\n", state.error);
}

// "1.2.10" > "1.2.9", "1.0.1" > "1.0"; stable > beta > alpha.
int compareVersions(const char *a, const char *b) {
    return release_version::compare(a,b);
}

// The first "1.2" or "1.2.3" in a release tag or name ("v1.0.1", "V1.0.0").
bool findVersion(const char *s, char *out, size_t size) {
    return release_version::find(s,out,size);
}

// GET `url` over HTTPS, following redirects (GitHub sends release files to
// another host) with one tlsConnect() per host. On success `http` holds the
// response, with the body to read.
//
// GitHub's release file host takes a link that is only valid for a few
// minutes (a JWT, refused with the non-standard status 618: expired, not yet
// valid, altered). A refused link is not retried as is: the whole chain is
// asked again, for a fresh one, a little later.
constexpr int kStatusLinkRefused = 618;  // release-assets.githubusercontent.com
constexpr int kFreshLinkAttempts = 3;
bool httpsGet(String url, WiFiClientSecure &tls, HTTPClient &http, int *code) {
    static const char *kHeaders[] = {"Location"};
    const String first = url;
    for (int attempt = 1; attempt <= kFreshLinkAttempts; attempt++) {
        url = first;
        bool redirected = false, fresh = false;
        for (int hop = 0; hop < 5 && !fresh; hop++) {
            if (!url.startsWith("https://")) return false;
            int slash = url.indexOf('/', 8);
            String host = url.substring(8, slash < 0 ? url.length() : slash);
            http.end();
            tls.stop();
            // Reuse: HTTPClient (Arduino-ESP32 3.x) closes a connection it may not
            // reuse and opens its own, unguarded handshake in GET(); the one
            // tlsConnectSignedContent() made below must be the one used.
            http.setReuse(true);
            http.setTimeout(15000);
            http.setFollowRedirects(HTTPC_DISABLE_FOLLOW_REDIRECTS);
            if (!http.begin(tls, url)) return false;
            // begin() may close the client when its host changes, including on
            // the first request. Connect afterwards so GET() reuses the guarded
            // handshake instead of starting another one with the miner running.
            // Releases are trusted by their signature: an unverifiable certificate
            // must not stop updates (tlsConnectSignedContent()).
            if (!tlsConnectSignedContent(tls, host.c_str())) return false;
            // Retain the legacy binary version marker required by existing offline OTA validators.
            http.setUserAgent("BDLMinerV1/" FW_VERSION);
            http.addHeader("Accept", "application/vnd.github+json");
            http.collectHeaders(kHeaders, 1);
            *code = http.GET();
            if (*code >= 300 && *code < 400 && http.hasHeader("Location")) {
                url = http.header("Location");
                redirected = true;
                continue;
            }
            if (*code == HTTP_CODE_OK) return true;
            // Only a link GitHub handed out (after a redirect) can be refreshed.
            bool refused = redirected && (*code == kStatusLinkRefused || *code == 403 || *code == 404 || *code == 410);
            if (!refused || attempt == kFreshLinkAttempts) return false;
            logf("[update] %s refused the download link (HTTP %d%s); asking GitHub for a fresh one\n", host.c_str(), *code,
                 *code == kStatusLinkRefused ? ": expired or not yet valid" : "");
            http.end();
            tls.stop();
            delay(2000);
            fresh = true;
        }
        if (!fresh) return false;  // too many redirects
    }
    return false;
}

// Prefer the small release manifest: it is served as a release asset and does
// not consume GitHub's unauthenticated API quota shared by miners on one IP.
// The API fallback keeps older releases, which have no manifest, discoverable.
bool fetchRelease(const char *url, JsonDocument &doc, String &error) {
    WiFiClientSecure tls;
    HTTPClient http;
    int code = 0;
    if (!httpsGet(url, tls, http, &code)) {
        error = "HTTP " + String(code);
        return false;
    }
    JsonDocument filter;
    filter["tag_name"] = true;
    filter["name"] = true;
    filter["assets"][0]["name"] = true;
    filter["assets"][0]["browser_download_url"] = true;
    TlsBodyReader body(tls);
    DeserializationError err =
        http.getSize() > 0 ? deserializeJson(doc, body, DeserializationOption::Filter(filter))
                           : deserializeJson(doc, http.getString(), DeserializationOption::Filter(filter));
    http.end();
    if (err) {
        error = err.c_str();
        return false;
    }
    return true;
}

// Unreadable or incomplete newer releases are retried sooner.
bool check() {
    JsonDocument doc;
    const uint8_t channel=settings.updateChannel;
    String manifestError, apiError;
    if(channel){
        const char *repo=GITHUB_REPO,*slash=strchr(repo,'/');char url[240];
        if(!slash){setError("invalid update repository");return false;}
        snprintf(url,sizeof(url),"https://%.*s.github.io/%s/updates/beta.json",int(slash-repo),repo,slash+1);
        if(!fetchRelease(url,doc,manifestError)){setError("beta channel unavailable: %s",manifestError.c_str());return false;}
    }else if (!fetchRelease("https://github.com/" GITHUB_REPO "/releases/latest/download/update.json", doc, manifestError)) {
        doc.clear();
        if (!fetchRelease("https://api.github.com/repos/" GITHUB_REPO "/releases/latest", doc, apiError)) {
            setError("release unavailable: %s", ("manifest " + manifestError + "; API " + apiError).c_str());
            return false;
        }
    }
    char version[24];
    if (!findVersion(doc["tag_name"] | "", version, sizeof(version)) &&
        !findVersion(doc["name"] | "", version, sizeof(version))) {
        setError("no version number in the release tag or name");
        return false;
    }
    String fw, sig;
    release_version::Version parsed;
    if(!release_version::parse(version,parsed)){setError("invalid release version %s",version);return false;}
    if(!channel&&parsed.pre[0]){setError("beta release requires the beta channel");return false;}
    if(channel!=settings.updateChannel)return false; // a live channel change invalidates the response
    for (JsonObject a : doc["assets"].as<JsonArray>()) {
        String name = a["name"] | "";
        if (name == FW_ENV "-firmware.bin") fw = a["browser_download_url"] | "";
        if (name == FW_ENV "-firmware.bin.sig") sig = a["browser_download_url"] | "";
    }
    bool newer = compareVersions(version, FW_VERSION) > 0;
    bool available = newer && fw.length() && sig.length();
    bool wasAvailable;
    portENTER_CRITICAL(&stateMux);
    wasAvailable = state.available && !strcmp(state.version, version);
    state.checked = true;
    state.available = available;
    strlcpy(state.version, version, sizeof(state.version));
    state.error[0] = 0;
    portEXIT_CRITICAL(&stateMux);
    firmwareUrl = fw;
    signatureUrl = sig;
    if (newer && !available) setError("release %s has no signed firmware for " FW_ENV, version);
    else if (available && !strcmp(version, refusedVersion)) setError("%s", refusedError);
    logf("[update] latest release %s, this firmware %s%s\n", version, FW_VERSION,
         available ? ": update available" : "");
    if (available && !wasAvailable) {
        char msg[96];
        snprintf(msg, sizeof(msg), "Version %s is available (this miner runs %s).", version, FW_VERSION);
        notify("Firmware update", msg, 3, "arrow_up");
    }
    return !(newer && !available); // release assets can still be uploading; check again sooner
}

// 0 when `sig` is a valid signature of `hash` (SHA-256) by the release key.
int verifySignature(const uint8_t hash[32], const uint8_t *sig, size_t sigLen) {
    mbedtls_pk_context pk;
    mbedtls_pk_init(&pk);
    int r = mbedtls_pk_parse_public_key(&pk, (const unsigned char *)kUpdatePublicKey, sizeof(kUpdatePublicKey));
    if (!r) r = mbedtls_pk_verify(&pk, MBEDTLS_MD_SHA256, hash, 32, sig, sigLen);
    mbedtls_pk_free(&pk);
    return r;
}

GithubInstallResult install() {
    ota::Guard update(ota::Owner::Github);
    if (!update) {
        setError("another firmware update is already running");
        return GithubInstallResult::Retry;
    }
    if (!esp_ota_get_next_update_partition(nullptr)) {
        setError("this board has no room for a second firmware: update by USB");
        return GithubInstallResult::Refused;
    }
    portENTER_CRITICAL(&stateMux);
    state.installing = true;
    state.progress = 0;
    state.error[0] = 0;
    portEXIT_CRITICAL(&stateMux);
    logf("[update] installing %s\n", state.version);
    // OTA ownership pauses miners. Only tlsConnect() takes the TLS/SHA
    // handshake locks, always in the same order; no outer SHA reservation.

    WiFiClientSecure tls;
    HTTPClient http;
    int code = 0;
    uint8_t sig[MBEDTLS_PK_SIGNATURE_MAX_SIZE];
    size_t sigLen = 0;
    bool ok = false;
    GithubInstallResult result = GithubInstallResult::Retry;
    mbedtls_sha256_context sha;
    mbedtls_sha256_init(&sha);
    do {
        if (!httpsGet(signatureUrl, tls, http, &code)) {
            setError("cannot download the signature (HTTP %s)",
                     (String(code) + (code == kStatusLinkRefused ? ": GitHub refused its download link" : "")).c_str());
            break;
        }
        int signatureSize = http.getSize();
        if (signatureSize <= 0 || signatureSize > int(sizeof(sig))) {
            setError("invalid signature response length");
            break;
        }
        sigLen = TlsBodyReader(tls).readBytes(sig, signatureSize);
        http.end();
        if (sigLen != size_t(signatureSize)) {
            setError("signature download interrupted");
            break;
        }
        if (!httpsGet(firmwareUrl, tls, http, &code)) {
            setError("cannot download the firmware (HTTP %s)",
                     (String(code) + (code == kStatusLinkRefused ? ": GitHub refused its download link" : "")).c_str());
            break;
        }
        int size = http.getSize();
        if (size <= 0 || !Update.begin(size)) {
            setError("cannot start the update: %s", Update.errorString());
            break;
        }
        sha256Starts(&sha, 0);
        WiFiClient *stream = http.getStreamPtr();
        if (!stream) {
            Update.abort();
            setError("firmware stream disconnected");
            break;
        }
        static uint8_t buf[2048];
        int done = 0;
        uint32_t lastData = millis();
        while (done < size && millis() - lastData < 20000) {
            int n = stream->available() ? stream->readBytes(buf, min((int)sizeof(buf), size - done)) : 0;
            if (n <= 0) {
                vTaskDelay(pdMS_TO_TICKS(5));
                continue;
            }
            lastData = millis();
            sha256Update(&sha, buf, n);
            if (Update.write(buf, n) != (size_t)n) break;
            done += n;
            portENTER_CRITICAL(&stateMux);
            state.progress = (uint64_t)done * 100 / size;
            portEXIT_CRITICAL(&stateMux);
            vTaskDelay(1);  // the screen and web server keep running
        }
        http.end();
        if (done != size) {
            Update.abort();
            setError("download interrupted at %s%%", String(state.progress).c_str());
            break;
        }
        uint8_t hash[32];
        sha256Finish(&sha, hash);
        if (verifySignature(hash, sig, sigLen)) {
            Update.abort();
            setError("bad signature: update refused");
            result = GithubInstallResult::Refused;
            break;
        }
        if (!Update.end(true)) {
            setError("cannot finish the update: %s", Update.errorString());
            break;
        }
        ok = true;
    } while (false);
    mbedtls_sha256_free(&sha);

    // After a success `installing` stays set: the screen keeps showing the
    // update window until the restart.
    portENTER_CRITICAL(&stateMux);
    if (ok) state.progress = 100;
    else state.installing = false;
    portEXIT_CRITICAL(&stateMux);
    if (ok) {
        trialConfirm("a newer firmware is installed");  // the running one worked
        trialExpect(state.version);
        char why[48];
        snprintf(why, sizeof(why), "updated to %s from GitHub", state.version);
        logf("[update] %s, restarting\n", why);
        minerSaveStats();
        setRestartReason(why);
        delay(500);
        ESP.restart();
    }
    return ok ? GithubInstallResult::Done : result;
}

// The anonymous usage count: GitHub counts the downloads of each release
// file, so fetching ping-<board>.txt of this version once a day makes the
// download counts read "miner-days" per version and board
// (tools/miner_stats.py). Nothing about the miner is sent; firmware built
// locally, whose version has no release, just gets a 404.
void ping() {
    WiFiClientSecure tls;
    HTTPClient http;
    int code = 0;
    bool ok = httpsGet("https://github.com/" GITHUB_REPO "/releases/download/v" FW_VERSION "/ping-" FW_ENV ".txt",
                       tls, http, &code);
    http.end();
    logf("[update] usage count: %s\n", ok ? "counted" : String("HTTP " + String(code)).c_str());
}

}  // namespace

void ghUpdatePoll() {
    if (ota::busy()) return;
    static uint8_t activeChannel=0;
    if(activeChannel!=settings.updateChannel){
        activeChannel=settings.updateChannel;installRequested=false;retryInstall=false;installFailures=0;
        firmwareUrl="";signatureUrl="";nextCheck=millis();installRetryAt=0;
        portENTER_CRITICAL(&stateMux);
        state.checked=false;state.available=false;state.version[0]=0;state.error[0]=0;
        portEXIT_CRITICAL(&stateMux);
    }
    bool due = settings.githubUpdate && (installRequested || checkRequested || (int32_t)(millis() - nextCheck) >= 0);
    if (due) {
        if (!WiFi.isConnected() || ESP.getFreeHeap() < kMinHeap) {
            static uint8_t lastWait = 0;
            uint8_t wait = WiFi.isConnected() ? 1 : 2;
            if (wait != lastWait || !ghUpdateState().error[0]) {
                setError("update waiting: %s", wait == 1 ? "low memory" : "no WiFi");
                lastWait = wait;
            }
            return;
        }
        bool requestedInstall = installRequested.exchange(false);
        bool requestedCheck = checkRequested.exchange(false);
        bool ready = true;
        if (requestedCheck || !requestedInstall) {
            portENTER_CRITICAL(&stateMux);
            state.checking = true;
            portEXIT_CRITICAL(&stateMux);
            ready = check();
            nextCheck = millis() + (ready ? kCheckMs : kRetryMs);
            portENTER_CRITICAL(&stateMux);
            state.checking = false;
            portEXIT_CRITICAL(&stateMux);
        }
        GithubUpdate current = ghUpdateState();
        bool automatic = settings.githubUpdate == 2 && strcmp(current.version, refusedVersion);
        if (ready && (requestedInstall || retryInstall || automatic) && current.available) {
            GithubInstallResult result = install();
            if (result == GithubInstallResult::Refused) {
                GithubUpdate failed = ghUpdateState();
                strlcpy(refusedVersion, failed.version, sizeof(refusedVersion));
                strlcpy(refusedError, failed.error, sizeof(refusedError));
            }
            retryInstall = result == GithubInstallResult::Retry;
            if (retryInstall) {
                if (installFailures < 4) ++installFailures;
                uint32_t delay = min<uint32_t>(kRetryMs << (installFailures - 1), 30 * 60 * 1000UL);
                nextCheck = millis() + delay;
                installRetryAt = nextCheck ? nextCheck : 1;
                logf("[update] will retry the installation in %u seconds\n", (unsigned)(delay / 1000));
            } else {
                installFailures = 0;
                installRetryAt = 0;
            }
        } else if (ready && !ghUpdateState().available) {
            retryInstall = false;
            installFailures = 0;
            installRetryAt = 0;
        } else if (!ready && retryInstall) {
            installRetryAt = nextCheck ? nextCheck : 1;
        }
        // Metadata checks and requested installations take priority over the
        // optional usage ping. Keep this poll to one operation on the network.
        return;
    }
    if (settings.usageCount && (int32_t)(millis() - nextPing) >= 0 && WiFi.isConnected() &&
        ESP.getFreeHeap() >= kMinHeap) {
        nextPing = millis() + kPingMs;
        ping();
    }
}

void ghUpdateRefuse(const char *version, const char *why) {
    strlcpy(refusedVersion, version, sizeof(refusedVersion));
    strlcpy(refusedError, why, sizeof(refusedError));
}

GithubUpdate ghUpdateState() {
    portENTER_CRITICAL(&stateMux);
    GithubUpdate s = state;
    portEXIT_CRITICAL(&stateMux);
    s.checking = s.checking || checkRequested.load();
    s.installing = s.installing || installRequested.load();
    uint32_t retryAt = installRetryAt.load();
    int32_t remaining = retryAt - millis();
    s.retrySeconds = retryAt && remaining > 0 ? (uint32_t(remaining) + 999) / 1000 : 0;
    return s;
}

bool ghUpdateCheck() {
    GithubUpdate current = ghUpdateState();
    if (!settings.githubUpdate || current.checking || current.installing || ota::busy()) return false;
    return !checkRequested.exchange(true);
}

bool ghUpdateInstall() {
    GithubUpdate current = ghUpdateState();
    if (!settings.githubUpdate || !current.available || current.checking || current.installing || ota::busy()) return false;
    if (installRequested.exchange(true)) return false;
    portENTER_CRITICAL(&stateMux);
    ++state.installRequest;
    portEXIT_CRITICAL(&stateMux);
    return true;
}
