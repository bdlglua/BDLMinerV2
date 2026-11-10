#!/usr/bin/env python3
"""Run the production private reporter with controlled network/time and real JSON."""
from pathlib import Path
import hashlib
import subprocess
import tempfile

root=Path(__file__).resolve().parents[2]
source=(root/'src/remote.cpp').read_text()
source=source[source.index('namespace {'):]
prefix=r'''
#include <ArduinoJson.h>
#include <cassert>
#include <cmath>
#include <cstdint>
#include <ctime>
#include <string>
#include <map>
#include <vector>
#include <cstdarg>
#include "core/remote_data.h"
#include "core/public_pool.h"
#include "core/coin.h"
#define FW_VERSION "2.3.0"
#define FW_ENV "m5stickc-plus2"
#define DEFAULT_USER_AGENT "BDLMinerV2/2.3.0"
#define HEX 16
#define HTTPC_DISABLE_FOLLOW_REDIRECTS 0
struct String:std::string {
 using std::string::string;
 String(const std::string &s):std::string(s){}
 String(uint32_t n,int){char b[16];snprintf(b,sizeof(b),"%x",n);assign(b);}
 size_t length()const{return size();}
};
struct {bool remoteEnabled=false;String remoteKey,remoteName,btcAddress;unsigned miningCoin=0;} settings;
uint32_t now=0;uint32_t millis(){return now;}
struct {bool connected=true;bool isConnected(){return connected;}} WiFi;
bool memoryOk=true,verified=true;bool tlsMemoryOk(){return memoryOk;}
struct {uint64_t getEfuseMac(){return 0x112233445566ULL;}} ESP;
namespace board {const char *modelName(){return "M5StickC Plus2";}float temp=48;float temperature(){return temp;}}
struct MinerStats {uint32_t accepted=2,rejected=0;double bestDiff=123;bool poolConnected=true;} stats;
MinerStats minerStats(){return stats;}
double rate=1e6;double minerHashrate(){return rate;}
uint64_t minerTotalHashes(){return 600000000;}
uint32_t minerUptimeSeconds(){return now/1000;}
uint8_t minerPowerPercent(){return rate>0?100:0;}
uint32_t webBootId(){return 0x1234abcd;}
bool trialPending(){return false;}
String pool="public-pool.io";const String &netPoolHost(){return pool;}
struct WiFiClientSecure {};
const char kExplorerRootCA[]="ROOT";
unsigned handshakes=0;bool tlsConnectVerified(WiFiClientSecure &,const char *host,const char *root,uint16_t port){
 assert(!strcmp(host,"api.bdl-pool.com"));assert(port==36018);assert(!strcmp(root,"ROOT"));++handshakes;return verified;
}
std::vector<std::string> sent,logs;std::map<std::string,std::string> headers;
void logf(const char *format,...){char b[256];va_list args;va_start(args,format);vsnprintf(b,sizeof(b),format,args);va_end(args);logs.emplace_back(b);}
struct HTTPClient {
 void setTimeout(int){}
 void setFollowRedirects(int mode){assert(mode==HTTPC_DISABLE_FOLLOW_REDIRECTS);}
 bool begin(WiFiClientSecure &,const char *url){assert(!strcmp(url,"https://api.bdl-pool.com:36018/api/remote/report"));return true;}
 void setUserAgent(const char *v){assert(!strcmp(v,DEFAULT_USER_AGENT));}
 void addHeader(const char *name,const String &value){headers[name]=value;}
 int POST(uint8_t *data,size_t size){sent.emplace_back((char*)data,size);return 204;}
 void end(){}
};
'''
key='0123456789abcdef'*4
expected=hashlib.sha256(key.encode()+(0x112233445566).to_bytes(8,'little')).hexdigest()[:16]
address_id=hashlib.sha256(b'address:bc1qexample0address'+(0x112233445566).to_bytes(8,'little')).hexdigest()[:16]
tests=r'''
int main(){
 const char *key="0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
 assert(remote_data::validKey(key));assert(!remote_data::validKey(nullptr));assert(!remote_data::validKey("short"));
 char id[17],other[17];remote_data::deviceId(key,0x112233445566ULL,id);assert(!strcmp(id,"EXPECTED_ID"));
 remote_data::deviceId(key,0x112233445567ULL,other);assert(strcmp(id,other));
 now=60000;remotePoll();assert(sent.empty()&&handshakes==0);
 settings.remoteEnabled=true;remotePoll();assert(sent.empty());
 settings.remoteKey=key;settings.remoteName="M5 \"bureau\"";now=59999;remotePoll();assert(sent.empty());
 now=60000;remotePoll();assert(sent.size()==1);
 assert(headers["Authorization"]==std::string("Bearer ")+key);
 JsonDocument report;assert(!deserializeJson(report,sent.back()));
 assert(report["id"]==id && report["name"]=="M5 \"bureau\"" && report["coin"]=="BTC");
 assert(report["boot_id"]=="1234abcd" && report["accepted"]==2 && report["pool"]=="public-pool.io");
 assert(sent.back().find(key)==std::string::npos && sent.back().find("112233445566")==std::string::npos);
 assert(sent.back().find("address")==std::string::npos && sent.back().find("password")==std::string::npos);
 now+=299999;remotePoll();assert(sent.size()==1);now++;remotePoll();assert(sent.size()==2);
 verified=false;now+=300000;remotePoll();assert(sent.size()==2);verified=true;
 now+=60000;remotePoll();assert(sent.size()==2); // no rapid retries after a failed TLS session
 now=remoteNext;memoryOk=false;remotePoll();assert(sent.size()==2);memoryOk=true;
 now=remoteNext;WiFi.connected=false;remotePoll();assert(sent.size()==2);WiFi.connected=true;
 now=remoteNext;rate=0;stats.poolConnected=false;board::temp=NAN;remotePoll();assert(sent.size()==3);
 assert(!deserializeJson(report,sent.back()));assert(report["hashrate"]==0 && report["temp_c"].isNull() && report["pool"]=="unknown");
 settings.remoteEnabled=false;now+=300000;remotePoll();assert(sent.size()==3);
 for(const auto &line:logs){assert(line.find(key)==std::string::npos);assert(line.find(id)==std::string::npos);}
 // Without a key, the payout address (before any .worker) names the space.
 char address[101];
 assert(remote_data::payoutAddress(" bc1qExample0address.rig1",address)&&!strcmp(address,"bc1qExample0address"));
 assert(!remote_data::payoutAddress("",address)&&!address[0]);assert(!remote_data::payoutAddress("ab.c",address));
 assert(!remote_data::payoutAddress("bad/address",address));assert(!remote_data::payoutAddress(std::string(101,'a').c_str(),address));
 assert(remote_data::payoutAddress(std::string(100,'a').c_str(),address));
 assert(remote_data::canReport("", "bc1qexample0address")&&remote_data::canReport(key,"")&&!remote_data::canReport("short","x"));
 char upper[17],lower[17];remote_data::addressDeviceId("BC1QEXAMPLE0ADDRESS",0x112233445566ULL,upper);remote_data::addressDeviceId("bc1qexample0address",0x112233445566ULL,lower);
 assert(!strcmp(upper,lower)&&!strcmp(lower,"ADDRESS_ID")&&strcmp(lower,id));
 settings.remoteKey="";settings.remoteEnabled=true;settings.btcAddress="bc1qexample0address.rig1";now=remoteNext;remotePoll();assert(sent.size()==4);
 assert(headers["Authorization"]=="Address bc1qexample0address");
 assert(!deserializeJson(report,sent.back()));assert(report["id"]==std::string(lower));
 assert(sent.back().find("bc1q")==std::string::npos&&sent.back().find("rig1")==std::string::npos);
 settings.btcAddress="";now=remoteNext;remotePoll();assert(sent.size()==4);
 for(const auto &line:logs){assert(line.find("bc1q")==std::string::npos);assert(line.find(lower)==std::string::npos);}
 puts("remote reporter: opt-in, verified TLS, key or payout address, secret isolation, identifiers, schedule, pause/offline: OK");
}
'''.replace('EXPECTED_ID',expected).replace('ADDRESS_ID',address_id)
with tempfile.TemporaryDirectory() as tmp:
    src=Path(tmp)/'remote.cpp';exe=Path(tmp)/'remote'
    src.write_text(prefix+source+tests)
    subprocess.run(['g++','-std=c++17','-Wall','-I'+str(root/'src'),'-I'+str(root/'.pio/libdeps/esp32-headless/ArduinoJson/src'),str(src),str(root/'src/core/sha256.cpp'),'-o',str(exe)],check=True)
    subprocess.run([str(exe)],check=True)
