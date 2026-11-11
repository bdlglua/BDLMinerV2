// Shared page frame: navigation, footer and chat. Page content goes in <main>.
import Head from 'next/head';
import type {ReactNode} from 'react';
import Icon from './Icons';

// Start before styles/scripts finish loading; site.js validates age and schema before displaying it.
const statsBootstrap="if(new URLSearchParams(location.search).get('statsApi')!=='go'){try{window.BDL_PUBLIC_STATS_SEED=fetch('https://raw.githubusercontent.com/bdlglua/BDLMinerV2/public-stats/summary.json',{credentials:'omit',cache:'default',signal:AbortSignal.timeout(5000)}).then(r=>r.ok?r.json():null).catch(()=>null)}catch{}}";

interface LayoutProps {
 children: ReactNode;
 flasher?: boolean;
 remote?: boolean;
 chart?: boolean;
}

export default function Layout({children,flasher=false,remote=false,chart=false}:LayoutProps){
 return <>
  <Head>
   <title>__PAGE_TITLE__</title>
   <meta name="viewport" content="width=device-width,initial-scale=1"/>
   {chart&&<link rel="preconnect" href="https://api.bdl-pool.com:36018" crossOrigin="anonymous"/>}
   {chart&&<script id="public-stats-bootstrap" dangerouslySetInnerHTML={{__html:statsBootstrap}}/>}
   {flasher&&<script type="module" src="assets/vendor/esp-web-tools-10.4.0/install-button.js"/>}
   {remote&&<meta name="referrer" content="no-referrer"/>}
   {remote&&<meta name="bdl-remote-api" content="https://api.bdl-pool.com:36018"/>}
   {remote&&<link rel="stylesheet" href="assets/remote.css"/>}
   {remote&&<script defer src="assets/remote.js"/>}
   {chart&&<script defer src="assets/vendor/chart.umd.min.js"/>}
   {chart&&<script defer src="assets/pulse-chart.js"/>}
   {chart&&<script defer src="assets/home-chart.js"/>}
  </Head>

<a className="skip" href="#main" data-t="skip">Skip to content</a>
<nav aria-label="Main">
 <div className="wrap">
  <a className="brand" href="index.html" aria-label="BDLMiner V2">
   <svg width="30" height="30" viewBox="0 0 100 100" aria-hidden="true"><g fill="#f7931a"><rect x="24" y="24" width="52" height="52" rx="10" /><g id="pins"><rect x="31" y="8" width="6" height="12" rx="2" /><rect x="47" y="8" width="6" height="12" rx="2" /><rect x="63" y="8" width="6" height="12" rx="2" /></g><use href="#pins" transform="translate(0 72)" /><use href="#pins" transform="rotate(90 50 50)" /><use href="#pins" transform="rotate(90 50 50) translate(0 72)" /></g><text x="50" y="64" textAnchor="middle" fontSize="42" fontWeight="900" fontFamily="system-ui,sans-serif" fill="#15100a">B</text></svg>
   <span>BDLMiner <b>V2</b></span>
  </a>
  <div className="links" id="site-links">
   <a href="boards.html" data-t="navBoards">Boards</a>
   <a href="flash.html" data-t="navInstall" className="nav-flash">Flash</a>
   <a href="demo.html" data-t="navDemo">Demo</a>
   <a href="https://bdl-pool.com/" id="nav-pool">BDL Pool</a>
   <details className="site-more" id="nav-more"><summary data-t="navMore">Explore</summary><div className="site-more-menu">
    <a href="start.html" id="nav-discover" data-t="navStart">Get started</a>
    <a href="faq.html" data-t="navFAQ">FAQ</a>
    <a href="releases.html" data-t="navNews">Releases</a>
    <a href="stats.html" data-t="navStats">Statistics</a>
    <a href="remote.html" data-t="navRemote">My miners</a>
    <a href="gallery.html" data-t="navCommunity">Community</a>
   </div></details>
   <a href="support.html" id="nav-support" data-t="navSupport">Support</a>
  </div>
  <button id="theme" type="button" aria-pressed="false" data-al="themeDark" aria-label="Dark theme"><svg className="moon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" /></svg><svg className="sun" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true"><circle cx="12" cy="12" r="4" /><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" /></svg></button>
  <button id="lang" type="button">Français</button>
  <button id="menu" type="button" aria-expanded="false" aria-controls="site-links" data-al="menu" aria-label="Menu"><span className="menu-open"><Icon name="menu"/></span><span className="menu-close"><Icon name="close"/></span></button>
 </div>
</nav>

<main id="main" className="page-content">
{children}
</main>

<footer>
 <nav className="footer-nav wrap" aria-label="All pages" data-al="navPages">
  <a href="index.html" data-t="navHome">Home</a><a href="boards.html" data-t="navBoards">Boards</a>
  <a href="flash.html" data-t="navInstall">Flash</a><a href="stats.html" data-t="navStats">Statistics</a>
  <a href="remote.html" data-t="navRemote">My miners</a><a href="https://github.com/bdlglua/BDLMinerV2">GitHub</a>
 </nav>
 <div className="wrap">
  <span data-t="license">BDLMiner V2 is free software under the GNU GPL v3.0.</span>
  <span><a href="https://github.com/bdlglua/BDLMinerV2/blob/main/SECURITY.md" data-t="security">Security</a> · <a href="https://github.com/bdlglua/BDLMinerV2/blob/main/ACCESSIBILITY.md" data-t="accessibility">Accessibility</a> · <a href="https://github.com/bdlglua/BDLMinerV2/releases" data-t="releases">Releases</a></span>
 </div>
</footer>

<button type="button" className="chat-bubble" id="chat-bubble" aria-controls="chat" aria-expanded="false" data-al="chatOpen" aria-label="Open the chat">
 <Icon name="message"/>
 <span className="badge" id="chat-unread" hidden></span>
</button>
<section className="chat" id="chat" role="dialog" aria-labelledby="chat-title" hidden>
 <header>
  <h2 id="chat-title" data-t="chatTitle">Chat</h2>
  <button type="button" className="chat-online" id="chat-people" aria-pressed="false" aria-controls="chat-members" data-al="chatMembers" aria-label="Members online"><span className="dot on" aria-hidden="true"></span><b id="chat-online">–</b> <span data-t="online">online</span> <span className="caret" aria-hidden="true">▾</span></button>
  <button type="button" className="chat-icon" id="chat-gear" aria-pressed="false" aria-controls="chat-settings" data-al="chatSettings" aria-label="Settings">
   <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" /></svg>
  </button>
  <button type="button" className="chat-icon" id="chat-close" data-al="chatClose" aria-label="Close the chat"><Icon name="close"/></button>
 </header>
 <ol className="chat-log" id="chat-log" aria-live="polite" aria-relevant="additions"></ol>
 <div className="chat-settings" id="chat-settings" hidden>
  <h3 data-t="chatSettings">Settings</h3>
  <div className="field">
   <label htmlFor="chat-name" data-t="chatName">Name</label>
   <div className="row"><input id="chat-name" maxLength={24} autoComplete="nickname" data-ph="chatNamePh" placeholder="Your name" />
    <button type="button" className="btn-s" id="chat-save" data-t="chatSave">Save</button></div>
   <p className="hint" id="chat-when" data-t="chatNameHint">Changeable every 30 days</p>
  </div>
  <div className="field">
   <label data-t="chatCodeTitle">Other device</label>
   <div className="row"><button type="button" className="btn-s ghost" id="chat-qr-show" aria-expanded="false" aria-controls="chat-qr" data-t="chatQrShow">My QR code</button></div>
   <div className="chat-qr" id="chat-qr" hidden>
    <div id="chat-qr-img" role="img" data-al="chatQrAlt" aria-label="QR code to open the chat with your name"></div>
    <p className="hint" data-t="chatCodeHint">Scan it with your phone. Show it to no one.</p>
    <button type="button" className="btn-s ghost" id="chat-link" data-t="chatLink">Copy the link</button>
   </div>
  </div>
  <button type="button" className="btn-s" id="chat-back" data-t="chatBack">Back to the chat</button>
 </div>
 <div className="chat-members" id="chat-members" hidden>
  <h3 data-t="chatMembers">Members online</h3>
  <ul id="chat-member-list"></ul>
  <p className="hint" id="chat-guests"></p>
  <button type="button" className="btn-s" id="chat-members-back" data-t="chatBack">Back to the chat</button>
 </div>
 <p className="chat-note" id="chat-note" role="status"></p>
 <form id="chat-form" autoComplete="off">
  <div className="chat-attach" id="chat-attach" hidden><span id="chat-attach-name"></span>
   <button type="button" className="chat-icon" id="chat-attach-drop" data-al="chatRemove" aria-label="Remove"><Icon name="close"/></button></div>
  <div className="compose">
   <button type="button" className="chat-icon" id="chat-file" data-al="chatAttach" aria-label="Photo or video"><Icon name="paperclip"/></button>
   <input type="file" id="chat-file-input" accept="image/*,video/*" hidden />
   <button type="button" className="chat-icon rec" id="chat-rec" aria-pressed="false" data-al="chatRecord" aria-label="Voice message"><Icon name="mic"/></button>
   <label className="skip" htmlFor="chat-text" data-t="chatMessage">Message</label>
   <textarea id="chat-text" rows={1} maxLength={500} data-ph="chatTextPh" placeholder="Message…"></textarea>
   <button type="submit" className="send" id="chat-send" data-al="chatSend" aria-label="Send"><Icon name="arrow"/></button>
  </div>
 </form>
 <p className="chat-rules" data-t="chatRules">Public messages, photos and videos, kept 7 days. Never share a seed phrase, a private key or a password.</p>
</section>
 </>;
}
