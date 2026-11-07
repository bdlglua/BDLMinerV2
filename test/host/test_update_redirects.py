#!/usr/bin/env python3
"""Exercise production HTTPS redirect handling with local transport fixtures."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
source = (root / 'src/ghupdate.cpp').read_text()
routine = source[source.index('constexpr int kStatusLinkRefused'):source.index('// Prefer the small release manifest:')]
fixture = r'''
#include <cassert>
#include <cstdio>
#include <map>
#include <string>
#include <vector>
#define FW_VERSION "test"
constexpr int HTTP_CODE_OK = 200;
constexpr int HTTPC_DISABLE_FOLLOW_REDIRECTS = 0;
struct String : std::string {
    using std::string::string;
    String(const std::string &s) : std::string(s) {}
    bool startsWith(const char *prefix) const { return rfind(prefix, 0) == 0; }
    int indexOf(char c, unsigned at) const { auto p=find(c,at); return p==npos?-1:int(p); }
    String substring(unsigned first, unsigned last) const { return substr(first,last-first); }
};
struct Reply { int code=0; std::string location; };
std::map<std::string,Reply> replies;
std::vector<std::string> requests, hosts;
std::string deniedHost;
int refusals=0, delays=0;  // the asset host refuses the link this many times
void delay(unsigned ms) { assert(ms==2000); ++delays; }
void logf(const char *, ...) {}
struct WiFiClientSecure { void stop() {} };
bool tlsConnectSignedContent(WiFiClientSecure &, const char *host) {
    hosts.emplace_back(host); return deniedHost!=host;
}
struct HTTPClient {
    String url; Reply reply;
    void end() {}
    void setReuse(bool value) { assert(value); }  // GET() must use the guarded handshake
    void setTimeout(unsigned value) { assert(value==15000); }
    void setFollowRedirects(int value) { assert(value==HTTPC_DISABLE_FOLLOW_REDIRECTS); }
    bool begin(WiFiClientSecure &,const String &value) { url=value; return true; }
    void setUserAgent(const char *value) { assert(std::string(value)=="BDLMinerV1/test"); }
    void addHeader(const char *,const char *) {}
    void collectHeaders(const char **,int) {}
    int GET() { requests.emplace_back(url); reply=replies[url];
        if (refusals && url.find("release-assets")!=std::string::npos) { --refusals; reply={618,""}; }
        return reply.code; }
    bool hasHeader(const char *) { return !reply.location.empty(); }
    String header(const char *) { return reply.location; }
};
'''
checks = r'''
int main() {
    const char *old="https://github.com/bdlglua/BDLMinerV1/releases/latest/download/update.json";
    const char *renamed="https://github.com/bdlglua/BDLMinerV2/releases/latest/download/update.json";
    const char *tagged="https://github.com/bdlglua/BDLMinerV2/releases/download/v2.0.1/update.json";
    const char *asset="https://release-assets.githubusercontent.com/fixture";
    replies={{old,{301,renamed}},{renamed,{302,tagged}},{tagged,{302,asset}},{asset,{200,""}}};
    WiFiClientSecure tls; HTTPClient http; int code=0;
    assert(httpsGet(old,tls,http,&code) && code==200);
    assert((requests==std::vector<std::string>{old,renamed,tagged,asset}));
    assert(hosts.size()==4 && hosts.back()=="release-assets.githubusercontent.com");

    requests.clear(); hosts.clear(); replies[old]={302,"http://insecure.example/update.json"};
    assert(!httpsGet(old,tls,http,&code) && requests.size()==1 && hosts.size()==1);
    requests.clear(); hosts.clear(); replies[old]={301,old};
    assert(!httpsGet(old,tls,http,&code) && requests.size()==5 && hosts.size()==5);
    requests.clear(); hosts.clear(); deniedHost="release-assets.githubusercontent.com";
    assert(!httpsGet(asset,tls,http,&code) && requests.empty() && hosts.size()==1);
    // GitHub's file host refuses a link (618: expired or not yet valid): the
    // whole chain is asked again for a fresh link, a bounded number of times.
    deniedHost.clear(); replies[old]={301,renamed};
    requests.clear(); hosts.clear(); refusals=1; delays=0;
    assert(httpsGet(old,tls,http,&code) && code==200 && delays==1);
    assert((requests==std::vector<std::string>{old,renamed,tagged,asset,old,renamed,tagged,asset}));
    requests.clear(); refusals=5; delays=0;
    assert(!httpsGet(old,tls,http,&code) && code==618 && requests.size()==12 && delays==2);
    // A refusal without a redirect (no link handed out) is final.
    requests.clear(); refusals=1; delays=0;
    assert(!httpsGet(asset,tls,http,&code) && code==618 && requests.size()==1 && delays==0);
    refusals=0;
    puts("Update redirects: legacy repository/latest/tag/asset chain, HTTPS-only hops, bounded loops, TLS failures and fresh links after a refusal passed");
}
'''
with tempfile.TemporaryDirectory(prefix='bdl-update-redirects-') as temp:
    cpp = Path(temp) / 'redirects.cpp'
    binary = Path(temp) / 'redirects'
    cpp.write_text(fixture + routine + checks)
    subprocess.run(['g++', '-std=c++17', '-O2', '-Wall', str(cpp), '-o', str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
