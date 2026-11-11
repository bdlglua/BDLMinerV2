// Independent wire vectors and electrical interlocks for the Gamma backend.
#include "core/bm1370.h"
#include "core/stratum.h"
#include <ArduinoJson.h>
#include <cassert>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <vector>
using namespace bm1370;
static void golden(const Packet &p, const char *hex) { assert(work::toHex(p.bytes.data(),p.size)==hex); }
static std::vector<uint8_t> response(const char *hex) {
    std::vector<uint8_t> out; assert(work::fromHex(hex,out)); return out;
}
int main() {
    golden(versionMask(0x1fffe000),"55aa510900a49000ffff1c");
    golden(versionMask(0),"55aa510900a49000000017");
    golden(reg(0xa8,0x00070000),"55aa510900a80007000003");
    const uint32_t masks[]{0,0xe0,0xf0,0xff,0xf8ff};
    const uint8_t powers[]{0,3,4,8,13};
    for(int i=0;i<5;++i) {
        auto p=ticketMask(powers[i]);
        uint32_t value=uint32_t(p.bytes[6])<<24|uint32_t(p.bytes[7])<<16|uint32_t(p.bytes[8])<<8|p.bytes[9];
        assert(value==masks[i]);
    }
    assert(!ticketMask(32).size);
    float actual=0;auto pll=frequency(525,&actual);
    assert(pll.size==11&&std::fabs(actual-525)<.01);
    assert(!frequency(NAN).size&&!frequency(801).size&&!frequency(0).size);
    for(float f=50;f<=800;f+=6.25f){
        auto packet=frequency(f,&actual);assert(packet.size==11&&std::fabs(actual-f)<3);
        // Decode the actual PLL wire fields independently of the encoder's result.
        unsigned fb=packet.bytes[7],ref=packet.bytes[8];
        unsigned p1=(packet.bytes[9]>>4)+1,p2=(packet.bytes[9]&15)+1;
        assert(fb>=160&&fb<=239&&(ref==1||ref==2)&&p1<=7&&p2<=7&&p1>p2);
        assert(std::fabs(25.0*fb/(ref*p1*p2)-actual)<.001);
    }
    assert(frequency(800,&actual).size&&actual==800);
    uint8_t header[80];for(int i=0;i<80;++i)header[i]=i;
    auto p=job(15,header);assert(p.size==88&&p.bytes[4]==120&&p.bytes[5]==1);
    assert(!memcmp(p.bytes.data()+6,header+76,4));
    assert(!memcmp(p.bytes.data()+10,header+72,4));
    assert(!memcmp(p.bytes.data()+14,header+68,4));
    for(int w=0;w<8;++w){
        assert(!memcmp(p.bytes.data()+18+4*w,header+36+4*(7-w),4));
        assert(!memcmp(p.bytes.data()+50+4*w,header+4+4*(7-w),4));
    }
    assert(!memcmp(p.bytes.data()+82,header,4));
    // Python binascii.crc_hqx(bytes(range fixture wire), 0xffff) -> 0xd6e1.
    assert(p.bytes[86]==0xd6&&p.bytes[87]==0xe1);
    assert(!job(16,header).size);
    Receiver rx;Response decoded;
    auto wire=response("aa559b044c0a0030112288"); // independent CRC patched below
    wire[10]=0x80;
    for(int i=0;i<32;++i){wire[10]=0x80|i;if(crc5(wire.data()+2,9)==0)break;}
    int hits=0;
    auto feed=[&](const std::vector<uint8_t>&data){for(auto b:data)if(rx.push(b,decoded))++hits;};
    feed({0,0xaa,1,0xaa});feed(wire);
    assert(hits==1&&decoded.job&&decoded.slot==3&&decoded.nonce==0x0a4c049b&&decoded.versionBits==0x02244000);
    for(int bit=0;bit<72;++bit){auto bad=wire;bad[2+bit/8]^=1u<<(bit%8);feed(bad);assert(hits==1);}
    feed(wire);assert(hits==2);
    auto chip=response("aa55137000000000000000");
    for(int i=0;i<32;++i){chip[10]=i;if(crc5(chip.data()+2,9)==0)break;}
    feed(chip);assert(hits==3&&!decoded.job&&(decoded.value>>16)==0x1370);
    assert(std::fabs(linear11(encodeLinear11(4.8))-4.8)<.01);
    assert(std::fabs(linear11(encodeLinear11(-12.5))+12.5)<.01);
    uint16_t v;assert(voltageWord(1.15,0x17,v)&&v==589);
    assert(voltageWord(1.15,0x97,v)&&v==589); // REL bit (relative limits, the TPS546 default)
    assert(!voltageWord(1.15,0x37,v)&&!voltageWord(NAN,0x17,v)&&!voltageWord(3,0x17,v));
    assert(diodeTemperature(55,0x80)==55.5f);
    assert(std::isnan(diodeTemperature(0x7f,0x00))&&std::isnan(diodeTemperature(0x7f,0xe0)));
    assert(fanRpm(1800)==3000&&fanRpm(0)==0&&fanRpm(0xffff)==0&&fanRpm(1)==0);
    Health h{55,60,5.0,1.15,15,3000,0};assert(!h.fault(true));
    auto healthy=h;
    h.temperature=74.9f;assert(!h.fault(true));h.temperature=75;assert(h.fault(true));h=healthy;h.temperature=NAN;assert(h.fault(true));
    assert(!h.fault(false,false)&&!h.notReady());
    // A stopped fan stops mining; before power-on, the ASIC waits for it.
    h=healthy;h.rpm=0;assert(h.fault(true)&&!h.fault(false,false)&&h.notReady());
    // 4.8 V to start, then down to 4.6 V while mining (a 5 V supply sags under load).
    h=healthy;h.inputVoltage=4.7;assert(!h.fault(true)&&h.notReady()&&!h.fault(false,false));
    h.inputVoltage=4.55;assert(h.fault(true));h.inputVoltage=0;assert(!h.fault(false,false)&&h.notReady());
    h.inputVoltage=5.6;assert(h.fault(true)&&h.fault(false,false));
    h=healthy;h.regulatorTemperature=104.9f;assert(!h.fault(true));h.regulatorTemperature=105;assert(h.fault(true));
    h=healthy;h.coreVoltage=1.3;assert(h.fault(true));h=healthy;h.current=30;assert(h.fault(true));
    // Regulator status: the faults ESP-Miner stops on, not warnings or latched flags.
    for(uint16_t flag:{0x0020,0x0010,0x0004}){h=healthy;h.status=flag;assert(h.fault(true)&&h.fault(false,false));}
    for(uint16_t flag:{0x0040,0x0008}){h=healthy;h.status=flag;assert(h.fault(true)&&!h.fault(false,false));}
    for(uint16_t flag:{0x8000,0x4000,0x2000,0x1000,0x0800,0x0200,0x0080,0x0002,0x0001}){
        h=healthy;h.status=flag;assert(!h.fault(true)&&!h.fault(false,false));
    }
    h=healthy;h.status=0x0840;h.coreVoltage=0;assert(!h.fault(false));assert(h.fault(true));
    // Reported regulator shutdown: identify its latched overcurrent instead
    // of blaming the low voltage/current sampled after the output was cut.
    h=healthy;h.status=0x4851;h.inputVoltage=5.38f;h.coreVoltage=.027f;h.current=-.3f;h.temperature=72.6f;
    assert(!strcmp(h.fault(true),"Regulator output overcurrent; restart required"));
    assert(!strcmp(h.fault(false,false),"Regulator output overcurrent; restart required"));
    h.status=0x4841;assert(!strcmp(h.fault(true),"Regulator reports a fault; restart required"));
    assert(!h.fault(false,false)); // expected OFF while intentionally unpowered
    h.status=0;assert(!strcmp(h.fault(true),"ASIC voltage fault; restart required"));
    // Whole numbers as ESP-Miner encodes them.
    assert(encodeLinear11Int(0)==0&&encodeLinear11Int(3)==3&&encodeLinear11Int(105)==105&&encodeLinear11Int(650)==650);
    assert(encodeLinear11Int(2000)==(1<<11|1000)&&linear11(encodeLinear11Int(2000))==2000);
    // 525-800 MHz in 25 MHz steps, and ESP-Miner's 690 MHz preset.
    for(unsigned mhz=525;mhz<=800;mhz+=25)assert(validProfile(mhz));
    assert(validProfile(690));
    assert(!validProfile(524)&&!validProfile(825)&&!validProfile(801)&&!validProfile(691)&&!validProfile(601)&&!validProfile(526)&&!validProfile(790));
    for(unsigned mv:{1000,1060,1100,1150,1200,1250,1300})assert(validVoltage(mv));
    assert(!validVoltage(999)&&!validVoltage(1050)&&!validVoltage(1251)&&!validVoltage(1301)&&!validVoltage(1350)&&!validVoltage(0));
    // The voltage that goes with each profile: never below stock, rising with the frequency.
    unsigned previous=0;
    for(unsigned mhz=525;mhz<=800;mhz+=25){unsigned mv=recommendedMv(mhz);assert(validVoltage(mv)&&mv>=kStockMv&&mv>=previous);previous=mv;}
    assert(recommendedMv(525)==1150&&recommendedMv(600)==1150&&recommendedMv(625)==1200&&recommendedMv(690)==1250&&recommendedMv(750)==1300&&recommendedMv(800)==1300);
    assert(kMaxUserTempC==73);
    // The core voltage window follows the chosen voltage: 1.00-1.25 V at 1150 mV.
    {Health v=healthy;v.coreVoltage=1.0f;assert(!v.fault(true));v.coreVoltage=1.24f;assert(!v.fault(true));
     v.coreVoltage=1.26f;assert(v.fault(true));v.coreVoltage=0.98f;assert(v.fault(true));
     v.coreTarget=1.25f;v.coreVoltage=1.25f;assert(!v.fault(true));v.coreVoltage=1.34f;assert(!v.fault(true));
     v.coreVoltage=1.36f;assert(v.fault(true));v.coreVoltage=1.09f;assert(v.fault(true));
     v.coreTarget=1.0f;v.coreVoltage=0.9f;assert(!v.fault(true));v.coreVoltage=0.84f;assert(v.fault(true));}
    ErrorMonitor guard;assert(!guard.update(100,1000,0));
    assert(!guard.update(110,1300,1000)); // short samples do not count as windows
    assert(!guard.update(200,4000,10000)&&guard.percent>3);
    assert(!guard.update(300,7000,20000));
    assert(guard.update(400,10000,30000)); // 3 windows >=2 percent
    guard.reset();assert(!guard.update(UINT32_MAX-20,UINT32_MAX-200,100));
    assert(!guard.update(9,2799,10100)&&guard.percent==1.0f); // both counters wrap
    assert(!guard.update(20,2800,30101)&&guard.percent<0); // stale window
    guard.reset();guard.update(0,0,0);guard.update(100,3000,10000);
    assert(!guard.update(100,6000,20000)&&guard.badWindows==0); // healthy window breaks the streak
    Counter c;assert(!c.update(UINT32_MAX-100,100));
    assert(c.update(99,1100)==(uint64_t(200)<<32));
    assert(!c.update(0,2100));assert(c.update(250,3100)==(uint64_t(250)<<32));
    c.reset();assert(!c.update(500,4100));assert(!c.update(1000,10100));
    assert(c.update(1233,11100)==1000727379968ULL);
    stratum::Protocol proto;proto.reset();auto configure=proto.configureVersionRolling();
    JsonDocument json;assert(!deserializeJson(json,configure));assert(json["method"]=="mining.configure");
    proto.handleLine(R"({"id":4,"result":{"version-rolling":true,"version-rolling.mask":"1fffe000"},"error":null})");
    stratum::Share share{};strcpy(share.jobId,"a");strcpy(share.extranonce2,"00000001");
    share.versionMask=kVersionMask;share.version=0x20002000;
    assert(!deserializeJson(json,proto.submit(share))&&json["params"].size()==6&&json["params"][5]=="00002000");
    proto.reset();assert(proto.submit(share).empty());share.versionMask=0;
    assert(!deserializeJson(json,proto.submit(share))&&json["params"].size()==5);
    proto.configureVersionRolling();
    assert(proto.handleLine(R"({"id":4,"result":{"version-rolling":true,"version-rolling.mask":"ffffffff"}})")==stratum::Event::Error);
    proto.reset();
    assert(proto.handleLine(R"({"method":"mining.set_version_mask","params":["1fffe000"]})")==stratum::Event::Error);
    puts("BM1370: wire vectors, fragmented/corrupt UART, PLL, PMBus, thermal/fan/supply interlocks, 64-bit counter and BIP310 passed");
}
