import Layout from '../components/Layout';

export const config={unstable_runtimeJS:false};

// "My miners": private remote monitoring by payout address (or a pre-2.5.4
// access key). Readings come from the statistics service; the page shows them
// only and can never change a miner. Texts in data-i are translated by
// assets/remote.js, which follows the site's language and theme.
const GUIDE='https://github.com/bdlglua/BDLMinerV2/blob/main/docs/remote-access.md';

export default function RemotePage(){
 return <Layout remote>
<section id="remote" className="remote">
 <div className="wrap">
  <div className="remote-intro">
   <div>
    <p className="remote-pill" data-i="Remote monitoring">Remote monitoring</p>
    <h1 data-i="Your miners, wherever you are.">Your miners, wherever you are.</h1>
    <p className="lead" data-i="Each board sends its readings directly, every five minutes. No computer needs to stay on at home.">Each board sends its readings directly, every five minutes. No computer needs to stay on at home.</p>
   </div>
   <div className="remote-total"><span data-i="Online hashrate">Online hashrate</span><strong id="rm-total">—</strong><small id="rm-updated"></small>
    <button type="button" id="rm-logout" className="remote-logout" hidden data-i="Sign out">Sign out</button></div>
  </div>

  <div id="rm-login" className="remote-panel">
   <h2 data-i="See your miners">See your miners</h2>
   <p className="remote-muted" data-i="Enter the payout address your miners use. It is the one in Settings → Pool on each board.">Enter the payout address your miners use. It is the one in Settings → Pool on each board.</p>
   <form id="rm-login-form">
    <label htmlFor="rm-key" data-i="Wallet address">Wallet address</label>
    <div className="remote-row"><input id="rm-key" type="text" required maxLength="160" autoComplete="off" autoCapitalize="off" spellCheck="false" placeholder="bc1q…"/><button type="submit" className="remote-primary" data-i="View my miners">View my miners</button></div>
    <label className="remote-check"><input id="rm-remember" type="checkbox" defaultChecked/><span data-i="Remember this address on this device">Remember this address on this device</span></label>
   </form>
   <p className="remote-note" data-i="Anyone who knows this address can see these readings, as on your pool's page. Nothing here can change a miner. A private access key created before BDLMiner 2.5.4 also works in this field.">Anyone who knows this address can see these readings, as on your pool&apos;s page. Nothing here can change a miner. A private access key created before BDLMiner 2.5.4 also works in this field.</p>
  </div>

  <p id="rm-notice" className="remote-notice" role="status" aria-live="polite"></p>
  <div id="rm-miners" className="remote-miners" aria-live="polite"></div>

  <div id="rm-empty" className="remote-panel" hidden>
   <h2 data-i="Waiting for your first miner">Waiting for your first miner</h2>
   <p data-i="On each miner (BDLMiner 2.5.4 or newer), open Settings → Network → Private remote monitoring, enable it, then save and restart. The miner uses its payout address: nothing to copy. The first report arrives after about one minute.">On each miner (BDLMiner 2.5.4 or newer), open Settings → Network → Private remote monitoring, enable it, then save and restart. The miner uses its payout address: nothing to copy. The first report arrives after about one minute.</p>
   <a href={GUIDE} data-i="Setup guide">Setup guide</a>
  </div>

  <div id="rm-history" className="remote-panel remote-performance" hidden>
   <div className="remote-heading"><div><p className="remote-eyebrow" data-i="Performance">Performance</p><h2 data-i="Mining statistics">Mining statistics</h2><p id="rm-history-range" className="remote-muted"></p></div>
    <div className="remote-actions"><select id="rm-history-miner" aria-label="Miner"></select><select id="rm-hours" aria-label="History window" defaultValue="24"><option value="1">1 h</option><option value="6">6 h</option><option value="24">24 h</option><option value="168">7 d</option></select><button id="rm-export-history" type="button" disabled data-i="Export CSV">Export CSV</button></div></div>
   <div className="remote-statistics" aria-live="polite">
    <div className="remote-stat"><span data-i="Average hashrate">Average hashrate</span><strong id="rm-stat-average">—</strong><small id="rm-stat-variation">—</small></div>
    <div className="remote-stat"><span data-i="Peak hashrate">Peak hashrate</span><strong id="rm-stat-maximum">—</strong><small id="rm-stat-minimum">—</small></div>
    <div className="remote-stat"><span data-i="Average temperature">Average temperature</span><strong id="rm-stat-temperature">—</strong><small id="rm-stat-temperature-max">—</small></div>
    <div className="remote-stat"><span data-i="Reports received">Reports received</span><strong id="rm-stat-reports">—</strong><small id="rm-stat-restarts">—</small></div>
   </div>
   <div className="remote-chart-toolbar"><div className="remote-metric-switch" role="group" aria-label="Chart metric"><button type="button" data-metric="rate" aria-pressed="true" data-i="Hashrate">Hashrate</button><button type="button" data-metric="temp" aria-pressed="false" data-i="Temperature">Temperature</button></div><span className="remote-average-legend" data-i="Period average">Period average</span></div>
   <div className="remote-chart-wrap"><canvas id="rm-chart" role="img" tabIndex="0" aria-label="Mining history. Use left and right arrows to inspect readings." aria-describedby="rm-history-note"></canvas><div id="rm-chart-tooltip" className="remote-chart-tooltip" role="status" hidden><time></time><strong></strong><small></small></div><p id="rm-chart-empty" className="remote-chart-empty" hidden></p></div>
   <p id="rm-history-note" className="remote-muted"></p>
   <details className="remote-readings"><summary data-i="Recent readings">Recent readings</summary><div className="remote-table-scroll"><table><thead><tr><th data-i="Time">Time</th><th data-i="Average hashrate">Average hashrate</th><th data-i="Temperature">Temperature</th><th data-i="Reports received">Reports received</th></tr></thead><tbody id="rm-history-rows"></tbody></table></div></details>
  </div>

  <details id="rm-manage" className="remote-panel" hidden><summary data-i="Manage my private data">Manage my private data</summary><p data-i="Revoke this key and permanently erase its readings and history. All paired miners must then use a new key.">Revoke this key and permanently erase its readings and history. All paired miners must then use a new key.</p><button id="rm-revoke" type="button" className="remote-danger" data-i="Erase data and revoke key">Erase data and revoke key</button></details>

  <div className="remote-foot"><p data-i="Reports every five minutes; history kept for seven days.">Reports every five minutes; history kept for seven days.</p><p data-i="Readings only: this page cannot change settings, restart a miner or install firmware.">Readings only: this page cannot change settings, restart a miner or install firmware.</p><a href={GUIDE} data-i="Setup and privacy">Setup and privacy</a></div>
 </div>
</section>
 </Layout>;
}
