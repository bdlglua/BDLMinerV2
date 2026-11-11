// Finds a text marker in a firmware image as it is downloaded, chunk by chunk.
//
// A release signature covers the bytes of one <env>-firmware.bin, not the
// answer that announced it: a signed image of another board, or of an older
// version, carries a valid signature too. Every release image holds its
// board ("<env>\0") and its version ("BDLMinerV1/<version>\0"), which
// tools/firmware_image.py requires before a release is signed; the updater
// looks for the same two markers before it accepts a download.
#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>

namespace image_marker {

class Scanner {
public:
    // The marker is `prefix` + `text` + its terminating NUL. False when it is
    // empty or too long: found() then stays false.
    bool set(const char *prefix, const char *text) {
        length_ = matched_ = 0;
        found_ = false;
        if (!prefix || !text || !*text) return false;
        size_t a = strlen(prefix), b = strlen(text);
        if (a + b + 1 > sizeof(pattern_)) return false;
        memcpy(pattern_, prefix, a);
        memcpy(pattern_ + a, text, b + 1);
        length_ = a + b + 1;
        // Knuth-Morris-Pratt: a marker cut by a chunk boundary, or starting
        // inside a partial match, is still found.
        fallback_[0] = 0;
        for (size_t i = 1, k = 0; i < length_; i++) {
            while (k && pattern_[i] != pattern_[k]) k = fallback_[k - 1];
            if (pattern_[i] == pattern_[k]) k++;
            fallback_[i] = uint8_t(k);
        }
        return true;
    }

    void feed(const uint8_t *data, size_t size) {
        if (!length_ || found_) return;
        for (size_t i = 0; i < size; i++) {
            while (matched_ && data[i] != pattern_[matched_]) matched_ = fallback_[matched_ - 1];
            if (data[i] == pattern_[matched_] && ++matched_ == length_) {
                found_ = true;
                return;
            }
        }
    }

    bool found() const { return found_; }

private:
    uint8_t pattern_[96] = {};
    uint8_t fallback_[96] = {};
    size_t length_ = 0, matched_ = 0;
    bool found_ = false;
};

}  // namespace image_marker
