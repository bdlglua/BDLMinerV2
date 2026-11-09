import Layout from '../components/Layout';

export const config:StaticExportPageConfig={unstable_runtimeJS:false};

export default function GalleryPage(){
 return <Layout>
<section id="gallery" className="discovery-section"><div className="wrap" id="discovery-gallery"></div></section>

<section id="community">
 <div className="wrap">
  <h2 data-t="communityTitle">Community</h2>
  <p className="lead" data-t="communityLead">Questions, setups, best shares and ideas: come and say hello.</p>
  <div className="grid">
   <a className="card" href="https://discord.gg/S8kSgD6Xe9" style={{"textDecoration":"none","color":"inherit"}}><div className="icon" aria-hidden="true">💬</div><h3>Discord</h3><p data-t="cDiscord">Live help, announcements and the support tickets.</p></a>
   <a className="card" href="https://www.reddit.com/r/bdlminer/" style={{"textDecoration":"none","color":"inherit"}}><div className="icon" aria-hidden="true">🟠</div><h3>Reddit</h3><p data-t="cReddit">r/bdlminer: show your setup and share your records.</p></a>
   <a className="card" href="https://github.com/bdlglua/BDLMinerV2" style={{"textDecoration":"none","color":"inherit"}}><div className="icon" aria-hidden="true">🐙</div><h3>GitHub</h3><p data-t="cGithub">Source code, documentation, releases, bug reports and ideas.</p></a>
  </div>
 </div>
</section>
 </Layout>;
}
