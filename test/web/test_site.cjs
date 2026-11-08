// Exercise packaged Pages files, their real interfaces and isolated demo requests.
const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path');
const {spawn}=require('node:child_process');
const {chromium}=require('playwright');
async function main(){
 const child=spawn('python3',[path.join(__dirname,'site_fixture.py')],{stdio:['ignore','pipe','pipe']});let stderr='';child.stderr.on('data',d=>stderr+=d);
 const url=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error('Site fixture did not start: '+stderr)),15000);child.stdout.once('data',d=>{clearTimeout(timer);resolve(d.toString().trim())});child.once('exit',code=>{clearTimeout(timer);reject(Error('Site fixture exited '+code+': '+stderr))})});
 let browser;
 try{
  browser=await chromium.launch({headless:true,executablePath:process.env.BDL_CHROMIUM_PATH||(fs.existsSync('/usr/bin/chromium')?'/usr/bin/chromium':undefined),args:['--no-sandbox']});
  const context=await browser.newContext({viewport:{width:1440,height:1000}}),page=await context.newPage(),errors=[],requests=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>requests.push(r.url()));
  if(process.env.BDL_VERBOSE)page.on('console',message=>{if(message.type()==='error')console.error('BROWSER:',message.text())});
  await page.route('https://**/*',route=>route.fulfill({status:200,contentType:'text/javascript',body:''}));
  let communityStats={at:Date.now(),miners:3,hashrate:2.5e6,coins:[{name:"BTC",miners:2,hashrate:2e6},{name:"BCH",miners:1,hashrate:5e5}],boards:[],versions:[],fleet:[],history:[],recent:[],record:null,
   pools:[{name:'public-pool.io',miners:1,hashrate:1e6},{name:'other',miners:1,hashrate:1e6},{name:'unknown',miners:1,hashrate:5e5}]};
  await page.route('https://bdlminer-stats.bdlminer-bot.workers.dev/api/summary',route=>route.fulfill({status:200,contentType:'application/json',body:JSON.stringify(communityStats)}));
  await page.addInitScript(()=>{window.WebSocket=class{constructor(){this.readyState=1;setTimeout(()=>this.onopen?.(),0)}send(){}close(){}}});
  await page.goto(url);await page.locator('[data-t="ctaInstall"]').waitFor();
  assert.equal(await page.locator('[data-t="ctaInstall"]').textContent(),'Flash');
  assert.equal(await page.locator('#board-comparison,#demo-frame,#faq-search,#support').count(),0,'home must contain only home content');
  assert.equal(await page.locator('main h1').count(),1);
  await page.locator('#site-links a[href="boards.html"]').click();await page.waitForURL(url+'/boards.html');
  await page.waitForSelector('.board-card');
  assert.equal(await page.locator('#board-comparison .board-card').count(),2);
  assert.match(await page.locator('#board-comparison').textContent(),/1\.00 MH\/s/);
  assert.match(await page.locator('#board-comparison').textContent(),/1\.07 TH\/s/);
  await page.locator('[data-filter="s3"]').click();assert.doesNotMatch(await page.locator('.comparison-grid').textContent(),/1\.00 MH\/s/);
  assert.match(await page.locator('.comparison-grid').textContent(),/To be measured/);
  await page.locator('[data-board-env="lilygo-t-display-s3"]').click();await page.waitForURL(url+'/flash.html?board=lilygo-t-display-s3');
  assert.equal(await page.locator('#board').inputValue(),'lilygo-t-display-s3');
  assert.equal(await page.locator('#installer').getAttribute('manifest'),'manifest-lilygo-t-display-s3.json');
  await page.goto(url+'/flash.html?board=../../unknown');
  assert.equal(await page.locator('#board').inputValue(),'lilygo-t-display-s3','invalid profiles must not replace the saved board');
  await page.goto(url+'/start.html');await page.locator('#route-axeos').click();
  assert.match(await page.locator('#route-content').textContent(),/601.*5 V/s);
  await page.locator('#route-content a[href="flash.html?board=bitaxe-gamma-601"]').click();await page.waitForURL(url+'/flash.html?board=bitaxe-gamma-601');
  assert.equal(await page.locator('#board').inputValue(),'bitaxe-gamma-601');
  await page.goto(url+'/faq.html');await page.locator('#faq-search').fill('USB');
  assert.ok(await page.locator('#faq-list details').count()>0);
  await page.locator('#faq-list summary').first().click();assert.equal(await page.locator('#faq-list details').first().getAttribute('open'),'');
  // Case, accents, hyphens and spaces do not matter: "wifi" finds "Wi-Fi".
  await page.locator('#faq-search').fill('wifi');assert.ok(await page.locator('#faq-list details').count()>0,'wifi must find the Wi-Fi answers');
  await page.locator('#faq-search').fill('zzzz-no-match');assert.equal(await page.locator('#faq-empty').isVisible(),true);
  await page.goto(url+'/gallery.html');await page.locator('.gallery-invite').waitFor();
  assert.equal(await page.locator('#gallery .gallery-card').count(),0);
  assert.equal(await page.locator('#gallery a[href*="community_setup.yml"]').count(),1);
  await page.locator('#nav-support').click();await page.waitForURL(url+'/support.html');
  assert.equal(await page.locator('#support a[href="https://github.com/sponsors/bdlglua"]').count(),1);
  await page.locator('#lang').click();assert.equal(await page.locator('#nav-support').textContent(),'Soutenir');
  assert.match(await page.locator('#support').textContent(),/Cartes de test/);
  await page.locator('#site-links a[href="flash.html"]').click();await page.waitForURL(url+'/flash.html');
  assert.equal(await page.locator('#installer [data-t="install"]').textContent(),'Flasher');
  await page.goto(url);assert.equal(await page.locator('[data-t="ctaInstall"]').textContent(),'Flasher');
  await page.goto(url+'/start.html');await page.locator('#route-axeos').click();
  assert.match(await page.locator('#discovery-start').textContent(),/Alimentation régulée 5 V/);
  await page.locator('#site-links a[href="demo.html"]').click();await page.waitForURL(url+'/demo.html');
  const frame=page.frameLocator('#demo-frame');await frame.locator('#m .hero').waitFor();
  assert.match(await frame.locator('.demo-notice').textContent(),/Données fictives/);
  await frame.locator('a[href="settings.html"]').first().click();await frame.locator('#k_power').waitFor();
  await frame.locator('#k_power').fill('50');await frame.locator('#save').click();
  await frame.locator('#msg').filter({hasText:/appliqu|applied/i}).waitFor({timeout:5000}).catch(async error=>{console.error('Demo save status:',await frame.locator('#msg').textContent(),errors,await frame.locator('input:invalid,select:invalid').evaluateAll(nodes=>nodes.map(n=>({id:n.id,value:n.value,message:n.validationMessage}))),await frame.locator('#f').evaluate(f=>({onsubmit:f.getAttribute('onsubmit'),handler:typeof saveSettings,form:document.getElementById('save').form?.id,busy:window.busy})));throw error});
  await frame.locator('a[href="index.html"]').first().click();await frame.locator('#m .hero').filter({hasText:/502/}).waitFor();
  await frame.locator('.demo-notice select').selectOption('pool');
  await frame.locator('#mining-state').filter({hasText:/attente du pool/}).waitFor();
  const demoFrame=page.frames().find(f=>f.url().includes('/demo/miner/'));
  assert.equal(await demoFrame.evaluate(async()=>{try{await fetch('http://192.168.1.1/api/settings',{method:'POST',body:'{}'});return false}catch{return true}}),true);
  await page.locator('#demo-tab-hub').click();const hub=page.frameLocator('#demo-frame');await hub.locator('.miner').first().waitFor();
  assert.equal(await hub.locator('.miner').count(),3);assert.equal(await hub.locator('.miner.offline').count(),1);
  const gamma=hub.locator('[data-id="gamma"]');await gamma.locator('input').fill('50');await gamma.getByRole('button',{name:'Appliquer',exact:true}).click();
  await gamma.locator('.result').filter({hasText:/Commande envoyée/}).waitFor();
  assert.match(await gamma.locator('.rate').textContent(),/535 G/);
  await gamma.getByRole('button',{name:'Diagnostiquer le pool'}).click();await gamma.locator('.result').filter({hasText:/Stratum V2/}).waitFor();
  const gammaPage=await context.newPage();gammaPage.on('pageerror',e=>errors.push(e.message));gammaPage.on('request',r=>requests.push(r.url()));
  await gammaPage.goto(await gamma.locator('a').first().getAttribute('href'));
  await gammaPage.locator('#m .hero').filter({hasText:/TH\/s/}).waitFor();
  assert.match(await gammaPage.locator('#device-grid').textContent(),/BM1370|525 MHz/);
  await gammaPage.close();
  assert.equal(requests.some(u=>new URL(u).origin===url&&new URL(u).pathname.startsWith('/api/')),false,'demo calls must stay in memory');
  assert.equal(requests.some(u=>/^http:\/\/192\./.test(u)),false,'the demo must never contact a LAN miner');
  const pages={home:'index.html',start:'start.html',boards:'boards.html',flash:'flash.html',demo:'demo.html',faq:'faq.html',releases:'releases.html',stats:'stats.html',gallery:'gallery.html',support:'support.html',remote:'remote.html'};
  for(const prefix of ['', '/beta', '/BDLMinerV2', '/BDLMinerV2/beta']){
   const base=url+prefix;
   for(const [key,name]of Object.entries(pages)){
    const response=await page.request.get(base+'/'+name);assert.equal(response.status(),200,name+' must be a real page');
    const html=await response.text();assert.ok(html.includes('data-page="'+key+'"'));assert.ok(!html.includes('__PAGE_CONTENT__'));
    assert.equal(html.includes('src="https://unpkg.com/esp-web-tools@'),key==='flash','load the USB tool only on its own page');
   }
   await page.goto(base+'/stats.html');await page.locator('#st-pools tbody tr').first().waitFor();
   assert.match(await page.locator('#st-coins').textContent(),/BTC.*BCH/s);
   assert.match(await page.locator('#st-pools').textContent(),/Public Pool.*Autre \/ pool privée.*Pool non renseignée/s);
   assert.equal(await page.locator('[data-t="poolsHelp"]').count(),0);
   const breakdownStyle=await page.evaluate(()=>{
    const board=document.querySelector('#st-boards').closest('.card'),pool=document.querySelector('#st-pools').closest('.card');
    const heading=card=>{const s=getComputedStyle(card.querySelector('h3'));return[s.fontSize,s.fontWeight,s.color,s.textTransform]};
    return{gap:pool.getBoundingClientRect().top-board.parentElement.getBoundingClientRect().bottom,board:heading(board),pool:heading(pool)};
   });
   assert.ok(breakdownStyle.gap>=12,'separate the pool card from the other breakdowns');
   assert.deepEqual(breakdownStyle.pool,breakdownStyle.board,'all breakdown headings should have the same appearance');
   assert.equal(await page.locator('#st-pools a').count(),1);
   assert.equal(await page.locator('#st-pools a').getAttribute('href'),'https://web.public-pool.io/');
   assert.equal(await page.locator('#st-pools .pool-site').textContent(),'https://web.public-pool.io/');
   assert.equal(await page.locator('#st-pools a').getAttribute('rel'),'noopener noreferrer');
   await page.goto(base+'/#support');await page.waitForURL(base+'/support.html');await page.locator('.support-panel').waitFor();
   assert.equal(await page.locator('main h1').count(),1);
   await page.goto(base+'/releases.html');await page.locator('.release-card').first().waitFor();
   assert.equal(await page.locator('#discovery-news a[href="'+(prefix.endsWith('/beta')?'../':'./')+'flash.html"]').count(),1);
   assert.equal(await page.locator('#discovery-news a[href="'+(prefix.endsWith('/beta')?'./':'beta/')+'flash.html"]').count(),1);
   await page.goto(base+'/boards.html');await page.locator('[data-filter="s3"]').click();
   await page.locator('[data-board-env="lilygo-t-display-s3"]').click();await page.waitForURL(base+'/flash.html?board=lilygo-t-display-s3');
   assert.equal(await page.locator('#board').inputValue(),'lilygo-t-display-s3');
   await page.goto(base+'/#demo');await page.waitForURL(base+'/demo.html');await page.frameLocator('#demo-frame').locator('#m .hero').waitFor();
   assert.ok(page.frames().some(frame=>frame.url().startsWith(base+'/demo/miner/')));
  }
  // Public period statistics: anonymous totals, not private miner telemetry.
  const originalStats=structuredClone(communityStats);
  const stamp=Date.now(),measured=[.9,1,1.1,1.1,1,.9].map((value,i)=>({at:stamp-(30-i*5)*60000,hashrate:value*1e12,miners:6+i%3,average_miners:6+i%3,samples:1,interval_ms:300000}));
  const period={from:stamp-3600000,to:stamp,samples:6,expected_samples:12,average_hashrate:1e12,minimum_hashrate:.9e12,maximum_hashrate:1.1e12,variation_percent:8.16496580927726,average_miners:7,minimum_miners:6,maximum_miners:8,first_at:measured[0].at,last_at:measured.at(-1).at};
  Object.assign(communityStats,{at:stamp,hashrate:1.05e12,miners:7,recent:[...measured,{at:stamp,hashrate:1.05e12,miners:7,live:true}],history:[],periods:{1:period,6:{...period,expected_samples:72},24:{...period,expected_samples:288},168:{...period,expected_samples:2016}}});
  await page.goto(url+'/stats.html');await page.locator('#st-period-average').filter({hasText:'1 TH/s'}).waitFor();
  await page.locator('[data-range=hour]').click();assert.equal(await page.locator('#st-export').count(),0);
  assert.equal(await page.locator('#st-period-maximum').textContent(),'1,1 TH/s');assert.match(await page.locator('#st-period-minimum').textContent(),/900 GH\/s/);
  assert.equal(await page.locator('#st-period-miners').textContent(),'7');assert.equal(await page.locator('#st-period-samples').textContent(),'6');assert.match(await page.locator('#st-period-coverage').textContent(),/50 %/);
  assert.equal(await page.locator('#st-hashrate').textContent(),'1,05 TH/s','keep the live value separate from the period average');
  const origin=await page.locator('#st-chart').evaluate(svg=>({grid:Number(svg.querySelector('.grid-h').getAttribute('x1')),line:Number(svg.querySelector('.line').getAttribute('d').match(/^M([\d.]+)/)[1])}));
  assert.ok(Math.abs(origin.grid-origin.line)<1,'the first observed measurement starts at the plot origin');
  await page.locator('#st-chart').focus();await page.keyboard.press('Home');assert.equal(await page.locator('#st-tooltip').isVisible(),true);assert.match(await page.locator('#st-tooltip strong').textContent(),/900 GH\/s/);
  await page.locator('[data-st-metric=miners]').click();await page.locator('#st-chart').focus();await page.keyboard.press('End');assert.match(await page.locator('#st-tooltip strong').textContent(),/^7 miners/);
  await page.locator('[data-st-metric=hashrate]').click();await page.locator('#st-chart').scrollIntoViewIfNeeded();const chartBox=await page.locator('#st-chart').boundingBox();await page.mouse.move(chartBox.x+chartBox.width/2,chartBox.y+60);assert.equal(await page.locator('#st-tooltip').isVisible(),true);
  await page.locator('.public-readings summary').click();assert.equal(await page.locator('#st-readings tr').count(),7);
  await page.locator('#theme').click();assert.equal(await page.locator('html').getAttribute('data-theme'),'light');await page.locator('#theme').click();
  if(process.env.BDL_SCREENSHOT_DIR){
   fs.mkdirSync(process.env.BDL_SCREENSHOT_DIR,{recursive:true});await page.locator('#st-performance').screenshot({path:path.join(process.env.BDL_SCREENSHOT_DIR,'public-statistics-desktop.png'),style:'nav,#chat-bubble{visibility:hidden}'});
   await page.setViewportSize({width:375,height:812});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);await page.locator('#st-performance').screenshot({path:path.join(process.env.BDL_SCREENSHOT_DIR,'public-statistics-mobile.png'),style:'nav,#chat-bubble{visibility:hidden}'});await page.setViewportSize({width:1440,height:1000});
  }
  communityStats.history=[{at:stamp-2*3600000,end_at:stamp-3600000,hashrate:1e12,miners:7,samples:12,interval_ms:3600000},{at:stamp-3600000,end_at:stamp-600000,hashrate:1.1e12,miners:8,samples:10,missing:true,interval_ms:3600000}];
  await page.goto(url+'/stats.html');await page.locator('#st-period-average').filter({hasText:'1 TH/s'}).waitFor();await page.locator('[data-range=week]').click();assert.equal(await page.locator('[data-range=week]').getAttribute('aria-pressed'),'true');assert.match(await page.locator('#st-period-coverage').textContent(),/2.?016/);
  assert.equal(await page.locator('#st-chart .line').count(),0,'do not join hourly averages across a partially missing hour');
  communityStats.periods={24:{samples:0,expected_samples:288}};communityStats.recent=[];communityStats.history=[];
  await page.goto(url+'/stats.html');await page.locator('#st-chart-empty').filter({hasText:/Aucune mesure/}).waitFor();assert.equal(await page.locator('#st-export').count(),0);assert.equal(await page.locator('#st-period-average').textContent(),'—');
  communityStats=originalStats;
  communityStats.coins=[{name:'BSV',miners:1,hashrate:1e6},{name:'XEC',miners:1,hashrate:1e6},{name:'DGB',miners:1,hashrate:5e5},{name:'PPC',miners:1,hashrate:1e6}];
  await page.goto(url+'/stats.html');await page.locator('#st-coins tbody tr').first().waitFor();
  assert.equal(await page.locator('#st-coins tbody tr').count(),4);
  assert.match(await page.locator('#st-coins').textContent(),/BSV.*XEC.*DGB.*PPC/s);
  delete communityStats.pools;await page.goto(url+'/stats.html');
  await page.locator('#st-pools tbody tr').first().waitFor();
  assert.equal(await page.locator('#st-pools tbody tr').count(),1);
  assert.match(await page.locator('#st-pools').textContent(),/Pool non renseignée.*3.*2,5 MH\/s/s);
  assert.equal(await page.locator('#st-pools a').count(),0,'undisclosed pools must have no link');
  await page.locator('#lang').click();assert.match(await page.locator('#st-pools').textContent(),/Not shared/);
  communityStats.pools=[{name:'<img src=x onerror=alert(1)>',miners:3,hashrate:2.5e6}];
  await page.goto(url+'/stats.html');await page.locator('#st-pools tbody tr').first().waitFor();
  assert.match(await page.locator('#st-pools').textContent(),/Other \/ private pool/);
  assert.equal(await page.locator('#st-pools img').count(),0);
  assert.equal(await page.locator('#st-pools a').count(),0,'unrecognised names must never become links');
  const poolSites=JSON.parse(fs.readFileSync(path.join(__dirname,'../../installer/content.json'))).pool_links;
  communityStats.pools=Object.keys(poolSites).map(name=>({name,miners:1,hashrate:1e6,url:'https://attacker.example/worker'}));
  communityStats.miners=communityStats.pools.length;communityStats.hashrate=communityStats.miners*1e6;
  await page.goto(url+'/stats.html');await page.locator('#st-pools tbody tr').first().waitFor();
  assert.equal(await page.locator('#st-pools a').count(),Object.keys(poolSites).length);
  for(const destination of Object.values(poolSites))assert.equal(await page.locator('#st-pools a[href="'+destination+'"]').count(),1);
  assert.doesNotMatch(await page.locator('#st-pools').innerHTML(),/attacker\.example/);
  // The image credits are full of unbreakable revision hashes and long links.
  await page.setViewportSize({width:375,height:812});await page.goto(url+'/assets/boards/CREDITS.html');
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true,'CREDITS.html should fit mobile');
  for(const name of Object.values(pages)){
   await page.setViewportSize({width:375,height:812});await page.goto(url+'/'+name);
   await page.locator('main h1').first().waitFor();
   assert.equal(await page.locator('main h1').count(),1,name+' should have one main heading');
   assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true,name+' should fit mobile');
  }
  await page.locator('#menu').click();await page.locator('#nav-more summary').click();
  await page.locator('#nav-more a[href="faq.html"]').click();await page.waitForURL(url+'/faq.html');
  assert.equal(await page.locator('#site-links').evaluate(e=>e.classList.contains('open')),false);
  await page.goto(url+'/beta/flash.html?board=esp32c3-headless');
  assert.equal(await page.locator('#board').inputValue(),'esp32c3-headless');
  const manifest=await (await page.request.get(url+'/beta/'+await page.locator('#installer').getAttribute('manifest'))).json();
  assert.equal(manifest.version,'2.0.2-beta.1');
  // "My miners" is a site page: shared navigation, site language and theme, and
  // readings fetched from the statistics service by payout address only.
  await page.setViewportSize({width:1440,height:1000});
  const remoteAPI='https://bdlminer-stats.bdlminer-bot.workers.dev',remoteCalls=[],now=Date.now();
  const remoteMiner={id:'0123456789abcdef',name:'M5 bureau',board:'m5stickc-plus2',v:'2.5.4',coin:'BTC',online:true,age_s:42,hashrate:1.2e6,temp_c:47.5,accepted:12,rejected:1,uptime_s:7200,power_percent:100,best:4096,pool_connected:true,pool:'public-pool.io'};
  await page.route(remoteAPI+'/api/remote**',async route=>{
   const request=route.request(),target=new URL(request.url());remoteCalls.push({path:target.pathname,auth:request.headers().authorization,referer:request.headers().referer});
   if(target.pathname==='/api/remote/history')return route.fulfill({status:200,contentType:'application/json',headers:{'Access-Control-Allow-Origin':'*'},body:JSON.stringify({interval_ms:300000,window_start:now-86400000,window_end:now,
    points:[0,1,2,3].map(i=>({t:now-(4-i)*300000,rate:1.1e6+i*5e4,temp:46+i/2,samples:1,boot:'aabbccdd'})),
    summary:{average:1.2e6,minimum:1.1e6,maximum:1.25e6,variation_percent:4,temperature_average:46.8,temperature_maximum:47.5,reports:4,restarts:0,first_at:now-1200000,last_at:now-300000}})});
   return route.fulfill({status:200,contentType:'application/json',headers:{'Access-Control-Allow-Origin':'*'},body:JSON.stringify({hashrate:1.2e6,recorded_at:now,miners:[remoteMiner]})});
  });
  await page.goto(url+'/remote.html');await page.evaluate(()=>{localStorage.setItem('lang','en');localStorage.removeItem('bdl-remote-address')});await page.reload();
  await page.locator('#rm-login-form').waitFor();
  assert.equal(await page.locator('body').getAttribute('data-page'),'remote');
  assert.equal(await page.locator('#nav-more a[href="remote.html"]').count(),1,'My miners is in the shared navigation');
  assert.equal(await page.locator('meta[name="referrer"]').getAttribute('content'),'no-referrer');
  assert.equal(await page.locator('#rm-login h2').textContent(),'See your miners');
  await page.locator('#rm-key').fill('bc1qremotetest.worker1');await page.locator('#rm-login-form button[type=submit]').click();
  await page.locator('#rm-miners .miner h2').filter({hasText:'M5 bureau'}).waitFor();
  await page.locator('#rm-stat-reports').filter({hasText:'4'}).waitFor();
  assert.ok(remoteCalls.length>=2&&remoteCalls.every(c=>c.auth==='Address bc1qremotetest'),'the payout address (without worker) is the only credential');
  assert.ok(remoteCalls.every(c=>!c.referer),'no referrer is sent to the statistics service');
  assert.equal(await page.locator('#rm-login').isVisible(),false);assert.equal(await page.locator('#rm-logout').isVisible(),true);
  assert.equal(await page.locator('#rm-manage').isVisible(),false,'an address space cannot be erased');
  assert.match(await page.locator('#rm-total').textContent(),/1\.2 MH\/s/);
  await page.locator('#lang').click();await page.locator('#rm-miners .state').filter({hasText:'En ligne'}).waitFor();
  assert.equal(await page.locator('#rm-login h2').textContent(),'Voir vos mineurs');
  assert.equal(await page.locator('#rm-chart').evaluate(c=>c.width>0),true);
  await page.locator('#theme').click();assert.equal(await page.locator('html').getAttribute('data-theme'),'light');
  assert.notEqual(await page.locator('.remote-panel.remote-performance').evaluate(e=>getComputedStyle(e).backgroundColor),'rgb(28, 29, 34)','the page follows the light theme');
  await page.locator('#theme').click();
  await page.locator('#rm-logout').click();await page.locator('#rm-login').waitFor();
  assert.equal(await page.evaluate(()=>localStorage.getItem('bdl-remote-address')),null);
  await page.goto(url+'/remote/');await page.waitForURL(url+'/remote.html');
  await page.goto(url+'/beta/remote/');await page.waitForURL(url+'/beta/remote.html');
  await page.unroute(remoteAPI+'/api/remote**');
  assert.deepEqual(errors,[],'uncaught site/demo browser errors');
  if(process.env.BDL_SCREENSHOT_DIR){fs.mkdirSync(process.env.BDL_SCREENSHOT_DIR,{recursive:true});await page.setViewportSize({width:1440,height:1000});await page.goto(url);await page.locator('[data-t=ctaInstall]').waitFor();await page.screenshot({path:path.join(process.env.BDL_SCREENSHOT_DIR,'site-discovery.png'),fullPage:true})}
  console.log('PASS site: separate pages, Flasher label, shared navigation, board transfer, old-link redirects, both hosting prefixes/channels, mobile layouts and isolated interactive demos');
 }finally{if(browser)await browser.close();child.kill('SIGTERM')}
}
main().catch(error=>{console.error(error);process.exitCode=1});
