import Layout from '../components/Layout';
import Icon from '../components/Icons';
import Coin from '../components/Coin';

export const config={unstable_runtimeJS:false};

const features=[
 ['chip','f1t','Hardware SHA-256','f1','Uses the ESP32\'s SHA engine: about 1 MH/s on an M5StickC Plus2, with software mining as a fallback.'],
 ['link','f2t','Stratum V1 and V2','f2','Any solo pool, native Stratum V2 with Noise encryption, a backup pool and automatic failover.'],
 ['chart','f3t','Live web dashboard','f3','Hashrate, accepted and rejected shares, history, settings and pool diagnostics from your browser.'],
 ['shield','f4t','Signed automatic updates','f4','Stable by default, beta by choice. Signed updates run on trial and roll back if they fail.'],
 ['bell','f5t','Alerts','f5','ntfy, Discord or Telegram webhooks: best shares, low hashrate, a daily summary and, who knows, a block.'],
 ['home','f6t','Home Assistant','f6','MQTT discovery with controls, and Prometheus metrics for your own graphs.'],
 ['screen','f7t','Configurable screens','f7','Choose your metrics, their order and enabled pages, in English or French. Headless boards work too.'],
 ['bolt','f8t','ASIC boards too','f8','Experimental support for the Bitaxe Gamma 601 and the NerdNOS, from hundreds of GH/s up to about 1 TH/s.'],
 ['grid','f9t','Fleet hub','f9','Monitor several miners, keep local history and run supervised Gamma calibration from Linux or a Raspberry Pi.'],
];
const explore=[
 ['boards.html','navBoards','Boards','exploreBoards','Compare chips, displays and measured hashrates.'],
 ['demo.html','navDemo','Demo','exploreDemo','Try the dashboard and fleet hub without a miner.'],
 ['start.html','navStart','Get started','exploreStart','First installation, updates and moving from AxeOS.'],
 ['faq.html','navFAQ','FAQ','exploreFAQ','USB, Wi-Fi, shares and performance troubleshooting.'],
 ['releases.html','navNews','Releases','exploreNews','Latest changes and the stable and beta channels.'],
 ['gallery.html','navCommunity','Community','exploreCommunity','Share your setup and meet other miners.'],
];
const pools=['Public Pool','CKPool','OCEAN','NerdMiner.io','Braiins','ViaBTC'];
const families=[['all','famAll','All boards'],['esp32','famEsp32','ESP32'],['s3','famS3','ESP32-S3'],['s2c3','famS2c3','S2 / C3'],['asic','famAsic','ASIC']];

