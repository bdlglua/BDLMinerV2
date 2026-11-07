#include <cassert>
#include <cstdio>
#include "settings_apply.h"

Settings settings;
const SettingInfo kSettings[] = {
    {"coin", SettingType::Number, &settings.miningCoin, 8, "", ""},
    {"addr", SettingType::Text, &settings.btcAddress, 0, "", ""},
    {"sharepool", SettingType::Bool, &settings.sharePool, 0, "", ""},
    {"remote", SettingType::Bool, &settings.remoteEnabled, 0, "", ""},
    {"remotekey", SettingType::Secret, &settings.remoteKey, 0, "", ""},
    {"rname", SettingType::Text, &settings.remoteName, 0, "", ""},
    {"channel", SettingType::Number, &settings.updateChannel, 8, "", ""},
    {"customscreen", SettingType::Bool, &settings.customScreen, 0, "", ""},
    {"screen0", SettingType::Number, &settings.screenWidget0, 8, "", ""},
    {"pages", SettingType::Number, &settings.screenPages, 8, "", ""},
#ifdef BOARD_GAMMA601
    {"asicmhz", SettingType::Number, &settings.asicFrequency, 16, "", ""},
    {"asicmv", SettingType::Number, &settings.asicVoltage, 16, "", ""},
    {"fanauto", SettingType::Bool, &settings.fanAuto, 0, "", ""},
    {"fanpct", SettingType::Number, &settings.fanPercent, 8, "", ""},
#endif
    {"pool", SettingType::Text, &settings.poolHost, 0, "", ""},
    {"pool2", SettingType::Text, &settings.backupHost, 0, "", ""},
    {"port", SettingType::Number, &settings.poolPort, 16, "", ""},
    {"power", SettingType::Number, &settings.miningPower, 8, "", ""},
    {"webpass", SettingType::Secret, &settings.webPassword, 0, "", ""},
    {"diff", SettingType::Decimal, &settings.suggestedDifficulty, 0, "", ""},
    {"saver", SettingType::Bool, &settings.batterySaver, 0, "", ""},
    {"maxtemp", SettingType::Number, &settings.maxTemp, 8, "", ""},
    {"nightfrom", SettingType::Number, &settings.nightFrom, 8, "", ""},
    {"uaauto", SettingType::Bool, &settings.userAgentAuto, 0, "", ""},
    {"ua", SettingType::Text, &settings.userAgent, 0, "", ""},
    {"proto", SettingType::Number, &settings.poolProtocol, 8, "", ""},
    {"proto2", SettingType::Number, &settings.backupProtocol, 8, "", ""},
    {"sv2key", SettingType::Text, &settings.poolAuthority, 0, "", ""},
    {"sv2key2", SettingType::Text, &settings.backupAuthority, 0, "", ""},
};
const size_t kSettingCount = sizeof(kSettings) / sizeof(kSettings[0]);

