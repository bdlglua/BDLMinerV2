import Layout from '../components/Layout';
import HashrateHistory from '../components/HashrateHistory';
import Icon from '../components/Icons';

export const config:StaticExportPageConfig={unstable_runtimeJS:false};

const features=[
 ['chip','f1t','Hardware SHA-256','boards.html'],
 ['link','f2t','Stratum V1 and V2','start.html'],
 ['chart','f3t','Web dashboard','demo.html'],
 ['grid','featureStats','Live statistics','stats.html'],
 ['shield','f4t','Signed updates','releases.html'],
 ['bolt','featureFlash','Web flasher','flash.html'],
] as const;
const pools=['Public Pool','CKPool','OCEAN','NerdMiner.io','Braiins','ViaBTC'];
const families=[['all','famAll','All boards'],['esp32','famEsp32','ESP32'],['s3','famS3','ESP32-S3'],['s2c3','famS2c3','S2 / C3'],['asic','famAsic','ASIC']];

export default function HomePage(){
 return <Layout chart>
<section className="hero-band" id="top">
 <div className="wrap hero hero-stage">
 <div className="hero-copy">
  <p className="hero-kicker" data-t="heroKicker">Open-source firmware · ESP32 &amp; ASIC</p>
  <h1 data-h="title">Bitcoin solo mining, <b>on small hardware</b></h1>
  <p className="hero-lead" data-t="tagline">Open-source miner firmware for ESP32 boards and Bitaxe</p>
  <div className="buttons">
   <a className="btn" href="flash.html" data-t="ctaInstall">Install</a>
   <a className="btn ghost" href="https://github.com/bdlglua/BDLMinerV2/releases/download/v__VERSION__/BDLMiner-__VERSION__-Linux-kit.zip" data-t="ctaHub">Linux hub</a>
  </div>
  <div className="partners">
   <span className="partners-label" data-t="worksWith">Solo pool support</span>
   <ul>{pools.map(p=><li key={p}>{p}</li>)}</ul>
  </div>
  <div className="text-links">
   <a href="boards.html" data-t="navBoards">Boards</a>
   <a href="https://github.com/bdlglua/BDLMinerV2" aria-label="View BDLMinerV2 source code and stable release on GitHub">GitHub</a>
   <span className="s" data-t="version">Latest firmware: __VERSION__</span>
  </div>
 </div>

 <div className="hero-product">
 <figure className="hardware-figure">
  <img className="hardware-photo" src="assets/boards/gamma.webp" width="826" height="960" alt="Bitaxe Gamma mining board with its display, ESP32 module and cooling fan" data-alt="heroHardwareAlt" fetchPriority="high"/>
  <figcaption><span>Bitaxe Gamma 601</span><span>ASIC · SHA-256</span></figcaption>
 </figure>
 <aside className="hero-mini" aria-label="BDLMiner network">
  <div className="hero-mini-head"><span data-t="heroNetwork">BDLMiner network</span><span className="hero-mini-period">24 h</span></div>
  <strong id="hero-network-rate">—</strong>
  <svg id="hero-network-spark" viewBox="0 0 260 110" aria-hidden="true"><path className="hero-spark-grid" d="M0 25H260M0 60H260M0 95H260"/></svg>
  <p data-t="heroMeasured">Measured community hashrate</p>
 </aside>
 </div>
 </div>
 <div className="wrap">
  <div className="ticker-panel">
   <h2 className="pulse-ticker-title" data-t="tickerTitle">Network / Live miners</h2>
   <div className="family-tabs" role="group" aria-label="Board family" data-al="famGroup" data-target="cards">
    {families.map(([key,tk,label],i)=><button type="button" key={key} data-family={key} aria-pressed={i===0?'true':'false'} data-t={tk}>{label}</button>)}
   </div>
   <div className="ticker-cards" id="hm-cards"><p className="ticker-empty">–</p></div>
  </div>
 </div>
</section>


<section id="features" className="pulse-features"><div className="wrap">
 <h2 className="sr-only" data-t="featTitle">Made for solo mining</h2>
 <div className="pulse-shortcuts">
  {features.map(([icon,tk,title,href])=><a href={href} key={tk}><Icon name={icon}/><span data-t={tk}>{title}</span></a>)}
 </div>
</div></section>

<section className="pulse-chart-section"><div className="wrap">
 <HashrateHistory/>
</div></section>

<section id="market"><div className="wrap">
 <div className="section-head">
  <h2 data-t="marketTitle">Network snapshot</h2>
  <a href="stats.html" data-t="marketAll">Full statistics</a>
 </div>
 <div className="market-tools">
  <div className="family-tabs" role="group" aria-label="Board family" data-al="famGroup" data-target="table">
   {families.map(([key,tk,label],i)=><button type="button" key={key} data-family={key} aria-pressed={i===0?'true':'false'} data-t={tk}>{label}</button>)}
  </div>
  <label className="market-search"><span className="sr-only" data-t="searchLabel">Search a board</span><input type="search" id="hm-search" placeholder="Search a board" data-ph="searchBoard" autoComplete="off"/></label>
 </div>
 <p className="lead small" data-t="marketLead">Community hashrate by board · updated every 20 seconds.</p>
  <p className="s" id="hm-status" role="status" aria-live="polite"></p>
 <div className="market-card">
  <table className="market" id="home-market">
   <thead><tr><th className="rank">#</th><th data-t="colBoard">Board</th><th className="num" data-t="colHashrate">Hashrate</th><th className="num col-miners" data-t="colMiners">Miners</th><th className="col-share" data-t="colShare">Share of miners</th><th><span className="sr-only" data-t="navInstall">Flash</span></th></tr></thead>
   <tbody><tr><td colSpan={6} className="empty">–</td></tr></tbody>
  </table>
 </div>
</div></section>



<section className="device-section"><div className="wrap"><h2 data-t="screensTitle">On-device screens</h2>
 <div className="hero-art">
  <div className="device">
   <img id="screen" src="assets/screen-mining.png" width="320" height="170" alt="The miner's screen: 1.005 MH/s, 10827 shares, best difficulty 7.82, 52.1 °C" data-alt="altMining"/>
    <div className="thumbs" role="group" aria-label="Screens" data-al="screens">
     <button type="button" data-screen="graph" aria-pressed="false"><img src="assets/screen-graph.png" width="320" height="170" alt="Hashrate chart screen" data-alt="altGraph"/></button>
     <button type="button" data-screen="global" aria-pressed="false"><img src="assets/screen-global.png" width="320" height="170" alt="Bitcoin price and network screen" data-alt="altGlobal"/></button>
     <button type="button" data-screen="clock" aria-pressed="false"><img src="assets/screen-clock.png" width="320" height="170" alt="Clock screen" data-alt="altClock"/></button>
    </div>
    <p className="device-caption" data-t="deviceCaption">Live stats on the device</p>
   </div>
  </div>
</div></section>

<section id="lottery"><div className="wrap">
 <p className="lottery-note" data-t="lottery">Solo mining is a hobby, not a source of income. At ESP32 hashrates, a block is extremely unlikely.</p>
</div></section>
 </Layout>;
}
