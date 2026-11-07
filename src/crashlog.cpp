#include "crashlog.h"

#include <esp_core_dump.h>
#include <esp_ota_ops.h>
#include <esp_idf_version.h>
#if ESP_IDF_VERSION_MAJOR >= 5
#include <esp_app_desc.h>
#endif
#include <esp_flash.h>
#include <Preferences.h>

#include <ctime>

#include "log.h"

// ESP-IDF stops the chip after abort(), a failed assert or a task watchdog
// through panic_abort(), which writes to address 0 on purpose: its "cause 29"
// is not the problem.
extern "C" void panic_abort(const char *details);

namespace {

bool present = false;
String summary;
size_t imageAddress = 0, imageSize = 0;

// The last crashes, newest first: "time|version|summary" lines kept in NVS so
// that a report replaced by a later crash, or erased, still leaves a trace.
// Time is 0 until the clock is set after the boot that found the report.
constexpr int kHistory = 5;
String history;  // guarded by historyLock: the web task reads it, the loop dates it
SemaphoreHandle_t historyLock = xSemaphoreCreateMutex();
bool timePending = false;

struct Locked {
    Locked() { xSemaphoreTake(historyLock, portMAX_DELAY); }
    ~Locked() { xSemaphoreGive(historyLock); }
};

uint32_t fnv(const void *data, size_t size, uint32_t h = 2166136261u) {
    for (size_t i = 0; i < size; i++) h = (h ^ ((const uint8_t *)data)[i]) * 16777619u;
    return h;
}

void saveHistory() {
    Preferences p;
    if (!p.begin("crash", false)) return;
    if (history.isEmpty()) p.remove("hist");
    else p.putString("hist", history);
    p.end();
}

// Adds the report found at boot unless it was already recorded. The crash
// happened in the previous run, so its version is the one that run had.
void record(uint32_t fingerprint, const String &core) {
    Preferences p;
    if (!p.begin("crash", false)) return;
    String previous = p.getString("ver", "");
    p.putString("ver", FW_VERSION);
    history = p.getString("hist", "");
    bool fresh = fingerprint && p.getUInt("fp", 0) != fingerprint;
    if (fresh) p.putUInt("fp", fingerprint);
    p.end();
    if (!fresh) return;
    String line = "0|" + (previous.isEmpty() ? String("?") : previous) + "|" + core;
    line.replace("\n", " ");
    history = history.isEmpty() ? line : line + "\n" + history;
    int count = 0, cut = -1;
    for (int i = 0; i < (int)history.length(); i++)
        if (history[i] == '\n' && ++count == kHistory) cut = i;
    if (cut >= 0) history.remove(cut);
    saveHistory();
    timePending = true;
}

#if CONFIG_IDF_TARGET_ARCH_XTENSA
// "LoadProhibited (cause 28) at 0x00000010": the usual Xtensa exception causes.
String exceptionName(uint32_t cause, uint32_t address) {
    static const struct { uint8_t cause; const char *name; } kNames[] = {
        {0, "IllegalInstruction"},    {2, "InstructionFetchError"}, {3, "LoadStoreError"},
        {6, "IntegerDivideByZero"},   {9, "LoadStoreAlignment"},    {20, "InstFetchProhibited"},
        {28, "LoadProhibited"},       {29, "StoreProhibited"},
    };
    char out[64];
    const char *name = "exception";
    for (const auto &k : kNames)
        if (k.cause == cause) name = k.name;
    if (cause == 3 || cause == 9 || cause >= 20)
        snprintf(out, sizeof(out), "%s (cause %u) at 0x%08x", name, (unsigned)cause, (unsigned)address);
    else
        snprintf(out, sizeof(out), "%s (cause %u)", name, (unsigned)cause);
    return out;
}
#endif

}  // namespace

