// The updater's board and version markers, found in a stream of chunks.
#include <algorithm>
#include <cassert>
#include <cstdio>
#include <string>
#include <vector>

#include "core/image_marker.h"

using image_marker::Scanner;

static bool scan(const std::string &image, const char *prefix, const char *text, size_t chunk) {
    Scanner s;
    if (!s.set(prefix, text)) return false;
    for (size_t at = 0; at < image.size(); at += chunk)
        s.feed((const uint8_t *)image.data() + at, std::min(chunk, image.size() - at));
    return s.found();
}

int main() {
    using namespace std::string_literals;
    const std::string image = "\xe9header"s + "\0"s + "esp32s3-headless\0"s + "padding" + "\0"s +
                              "BDLMinerV1/2.6.0\0"s + "BDLMinerV2/2.6.0\0"s + "tail";
    // Whatever the size of the chunks the download arrives in.
    for (size_t chunk = 1; chunk <= image.size(); chunk++) {
        assert(scan(image, "", "esp32s3-headless", chunk));
        assert(scan(image, "BDLMinerV1/", "2.6.0", chunk));
        // Another board, even one whose name ends the same way.
        assert(!scan(image, "", "esp32-headless", chunk));
        assert(!scan(image, "", "esp32s3", chunk));  // the NUL is part of the marker
        // An older or a newer version than the one announced.
        assert(!scan(image, "BDLMinerV1/", "2.5.9", chunk));
        assert(!scan(image, "BDLMinerV1/", "2.6", chunk));
        assert(!scan(image, "BDLMinerV1/", "2.6.0-beta.1", chunk));
    }
    // A marker that begins inside a partial match of itself.
    assert(scan("aaab\0"s, "", "aab", 1) && scan("ababac\0"s, "", "abac", 2));
    assert(scan("BDLMinerV1/BDLMinerV1/2.6.0\0"s, "BDLMinerV1/", "2.6.0", 3));
    // The image's last bytes must hold the whole marker.
    assert(!scan("BDLMinerV1/2.6.0"s, "BDLMinerV1/", "2.6.0", 4));
    // Nothing to look for, or too long to hold: never "found".
    Scanner s;
    assert(!s.set("", "") && !s.set(nullptr, "x") && !s.set("", nullptr));
    assert(!s.set("", std::string(96, 'a').c_str()) && s.set("", std::string(95, 'a').c_str()));
    assert(!scan(image, "", "", 8));
    // set() starts a new search.
    assert(s.set("", "ab"));
    s.feed((const uint8_t *)"ab\0", 3);
    assert(s.found() && s.set("", "cd") && !s.found());
    puts("Firmware image markers: board and version found across chunk boundaries, other boards and versions refused passed");
}
