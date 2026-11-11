#!/usr/bin/env python3
"""Execute the production /api/log handler: the web password protects the log."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
source = (root / 'src/web.cpp').read_text()
handlers = (source[source.index('bool ownHost(const String &host) {'):source.index('void restartSoon(')] +
            source[source.index('// The log names networks'):source.index('void buildPoolDiagnostics(')])
prefix = r'''
#include <cassert>
#include <cstdio>
#include <string>
#include "web_validation.h"
struct String : std::string {
    using std::string::string;
    String(const std::string &s) : std::string(s) {}
    bool isEmpty() const { return empty(); }
};
enum HTTPAuthMethod { BASIC_AUTH };
struct Server {
    std::string host = "192.168.1.20", origin, user, password;
    int status = 0; std::string body; bool challenged = false;
    String hostHeader() const { return host; }
    String header(const char *) const { return origin; }
    bool authenticate(const char *u, const char *p) const { return user == u && password == p; }
    void requestAuthentication(HTTPAuthMethod, const char *, const char *) { status = 401; challenged = true; }
    void sendHeader(const char *, const char *) {}
    void send(int code, const char *, const String &text) { status = code; body = text; }
} server;
struct { String webPassword; } settings;
String hostName = "bdlminer";
unsigned long millis() { return 0; }
String logText() { return "00:00:01 [boot] test\n"; }
'''
tests = r'''
static void request(const char *password, const char *origin = "") {
    server.status = 0; server.body.clear(); server.challenged = false;
    server.user = password ? "admin" : ""; server.password = password ? password : ""; server.origin = origin;
    handleLog();
}
int main() {
    // No web password (the default): readable from the miner's own pages.
    request(nullptr); assert(server.status == 200 && server.body.find("[boot]") != std::string::npos);
    // Another site's page cannot read it.
    request(nullptr, "http://evil.example"); assert(server.status == 403 && server.body.find("[boot]") == std::string::npos);
    // With a web password, the log asks for it like the settings.
    settings.webPassword = "secret";
    request(nullptr); assert(server.status == 401 && server.challenged && server.body.empty());
    request("wrong"); assert(server.status == 401 && server.body.empty());
    request("secret"); assert(server.status == 200 && server.body.find("[boot]") != std::string::npos);
    // A name of another site leading here is refused, and after a few of them so is the miner's own
    // name under a foreign domain; its address keeps working.
    settings.webPassword = "";
    server.host = "bdlminer.fritz.box"; request(nullptr); assert(server.status == 200);
    for (const char *guess : {"miner-1.evil.example", "miner-2.evil.example", "miner-3.evil.example"}) {
        server.host = guess; request(nullptr); assert(server.status == 403 && server.body.find("[boot]") == std::string::npos);
    }
    server.host = "bdlminer.fritz.box"; request(nullptr); assert(server.status == 403);
    server.host = "192.168.1.20"; request(nullptr); assert(server.status == 200);
    puts("Log access: open without a web password, password required when one is set, other sites refused passed");
}
'''
with tempfile.TemporaryDirectory(prefix='bdl-log-') as tmp:
    test = Path(tmp) / 'test.cpp'
    binary = Path(tmp) / 'test'
    test.write_text(prefix + handlers + tests)
    json = root / '.pio/libdeps/esp32-headless/ArduinoJson/src'  # installed by run.sh
    subprocess.run(['g++', '-std=c++17', '-O2', '-Wall', '-I' + str(root / 'src'), '-I' + str(json), str(test), '-o', str(binary)],
                   check=True)
    subprocess.run([str(binary)], check=True)
