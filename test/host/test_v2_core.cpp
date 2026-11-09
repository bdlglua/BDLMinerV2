#include "core/release_version.h"
#include "core/pool_diagnostics.h"
#include "core/display_config.h"
#include "core/share_reason.h"
#include <cassert>
#include <cstring>
#include <cstdio>
#include <initializer_list>
int main(){
    const char *ordered[]={"2.0.0-alpha","2.0.0-alpha.1","2.0.0-alpha.beta","2.0.0-beta","2.0.0-beta.2","2.0.0-beta.11","2.0.0-rc.1","2.0.0"};
    for(unsigned i=1;i<sizeof(ordered)/sizeof(*ordered);++i)assert(release_version::compare(ordered[i-1],ordered[i])<0);
    assert(release_version::compare("1.0","1.0.0")==0);
    assert(release_version::compare("2.0.0+abc","2.0.0+def")==0);
    for(const char *text:{"2.0.0-beta.01","01.0.0","2.0.0-","2.0.0+","2.0.0.1","2.0.0.","2.0.0-x..a","2.0.0oops"}){release_version::Version v;assert(!release_version::parse(text,v));}
    char version[24];assert(release_version::find("BDLMiner V2 v2.0.0-beta.11",version,sizeof(version))&&!strcmp(version,"2.0.0-beta.11"));
    pool_diagnostics::Input in;in.connected=true;in.difficulty=10000;in.hashrate=1e6;
    assert(pool_diagnostics::diagnose(in).issue==pool_diagnostics::Issue::WaitingWork);
    in.work=true;auto report=pool_diagnostics::diagnose(in);
    assert(report.compatible&&report.issue==pool_diagnostics::Issue::HighDifficulty);
    assert(report.expectedShareSeconds/86400>496&&report.expectedShareSeconds/86400<498);
    assert(!std::isfinite(report.shareBasedHashrate));
    in.requiredBits=8;assert(pool_diagnostics::diagnose(in).issue==pool_diagnostics::Issue::RollingRequired);
    in.mask=0x1fffe000;in.hashrate=1e12;in.accepted=9;in.acceptedWork=81661.6889;in.seconds=179;
    report=pool_diagnostics::diagnose(in);assert(report.compatible&&report.rollingBits==16&&std::isfinite(report.shareBasedHashrate));
    in.connected=false;in.work=false;in.status="Pool needs version rolling";
    assert(pool_diagnostics::diagnose(in).issue==pool_diagnostics::Issue::RollingRequired);
    assert(stratum::rejectReason(23,"")==stratum::RejectReason::LowDifficulty);
    assert(stratum::rejectReason(0,"stale-share")==stratum::RejectReason::Stale);
    assert(stratum::rejectReason(0,"duplicate-share")==stratum::RejectReason::Duplicate);
    assert(display_config::nextPage(0,8,0x81)==7&&display_config::nextPage(7,8,0x81)==0);
    assert(display_config::nextPage(0,3,0)==0);
    display_config::Snapshot s;s.hashrate=1.093e12;s.temperature=53;s.fanRpm=8667;
    char value[32];display_config::value(1,s,value,sizeof(value),false);assert(strstr(value,"TH/s"));
    display_config::value(8,s,value,sizeof(value),true);assert(!strcmp(value,"8667 RPM"));
    char guard[3]={'x','x','z'};volatile size_t shortSize=2;
    display_config::value(1,s,guard,shortSize,false);assert(guard[1]==0&&guard[2]=='z');
    puts("V2 core: SemVer precedence, honest share intervals/estimates, ASIC requirements, rejection reasons and bounded display configuration passed");
}
