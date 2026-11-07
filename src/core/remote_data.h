// Opt-in telemetry identifiers. No raw MAC or pool host leaves; the payout address
// is only sent as the Authorization of address-based spaces, never in the body.
#pragma once
#include <cstddef>
#include <cstdint>
#include <cstring>
#include "sha256.h"

namespace remote_data {
inline bool validKey(const char *key) {
    if (!key || std::strlen(key) != 64) return false;
    for (unsigned i = 0; i < 64; ++i)
        if (!((key[i] >= '0' && key[i] <= '9') || (key[i] >= 'a' && key[i] <= 'f'))) return false;
    return true;
}
inline void deviceId(const char *key, uint64_t mac, char out[17]) {
    uint8_t input[72], digest[32];
    std::memcpy(input, key, 64);
    for (unsigned i = 0; i < 8; ++i) input[64+i] = uint8_t(mac >> (i*8));
    sha::sha256(input, sizeof(input), digest);
    static constexpr char hex[] = "0123456789abcdef";
    for (unsigned i = 0; i < 8; ++i) { out[i*2] = hex[digest[i] >> 4]; out[i*2+1] = hex[digest[i] & 15]; }
    out[16] = 0;
    std::memset(input, 0, sizeof(input));
}
// The payout part of a stratum user ("address.worker" -> "address"), as the
// stats service folds it. Returns false when it cannot name a remote space.
inline bool payoutAddress(const char *user, char out[101]) {
    out[0] = 0;
    if (!user) return false;
    while (*user == ' ' || *user == '\t') ++user;
    size_t n = 0;
    for (; user[n] && user[n] != '.' && user[n] != ' ' && user[n] != '\t'; ++n) {
        const char c = user[n];
        const bool ok = (c >= '0' && c <= '9') || (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
                        c == ':' || c == '_' || c == '-';
        if (!ok || n >= 100) { out[0] = 0; return false; }
        out[n] = c;
    }
    out[n] = 0;
    if (n < 4) { out[0] = 0; return false; }
    return true;
}
inline void addressDeviceId(const char *address, uint64_t mac, char out[17]) {
    uint8_t input[8 + 100 + 8], digest[32];
    size_t n = 0;
    for (const char *p = "address:"; *p; ++p) input[n++] = uint8_t(*p);
    for (const char *p = address; *p && n < 108; ++p) input[n++] = uint8_t(*p >= 'A' && *p <= 'Z' ? *p + 32 : *p);
    for (unsigned i = 0; i < 8; ++i) input[n++] = uint8_t(mac >> (i*8));
    sha::sha256(input, n, digest);
    static constexpr char hex[] = "0123456789abcdef";
    for (unsigned i = 0; i < 8; ++i) { out[i*2] = hex[digest[i] >> 4]; out[i*2+1] = hex[digest[i] & 15]; }
    out[16] = 0;
}
// Remote monitoring can run when a key or a usable payout address names its space.
inline bool canReport(const char *key, const char *user) {
    char address[101];
    return validKey(key) || payoutAddress(user, address);
}
} // namespace remote_data
