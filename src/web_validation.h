// Validation shared with host tests. Check a backup completely before any
// settings or lifetime statistics are written to flash.
#pragma once

#include <ArduinoJson.h>
#include <ctype.h>
#include <math.h>
#include <stdint.h>
#include <string.h>

namespace webvalidation {

inline bool nonnegativeNumber(JsonVariantConst v) {
    return v.is<double>() && isfinite(v.as<double>()) && v.as<double>() >= 0;
}

inline const char *backupError(JsonObjectConst backup, size_t maxTopShares) {
    // Settings-only exports from earlier firmware have no format marker.
    if (backup["bdlminer_backup"].isUnbound()) return nullptr;
    if (!backup["bdlminer_backup"].is<unsigned>() || backup["bdlminer_backup"].as<unsigned>() != 1)
        return "Unsupported backup version";
    if (!backup["settings"].is<JsonObjectConst>()) return "Invalid backup settings";
    if (!backup["stats"].is<JsonObjectConst>()) return "Missing backup statistics";
    JsonObjectConst st = backup["stats"];
    if (!nonnegativeNumber(st["best"]) || !st["hashes"].is<uint64_t>() ||
        !st["shares"].is<uint32_t>() || !st["blocks"].is<uint32_t>() || !st["seconds"].is<uint32_t>() ||
        !st["top"].is<JsonArrayConst>())
        return "Invalid or incomplete backup statistics";
    JsonArrayConst top = st["top"];
    if (top.size() > maxTopShares) return "Too many best shares in backup";
    // JSON serialization can round nearby difficulties differently. Do not
    // reject an exported backup because this slightly changes their ordering.
    for (JsonVariantConst entry : top) {
        if (!entry.is<JsonObjectConst>() || !nonnegativeNumber(entry["d"]) ||
            entry["d"].as<double>() <= 0 || !entry["t"].is<uint32_t>())
            return "Invalid best share in backup";
    }
    return nullptr;
}

inline bool sameText(const char *a, const char *b, size_t n) {
    for (size_t i = 0; i < n; i++)
        if (tolower((unsigned char)a[i]) != tolower((unsigned char)b[i])) return false;
    return true;
}

inline bool localSuffix(const char *host, size_t n) {
    static const char *kLocal[] = {".local", ".lan", ".home", ".home.arpa", ".internal", ".localdomain"};
    for (const char *suffix : kLocal) {
        size_t s = strlen(suffix);
        if (n > s && sameText(host + n - s, suffix, s)) return true;
    }
    return false;
}

// The Host header of a request that changes the miner or reads its secrets.
// A name another site controls could lead to the miner (DNS rebinding): only
// an IP address, a name without a dot or a local name (.local, .lan...) are
// accepted as they are (Local). The miner's own name under any other domain
// (bdlminer-xxxx.fritz.box, as routers name it) is OwnNamed: accepted too,
// but its four hexadecimal digits can be guessed, see RebindingGuard.
enum class HostKind : uint8_t { Refused, Local, OwnNamed };
inline HostKind hostKind(const char *host, const char *ownName) {
    if (!host || !*host || *host == '[') return HostKind::Local;  // no header (not a browser) or an IPv6 address
    size_t n = strcspn(host, ":");
    if (n && host[n - 1] == '.') n--;
    if (strspn(host, "0123456789.") >= n) return HostKind::Local;  // IPv4
    const char *dot = (const char *)memchr(host, '.', n);
    if (!dot || localSuffix(host, n)) return HostKind::Local;
    size_t own = strlen(ownName);
    if (own && (size_t)(dot - host) == own && sameText(host, ownName, own)) return HostKind::OwnNamed;
    return HostKind::Refused;
}
inline bool hostAllowed(const char *host, const char *ownName) {
    return hostKind(host, ownName) != HostKind::Refused;
}

// A page of another site that leads its own name to the miner has to guess
// the miner's name (65536 of them) for that name to be accepted. Each wrong
// name is a request with a refused Host: after a few of them, the miner's
// name under a foreign domain is refused as well for an hour, counted from
// the last refused request. IP addresses and local names keep working.
struct RebindingGuard {
    static constexpr uint8_t kMisses = 3;
    static constexpr uint32_t kLockMs = 60 * 60 * 1000UL;
    uint8_t misses = 0;
    uint32_t lastMiss = 0;
    bool allowed(const char *host, const char *ownName, uint32_t now) {
        HostKind kind = hostKind(host, ownName);
        if (kind == HostKind::Local) return true;
        if (misses && now - lastMiss >= kLockMs) misses = 0;
        if (kind == HostKind::Refused) {
            if (misses < kMisses) misses++;
            lastMiss = now;
            return false;
        }
        return misses < kMisses;
    }
};

// The Origin header, when the browser sends one (every cross-site POST has
// it), must be this very miner: another site's page cannot use it.
inline bool originAllowed(const char *origin, const char *host) {
    if (!origin || !*origin) return true;
    if (strncmp(origin, "http://", 7)) return false;
    origin += 7;
    size_t n = strlen(origin);
    return host && strlen(host) == n && sameText(origin, host, n);
}

// "10.1.2.3": a private, link-local, loopback or carrier-grade (VPN) address.
inline bool privateIPv4(const char *text, size_t n) {
    unsigned part[4] = {}, count = 0, digits = 0;
    for (size_t i = 0; i < n; i++) {
        char c = text[i];
        if (c == '.') {
            if (!digits || ++count > 3) return false;
            digits = 0;
        } else if (c >= '0' && c <= '9') {
            part[count] = part[count] * 10 + unsigned(c - '0');
            if (++digits > 3 || part[count] > 255) return false;
        } else {
            return false;
        }
    }
    if (count != 3 || !digits) return false;
    return part[0] == 10 || part[0] == 127 || (part[0] == 172 && part[1] >= 16 && part[1] <= 31) ||
           (part[0] == 192 && part[1] == 168) || (part[0] == 169 && part[1] == 254) ||
           (part[0] == 100 && part[1] >= 64 && part[1] <= 127);
}

// The Origin of a page of the local network (another miner's dashboard, a
// home server): http, at a private address, a name without a dot or a local
// name. Any other page reads the statistics without what names their owner.
inline bool originLocal(const char *origin) {
    if (!origin || strncmp(origin, "http://", 7)) return false;
    const char *host = origin + 7;
    if (*host == '[') return false;
    size_t n = strcspn(host, ":/");
    if (n && host[n - 1] == '.') n--;
    if (!n) return false;
    if (strspn(host, "0123456789.") >= n) return privateIPv4(host, n);
    return !memchr(host, '.', n) || localSuffix(host, n);
}

}  // namespace webvalidation
