// One document for every page. The __PLACEHOLDERS__ are filled per release
// channel by tools/make_site_pages.py; the pages ship no Next.js runtime.
import {Html,Head,Main,NextScript} from 'next/document';

const description='Open-source Bitcoin solo mining firmware for supported ESP32 and Bitaxe boards.';
const themeBoot="try{if(localStorage.getItem('theme')==='dark')document.documentElement.dataset.theme='dark'}catch(e){}";

export default function Document(){
 return <Html lang="en">
  <Head>
   <meta name="description" content={description}/>
   <meta property="og:title" content="BDLMinerV2 | Bitcoin solo mining firmware"/>
   <meta property="og:description" content="Open-source Bitcoin solo mining firmware for supported ESP32 and Bitaxe boards."/>
   <meta property="og:image" content="assets/screen-mining.png"/>
   <meta name="theme-color" content="#f4f3ef"/>
   <link rel="icon" href="assets/icon.svg" type="image/svg+xml"/>
   <script dangerouslySetInnerHTML={{__html:themeBoot}}/>
   <link rel="stylesheet" href="assets/site.css"/>
   <link rel="stylesheet" href="assets/discovery.css"/>
   <script id="site-data" type="application/json" dangerouslySetInnerHTML={{__html:'__SITE_DATA__'}}/>
   <script dangerouslySetInnerHTML={{__html:"window.BDL_BOARDS=__BOARDS__;window.BDL_VERSION='__VERSION__';"}}/>
   <script defer src="assets/site.js"/>
   <script defer src="assets/discovery.js"/>
  </Head>
  <body data-page="__PAGE_KEY__"><Main/><NextScript/></body>
 </Html>;
}