export default function HomePage(){
 return <Layout>
<section className="hero-band" id="top">
 <div className="wrap hero">
  <div className="hero-copy">
   <h1 data-h="title">Solo Bitcoin mining on a <b>pocket-sized</b> ESP32</h1>
   <p className="hero-lead" data-t="tagline">BDLMiner V2 is free, open-source firmware that turns an M5StickC Plus2 or another ESP32 board into a solo "lottery" Bitcoin miner, with a live screen, a web dashboard and automatic signed updates.</p>
   <div className="buttons">
    <a className="btn" href="flash.html" data-t="ctaInstall">Flash</a>
    <a className="btn ghost" href="https://github.com/bdlglua/BDLMinerV2/releases/download/v__VERSION__/BDLMiner-__VERSION__-Linux-kit.zip" data-t="ctaHub">Linux / Raspberry Pi hub</a>
   </div>
   <div className="partners">
    <span className="partners-label" data-t="worksWith">Works with solo pools</span>
    <ul>{pools.map(p=><li key={p}>{p}</li>)}</ul>
   </div>
   <div className="text-links">
    <a href="https://github.com/bdlglua/BDLMinerV2" aria-label="View BDLMinerV2 source code and stable release on GitHub">GitHub ↗</a>
    <a href="https://discord.gg/S8kSgD6Xe9">Discord ↗</a>
    <span className="s" data-t="version">Firmware available: __VERSION__.</span>
   </div>
  </div>
  <div className="hero-art">
   <Coin className="coin-a"/><Coin className="coin-b"/><Coin className="coin-c"/>
   <div className="device">
    <img id="screen" src="assets/screen-mining.png" width="320" height="170" alt="The miner's screen: 1.005 MH/s, 10827 shares, best difficulty 7.82, 52.1 °C" data-alt="altMining"/>
    <div className="thumbs" role="group" aria-label="Screens" data-al="screens">
     <button type="button" data-screen="graph" aria-pressed="false"><img src="assets/screen-graph.png" width="320" height="170" alt="Hashrate chart screen" data-alt="altGraph"/></button>
     <button type="button" data-screen="global" aria-pressed="false"><img src="assets/screen-global.png" width="320" height="170" alt="Bitcoin price and network screen" data-alt="altGlobal"/></button>
     <button type="button" data-screen="clock" aria-pressed="false"><img src="assets/screen-clock.png" width="320" height="170" alt="Clock screen" data-alt="altClock"/></button>
    </div>
   </div>
   <div className="float-card" aria-live="polite">
    <span className="label" data-t="liveFleet">Live fleet</span>
    <strong id="hm-total">–</strong>
    <span className="sub"><b id="hm-miners">–</b> <span data-t="heroMiners">miners online</span></span>
    <svg className="spark" id="hm-spark" viewBox="0 0 120 36" preserveAspectRatio="none" aria-hidden="true"/>
   </div>
  </div>
 </div>
 <div className="wrap">
  <div className="ticker-panel">
   <div className="family-tabs" role="group" aria-label="Board family" data-al="famGroup" data-target="cards">
    {families.map(([key,tk,label],i)=><button type="button" key={key} data-family={key} aria-pressed={i===0?'true':'false'} data-t={tk}>{label}</button>)}
   </div>
   <div className="ticker-cards" id="hm-cards"><p className="ticker-empty">–</p></div>
  </div>
 </div>
</section>

<section id="market"><div className="wrap">
 <div className="section-head">
  <h2 data-t="marketTitle">The BDLMiner fleet, live</h2>
  <a href="stats.html" data-t="marketAll">All statistics</a>
 </div>
 <div className="market-tools">
  <div className="family-tabs" role="group" aria-label="Board family" data-al="famGroup" data-target="table">
   {families.map(([key,tk,label],i)=><button type="button" key={key} data-family={key} aria-pressed={i===0?'true':'false'} data-t={tk}>{label}</button>)}
  </div>
  <label className="market-search"><span className="sr-only" data-t="searchLabel">Search a board</span><input type="search" id="hm-search" placeholder="Search a board" data-ph="searchBoard" autoComplete="off"/></label>
 </div>
 <p className="lead small" data-t="marketLead">Miners sharing their hashrate anonymously, by board. Updated every 20 seconds.</p>
 <div className="market-card">
  <table className="market" id="home-market">
   <thead><tr><th className="rank">#</th><th data-t="colBoard">Board</th><th className="num" data-t="colHashrate">Hashrate</th><th className="num col-miners" data-t="colMiners">Miners</th><th className="col-share" data-t="colShare">Share of miners</th><th><span className="sr-only" data-t="navInstall">Flash</span></th></tr></thead>
   <tbody><tr><td colSpan={6} className="empty">–</td></tr></tbody>
  </table>
 </div>
</div></section>

<section id="explore"><div className="wrap">
 <h2 data-t="exploreTitle">Explore BDLMiner</h2>
 <p className="lead" data-t="exploreLead">Find the right board, try the interface, or get help with your next step.</p>
 <div className="home-links">
  {explore.map(([href,tk,title,dk,text])=><a className="card" href={href} key={href}><h3 data-t={tk}>{title}</h3><p data-t={dk}>{text}</p></a>)}
 </div>
</div></section>

<section id="features"><div className="wrap">
 <h2 data-t="featTitle">What it does</h2>
 <p className="lead" data-t="featLead">The miner runs on the board. An optional Linux or Raspberry Pi hub brings your miners together in one dashboard.</p>
 <div className="grid">
  {features.map(([icon,tk,title,dk,text])=><div className="card" key={tk}><span className="feature-icon"><Icon name={icon}/></span><h3 data-t={tk}>{title}</h3><p data-t={dk}>{text}</p></div>)}
 </div>
</div></section>

<section id="lottery"><div className="wrap">
 <h2 data-t="lotteryTitle">A lottery ticket, not an income</h2>
 <div className="note" data-t="lottery">A block pays 3.125 BTC plus fees to your own address, but at 1 MH/s finding one takes billions of years on average. Every hash has the same tiny chance, with no progress between them: the fun is in the build, the statistics and chasing your best difficulty. Spend on it only what you would on a gadget.</div>
</div></section>
 </Layout>;
}
