// Both public pages use the same chart and timestamped community measurements.
export default function HashrateHistory({statistics=false}:{statistics?:boolean}){
 return <article className="pulse-history-card" id={statistics?'st-performance':undefined}>
  <div className="pulse-history-heading">
   <h2 data-t="hashHistoryTitle">Hashrate history</h2>
   {statistics?<div className="pulse-history-status">
    <span className="live" id="st-live"><span className="dot none" aria-hidden="true"></span><span id="st-live-label" data-t="statsLoading">Loading statistics…</span></span>
    <span className="s" id="st-updated" aria-live="off"></span>
   </div>:<a href="stats.html" data-t="marketAll">Full statistics</a>}
  </div>
  <div className="pulse-chart-frame"><canvas id="cryptoChart" role="img" tabIndex={0} aria-label="Hashrate over the last 24 hours. Use left and right arrows to inspect measurements." aria-describedby="home-history-caption" hidden></canvas><p id="home-history-empty" className="pulse-chart-empty">Waiting for hashrate measurements.</p></div>
  <p id="home-history-caption" className="pulse-chart-caption"></p>
  <p id="home-history-inspect" className="sr-only" role="status"></p>
 </article>;
}
