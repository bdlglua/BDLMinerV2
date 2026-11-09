import Layout from '../components/Layout';

export const config:StaticExportPageConfig={unstable_runtimeJS:false};

export default function StatsPage(){
 return <Layout>
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
  <div className="card public-performance" id="st-performance">
   <div className="performance-head"><div><p className="performance-kicker" data-t="sPerformance">COMMUNITY PERFORMANCE</p><h2 data-t="sHistory">Mining statistics</h2><p className="s performance-dates" id="st-period-dates"></p></div>
    <div className="performance-actions"><div className="range" role="group" aria-label="Period" data-al="period"><button type="button" data-range="hour" aria-pressed="false">1 h</button><button type="button" data-range="six" aria-pressed="false">6 h</button><button type="button" data-range="day" aria-pressed="true">24 h</button><button type="button" data-range="week" aria-pressed="false" data-t="week">7 days</button></div></div>
   </div>
   <div className="chart-head"><div className="metric-tabs" role="group" aria-label="Chart metric" data-al="sMetric"><button type="button" data-st-metric="hashrate" aria-pressed="true" data-t="sHashrate">Hashrate</button><button type="button" data-st-metric="miners" aria-pressed="false" data-t="tMiners">Miners</button></div><span className="period-legend" data-t="sMean">Period average</span><span className="live" id="st-live"><span className="dot on" aria-hidden="true"></span><span id="st-live-label" data-t="live">Live</span></span><span className="s" id="st-updated" aria-live="off"></span></div>
   <div className="public-chart-wrap"><svg className="chart" id="st-chart" viewBox="0 0 960 320" role="img" tabIndex={0} aria-label="Mining history. Use left and right arrows to inspect measurements." aria-describedby="st-chart-note"></svg><div id="st-tooltip" className="public-chart-tooltip" role="status" hidden><time></time><strong></strong><small></small></div><p id="st-chart-empty" className="public-chart-empty" hidden></p></div>
   <div className="period-statistics" aria-live="polite">
    <div><span data-t="sAverage">Average hashrate</span><strong id="st-period-average">—</strong><small id="st-period-variation">—</small></div>
    <div><span data-t="sMaximum">Peak hashrate</span><strong id="st-period-maximum">—</strong><small id="st-period-minimum">—</small></div>
    <div><span data-t="sMiners">Average active miners</span><strong id="st-period-miners">—</strong><small id="st-period-peak-miners">—</small></div>
    <div><span data-t="sSamples">Available measurements</span><strong id="st-period-samples">—</strong><small id="st-period-coverage">—</small></div>
   </div>
   <p className="s chart-note" id="st-chart-note" data-t="sNote">Statistics use five-minute aggregate measurements. Missing periods are left empty; the live reading is excluded from period averages.</p>
   <details className="public-readings"><summary data-t="sRecent">Recent measurements</summary><div className="stats-table-scroll"><table><thead><tr><th data-t="sTime">Time</th><th data-t="sAverage">Average hashrate</th><th data-t="sMiners">Average active miners</th><th data-t="sSourceSamples">Source measurements</th></tr></thead><tbody id="st-readings"></tbody></table></div></details>
  </div>
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
  <p className="s" id="st-status" data-h="statsMore">Raw data: <a href="https://bdlminer-stats.bdlminer-bot.workers.dev/api/summary">JSON</a> · <a href="https://bdlminer-stats.bdlminer-bot.workers.dev/">statistics page</a>.</p>
 </div>
</section>
 </Layout>;
}