int main() {
    assert(!settings.remoteEnabled && settings.remoteKey.length()==0);
    {
        JsonDocument value;
        value["remote"]=true;
        assert(settingsError(value.as<JsonObjectConst>()));
        value["remotekey"]="0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        value["rname"]="M5 bureau";
        assert(!settingsError(value.as<JsonObjectConst>()));
        Settings paired=settings;settingsApply(paired,value.as<JsonObjectConst>());
        assert(paired.remoteEnabled && paired.remoteName=="M5 bureau");
        assert(!settingsCompare(paired,settings).live); // staging a key requires restart
        for(const char *key : {"short","ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789ab"}) {
            value["remotekey"]=key;assert(settingsError(value.as<JsonObjectConst>()));
        }
        const Settings original=settings;settings=paired;
        value.clear();value["remote"]=false;
        assert(!settingsError(value.as<JsonObjectConst>()));
        Settings disabled=settings;settingsApply(disabled,value.as<JsonObjectConst>());
        assert(settingsCompare(disabled,settings).live && !disabled.remoteEnabled);
        settings=original;
        // Without a key, a usable payout address is enough for remote monitoring.
        JsonDocument byAddress;byAddress["remote"]=true;byAddress["addr"]="bc1qexample0address.rig1";
        assert(!settingsError(byAddress.as<JsonObjectConst>()));
        byAddress["addr"]="ab";assert(settingsError(byAddress.as<JsonObjectConst>()));
    }

    assert(settings.miningCoin == coin::BTC);
    for(unsigned id:{unsigned(coin::BSV),unsigned(coin::XEC),unsigned(coin::DGB),unsigned(coin::PPC)}) {
        JsonDocument request;request["coin"]=id;request["pool"]="custom.coin.example";request["addr"]="account.worker";request["proto"]=0;
        assert(!settingsError(request.as<JsonObjectConst>()));Settings staged=settings;settingsApply(staged,request.as<JsonObjectConst>());
        assert(staged.miningCoin==id && !settingsCompare(staged,settings).live);
        request["proto"]=1;assert(settingsError(request.as<JsonObjectConst>()));request["proto"]=0;
        request["pool2"]="public-pool.io";assert(settingsError(request.as<JsonObjectConst>()));
    }
    assert(!coin::addressCompatible(coin::XEC,"bitcoincash:qpm2qsznhks23z7629mms6s4cwef74vcwvy22gdx6a"));
    assert(!coin::poolCompatible(coin::BSV,"xec.solopool.org"));

    {
        JsonDocument oldBackup; deserializeJson(oldBackup,R"({"pool":"public-pool.io","addr":"address.worker"})");
        settingsMigrateBackupCoin(oldBackup.as<JsonObject>(),true);assert(oldBackup["coin"].as<unsigned>()==coin::BTC);
        JsonDocument partial;deserializeJson(partial,R"({"power":75})");
        settingsMigrateBackupCoin(partial.as<JsonObject>(),false);assert(partial["coin"].isUnbound());
        JsonDocument modern;deserializeJson(modern,R"({"coin":1,"pool":"bch.example.org","addr":"account.worker"})");
        settingsMigrateBackupCoin(modern.as<JsonObject>(),true);assert(modern["coin"].as<unsigned>()==coin::BCH);
    }
    {
        JsonDocument change;
        assert(!deserializeJson(change, R"({"coin":1,"pool":"bch.example.org","addr":"bitcoincash:qpm2qsznhks23z7629mms6s4cwef74vcwvy22gdx6a","proto":0})"));
        assert(!settingsError(change.as<JsonObjectConst>()));
        Settings staged = settings;
        settingsApply(staged, change.as<JsonObjectConst>());
        assert(staged.miningCoin == coin::BCH && !settingsCompare(staged, settings).live);
        settingsCopyLive(settings, staged); assert(settings.miningCoin == coin::BTC);
        for (const char *raw : {R"({"coin":6})", R"({"coin":"BCH"})", R"({"coin":1})",
            R"({"coin":1,"pool":"public-pool.io"})",
            R"({"coin":1,"pool":"bch.example.org","proto":1})",
            R"({"coin":1,"pool":"stratum2+tcp://bch.example.org:3333/key"})",
            R"({"coin":1,"pool":"bch.example.org","pool2":"solo.ckpool.org"})",
            R"({"coin":1,"pool":"bch.example.org","addr":"bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh"})",
            R"({"coin":0,"addr":"bitcoincash:qpm2qsznhks23z7629mms6s4cwef74vcwvy22gdx6a"})"}) {
            JsonDocument invalid; assert(!deserializeJson(invalid, raw));
            assert(settingsError(invalid.as<JsonObjectConst>()));
        }
        assert(coin::poolCompatible(coin::BCH,"stratum+tcp://bch.custom.example:3333"));
        assert(!coin::poolCompatible(coin::BCH,"STRATUM+TCP://MAIN.PUBLIC-POOL.IO.:21496"));
        assert(coin::poolCompatible(coin::BCH,"public-pool.io.example.org"));
        assert(!coin::poolCompatible(coin::BTC,"b4c.f2pool.com"));
    }

    assert(settings.sharePool);
    Settings sharing = settings; JsonDocument sharingValues;
    assert(!deserializeJson(sharingValues, R"({"sharepool":false})"));
    assert(!settingsError(sharingValues.as<JsonObjectConst>()));
    settingsApply(sharing, sharingValues.as<JsonObjectConst>());
    assert(!sharing.sharePool && settingsCompare(sharing, settings).live);
    settingsCopyLive(settings, sharing); assert(!settings.sharePool);
    settings = Settings();
    assert(settings.updateChannel == 0 && !settings.customScreen && settings.screenPages == 255);
    for(const char *raw:{R"({"channel":2})",R"({"screen0":0})",R"({"screen0":9})",R"({"pages":2})",R"({"pages":0})",R"({"customscreen":"true"})"}){
        JsonDocument invalid;assert(!deserializeJson(invalid,raw));assert(settingsError(invalid.as<JsonObjectConst>()));
    }
    {
        Settings staged=settings;JsonDocument choices;
        assert(!deserializeJson(choices,R"({"channel":1,"customscreen":true,"screen0":8,"pages":5})"));
        assert(!settingsError(choices.as<JsonObjectConst>()));settingsApply(staged,choices.as<JsonObjectConst>());
        assert(settingsCompare(staged,settings).live);settingsCopyLive(settings,staged);
        assert(settings.updateChannel==1&&settings.customScreen&&settings.screenWidget0==8&&settings.screenPages==5);
        settings=Settings();
    }
    {
        JsonDocument legacy;deserializeJson(legacy,R"({"ua":"OldCustomAgent","uaauto":true})");
        settingsMigrateBackupAgent(legacy.as<JsonObject>());
        assert(!settingsError(legacy.as<JsonObjectConst>()));
        assert(legacy["ua"].as<std::string>()==DEFAULT_USER_AGENT && !legacy["uaauto"].as<bool>());
    }
    for(const char *selected:{"NerdMinerV2",DEFAULT_USER_AGENT}){
        Settings choice;JsonDocument request;request["ua"]=selected;request["uaauto"]=true;
        assert(!settingsError(request.as<JsonObjectConst>()));settingsApply(choice,request.as<JsonObjectConst>());
        assert(choice.userAgent==selected && !choice.userAgentAuto);
        auto html=user_agent::portalSelect(selected,DEFAULT_USER_AGENT);
        assert(html.find("<input")==std::string::npos && html.find("name='ua'")!=std::string::npos);
        unsigned options=0;for(size_t p=0;(p=html.find("<option ",p))!=std::string::npos;++p)++options;
        assert(options==2 && html.find(std::string("value='")+selected+"' selected")!=std::string::npos);
    }
    assert(settings.userAgent == DEFAULT_USER_AGENT);
    assert(!strcmp(DEFAULT_USER_AGENT, "BDLMinerV2/" FW_VERSION));
    for (const char *oldNative : {"BDLMinerV1/1.9.15", "BDLMinerV2/2.0.0"}) {
        Settings migrated; migrated.userAgent = oldNative;
        settingsNormalizeAgent(migrated);
        assert(migrated.userAgent == DEFAULT_USER_AGENT);
        assert(!legacyManualAgent(oldNative));
    }
    assert(legacyManualAgent("BDLMinerV1-custom") && legacyManualAgent("BDLMinerV2-custom"));
    Settings identity;
    identity.userAgent = "NerdMinerV2";
    settingsNormalizeAgent(identity);
    assert(identity.userAgent == "NerdMinerV2" && !identity.userAgentAuto);
    identity.userAgentAuto = false; identity.userAgent = "NerdMinerV2";
    settingsNormalizeAgent(identity);
    assert(identity.userAgent == "NerdMinerV2" && !identity.userAgentAuto);
    identity.userAgentAuto = true; identity.userAgent = "MySavedIdentity";
    settingsNormalizeAgent(identity);
    assert(identity.userAgent == DEFAULT_USER_AGENT && !identity.userAgentAuto);
    for (const auto &agent : {std::string(), std::string(65, 'x')}) {
        identity.userAgent = agent;
        settingsNormalizeAgent(identity);
        assert(identity.userAgent == DEFAULT_USER_AGENT);
    }
    settings.poolHost = "original.pool";
    settings.webPassword = "original-password";
    Settings pending = settings;
    JsonDocument changes;
    assert(!deserializeJson(changes, R"({"pool":"next.pool","port":3333,"power":40,"webpass":"next-password","diff":0.5,"saver":true})"));
    assert(settingsApply(pending, changes.as<JsonObjectConst>()) == 6);
    assert(settingsError(changes.as<JsonObjectConst>()) == nullptr);
    assert(pending.poolHost == "next.pool" && pending.poolPort == 3333);
    assert(pending.miningPower == 40 && pending.webPassword == "next-password");
    assert(pending.suggestedDifficulty == 0.5 && pending.batterySaver);
    assert(settings.poolHost == "original.pool" && settings.poolPort == DEFAULT_POOL_PORT);
    assert(settings.miningPower == 100 && settings.webPassword == "original-password");
    assert(settings.suggestedDifficulty == DEFAULT_DIFFICULTY && !settings.batterySaver);

    // An empty password removes it (the dashboard is then open); partial
    // backups leave other fields intact.
    assert(!deserializeJson(changes, R"({"webpass":"","power":-5,"port":100000})"));
    assert(settingsApply(pending, changes.as<JsonObjectConst>()) == 3);
    assert(pending.webPassword == "" && pending.poolHost == "next.pool");
    assert(pending.miningPower == 0 && pending.poolPort == 65535);
    assert(settings.webPassword == "original-password");

    // HTTP handlers validate every field before applying anything. Invalid
    // numeric/checkbox inputs must not silently become zero or true.
    for (const char *raw : {R"({"power":null})", R"({"power":""})", R"({"power":"25"})",
                           R"({"power":101})", R"({"power":-1})", R"({"power":12.5})",
                           R"({"port":0})", R"({"port":65536})", R"({"diff":0})",
                           R"({"diff":-1})", R"({"saver":"false"})", R"({"pool":null})",
                           R"({"pool":"   "})", R"({"maxtemp":25})", R"({"nightfrom":24})"}) {
        assert(!deserializeJson(changes, raw));
        assert(settingsError(changes.as<JsonObjectConst>()) != nullptr);
    }
    assert(!deserializeJson(changes, R"({"power":0,"port":65535,"diff":0.000001,"saver":false,"maxtemp":0,"nightfrom":255})"));
    assert(settingsError(changes.as<JsonObjectConst>()) == nullptr);
    puts("Staged settings preserve live network strings and clamp numeric fields: passed");
    for(const char *raw:{R"({"proto":2})",R"({"proto":1})",R"({"proto2":1,"sv2key2":"bad"})",R"({"uaauto":false,"ua":"   "})",R"({"uaauto":"true"})",R"({"ua":"CustomMiner"})",
                        R"({"proto":1,"sv2key":"9c4zpyJ2ndm4e8sP2uNc1VNCGxYjqaxWS6wUCjk8zFj6njFquH7"})"}){
        assert(!deserializeJson(changes,raw));assert(settingsError(changes.as<JsonObjectConst>())!=nullptr);
    }
    assert(!deserializeJson(changes,R"({"proto":1,"sv2key":"9c4zpyJ2ndm4e8sP2uNc1VNCGxYjqaxWS6wUCjk8zFj6njFquH6","proto2":0,"uaauto":false,"ua":"NerdMinerV2"})"));
    assert(settingsError(changes.as<JsonObjectConst>())==nullptr);
    assert(settingsApply(pending,changes.as<JsonObjectConst>())==5);
    assert(pending.poolProtocol==1&&pending.backupProtocol==0&&!pending.userAgentAuto&&pending.userAgent=="NerdMinerV2");
    assert(settings.poolProtocol==0&&!settings.userAgentAuto);
    // Pasted V2 URLs select V2 during sanitization. Validate that effective
    // protocol/key too, including backup URLs and whitespace around keys.
    for (const char *raw : {R"({"pool":"stratum2+tcp://public-pool.io:23330","proto":0})",
                           R"({"pool2":"stratum2+tcp://public-pool.io:23330/bad","proto2":0})"}) {
        assert(!deserializeJson(changes,raw)); assert(settingsError(changes.as<JsonObjectConst>())!=nullptr);
    }
    assert(!deserializeJson(changes,R"({"pool":"stratum2+tcp://public-pool.io:23330/9c4zpyJ2ndm4e8sP2uNc1VNCGxYjqaxWS6wUCjk8zFj6njFquH6","proto":1,"sv2key":""})"));
    assert(settingsError(changes.as<JsonObjectConst>())==nullptr);
    assert(!deserializeJson(changes,R"({"proto":1,"sv2key":" 9c4zpyJ2ndm4e8sP2uNc1VNCGxYjqaxWS6wUCjk8zFj6njFquH6  "})"));
    assert(settingsError(changes.as<JsonObjectConst>())==nullptr);
#ifdef BOARD_GAMMA601
    for(const char *raw:{R"({"fanpct":29})",R"({"fanpct":101})",R"({"fanpct":0})",R"({"fanpct":-1})",R"({"fanpct":30.5})",R"({"fanpct":null})",R"({"fanpct":"50"})",R"({"fanauto":1})"}) {
        assert(!deserializeJson(changes,raw));assert(settingsError(changes.as<JsonObjectConst>()));
    }
    changes.clear();changes["fanauto"]=true;changes["fanpct"]=40;
    assert(!settingsError(changes.as<JsonObjectConst>()));
    Settings fanSettings=settings;assert(settingsApply(fanSettings,changes.as<JsonObjectConst>())==2);
    assert(settingsCompare(fanSettings,settings).live&&settingsCompare(fanSettings,settings).count==2);
    Settings active=settings;settingsCopyLive(active,fanSettings);
    assert(active.fanAuto&&active.fanPercent==40&&!settings.fanAuto&&settings.fanPercent==100);
    for(const char *raw:{R"({"asicmhz":524})",R"({"asicmhz":825})",R"({"asicmhz":810})",R"({"asicmhz":691})",R"({"asicmhz":601})",R"({"asicmhz":526})",R"({"asicmhz":600.5})",R"({"asicmhz":"600"})",
                         R"({"asicmv":999})",R"({"asicmv":1050})",R"({"asicmv":1251})",R"({"asicmv":1350})",R"({"asicmv":1301})",R"({"asicmv":"1200"})",R"({"asicmv":1200.5})"}) {
        assert(!deserializeJson(changes,raw));assert(settingsError(changes.as<JsonObjectConst>()));
    }
    for(int mv:{1000,1060,1100,1150,1200,1250,1300}) {
        changes.clear();changes["asicmv"]=mv;
        assert(!settingsError(changes.as<JsonObjectConst>()));
        Settings staged=settings;assert(settingsApply(staged,changes.as<JsonObjectConst>())==1);
        assert(staged.asicVoltage==mv&&settings.asicVoltage==1150); // stock default
    }
    for(int mhz:{525,550,575,600,625,650,675,690,700,725,750,775,800}) {
        changes.clear();changes["asicmhz"]=mhz;
        assert(!settingsError(changes.as<JsonObjectConst>()));
        Settings staged=settings;assert(settingsApply(staged,changes.as<JsonObjectConst>())==1);
        assert(staged.asicFrequency==mhz&&settings.asicFrequency==525); // stock default
    }
    // The user limit pauses before the latching 70 C diode stop; 0 (off) stays allowed.
    for(const char *raw:{R"({"maxtemp":74})",R"({"maxtemp":75})",R"({"maxtemp":80})"}) {
        assert(!deserializeJson(changes,raw));assert(settingsError(changes.as<JsonObjectConst>()));
    }
    for(int limit:{0,30,65,68,73}) {changes.clear();changes["maxtemp"]=limit;assert(!settingsError(changes.as<JsonObjectConst>()));}
#endif
    puts("V1/V2 and explicit user agent settings: validation, staged isolation and preserved manual identity passed");
}