void crashBegin() {
    if (esp_core_dump_image_check() != ESP_OK || esp_core_dump_image_get(&imageAddress, &imageSize) != ESP_OK) {
        record(0, "");  // remembers this version for the next report
        return;
    }
    present = true;
    esp_core_dump_summary_t s;
    memset(&s, 0, sizeof(s));  // hashed whole below, padding included
    uint32_t fingerprint = fnv(&imageSize, sizeof(imageSize));
    if (esp_core_dump_get_summary(&s) != ESP_OK) {
        summary = "crash report present (no summary)";
        record(fingerprint, summary);
    } else {
        fingerprint = fnv(&s, sizeof(s), fingerprint);
        char line[220];
        // The dump survives updates: addresses only mean something to its own build.
        char running[65];
#if ESP_IDF_VERSION_MAJOR >= 5
        bool earlier = esp_app_get_elf_sha256(running, sizeof(running)) > 0 &&
#else
        bool earlier = esp_ota_get_app_elf_sha256(running, sizeof(running)) > 0 &&
#endif
                       strncmp(running, (const char *)s.app_elf_sha256, 16) != 0;
        uintptr_t abortAt = (uintptr_t)&panic_abort;
        bool aborted = !earlier && s.exc_pc >= abortAt && s.exc_pc < abortAt + 0x40;
#if CONFIG_IDF_TARGET_ARCH_XTENSA
        // Another build: panic_abort's own write to address 0 is the best sign left.
        bool likelyAbort = earlier && s.ex_info.exc_cause == 29 && s.ex_info.exc_vaddr == 0;
#else
        bool likelyAbort = false;
#endif
        int n = aborted       ? snprintf(line, sizeof(line), "abort() called in task %.16s", s.exc_task)
                : likelyAbort ? snprintf(line, sizeof(line), "abort() or a null pointer write in task %.16s", s.exc_task)
                              : snprintf(line, sizeof(line), "task %.16s, PC 0x%08x", s.exc_task, (unsigned)s.exc_pc);
#if CONFIG_IDF_TARGET_ARCH_XTENSA
        if (!aborted && !likelyAbort) n += snprintf(line + n, sizeof(line) - n, ", %s", exceptionName(s.ex_info.exc_cause, s.ex_info.exc_vaddr).c_str());
        if (s.exc_bt_info.depth) {
            n += snprintf(line + n, sizeof(line) - n, ", backtrace");
            for (uint32_t i = 0; i < s.exc_bt_info.depth && i < 8 && n < (int)sizeof(line) - 12; ++i)
                n += snprintf(line + n, sizeof(line) - n, " 0x%08x", (unsigned)s.exc_bt_info.bt[i]);
            if (s.exc_bt_info.corrupted && n < (int)sizeof(line) - 12) snprintf(line + n, sizeof(line) - n, " (corrupted)");
        }
#endif
        summary = line;
        record(fingerprint, summary);
        if (earlier) summary += " (earlier firmware)";
    }
    logf("[crash] last crash: %s; report %u bytes at /api/coredump\n", summary.c_str(), (unsigned)imageSize);
}

bool crashPresent() { return present; }

const String &crashSummary() { return summary; }

size_t crashSize() { return present ? imageSize : 0; }

bool crashRead(size_t offset, uint8_t *out, size_t length) {
    if (!present || offset > imageSize || length > imageSize - offset) return false;
    // esp_flash_read: spi_flash_read is gone from ESP-IDF 5, this exists in 4.4 too.
    return esp_flash_read(esp_flash_default_chip, out, imageAddress + offset, length) == ESP_OK;
}

bool crashErase() {
    if (present && esp_core_dump_image_erase() != ESP_OK) return false;
    Locked lock;
    present = false;
    summary = "";
    imageAddress = imageSize = 0;
    history = "";
    timePending = false;
    saveHistory();
    logf("[crash] report and history erased\n");
    return true;
}

void crashPoll() {
    if (!timePending) return;
    time_t now = time(nullptr);
    if (now < 1700000000) return;  // clock not set yet
    timePending = false;
    Locked lock;
    // Found at boot: the crash happened just before this run started.
    String stamp = String((uint32_t)(now - millis() / 1000));
    if (history.startsWith("0|")) {
        history = stamp + history.substring(1);
        saveHistory();
    }
}

void crashHistory(JsonArray out) {
    Locked lock;
    int start = 0;
    while (start < (int)history.length()) {
        int end = history.indexOf('\n', start);
        if (end < 0) end = history.length();
        String line = history.substring(start, end);
        int a = line.indexOf('|'), b = line.indexOf('|', a + 1);
        if (a > 0 && b > a) {
            JsonObject e = out.add<JsonObject>();
            e["t"] = (uint32_t)line.substring(0, a).toInt();
            e["version"] = line.substring(a + 1, b);
            e["summary"] = line.substring(b + 1);
        }
        start = end + 1;
    }
}

bool crashHasHistory() {
    Locked lock;
    return !history.isEmpty();
}
