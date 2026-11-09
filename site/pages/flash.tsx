import Layout from '../components/Layout';

export const config:StaticExportPageConfig={unstable_runtimeJS:false};

export default function FlashPage(){
 return <Layout flasher>
<section id="install">
 <div className="wrap">
  <h1 data-t="installTitle">Flash your board</h1>
  <p className="lead" data-t="installLead">No software to install: Chrome or Edge on a computer flashes the board over USB.</p>
  <p className="lead">Open-source firmware · <a href="https://github.com/bdlglua/BDLMinerV2">View BDLMinerV2 on GitHub ↗</a> · <a href="https://github.com/bdlglua/BDLMinerV2/releases/tag/v2.5.4">Stable v2.5.4</a></p>
  <div className="install">
   <div className="card">
    <label htmlFor="board" data-t="board">Your board</label>
    <select id="board"></select>
    <esp-web-install-button id="installer">
     <button slot="activate" className="btn" data-t="install">Flash</button>
     <span slot="unsupported" data-t="unsupported">Your browser cannot install: use Chrome or Edge on a computer.</span>
     <span slot="not-allowed" data-t="notallowed">The page must be opened over HTTPS.</span>
    </esp-web-install-button>
    <p className="s" data-t="erase">Version __VERSION__. The first install erases the board (settings and statistics).</p>
   </div>
   <div className="card">
    <ol>
     <li data-t="s1">Plug the board into this computer with a USB cable that carries data.</li>
     <li data-t="s2">Pick the board, click Flash and choose its serial port.</li>
     <li data-t="s3">When it restarts, join the WiFi network BDLMinerV2-XXXX (password MineYourCoins) and enter your WiFi, pool and BTC address.</li>
    </ol>
    <p className="s" data-h="after">Then open the dashboard at <b>http://bdlminer-xxxx.local</b>. Updates arrive by themselves.</p>
   </div>
  </div>
 </div>
</section>
 </Layout>;
}
