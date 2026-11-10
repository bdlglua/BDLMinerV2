import Layout from '../components/Layout';
import HashrateHistory from '../components/HashrateHistory';

export const config:StaticExportPageConfig={unstable_runtimeJS:false};

export default function StatsPage(){
 return <Layout chart>
<section id="stats">
 <div className="wrap">
  <h1 data-t="statsTitle">Network statistics</h1>
  <p className="lead" data-t="statsLead">Anonymous data · refreshed every 20 seconds.</p>
  <div className="tiles stats-overview" aria-live="polite">
   <div className="tile tile-primary"><div className="label" data-t="tHashrate">Total hashrate</div><div className="value accent" id="st-hashrate">–</div><div className="tile-caption" data-t="networkTotal">Across reporting miners</div></div>
   <div className="tile"><div className="label" data-t="tMiners">Miners</div><div className="value" id="st-miners">–</div><div className="tile-caption" data-t="networkMiners">Reporting devices</div></div>
   <div className="tile"><div className="label" data-t="tAverage">Average per miner</div><div className="value" id="st-average">–</div><div className="tile-caption" data-t="networkAverage">Current network average</div></div>
   <div className="tile"><div className="label" data-t="tRecord">Best hour</div><div className="value" id="st-record">–</div><div className="tile-caption" data-t="networkPeak">Peak hashrate</div></div>
  </div>
  <HashrateHistory statistics/>
  <div className="fleet-head">
   <h2 data-t="devicesTitle">Devices</h2>
   <span className="legend"><span className="dot on" aria-hidden="true"></span><span><b id="st-on">–</b> <span data-t="online">online</span></span></span>
   <span className="legend"><span className="dot off" aria-hidden="true"></span><span><b id="st-off">–</b> <span data-t="offline">offline</span></span></span>
  </div>
  <ul className="fleet" id="st-fleet"></ul>
  <button type="button" className="fleet-more" id="st-more" aria-expanded="false" aria-controls="st-fleet" hidden></button>
  <div className="stats-breakdowns">
   <div className="card"><h3 data-t="byBoard">By board</h3><table className="by" id="st-boards"></table></div>
   <div className="card"><h3 data-t="byVersion">By version</h3><table className="by" id="st-versions"></table></div>
   <div className="card"><h3 data-t="coinsTitle">By cryptocurrency</h3><table className="by" id="st-coins"></table></div>
   <div className="card"><h3 data-t="byPool">By pool</h3><table className="by" id="st-pools"></table></div>
  </div>
  <p className="s" id="st-status" role="status" aria-live="polite" data-h="statsMore">Raw data: <a href="https://api.bdl-pool.com:36018/api/summary">JSON</a>.</p>
 </div>
</section>
 </Layout>;
}
