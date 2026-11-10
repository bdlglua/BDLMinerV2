// Exercise the actual HTML/JavaScript embedded in the firmware with mocked APIs.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const { chromium } = require('playwright');

const zlib=require('node:zlib');
const webRoot=path.join(__dirname,'../../web');
const read=name=>fs.readFileSync(path.join(webRoot,name));
const clone = value => JSON.parse(JSON.stringify(value));
const baseStats = {
  board:'Test miner', firmware:'1.0.10', uptime_s:0, hashrate:0, total_hashes:0,
  engines:[{name:'hw',rate:0}], shares:{accepted:0,rejected:0}, best:{session:0,ever:0},
  blocks:0, over32bits:0, over64bits:0, top:[], pool:{host:'pool.example',port:3333,connected:true,status:'Mining',difficulty:1},
  pool_view:{updated_s:-1,supported:true}, network:{height:1,difficulty:1e14},
  address:'address.worker', address_type:'P2WPKH', lifetime:{hashes:0,shares:0,blocks:0,seconds:0},
  history:{hashrate:[0,0],interval_s:300}, heap:100000, temp_c:45, throttle:'reduced',
  power_percent:75, night:false, boot_reason:'power on', accent:'#f7931a',
  ip:'192.168.1.20', ssid:'WiFi',rssi:-45, peers:[], market:{updated_s:-1}, github_update:{available:false}
};
const settings = {
  capabilities:{user_agents:['NerdMinerV2','BDLMinerV2/2.1.1']},
  schema:[
    ['pool','text','Pool','Pool host',{}], ['port','number','Pool','Pool port',{min:1,max:65535}],
    ['addr','text','Pool','BTC address',{}], ['diff','decimal','Pool','Suggested difficulty',{min:0}],
    ['proto','number','Pool','Main pool protocol',{min:0,max:1}],['sv2key','text','Pool','Main SV2 authority key',{}],
    ['uaauto','bool','Pool','User agent mode',{}],['ua','text','Pool','Manual user agent',{}],
    ['pool2','text','Pool','Backup pool host',{}],['port2','number','Pool','Backup pool port',{min:1,max:65535}],
    ['proto2','number','Pool','Backup pool protocol',{min:0,max:1}],['sv2key2','text','Pool','Backup SV2 authority key',{}],
    ['power','number','Safety','Mining power',{min:0,max:100}],
    ['maxtemp','number','Safety','Temperature limit',{min:0,max:255}],
    ['nightfrom','number','Display','Night hour',{min:0,max:255}],
    ['saver','bool','Safety','Battery saver',{}], ['webpass','secret','Network','Web password',{}]
  ],
  values:{pool:'pool.example',port:3333,addr:'address.worker',diff:0.0002,proto:0,sv2key:'',uaauto:false,ua:'BDLMinerV2/2.1.1',pool2:'',port2:3333,proto2:0,sv2key2:'',power:75,maxtemp:65,nightfrom:255,saver:false,webpass:'password'}
};
let browser;
const cases=[];
function check(name,fn){ cases.push({name,fn}); }
function json(res,value,status=200){res.writeHead(status,{'Content-Type':'application/json'});res.end(JSON.stringify(value));}

async function fixture(options={}){
  const state={settings:clone(settings),postStatus:options.postStatus||200,postText:options.postText||null,requests:[],stats:clone(baseStats),posts:[],inflight:0,peak:0,settingsCalls:0,logCalls:0,failLog:!!options.logFailure,historyCalls:0,history:options.history||{hashrate:[1000000,900000],temperature_c:[45,55],power_percent:[100,50],coverage_percent:[100,100],interval_s:300,revision:1},diagnostics:options.diagnostics||{uptime_s:0,firmware:'test',tls:{handshakes:2,failures:0,total_ms:1000,last_ms:500},engines:[{index:0,name:'hw',hashing_ms:10000,rest_ms:20,waiting_ms:30,paused_ms:40}]}};
  if(options.stats) Object.assign(state.stats,options.stats);
  state.statsStatus=200;state.logDelay=0;state.logText='Miner log is available';
  if(options.capabilities)state.settings.capabilities={...state.settings.capabilities,...options.capabilities};
  if(options.live)state.settings.live=['power','maxtemp','nightfrom','saver'];
  if(options.poolSharing){
    state.settings.schema.push(['sharehr','bool','Network','Share my hashrate anonymously',{}],['sharepool','bool','Network','Share my pool anonymously',{}]);
    Object.assign(state.settings.values,{sharehr:true,sharepool:true});
    state.settings.live=[...(state.settings.live||[]),'sharehr','sharepool'];
  }
  if(options.remote){
    state.settings.schema.push(['remote','bool','Network','Private remote monitoring',{}],['remotekey','secret','Network','Personal access key',{}],['rname','text','Network','Miner name',{}]);
    Object.assign(state.settings.values,{remote:false,remotekey:'',rname:''});state.settings.live=[...(state.settings.live||[]),'remote'];
  }
  if(options.coins){
    state.settings.schema.unshift(['coin','number','Pool','Cryptocurrency',{min:0,max:5}]);
    state.settings.values.coin=0;
  }
  if(options.v2){
    for(const key of ['channel','screen0','screen1','screen2','screen3','pages'])state.settings.schema.push([key,'number',key==='channel'?'Network':'Display',key,{min:0,max:255}]);
    state.settings.schema.push(['customscreen','bool','Display','Custom screen',{}]);
    Object.assign(state.settings.values,{channel:0,customscreen:false,screen0:1,screen1:2,screen2:4,screen3:3,pages:255});
    state.settings.live=[...(state.settings.live||[]),'channel','customscreen','screen0','screen1','screen2','screen3','pages'];
    state.settings.capabilities={display_pages:8,...state.settings.capabilities};
  }
  if(options.gamma){
    state.settings.schema.push(['asicmhz','number','Safety','Gamma ASIC frequency (MHz)',{min:525,max:800}],['asicmv','number','Safety','Gamma ASIC core voltage (mV)',{min:1000,max:1300}],['fanauto','bool','Safety','Fan control',{}],['fanpct','number','Safety','Manual fan speed (%)',{min:30,max:100}]);
    Object.assign(state.settings.values,{asicmhz:525,asicmv:1150,fanauto:false,fanpct:100});state.settings.live=[...(state.settings.live||[]),'fanauto','fanpct'];
    state.settings.capabilities={...state.settings.capabilities,fan_control:true,asic:true,thermal_steps:false,...(options.v2?{display_pages:3}:{})};
  }
  const server=http.createServer(async(req,res)=>{
    const chunks=[];for await(const chunk of req)chunks.push(chunk);
    const body=Buffer.concat(chunks).toString();state.requests.push(req.method+' '+req.url);
    if(req.method==='POST')state.posts.push({url:req.url,body});
    if(req.url==='/site'){
      res.writeHead(200,{'Content-Type':'text/html; charset=utf-8'});
      return res.end(fs.readFileSync(path.join(__dirname,'../../installer/site/index.html'),'utf8').replaceAll('__PAGE_KEY__','home').replaceAll('__PAGE_TITLE__','BDLMiner V2: demo').replace('__BOARDS__',JSON.stringify([{env:'m5stickc-plus2',name:'M5StickC Plus2'}])).replaceAll('__VERSION__','test'));
    }
    if(['/', '/settings', '/log'].includes(req.url)){
      const page=req.url==='/'?'dashboard':req.url==='/settings'?'settings':'log';
      res.writeHead(200,{'Content-Type':'text/html; charset=utf-8','Cache-Control':'no-store'});
      return res.end('<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="stylesheet" href="/assets/style.css">'+"<script>const LANG='"+(options.language||'en')+"';</script>"+'<script defer src="/assets/common.js"></script><script defer src="/assets/'+page+'.js"></script></head><body>'+read('header.html')+read(page+'.html')+'</div></div></body></html>');
    }
    if(req.url.startsWith('/assets/')){
      const name=req.url.slice('/assets/'.length).split('?')[0];
      if(['site.js','site.css','discovery.js','discovery.css','pulse-theme.css','pulse-chart.js','home-chart.js','vendor/chart.umd.min.js'].includes(name)){
        res.writeHead(200,{'Content-Type':name.endsWith('.css')?'text/css':'text/javascript'});
        return res.end(fs.readFileSync(path.join(__dirname,'../../installer/assets',name)));
      }
      if(!['style.css','common.js','dashboard.js','settings.js','log.js'].includes(name)){res.writeHead(404);return res.end()}
      res.writeHead(200,{'Content-Type':name.endsWith('.css')?'text/css':'text/javascript','Content-Encoding':'gzip'});
      return res.end(zlib.gzipSync(read(name)));
    }
    if(req.url==='/api/settings'&&req.method==='GET'){
      state.settingsCalls++;
      if(options.settingsDelay)await new Promise(r=>setTimeout(r,options.settingsDelay));
      if(options.settingsFailure&&state.settingsCalls===1)return json(res,{error:'unavailable'},503);
      if(options.settingsDenied){res.writeHead(401);return res.end('Wrong password')}
      return json(res,state.settings);
    }
    if(req.url==='/api/events'&&options.stream){
      state.activeStreams=(state.activeStreams||0)+1;
      res.writeHead(200,{'Content-Type':'text/event-stream','Cache-Control':'no-store'});
      const push=()=>{state.streamed=(state.streamed||0)+1;res.write('data: '+JSON.stringify({...state.stats,hashrate:state.stats.hashrate+state.streamed})+'\n\n')};
      push();const timer=setInterval(push,100);res.on('close',()=>{clearInterval(timer);state.activeStreams--});return;
    }
    if(req.url.split('?')[0]==='/api/stats'){
      state.peak=Math.max(state.peak,++state.inflight);
      if(options.statsDelay)await new Promise(r=>setTimeout(r,options.statsDelay));
      state.inflight--;
      return json(res,state.stats,state.statsStatus);
    }
    if(req.url==='/api/pool/diagnostics')return json(res,state.stats.pool_diagnostics||{code:'ready',compatible:true});
    if(req.url==='/api/history'){state.historyCalls++;if(options.historyFailure){res.writeHead(503);return res.end('History unavailable')}return json(res,state.history)}
    if(req.url==='/api/diagnostics'){return json(res,{...state.diagnostics,uptime_s:state.stats.uptime_s})}
    if(req.url==='/api/log'){
      state.logCalls++;
      const text=state.logText;if(state.logDelay)await new Promise(r=>setTimeout(r,state.logDelay));
      if(state.failLog){res.writeHead(500);return res.end('Log unavailable')}
      return res.end(text);
    }
    if(req.url==='/slow-body'){
      res.writeHead(200,{'Content-Type':'application/json'});res.write('{');
      return setTimeout(()=>res.end('"ok":true}'),500);
    }
    if(req.method==='POST'){
      if(options.postGate)await options.postGate;
      if(options.postDelay)await new Promise(r=>setTimeout(r,options.postDelay));
      res.writeHead(state.postStatus,{'Content-Type':'text/plain'});return res.end(state.postText||(state.postStatus>=400?'Operation refused':'Saved, restarting'));
    }
    res.writeHead(404);res.end('Not found');
  });
  await new Promise(r=>server.listen(0,'127.0.0.1',r));
  const context=await browser.newContext({viewport:options.mobile?{width:375,height:812}:{width:1100,height:800}});
  const page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));
  // Speed up polling without changing request timeouts or server timing.
  if(!options.realPolling)await page.addInitScript(()=>{
    const timeout=window.setTimeout.bind(window),interval=window.setInterval.bind(window);
    window.setTimeout=(fn,ms,...args)=>timeout(fn,ms===3000||ms===10000?30:ms===5000?50:ms,...args);
    window.setInterval=(fn,ms,...args)=>interval(fn,ms===3000||ms===10000?30:ms===5000?50:ms,...args);
  });
  return {page,state,errors,url:'http://127.0.0.1:'+server.address().port,
    async close(){await context.close();server.closeAllConnections();await new Promise(r=>server.close(r));}};
}
async function loaded(f){await f.page.goto(f.url+'/settings');await f.page.waitForSelector('#k_pool');}
async function run(options,fn){const f=await fixture(options);try{await fn(f);assert.deepEqual(f.errors,[],'uncaught browser errors')}finally{await f.close()}}
async function loadSite(f){
  // Exercise the shipped site offline: no real chat messages, media uploads
  // or dependency downloads are made by this fixture.
  await f.page.route('https://**/*',route=>route.fulfill({status:200,contentType:'text/javascript',body:''}));
  await f.page.addInitScript(()=>{window.WebSocket=class {
    constructor(){this.readyState=1;window.__siteSocket=this;setTimeout(()=>this.onopen?.(),0)}
    send(){} close(){}
  }});
  await f.page.goto(f.url+'/site');await f.page.waitForFunction(()=>window.__siteSocket?.onmessage);
}

check('site chat keeps only the 100 newest message elements during a long session',()=>run({},async f=>{
  await loadSite(f);
  const result=await f.page.evaluate(()=>{
    for(let id=1;id<=125;++id)__siteSocket.onmessage({data:JSON.stringify({type:'message',message:{id,name:'Test',text:'message '+id,t:1793500000000+id}})});
    return {stored:chat.messages.length,rows:document.querySelectorAll('#chat-log>li').length,first:document.querySelector('#chat-log>li').dataset.id};
  });
  assert.deepEqual(result,{stored:100,rows:100,first:'26'});
}));
check('site voice decoding closes its audio context even for corrupt media',()=>run({},async f=>{
  await loadSite(f);
  const result=await f.page.evaluate(async()=>{
    let closed=0;
    window.AudioContext=class {async decodeAudioData(){throw Error('corrupt audio')}async close(){closed++}};
    const peaks=await voicePeaks(new Blob(['invalid audio']),'test');
    return {closed,bars:peaks.length,finite:peaks.every(Number.isFinite)};
  });
  assert.deepEqual(result,{closed:1,bars:36,finite:true});
}));

check('notification test ignores unrelated deliveries and polls its assigned ID',()=>run({},async f=>{
  f.state.settings.schema.push(['hook','secret','Notifications','Webhook',{}]);f.state.settings.values.hook='';
  await f.page.addInitScript(()=>{const delay=window.setTimeout.bind(window);window.setTimeout=(fn,ms,...args)=>delay(fn,ms===2000?20:ms,...args)});
  let posted=false,reads=0;const queries=[];
  await f.page.route('**/api/notify-test*',route=>{
    const post=route.request().method()==='POST';if(post)posted=true;
    const value=post?{id:42}:!posted?{id:0}:++reads===1?{id:99,title:'Miner too hot',ntfy:'HTTP 500',webhook:'off'}:{id:42,title:'Test notification',ntfy:'OK',webhook:'off'};
    if(!post)queries.push(new URL(route.request().url()).searchParams.get('id'));
    return route.fulfill({status:post?202:200,contentType:'application/json',body:JSON.stringify(value)});
  });
  await loaded(f);await f.page.evaluate(()=>testNotification());
  assert.equal(await f.page.locator('#notify-result').textContent(),'ntfy: OK · webhook: off');
  assert.equal(reads,2);assert.deepEqual(queries,['42','42']);
}));
check('notification test remains locked while settings controls change',()=>run({},async f=>{
  f.state.settings.schema.push(['hook','secret','Notifications','Webhook',{}]);f.state.settings.values.hook='';
  await f.page.addInitScript(()=>{const delay=window.setTimeout.bind(window);window.setTimeout=(fn,ms,...args)=>delay(fn,ms===2000?20:ms,...args)});
  let release,started,posts=0;const gate=new Promise(r=>release=r),posting=new Promise(r=>started=r);
  await f.page.route('**/api/notify-test*',async route=>{
    if(route.request().method()==='POST'){++posts;started();await gate;return route.fulfill({status:202,contentType:'application/json',body:'{"id":7}'})}
    return route.fulfill({status:200,contentType:'application/json',body:'{"id":7,"ntfy":"OK","webhook":"off"}'});
  });
  await loaded(f);await f.page.click('#notify-test');await posting;
  try{
    await f.page.selectOption('#k_ua','NerdMinerV2');assert.equal(await f.page.locator('#notify-test').isDisabled(),true);
    await f.page.evaluate(()=>testNotification());assert.equal(posts,1);
    await f.page.click('#discard');assert.equal(await f.page.locator('#notify-test').isDisabled(),true);
    assert.equal(await f.page.locator('#notify-result').textContent(),'Sending…');
  }finally{release()}
  await f.page.waitForFunction(()=>document.querySelector('#notify-result').textContent.includes('ntfy: OK'));
  assert.equal(await f.page.locator('#notify-test').isEnabled(),true);
}));
check('notification test reports a full queue in French and allows retry',()=>run({language:'fr'},async f=>{
  f.state.settings.schema.push(['hook','secret','Notifications','Webhook',{}]);f.state.settings.values.hook='';
  await f.page.addInitScript(()=>{const delay=window.setTimeout.bind(window);window.setTimeout=(fn,ms,...args)=>delay(fn,ms===2000?20:ms,...args)});
  let full=true;
  await f.page.route('**/api/notify-test*',route=>route.fulfill(full?{status:503,body:'Notifications are busy; try again shortly'}:{status:200,contentType:'application/json',body:'{"id":12,"ntfy":"OK","webhook":"off"}'}));
  await loaded(f);await f.page.evaluate(()=>testNotification());
  assert.match(await f.page.locator('#notify-result').textContent(),/occupées/);assert.equal(await f.page.locator('#notify-test').isEnabled(),true);
  full=false;await f.page.evaluate(()=>testNotification());assert.match(await f.page.locator('#notify-result').textContent(),/ntfy: OK/);
}));

check('settings cannot submit before load, and failed loads can be retried',()=>run({settingsDelay:150,settingsFailure:true},async f=>{
  await f.page.goto(f.url+'/settings');
  assert.equal(await f.page.locator('#save').isDisabled(),true);
  await f.page.waitForSelector('#retry:not([hidden])');await f.page.click('#retry');
  await f.page.waitForSelector('#k_pool');assert.equal(await f.page.locator('#save').isEnabled(),true);
}));

check('blank and out-of-range numeric fields never become zero-valued settings',()=>run({},async f=>{
  await loaded(f);await f.page.fill('#k_maxtemp','');await f.page.click('#save');
  await f.page.waitForTimeout(80);assert.equal(f.state.posts.length,0);
  await f.page.fill('#k_maxtemp','65');await f.page.fill('#k_power','101');await f.page.click('#save');
  await f.page.waitForTimeout(80);assert.equal(f.state.posts.length,0);
  await f.page.fill('#k_power','75');await f.page.fill('#k_diff','0');await f.page.click('#save');
  await f.page.waitForTimeout(80);assert.equal(f.state.posts.length,0);
}));

check('Enter submits once, and actions remain locked until restart',()=>run({postDelay:100},async f=>{
  await loaded(f);await f.page.fill('#k_pool','new.pool');await f.page.press('#k_pool','Enter');
  await f.page.waitForFunction(()=>document.querySelector('#save').disabled);
  await f.page.waitForTimeout(150);assert.equal(f.state.posts.length,1);
  assert.equal(JSON.parse(f.state.posts[0].body).pool,'new.pool');
  assert.equal(await f.page.locator('#restore').isDisabled(),true);
  assert.match(await f.page.locator('#msg').textContent(),/restarting/);
}));

check('pasted V2 URLs fill protocol port and authority for main and backup',()=>run({},async f=>{
  await loaded(f);
  const key='9c4zpyJ2ndm4e8sP2uNc1VNCGxYjqaxWS6wUCjk8zFj6njFquH6';
  for(const suffix of ['', '2']){
    await f.page.fill('#k_pool'+suffix,'stratum2+tcp://public-pool.io:23330/'+key);
    await f.page.locator('#k_pool'+suffix).blur();
    assert.equal(await f.page.locator('#k_pool'+suffix).inputValue(),'public-pool.io');
    assert.equal(await f.page.locator('#k_port'+suffix).inputValue(),'23330');
    assert.equal(await f.page.locator('#k_proto'+suffix).inputValue(),'1');
    assert.equal(await f.page.locator('#k_sv2key'+suffix).inputValue(),key);
    assert.equal(await f.page.locator('#k_sv2key'+suffix).isEnabled(),true);
  }
  await f.page.click('#save');await f.page.waitForTimeout(80);
  assert.equal(f.state.posts.length,1);
}));

check('invalid filtered settings are revealed before browser validation',()=>run({},async f=>{
  await loaded(f);await f.page.fill('#k_maxtemp','');
  await f.page.fill('#settings-search','pool');
  await f.page.click('#save');await f.page.waitForTimeout(80);
  assert.equal(f.state.posts.length,0);
  assert.equal(await f.page.locator('#k_maxtemp').isVisible(),true);
  assert.equal(await f.page.locator('#k_maxtemp').evaluate(e=>e===document.activeElement),true);
}));

check('failed saves display an error and permit retry',()=>run({postStatus:500},async f=>{
  await loaded(f);await f.page.click('#save');
  await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('refused'));
  assert.equal(await f.page.locator('#save').isEnabled(),true);
}));

check('polling never overlaps slow requests, and zero hashrate is displayed',()=>run({statsDelay:140},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#m .t');await f.page.waitForTimeout(450);
  assert.equal(f.state.peak,1);
  const rate=f.page.locator('#m .t').filter({has:f.page.locator('.l').filter({hasText:/^Hashrate$/})});
  assert.match(await rate.textContent(),/0 H\/s/);
}));

check('dashboard polls once per ten seconds without accelerated test timers',()=>run({realPolling:true,stats:{history:undefined,history_revision:1}},async f=>{
  await f.page.clock.install();
  await f.page.goto(f.url);await f.page.waitForSelector('#m .t');
  const calls=()=>f.state.requests.filter(r=>r.startsWith('GET /api/stats')).length;
  assert.equal(calls(),1);
  await f.page.clock.runFor(8000);assert.equal(calls(),1);
  f.state.stats.hashrate=800000;
  await f.page.clock.runFor(2500);
  await f.page.waitForFunction(()=>document.querySelector('#m .hero .v').textContent==='800 kH/s');
  // The history request follows the rendered statistics; on a busy runner it can reach the fixture a moment later.
  for(let i=0;i<100&&!f.state.historyCalls;i++)await new Promise(r=>setTimeout(r,10));
  assert.equal(calls(),2);assert.equal(f.state.historyCalls,1);assert.equal(f.state.peak,1);
}));

check('a stalled response body is also covered by the timeout',()=>run({},async f=>{
  await f.page.goto(f.url);const message=await f.page.evaluate(async()=>{
    try{await request('/slow-body',{timeout:60});return 'no timeout'}catch(e){return e.message}
  });assert.match(message,/timed out/i);
}));

check('64-bit share counter shows exact count, translated session scope and realistic interval',()=>run({language:'fr',stats:{hashrate:1000000,over64bits:7}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#m .t');
  const card=f.page.locator('#records-grid .t').filter({has:f.page.locator('.l').filter({hasText:/^Shares 64 bits$/})});
  assert.equal(await card.locator('.v').textContent(),'7');
  assert.match(await card.locator('.s').textContent(),/Depuis le redémarrage.*585 k ans/);
  f.state.stats.hashrate=0;f.state.stats.over64bits=0;
  await f.page.waitForFunction(()=>[...document.querySelectorAll('#records-grid .t')].some(e=>e.querySelector('.l')?.textContent==='Shares 64 bits'&&e.querySelector('.v')?.textContent==='0'));
  assert.match(await card.locator('.s').textContent(),/1 toutes les - en moyenne/);
}));

check('64-bit share display supports older miners without the counter',()=>run({stats:{over64bits:undefined}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#m .t');
  const card=f.page.locator('#records-grid .t').filter({has:f.page.locator('.l').filter({hasText:/^64-bit shares$/})});
  assert.equal(await card.locator('.v').textContent(),'0');
  assert.match(await card.locator('.s').textContent(),/Since restart/);
}));

check('peer URLs are normalized and fresh results appear on the first poll',()=>run({stats:{hashrate:1000,peers:['http://peer.test/','peer.test','bad.test']}},async f=>{
  await f.page.route('http://peer.test/api/stats?history=0',route=>route.fulfill({json:{hashrate:2000,pool:{connected:true},shares:{accepted:3},best:{ever:1}}}));
  await f.page.route('http://bad.test/api/stats?history=0',route=>route.fulfill({json:{broken:true}}));
  await f.page.goto(f.url);await f.page.waitForSelector('#p .t');
  assert.match(await f.page.locator('#p .t').first().textContent(),/3\.00 kH\/s/);
  assert.equal(await f.page.locator('#p .t').count(),3);
  assert.equal(await f.page.locator('#st').textContent(),'ONLINE');
}));

check('regression: repeated GitHub checks unlock when unchanged statistics return',()=>run({stats:{github_update:{mode:1,checked:true,available:false}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('.gh-check:enabled');
  for(let i=0;i<3;i++){
    await f.page.click('.gh-check');
    await f.page.waitForFunction(()=>!document.querySelector('.gh-check').disabled,{},{timeout:1000});
  }
  assert.equal(f.state.posts.filter(p=>p.url==='/api/github-check').length,3);
}));

check('regression: GitHub check and install cannot start together',()=>run({postDelay:200,stats:{github_update:{mode:1,available:true,version:'1.5.3'}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('.gh-install:enabled');
  const locked=await f.page.evaluate(()=>{
    document.querySelector('.gh-check').click();
    const locked=document.querySelector('.gh-install').disabled;
    document.querySelector('.gh-install').click();
    return locked;
  });
  assert.equal(locked,true);await f.page.waitForTimeout(50);assert.equal(f.state.posts.length,1);
}));

check('regression: disabled update mode also disables an available install',()=>run({stats:{github_update:{mode:0,available:true,version:'1.5.3'}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('.gh-install');
  assert.equal(await f.page.locator('.gh-install').isDisabled(),true);
}));

check('regression: GitHub command errors remain visible above old metadata errors',()=>run({postStatus:500,postText:'Operation refused',stats:{github_update:{mode:1,checked:true,available:false,error:'old metadata error'}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('.gh-check:enabled');await f.page.click('.gh-check');
  await f.page.waitForFunction(()=>document.querySelector('#updates').textContent.includes('Operation refused'),{},{timeout:1000});
  await f.page.waitForTimeout(100);assert.match(await f.page.locator('#updates').textContent(),/Operation refused/);
  assert.equal(await f.page.locator('.gh-check').isEnabled(),true);
}));

check('regression: malformed inline history preserves charts and pool status',()=>run({},async f=>{
  await f.page.goto(f.url);await f.page.waitForFunction(()=>document.querySelector('#st').textContent==='ONLINE');
  const count=await f.page.locator('#chart-count').textContent();
  f.state.stats.history={hashrate:[-1],interval_s:300};
  await f.page.waitForTimeout(150);
  assert.equal(await f.page.locator('#st').textContent(),'ONLINE');
  assert.equal(await f.page.locator('#chart-count').textContent(),count);
  assert.match(await f.page.locator('#history-status').textContent(),/Invalid response/);
}));

check('regression: lost statistics disable measurements until a fresh response',()=>run({},async f=>{
  await f.page.goto(f.url);await f.page.locator('#diagnostics>summary').click();await f.page.waitForSelector('#start-benchmark:enabled');
  f.state.statsStatus=503;
  await f.page.waitForFunction(()=>document.querySelector('#st').textContent==='NO CONNECTION');
  assert.equal(await f.page.locator('#start-benchmark').isDisabled(),true);
  f.state.statsStatus=200;await f.page.waitForSelector('#start-benchmark:enabled');
}));

check('regression: pausing the log discards an in-flight response',()=>run({},async f=>{
  await f.page.goto(f.url+'/log');await f.page.waitForFunction(()=>document.querySelector('#l').textContent.includes('available'));
  f.state.logDelay=200;f.state.logText='late log response';const previous=f.state.logCalls;
  for(let i=0;i<100&&f.state.logCalls===previous;i++)await f.page.waitForTimeout(10);
  assert.ok(f.state.logCalls>previous);
  await f.page.click('#pause-log');const text=await f.page.locator('#l').textContent();
  await f.page.waitForTimeout(250);assert.equal(await f.page.locator('#l').textContent(),text);
  await f.page.click('#pause-log');await f.page.waitForFunction(()=>document.querySelector('#l').textContent==='late log response');
}));

check('regression: slow history and peers do not delay live statistics',()=>run({stats:{history:undefined,history_revision:1,peers:['slow-peer.test']}},async f=>{
  let releaseHistory,releasePeer,peerStarted=false;
  const historyGate=new Promise(resolve=>releaseHistory=resolve),peerGate=new Promise(resolve=>releasePeer=resolve);
  await f.page.route('**/api/history',async route=>{await historyGate;await route.fulfill({json:f.state.history})});
  await f.page.route('http://slow-peer.test/**',async route=>{peerStarted=true;await peerGate;await route.fulfill({json:{hashrate:1,pool:{connected:true},shares:{accepted:0},best:{ever:0}}})});
  try{
    await f.page.goto(f.url);await f.page.waitForFunction(()=>document.querySelector('#st').textContent==='ONLINE');
    f.state.stats.shares.accepted=123;
    await f.page.waitForFunction(()=>Array.from(document.querySelectorAll('#m .t')).some(e=>e.querySelector('.l')?.textContent==='Shares'&&e.querySelector('.v')?.textContent==='123'),{},{timeout:2000});
    assert.equal(peerStarted,true);
  }finally{releaseHistory();releasePeer()}
}));

check('late history responses from a previous boot cannot replace current data',()=>run({stats:{history:undefined,history_revision:1,boot_id:'old',uptime_s:100}},async f=>{
  let releaseFirst,historyCalls=0;
  const gate=new Promise(resolve=>releaseFirst=resolve);
  await f.page.route('**/api/history',async route=>{
    const first=++historyCalls===1;
    if(first)await gate;
    await route.fulfill({json:{boot_id:first?'old':'new',revision:first?1:2,interval_s:300,hashrate:[first?999000:2000]}});
  });
  try{
    await f.page.goto(f.url);await f.page.waitForFunction(()=>document.querySelector('#st').textContent==='ONLINE');
    for(let i=0;i<100&&!historyCalls;i++)await f.page.waitForTimeout(10);
    assert.equal(historyCalls,1);
    f.state.stats={...f.state.stats,boot_id:'new',uptime_s:200,history_revision:2,shares:{accepted:77,rejected:0}};
    await f.page.waitForFunction(()=>Array.from(document.querySelectorAll('#m .t')).some(e=>e.querySelector('.l')?.textContent==='Shares'&&e.querySelector('.v')?.textContent==='77'));
    releaseFirst();await f.page.waitForFunction(()=>document.querySelector('#chart-average').textContent==='2.00 kH/s');
    assert.equal(await f.page.locator('#chart-count').textContent(),'1');
    assert.equal(await f.page.locator('#st').textContent(),'ONLINE');
  }finally{releaseFirst()}
}));

check('GitHub installation stays disabled across dashboard refreshes',()=>run({stats:{github_update:{available:true,version:'1.0.11',installing:true,notes:'45 %'}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('.gh-install');
  assert.equal(await f.page.locator('.gh-install').isDisabled(),true);
  await f.page.waitForTimeout(120);assert.equal(await f.page.locator('.gh-install').isDisabled(),true);
  assert.equal(f.state.posts.length,0);
}));

check('backup restore preserves uint64 integers and cancellation is harmless',()=>run({},async f=>{
  await loaded(f);
  await f.page.evaluate(()=>imp({files:[]}));assert.equal(f.state.posts.length,0);
  f.page.on('dialog',dialog=>dialog.accept());
  const raw='{"bdlminer_backup":1,"settings":{},"stats":{"hashes":18446744073709551615}}';
  await f.page.setInputFiles('#imp',{name:'backup.json',mimeType:'application/json',buffer:Buffer.from(raw)});
  await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('restored'));
  assert.equal(f.state.posts[0].body,raw);assert.equal(await f.page.locator('#save').isDisabled(),true);
}));

check('an accepted install request cannot be submitted again while waiting',()=>run({stats:{github_update:{available:true,version:'1.0.11',installing:false,notes:''}},postDelay:100},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('.gh-install');await f.page.click('.gh-install');
  await f.page.waitForTimeout(220);assert.equal(f.state.posts.length,1);
  assert.equal(await f.page.locator('.gh-install').isDisabled(),true);
}));

check('GitHub errors stay visible when no update is available and a manual check is queued once',async()=>{
 let release;const postGate=new Promise(resolve=>release=resolve);
 try{await run({language:'fr',stats:{github_update:{mode:1,checked:false,available:false,error:'release unavailable: HTTP 403'}},postGate},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('.gh-check');
  assert.match(await f.page.locator('#updates').textContent(),/HTTP 403/);
  assert.equal(await f.page.locator('.gh-check').textContent(),'Vérifier maintenant');
  await f.page.click('.gh-check');await f.page.waitForTimeout(60);
  assert.equal(f.state.posts.length,1);assert.equal(f.state.posts[0].url,'/api/github-check');
  assert.equal(await f.page.locator('.gh-check').isDisabled(),true);
  release();await f.page.waitForFunction(()=>!document.querySelector('.gh-check').disabled);
 })}finally{release()}
});

check('an installation failing before the next poll can be retried',()=>run({stats:{github_update:{available:true,version:'1.0.11',request_id:0}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('.gh-install');await f.page.click('.gh-install');
  await f.page.waitForTimeout(80);
  f.state.stats.github_update={available:true,version:'1.0.11',request_id:1,installing:false,error:'this board requires USB'};
  await f.page.waitForFunction(()=>!document.querySelector('.gh-install').disabled);
  assert.match(await f.page.locator('#updates').textContent(),/requires USB/);
}));

check('disabled updates and pending checks are shown on the dashboard',()=>run({stats:{github_update:{mode:0,available:false,checked:false}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('.gh-check');
  assert.equal(await f.page.locator('.gh-check').isDisabled(),true);
  assert.match(await f.page.locator('#updates').textContent(),/disabled in settings/);
  f.state.stats.github_update={mode:1,checking:true,available:false};
  await f.page.waitForFunction(()=>document.querySelector('.gh-check').textContent==='Checking…');
  assert.equal(await f.page.locator('.gh-check').isDisabled(),true);
}));

check('invalid backup files produce a translated message and can be retried',()=>run({language:'fr'},async f=>{
  await loaded(f);await f.page.setInputFiles('#imp',{name:'bad.json',mimeType:'application/json',buffer:Buffer.from('[]')});
  await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('sauvegarde'));
  assert.equal(f.state.posts.length,0);assert.equal(await f.page.locator('#restore').isEnabled(),true);
  assert.equal(await f.page.locator('#imp').inputValue(),'');
}));

check('factory images are rejected before upload; server failures allow retry',()=>run({postStatus:500},async f=>{
  await loaded(f);const buffer=Buffer.from([0xe9,1,2,3]);
  await f.page.setInputFiles('#fw',{name:'board-factory.bin',mimeType:'application/octet-stream',buffer});
  await f.page.click('#upload');await f.page.waitForTimeout(80);assert.equal(f.state.posts.length,0);
  await f.page.setInputFiles('#fw',{name:'board-firmware.bin',mimeType:'application/octet-stream',buffer});
  await f.page.click('#upload');await f.page.waitForFunction(()=>document.querySelector('#up').textContent.includes('refused'));
  assert.equal(f.state.posts.length,1);assert.equal(await f.page.locator('#upload').isEnabled(),true);
}));

check('log errors are visible and polling recovers',()=>run({logFailure:true},async f=>{
  await f.page.goto(f.url+'/log');await f.page.waitForFunction(()=>document.querySelector('#log-status').textContent.length>0);
  f.state.failLog=false;
  await f.page.waitForFunction(()=>document.querySelector('#l').textContent.includes('available'));
  assert.equal(await f.page.locator('#log-status').textContent(),'');
}));

check('settings stay within a mobile viewport and labels focus their inputs',()=>run({mobile:true,language:'fr'},async f=>{
  await loaded(f);assert.equal(await f.page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
  await f.page.locator('label[for="k_power"]').click();assert.equal(await f.page.evaluate(()=>document.activeElement.id),'k_power');
}));

check('selected pool identity updates when switching to backup without a mode label',()=>run({language:'fr'},async f=>{
  await loaded(f);
  assert.match(await f.page.locator('#group-pool').textContent(),/Choisis le user agent/);
  assert.equal(await f.page.locator('#k_ua').isEnabled(),true);
  f.state.stats.pool.user_agent='NerdMinerV2';f.state.stats.pool.protocol='Stratum V1';
  await f.page.goto(f.url);
  await f.page.waitForFunction(()=>document.querySelector('#pool-grid').textContent.includes('NerdMinerV2'));
  assert.match(await f.page.locator('#pool-grid').textContent(),/Stratum V1 · NerdMinerV2/);
  Object.assign(f.state.stats.pool,{host:'backup.example',backup:true,user_agent:'BDLMinerV2/test'});
  await f.page.waitForFunction(()=>document.querySelector('#pool-grid').textContent.includes('BDLMinerV2/test'));
  assert.match(await f.page.locator('#pool-grid').textContent(),/Pool \(secours\)/);
  assert.doesNotMatch(await f.page.locator('#pool-grid').textContent(),/NerdMinerV2/);
}));

check('user agent has exactly two choices without mode or free text in French',()=>run({language:'fr'},async f=>{
 await loaded(f);
 assert.equal(await f.page.locator('#k_uaauto').count(),0);
 assert.equal(await f.page.locator('#k_ua').evaluate(e=>e.tagName),'SELECT');
 assert.deepEqual(await f.page.locator('#k_ua option').allTextContents(),['NerdMinerV2','BDLMinerV2/2.1.1']);
 assert.doesNotMatch(await f.page.locator('#group-pool').innerText(),/Automatique|Manuel|saisie libre/);
 f.state.stats.pool.user_agent='NerdMinerV2';f.state.stats.pool.protocol='Stratum V1';
 await f.page.goto(f.url);await f.page.waitForFunction(()=>document.querySelector('#pool-grid').textContent.includes('NerdMinerV2'));
 assert.match(await f.page.locator('#pool-grid').innerText(),/Stratum V1 · NerdMinerV2/);
 assert.doesNotMatch(await f.page.locator('#pool-grid').innerText(),/automatique|manuel/);
}));
check('selected user agent and independent V1/V2 choices are saved',()=>run({},async f=>{
 await loaded(f);await f.page.selectOption('#k_ua','NerdMinerV2');await f.page.selectOption('#k_proto','1');
 assert.equal(await f.page.locator('#k_sv2key').isEnabled(),true);await f.page.click('#save');assert.equal(f.state.posts.length,0);
 await f.page.fill('#k_sv2key','9c4zpyJ2ndm4e8sP2uNc1VNCGxYjqaxWS6wUCjk8zFj6njFquH6');
 assert.equal(await f.page.locator('#k_proto2').inputValue(),'0');assert.equal(await f.page.locator('#k_sv2key2').isDisabled(),true);
 await f.page.click('#save');await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('restarting'));
 const saved=JSON.parse(f.state.posts[0].body);assert.equal(saved.uaauto,false);assert.equal(saved.ua,'NerdMinerV2');assert.equal(saved.proto,1);assert.equal(saved.proto2,0);
}));
check('native identity can be selected and discarded without editing text or main pool',()=>run({},async f=>{
 await loaded(f);await f.page.selectOption('#k_ua','NerdMinerV2');await f.page.click('#discard');
 assert.equal(await f.page.locator('#k_ua').inputValue(),'BDLMinerV2/2.1.1');
 await f.page.fill('#k_pool2','public-pool.io');await f.page.fill('#k_port2','23330');await f.page.selectOption('#k_proto2','1');
 await f.page.fill('#k_sv2key2','9c4zpyJ2ndm4e8sP2uNc1VNCGxYjqaxWS6wUCjk8zFj6njFquH6');
 await f.page.click('#save');await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('restarting'));
 const saved=JSON.parse(f.state.posts[0].body);assert.equal(saved.uaauto,false);assert.equal(saved.ua,'BDLMinerV2/2.1.1');assert.equal(saved.pool,'pool.example');assert.equal(saved.proto,0);assert.equal(saved.proto2,1);
}));

check('full performance is an explicit 100 percent choice and preserves thermal protection',()=>run({language:'fr'},async f=>{
  await loaded(f);await f.page.fill('#settings-search','puissance');
  assert.equal(await f.page.locator('#k_pool').isVisible(),false);
  await f.page.click('#preset-100');assert.equal(await f.page.locator('#k_power').inputValue(),'100');
  assert.equal(await f.page.locator('#k_maxtemp').inputValue(),'65');assert.equal(f.state.posts.length,0);
  await f.page.click('#save');await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('redémarrage'));
  const saved=JSON.parse(f.state.posts[0].body);assert.equal(saved.power,100);assert.equal(saved.maxtemp,65);
}));

check('ESP32 settings explain thermal steps and update their threshold',()=>run({},async f=>{
  await loaded(f);const help=f.page.locator('[data-key="maxtemp"] .field-help');
  assert.match(await help.textContent(),/Throttling starts at 55 °C.*Mining pauses at 65 °C/);
  await f.page.fill('#k_maxtemp','70');assert.match(await help.textContent(),/Throttling starts at 60 °C.*Mining pauses at 70 °C/);
}));
check('ASIC settings describe the two-minute thermal pause without ESP32 throttle steps',()=>run({capabilities:{ota:true,thermal_steps:false}},async f=>{
  await loaded(f);const help=f.page.locator('[data-key="maxtemp"] .field-help');
  assert.match(await help.textContent(),/Mining pauses at 65 °C.*at least 2 minutes.*5 °C/);
  assert.doesNotMatch(await help.textContent(),/Throttling starts/);
  await f.page.fill('#k_maxtemp','0');assert.equal(await help.textContent(),'Thermal protection is disabled.');
}));
check('ASIC thermal pause help is translated into French',()=>run({language:'fr',capabilities:{thermal_steps:false}},async f=>{
  await loaded(f);const text=await f.page.locator('[data-key="maxtemp"] .field-help').textContent();
  assert.match(text,/65 °C.*au moins 2 minutes.*5 °C/);assert.doesNotMatch(text,/Throttling|Mining|starts/);
}));

check('secret visibility toggles do not submit settings',()=>run({},async f=>{
  await loaded(f);const show=f.page.getByRole('button',{name:'Show Web password',exact:true});await show.click();
  assert.equal(await f.page.locator('#k_webpass').getAttribute('type'),'text');assert.equal(f.state.posts.length,0);
  await f.page.getByRole('button',{name:'Hide Web password',exact:true}).click();
  assert.equal(await f.page.locator('#k_webpass').getAttribute('type'),'password');
}));

check('settings navigation restores filtered groups and marks the active page',()=>run({},async f=>{
  await loaded(f);await f.page.fill('#settings-search','no-matching-setting');
  assert.equal(await f.page.locator('#settings-empty').isVisible(),true);
  await f.page.locator('#settings-nav a').first().click();assert.equal(await f.page.locator('#k_pool').isVisible(),true);
  assert.equal(await f.page.locator('[data-nav="/settings"]').getAttribute('aria-current'),'page');
}));

check('a refused password stays inside the translated interface',()=>run({language:'fr',settingsDenied:true},async f=>{
  await f.page.goto(f.url+'/settings');await f.page.waitForFunction(()=>/authentification|identif|connecte/i.test(document.querySelector('#msg').textContent));
  assert.equal(await f.page.locator('#save').isDisabled(),true);assert.equal(await f.page.locator('h1').textContent(),'Réglages et maintenance');
}));

check('USB-only boards explain the update path and prevent web installs',()=>run({capabilities:{ota:false},stats:{github_update:{available:true,supported:false,version:'1.1.0',release_url:'https://github.com/bdlglua/BDLMinerV2/releases/tag/v1.1.0'}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#updates a');assert.equal(await f.page.locator('.gh-install').count(),0);
  assert.match(await f.page.locator('#updates a').getAttribute('href'),/v1\.1\.0/);
  await loaded(f);await f.page.setInputFiles('#fw',{name:'board-firmware.bin',mimeType:'application/octet-stream',buffer:Buffer.from([0xe9,1,2,3])});
  assert.equal(await f.page.locator('#upload').isDisabled(),true);assert.match(await f.page.locator('#firmware-help').textContent(),/USB/);
}));

check('device diagnostics distinguish heat limits from configured power',()=>run({stats:{limitation_reason:'thermal',max_temp_c:65,power_limit_percent:100,power_percent:50}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#health:not([hidden])');assert.match(await f.page.locator('#health').textContent(),/Temperature protection/);
  assert.match(await f.page.locator('#device-grid').textContent(),/100 %/);
  f.state.stats.limitation_reason='full';f.state.stats.power_percent=100;
  await f.page.waitForFunction(()=>document.querySelector('#health').hidden);
}));

check('hidden tabs stop polling and visibility resumes without overlap',()=>run({statsDelay:50},async f=>{
  await f.page.addInitScript(()=>{window.__hidden=false;Object.defineProperty(document,'hidden',{get:()=>window.__hidden})});
  await f.page.goto(f.url);await f.page.waitForSelector('#m .t');
  await f.page.evaluate(()=>{window.__hidden=true;document.dispatchEvent(new Event('visibilitychange'))});
  await f.page.waitForTimeout(100);const paused=f.state.requests.filter(x=>x.startsWith('GET /api/stats')).length;
  await f.page.waitForTimeout(100);assert.equal(f.state.requests.filter(x=>x.startsWith('GET /api/stats')).length,paused);
  await f.page.evaluate(()=>{window.__hidden=false;document.dispatchEvent(new Event('visibilitychange'));window.dispatchEvent(new Event('online'))});
  await f.page.waitForTimeout(120);assert.ok(f.state.requests.filter(x=>x.startsWith('GET /api/stats')).length>paused);assert.equal(f.state.peak,1);
}));

check('hidden dashboards close their live stream and reopen exactly one on return',()=>run({stream:true},async f=>{
  await f.page.addInitScript(()=>{window.__hidden=false;Object.defineProperty(document,'hidden',{get:()=>window.__hidden})});
  await f.page.goto(f.url);await f.page.waitForFunction(()=>latestStats&&streamAt>0);
  assert.equal(f.state.activeStreams,1);
  await f.page.evaluate(()=>{window.__hidden=true;document.dispatchEvent(new Event('visibilitychange'))});
  await f.page.waitForTimeout(200);assert.equal(f.state.activeStreams,0);
  const snapshots=f.state.streamed;await f.page.waitForTimeout(150);assert.equal(f.state.streamed,snapshots);
  await f.page.evaluate(()=>{window.__hidden=false;document.dispatchEvent(new Event('visibilitychange'));document.dispatchEvent(new Event('visibilitychange'))});
  await f.page.waitForFunction(()=>latestStats&&streamAt>0);assert.equal(f.state.activeStreams,1);
  assert.equal(f.state.requests.filter(x=>x==='GET /api/events').length,2);
  await f.page.evaluate(()=>window.dispatchEvent(new Event('pagehide')));
  await f.page.waitForTimeout(150);assert.equal(f.state.activeStreams,0);
  await f.page.evaluate(()=>window.dispatchEvent(new Event('pageshow')));
  await f.page.waitForFunction(()=>streamAt>0);assert.equal(f.state.activeStreams,1);
}));

check('a delayed snapshot of the same boot cannot rewind live statistics or history',()=>run({stats:{boot_id:'same',uptime_s:100,history:undefined,history_revision:1}},async f=>{
  await f.page.goto(f.url);await f.page.waitForFunction(()=>latestStats?.uptime_s===100);
  const before=await f.page.evaluate(()=>({session:statsSession,history:historyRevision}));
  await f.page.evaluate(()=>show({...latestStats,uptime_s:99,hashrate:123,history_revision:0}));
  assert.equal(await f.page.evaluate(()=>latestStats.uptime_s),100);
  assert.equal(await f.page.evaluate(()=>statsSession),before.session);
  assert.equal(await f.page.evaluate(()=>historyRevision),before.history);
}));

check('log filtering and pause leave the original text available for download',()=>run({},async f=>{
  await f.page.goto(f.url+'/log');await f.page.waitForFunction(()=>document.querySelector('#l').textContent.includes('available'));
  await f.page.fill('#log-search','no-matching-line');assert.equal(await f.page.locator('#l').textContent(),'No matching entries.');
  await f.page.click('#pause-log');await f.page.waitForTimeout(80);const calls=f.state.logCalls;await f.page.waitForTimeout(100);assert.equal(f.state.logCalls,calls);
  const download=f.page.waitForEvent('download');await f.page.click('#download-log');const file=await download;assert.equal(file.suggestedFilename(),'bdlminer.log');
  await f.page.fill('#log-search','');assert.match(await f.page.locator('#l').textContent(),/available/);
  await f.page.click('#pause-log');await f.page.waitForTimeout(80);assert.ok(f.state.logCalls>calls);
}));

check('the live stream replaces polling and the dashboard follows it',()=>run({stream:true,stats:{hashrate:1000}},async f=>{
  await f.page.goto(f.url);await f.page.waitForFunction(()=>document.querySelector('#m')?.textContent.includes('kH/s'));
  await f.page.waitForTimeout(300);const polls=f.state.requests.filter(r=>r.startsWith('GET /api/stats')).length;
  await f.page.evaluate(()=>{const s=document.getElementById('st');return s&&s.dataset.state});
  await f.page.waitForTimeout(400);
  assert.ok(f.state.streamed>3);
  assert.equal(f.state.requests.filter(r=>r.startsWith('GET /api/stats')).length,polls); // no more polling
  assert.equal(await f.page.locator('#st').getAttribute('data-state'),'online');
}));

check('a failing pool statistics API is reported with the share estimate',()=>run({stats:{uptime_s:3600,shares:{accepted:2,rejected:0,work:2},pool_view:{updated_s:-1,supported:true,source:'public-pool.io:40557/api/client/',error:-4},integrations:{mqtt:'off',notify:{enabled:false}}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#pool-grid .t');
  const pool=await f.page.locator('#pool-grid').textContent();
  assert.match(pool,/estimated from accepted shares/);assert.match(pool,/statistics API is not answering \(no connection\)/);
  assert.match(await f.page.locator('#device-grid').textContent(),/Integrations⚠ check/);
}));

check('the browser reads a TLS 1.3-only public-pool API the miner cannot',()=>run({stats:{uptime_s:3600,hashrate:1.2e12,shares:{accepted:20,rejected:0,work:20},pool:{host:'public-pool.io',port:3333,connected:true,status:'Mining',difficulty:1024},address:'bc1qexampleaddressforbrowserpoolstatstest0000000000000000000000.gamma',pool_view:{updated_s:-1,supported:false}}},async f=>{
  const asked=[];
  await f.page.route('https://public-pool.io:40557/api/client/**',route=>{asked.push(route.request().url());
   route.fulfill({status:200,headers:{'access-control-allow-origin':'*'},contentType:'application/json',body:JSON.stringify({bestDifficulty:'15388278.28',workersCount:1,workers:[{sessionId:'a',name:'gamma',hashRate:'1180000000000'}],accounting:{totalAcceptedShares:1331}})})});
  await f.page.goto(f.url);
  await f.page.waitForFunction(()=>/read by this browser/.test(document.getElementById('pool-grid')?.textContent||''),null,{timeout:8000});
  const pool=await f.page.locator('#pool-grid').textContent();
  assert.match(pool,/1\.18 TH\/s/);assert.match(pool,/15\.4 M/);assert.match(pool,/public-pool\.io:40557\/api\/client\//);
  assert.deepEqual(asked,['https://public-pool.io:40557/api/client/bc1qexampleaddressforbrowserpoolstatstest0000000000000000000000']); // worker name not sent, asked once
}));

check('the browser does not ask the pool when the miner reads its API',()=>run({stats:{pool:{host:'public-pool.io',port:3333,connected:true,status:'Mining',difficulty:1},address:'bc1qexampleaddressforbrowserpoolstatstest0000000000000000000000',pool_view:{updated_s:60,supported:true,hashrate:1e6,best:2,workers:1,source:'public-pool.io:40557/api/client/'}}},async f=>{
  let asked=0;await f.page.route('https://public-pool.io:40557/**',route=>{asked++;route.abort()});
  await f.page.goto(f.url);await f.page.waitForSelector('#pool-grid .t');await f.page.waitForTimeout(300);
  assert.equal(asked,0);assert.doesNotMatch(await f.page.locator('#pool-grid').textContent(),/read by this browser/);
}));

check('NerdNOS ASIC tile shows only the readings the board has',()=>run({stats:{board:'NerdNOS',hashrate:2.1e11,engines:[{name:'BM1397',rate:2.1e11}],temperature_source:'asic',asic:{model:'BM1397',experimental:true,running:true,state:'Mining (experimental)',frequency_mhz:300,results:1234,hardware_error_percent:0.4}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#device-grid .t');
  const text=await f.page.locator('#device-grid').textContent();
  assert.match(text,/ASIC · BM1397/);assert.match(text,/300 MHz/);assert.match(text,/Verified ASIC results: 1234/);assert.match(text,/ASIC hardware errors: 0\.40 %/);
  assert.doesNotMatch(text,/undefined|Fan|Regulator|Input/);
  assert.match(await f.page.locator('#m').textContent(),/210 GH\/s/);
}));

check('a pool listing no worker is explained by the share interval',()=>run({stats:{hashrate:1e6,pool:{host:'public-pool.io',port:3333,connected:true,status:'Mining',difficulty:1},pool_view:{updated_s:60,supported:true,hashrate:0,best:116,workers:0,source:'public-pool.io:40557/api/client/'}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#pool-grid .t');
  assert.match(await f.page.locator('#pool-grid').textContent(),/No worker listed: .* at difficulty 1(\.0+)? this miner sends one every 1h 1[12]m\./);
}));

check('block candidates retain the proof height and distinguish a pool ACK from chain confirmation',()=>run({mobile:true,stats:{blocks:1,lifetime:{...baseStats.lifetime,blocks:3},network:{height:102,difficulty:1e14},block_candidate:{height:100,hash:'000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f',status:'share_accepted',chain_confirmed:null}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#records-grid .t');
  const tile=f.page.locator('#records-grid .t').filter({has:f.page.locator('.l',{hasText:'Block candidates'})});
  const text=await tile.textContent();
  assert.match(text,/Block candidates1/);assert.match(text,/lifetime 3/);
  assert.match(text,/Block height: 100/);assert.match(text,/Share accepted by pool/);
  assert.match(text,/chain confirmation unknown/);assert.match(text,/000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f/);
  assert.ok(await f.page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
}));

check('a saved block candidate remains visible after reboot without a new session count',()=>run({stats:{blocks:0,lifetime:{...baseStats.lifetime,blocks:1},block_candidate:{height:100,hash:'000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f',status:'unanswered',previous_session:true,chain_confirmed:null}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#records-grid .t');
  const tile=f.page.locator('#records-grid .t').filter({has:f.page.locator('.l',{hasText:'Block candidates'})});
  assert.match(await tile.textContent(),/Block candidates0/);
  assert.match(await tile.textContent(),/Saved from a previous session/);
  assert.match(await tile.textContent(),/No pool answer/);
}));

check('block candidate failures and unknown status are translated and safely rendered',()=>run({language:'fr',stats:{blocks:1,block_candidate:{height:100,hash:'<img src=x onerror=alert(1)>',status:'queue_full'}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#records-grid .t');
  assert.match(await f.page.locator('#records-grid').textContent(),/File de candidats pleine : non envoyé/);
  assert.equal(await f.page.locator('#records-grid img').count(),0);
  await f.page.evaluate(()=>{latestStats.block_candidate.status='unexpected';show(latestStats)});
  assert.match(await f.page.locator('#records-grid').textContent(),/État de transmission inconnu/);
}));

check('block candidate chain observations identify the source confirmations and separate payout',()=>run({stats:{blocks:1,block_candidate:{height:100,hash:'proof',status:'share_accepted',chain_confirmed:true,confirmations:3,chain_source:'mempool.space',chain_checked_s:30}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#records-grid .t');
  let text=await f.page.locator('#records-grid').textContent();
  assert.match(text,/Seen in the Bitcoin best chain · 3 confirmations/);
  assert.match(text,/Reported by mempool.space/);assert.match(text,/Pool payout is separate/);
  assert.doesNotMatch(text,/chain confirmation unknown/);
  f.state.stats.block_candidate.chain_confirmed=false;f.state.stats.block_candidate.confirmations=0;
  await f.page.evaluate(()=>{latestStats.block_candidate.chain_confirmed=false;latestStats.block_candidate.confirmations=0;show(latestStats)});
  assert.match(await f.page.locator('#records-grid').textContent(),/Outside the explorer best chain/);
  f.state.stats.block_candidate.chain_error=503;f.state.stats.block_candidate.chain_confirmed=true;
  await f.page.evaluate(()=>{latestStats.block_candidate.chain_error=503;latestStats.block_candidate.chain_confirmed=true;show(latestStats)});
  text=await f.page.locator('#records-grid').textContent();
  assert.match(text,/chain confirmation unknown/);assert.match(text,/Explorer check failed \(503\)/);
  assert.doesNotMatch(text,/Seen in the Bitcoin best chain/);
}));

check('unindexed block candidate remains uncertain and explorer text is translated',()=>run({language:'fr',stats:{blocks:1,block_candidate:{height:100,hash:'proof',status:'share_rejected',chain_confirmed:null,chain_not_found:true,chain_error:404,chain_source:'mempool.space',chain_checked_s:-1}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#records-grid .t');
  const text=await f.page.locator('#records-grid').textContent();
  assert.match(text,/confirmation sur la chaîne inconnue/);
  assert.match(text,/Pas encore indexé par l’explorateur/);
  assert.match(text,/Le paiement dépend de la pool/);
}));

check('recent shares list the pool answers, newest first',()=>run({stats:{shares_log:[{t:0,d:1.5,s:'pending'},{t:1793214889,d:0.25,s:'accepted',b:true},{t:1793214800,d:3,s:'rejected'},{t:1793214700,d:2,s:'unanswered'}]}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#records-grid .t');
  const text=await f.page.locator('#records-grid').textContent();
  assert.match(text,/Recent shares1 \/ 4 accepted/);assert.match(text,/--:--:--  … Waiting · diff 1\.5/);
  assert.match(text,/✓ Accepted · diff 0\.25\d* · backup pool/);assert.match(text,/✗ Rejected · diff 3/);assert.match(text,/\? No answer · diff 2/);
  assert.ok(text.indexOf('Waiting')<text.indexOf('Accepted'));
}));

check('integrations, pool reliability and crash history tiles',()=>run({stats:{pool:{host:'pool.example',port:3333,connected:true,status:'Mining',difficulty:1,connected_percent:98.4,sessions:3,failovers:1},integrations:{mqtt:'disconnected',mqtt_host:'ha.local:1883',notify:{enabled:true,title:'Test notification',ntfy:'off',webhook:'OK'}},pool_view:{updated_s:60,supported:true,hashrate:1e6,best:2,workers:1,source:'public-pool.io:40557/api/client/'},crash:{history:[{t:1793000000,version:'1.6.7',summary:'abort() called in task loopTask, backtrace 0x1'},{t:0,version:'1.6.6',summary:'task IDLE, LoadProhibited (cause 28) at 0x00000010'}]}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#device-grid .t');
  const pool=await f.page.locator('#pool-grid').textContent();
  assert.match(pool,/connected 98\.4 % since restart · 2 reconnections · 1 switch to backup/);assert.match(pool,/via public-pool\.io:40557/);
  const device=await f.page.locator('#device-grid').textContent();
  assert.match(device,/Integrations⚠ check/);assert.match(device,/MQTT: disconnected · ha\.local:1883/);assert.match(device,/webhook OK \(Test notification\)/);
  assert.match(device,/Crash history2/);assert.match(device,/v1\.6\.7 · abort\(\) called in task loopTask/);assert.match(device,/date unknown · v1\.6\.6/);
  assert.doesNotMatch(device,/backtrace 0x1/);assert.equal(await f.page.locator('#device-grid a[href="/api/coredump"]').count(),0);
}));

check('live settings save without a restart and keep the page usable',()=>run({live:true,postText:'{"restart":false,"changed":1}'},async f=>{
  await loaded(f);await f.page.fill('#k_power','60');
  assert.equal(await f.page.locator('#save').textContent(),'Save');assert.match(await f.page.locator('#dirty').textContent(),/applied right away/);
  await f.page.click('#save');await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('applied'));
  assert.equal(await f.page.locator('#save').isEnabled(),true);assert.equal(await f.page.locator('.field.changed').count(),0);
  assert.equal(JSON.parse(f.state.posts[0].body).power,60);
  await f.page.fill('#k_pool','other.pool');assert.equal(await f.page.locator('#save').textContent(),'Save and restart');
  assert.match(await f.page.locator('#dirty').textContent(),/needs a restart/);
}));

check('pool sharing is on by default and can be disabled live in French',()=>run({poolSharing:true,language:'fr',postText:'{"restart":false,"changed":1}'},async f=>{
  await loaded(f);
  assert.equal(await f.page.locator('#k_sharepool').isChecked(),true);
  assert.match(await f.page.locator('.field').filter({has:f.page.locator('#k_sharepool')}).textContent(),/Partager ma pool anonymement.*Activé par défaut/s);
  await f.page.uncheck('#k_sharepool');
  assert.equal(await f.page.locator('#save').textContent(),'Enregistrer');
  await f.page.click('#save');await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('appliqu'));
  assert.equal(JSON.parse(f.state.posts[0].body).sharepool,false);
  assert.equal(f.state.requests.some(r=>r.includes('/report')),false);
}));

check('private remote monitoring is opt-in, pairs a secret key and links to its own viewer',()=>run({remote:true,language:'fr'},async f=>{
  await loaded(f);assert.equal(await f.page.locator('#k_remote').isChecked(),false);
  assert.equal(await f.page.locator('#k_remotekey').getAttribute('type'),'password');
  assert.equal(await f.page.locator('#k_remotekey').getAttribute('maxlength'),'64');
  assert.equal(await f.page.locator('#k_rname').getAttribute('maxlength'),'48');
  const viewer=f.page.getByRole('link',{name:'Ouvrir mon dashboard distant'});
  assert.equal(await viewer.getAttribute('href'),'https://miner.bdl-pool.com/remote.html');
  const key='0123456789abcdef'.repeat(4);
  await f.page.fill('#k_remotekey',key);await f.page.fill('#k_rname','M5 bureau');await f.page.check('#k_remote');
  assert.equal(await f.page.locator('#save').textContent(),'Enregistrer et redémarrer');
  await f.page.click('#save');await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('redémarr'));
  const data=JSON.parse(f.state.posts[0].body);assert.equal(data.remote,true);assert.equal(data.remotekey,key);assert.equal(data.rname,'M5 bureau');
  assert.equal(f.state.requests.some(r=>r.includes('/api/remote')),false);
}));

check('choice controls save numbers and discarding restores the saved values',()=>run({},async f=>{
  await loaded(f);assert.equal(await f.page.locator('#k_nightfrom').inputValue(),'255');assert.equal(await f.page.locator('#discard').isVisible(),false);
  await f.page.selectOption('#k_nightfrom','22');assert.equal(await f.page.locator('#discard').isVisible(),true);
  assert.equal(await f.page.locator('.field.changed').count(),1);
  await f.page.click('#discard');assert.equal(await f.page.locator('#k_nightfrom').inputValue(),'255');assert.equal(await f.page.locator('.field.changed').count(),0);
  await f.page.selectOption('#k_nightfrom','22');await f.page.click('#save');
  await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('restarting'));
  assert.strictEqual(JSON.parse(f.state.posts[0].body).nightfrom,22);
}));

check('log categories and warnings filter whole device lines',()=>run({},async f=>{
  f.state.logText='00:00:01 [pool] Connecting\n00:00:02 [miner] hashing\n00:00:03 [wifi] connection failed';
  await f.page.goto(f.url+'/log');await f.page.waitForFunction(()=>document.querySelector('#l').textContent.includes('hashing'));
  assert.equal(await f.page.locator('#sum-lines').textContent(),'3');assert.equal(await f.page.locator('#sum-alerts').textContent(),'1');
  await f.page.click('[data-category=pool]');assert.equal(await f.page.locator('#l').textContent(),'00:00:01 [pool] Connecting');
  await f.page.click('[data-category=alerts]');assert.equal(await f.page.locator('#l').textContent(),'00:00:03 [wifi] connection failed');
  await f.page.click('[data-category=all]');await f.page.fill('#log-search','hash');assert.equal(await f.page.locator('#l mark').textContent(),'hash');
  assert.equal(await f.page.locator('#l').textContent(),'00:00:02 [miner] hashing');
}));

check('restart cancellation is harmless and accepted restarts lock actions',()=>run({},async f=>{
  await loaded(f);f.page.once('dialog',dialog=>dialog.dismiss());await f.page.click('#restart');assert.equal(f.state.posts.length,0);
  f.page.once('dialog',dialog=>dialog.accept());await f.page.click('#restart');
  await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('Restarting'));
  assert.equal(f.state.posts[0].url,'/api/restart');assert.equal(await f.page.locator('#save').isDisabled(),true);
}));

check('mobile dashboard and log remain within the viewport',()=>run({mobile:true,language:'fr'},async f=>{
  for(const page of ['/','/log']){await f.page.goto(f.url+page);await f.page.waitForTimeout(100);assert.equal(await f.page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true)}
}));


check('light polls fetch history only when its revision changes',()=>run({stats:{history:undefined,history_revision:1}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#history-grid .t');
  assert.equal(f.state.historyCalls,1);await f.page.waitForTimeout(150);assert.equal(f.state.historyCalls,1);
  assert.ok(f.state.requests.some(x=>x==='GET /api/stats?history=0'));
  f.state.history={...f.state.history,revision:2};f.state.stats.history_revision=2;
  await f.page.waitForTimeout(120);assert.equal(f.state.historyCalls,2);
}));
check('history failures do not mark mining offline and do not hammer the endpoint',()=>run({stats:{history:undefined,history_revision:1},historyFailure:true},async f=>{
  await f.page.goto(f.url);await f.page.waitForFunction(()=>document.querySelector('#history-status').textContent.length>0);
  await f.page.waitForTimeout(160);assert.equal(f.state.historyCalls,1);assert.equal(await f.page.locator('#st').textContent(),'ONLINE');
}));
check('history CSV retains sensor gaps and power limits',()=>run({stats:{history:undefined,history_revision:1},history:{hashrate:[1000000,500000],temperature_c:[null,60],power_percent:[100,50],coverage_percent:[100,98],interval_s:300,revision:1}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#history-grid .t');
  const waiting=f.page.waitForEvent('download');await f.page.click('#download-history');const download=await waiting;
  const stream=await download.createReadStream();let text='';for await(const chunk of stream)text+=chunk.toString();
  assert.match(text,/5,1000000,,100,100/);assert.match(text,/0,500000,60,50,98/);
}));
check('diagnostics show worker wall times and TLS counts',()=>run({},async f=>{
  await f.page.goto(f.url);await f.page.locator('#diagnostics>summary').click();await f.page.waitForSelector('#profile-grid .t');assert.match(await f.page.locator('#profile-grid').textContent(),/TLS handshakes/);
  assert.match(await f.page.locator('#profile-grid').textContent(),/Mining worker 0/);
}));
check('failed flash saves stay translated and allow another attempt',()=>run({language:'fr',postStatus:500,postText:'Settings could not be saved. Previous configuration kept; try again.'},async f=>{
  await loaded(f);await f.page.click('#save');await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('configuration précédente'));
  assert.equal(await f.page.locator('#save').isEnabled(),true);assert.equal(await f.page.locator('#restart').isEnabled(),true);
  f.state.postStatus=200;f.state.postText=null;await f.page.click('#save');await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('redémarrage'));assert.equal(f.state.posts.length,2);assert.equal(await f.page.locator('#save').isDisabled(),true);
}));
check('GitHub download retries are visible and can be requested immediately',()=>run({language:'fr',stats:{github_update:{available:true,mode:2,version:'1.2.0',retry_in_s:300,error:'download failed'}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('.gh-install');assert.equal(await f.page.locator('.gh-install').textContent(),'Réessayer maintenant');
  assert.match(await f.page.locator('#updates').textContent(),/Prochaine tentative dans 5m/);
}));
check('measurement uses actual hash deltas and exports no settings secrets',()=>run({},async f=>{
  await f.page.goto(f.url);await f.page.locator('#diagnostics>summary').click();await f.page.waitForSelector('#start-benchmark:enabled');await f.page.click('#start-benchmark');
  await f.page.waitForSelector('#stop-benchmark:enabled');f.state.stats.uptime_s=300;f.state.stats.total_hashes=300000000;f.state.stats.temp_c=60;f.state.stats.power_percent=50;
  await f.page.waitForFunction(()=>document.querySelector('#benchmark-status').textContent.includes('5m'));
  await f.page.click('#stop-benchmark');await f.page.waitForSelector('#download-benchmark:enabled');
  const waiting=f.page.waitForEvent('download');await f.page.click('#download-benchmark');const download=await waiting;
  const stream=await download.createReadStream();let text='';for await(const chunk of stream)text+=chunk.toString();const report=JSON.parse(text);
  assert.equal(report.hashrate_hps,1000000);assert.equal(report.elapsed_s,300);assert.equal(report.temperature_max_c,60);assert.equal(report.power_min_percent,50);
  assert.equal(report.sample_gaps,true);assert.ok(report.profiling_start&&report.profiling_end);assert.equal(text.includes('password'),false);
}));
check('measurement is cancelled by a restart or a hidden tab',()=>run({},async f=>{
  f.state.stats.uptime_s=100;f.state.stats.total_hashes=100000000;
  await f.page.addInitScript(()=>{window.__hidden=false;Object.defineProperty(document,'hidden',{get:()=>window.__hidden})});
  await f.page.goto(f.url);await f.page.locator('#diagnostics>summary').click();await f.page.waitForSelector('#start-benchmark:enabled');await f.page.click('#start-benchmark');await f.page.waitForSelector('#stop-benchmark:enabled');
  f.state.stats.uptime_s=2;f.state.stats.total_hashes=1000;await f.page.waitForFunction(()=>document.querySelector('#benchmark-status').textContent.includes('restarted'));
  assert.equal(await f.page.locator('#download-benchmark').isDisabled(),true);
  await f.page.click('#start-benchmark');await f.page.waitForSelector('#stop-benchmark:enabled');
  await f.page.evaluate(()=>{window.__hidden=true;document.dispatchEvent(new Event('visibilitychange'))});
  assert.match(await f.page.locator('#benchmark-status').textContent(),/cancelled/);assert.equal(await f.page.locator('#stop-benchmark').isDisabled(),true);
}));

check('pending dashboard poll cannot replace newer live data from another boot',()=>run({realPolling:true,stats:{boot_id:'old',uptime_s:100,total_hashes:100000000}},async f=>{
  await f.page.goto(f.url);await f.page.waitForFunction(()=>latestStats?.boot_id==='old');
  const stale=clone(f.state.stats);
  await f.page.route('**/api/stats?history=0',async route=>{
    await f.page.evaluate(()=>show({...latestStats,boot_id:'new',uptime_s:2,total_hashes:2000000}));
    await route.fulfill({status:200,contentType:'application/json',body:JSON.stringify(stale)});
  });
  const current=await f.page.evaluate(async()=>{streamAt=0;await tick();return {boot:latestStats.boot_id,uptime:latestStats.uptime_s,session:statsSession}});
  assert.deepEqual(current,{boot:'new',uptime:2,session:1});
}));
check('pending dashboard poll failure cannot mark newer live data offline',()=>run({realPolling:true},async f=>{
  await f.page.goto(f.url);await f.page.waitForFunction(()=>statsOnline);
  await f.page.route('**/api/stats?history=0',async route=>{
    await f.page.evaluate(()=>show({...latestStats,uptime_s:100,total_hashes:100000000}));
    await route.fulfill({status:503,body:'Old request failed'});
  });
  const current=await f.page.evaluate(async()=>{streamAt=0;await tick();return {online:statsOnline,status:document.querySelector('#st').textContent}});
  assert.deepEqual(current,{online:true,status:'ONLINE'});
}));
check('measurement preparation rejects a reboot while old diagnostics are in flight',()=>run({realPolling:true,stats:{boot_id:'old',uptime_s:100,total_hashes:100000000}},async f=>{
  await f.page.goto(f.url);await f.page.waitForFunction(()=>statsOnline);
  await f.page.route('**/api/diagnostics',async route=>{
    await f.page.evaluate(()=>show({...latestStats,boot_id:'new',uptime_s:200,total_hashes:200000000}));
    await route.fulfill({status:200,contentType:'application/json',body:JSON.stringify({...f.state.diagnostics,boot_id:'old',uptime_s:101})});
  });
  const result=await f.page.evaluate(async()=>{await startBenchmark();return {active:!!benchmark,status:benchmarkStatus.textContent}});
  assert.equal(result.active,false);assert.match(result.status,/restarted/);
}));


check('measurement automatically finishes after ten minutes of device uptime',()=>run({},async f=>{
  f.state.stats.uptime_s=100;f.state.stats.total_hashes=100000000;
  await f.page.goto(f.url);await f.page.locator('#diagnostics>summary').click();await f.page.waitForSelector('#start-benchmark:enabled');await f.page.click('#start-benchmark');await f.page.waitForSelector('#stop-benchmark:enabled');
  f.state.stats.uptime_s=700;f.state.stats.total_hashes=700000000;
  await f.page.waitForSelector('#download-benchmark:enabled');assert.match(await f.page.locator('#benchmark-status').textContent(),/1\.00 MH\/s/);
  assert.equal(await f.page.locator('#stop-benchmark').isDisabled(),true);
}));
check('boot identity invalidates history and measurements even with higher counters',()=>run({stats:{history:undefined,history_revision:1,boot_id:'old',uptime_s:100,total_hashes:100000000}},async f=>{
  f.state.diagnostics.boot_id='old';await f.page.goto(f.url);await f.page.waitForSelector('#history-grid .t');await f.page.locator('#diagnostics>summary').click();
  await f.page.click('#start-benchmark');await f.page.waitForSelector('#stop-benchmark:enabled');
  f.state.stats.boot_id='new';f.state.stats.uptime_s=200;f.state.stats.total_hashes=200000000;
  await f.page.waitForFunction(()=>document.querySelector('#benchmark-status').textContent.includes('restarted'));
  assert.equal(await f.page.locator('#download-benchmark').isDisabled(),true);assert.equal(f.state.historyCalls,2);
}));

check('chart periods filter locally, retain focus and export only the visible samples',()=>run({stats:{history:undefined,history_revision:1},history:{hashrate:Array.from({length:288},(_,i)=>(i+1)*1000),temperature_c:Array(288).fill(50),power_percent:Array(288).fill(100),coverage_percent:Array(288).fill(100),interval_s:300,revision:1}},async f=>{
 await f.page.goto(f.url);await f.page.waitForFunction(()=>document.querySelector('#chart-count').textContent==='73');
 await f.page.click('[data-range="3600"]');assert.equal(await f.page.locator('#chart-count').textContent(),'13');assert.equal(await f.page.locator('#chart-average').textContent(),'282 kH/s');
 await f.page.waitForTimeout(150);assert.equal(f.state.historyCalls,1);assert.equal(await f.page.locator('[data-range="3600"]').evaluate(e=>e===document.activeElement),true);
 const waiting=f.page.waitForEvent('download');await f.page.click('#download-history');const file=await waiting,stream=await file.createReadStream();let csv='';for await(const chunk of stream)csv+=chunk.toString();assert.equal(csv.trim().split('\n').length,14);assert.match(csv,/60,276000,50,100,100/);
 await f.page.reload();await f.page.waitForFunction(()=>document.querySelector('#chart-count').textContent==='13');assert.equal(await f.page.locator('[data-range="3600"]').getAttribute('aria-pressed'),'true');
 await f.page.click('[data-range="86400"]');assert.equal(await f.page.locator('#chart-count').textContent(),'288');
}));
check('charts preserve zeroes and missing positions and synchronize keyboard inspection',()=>run({stats:{history:undefined,history_revision:1,max_temp_c:65},history:{hashrate:[1000000,null,0,500000],temperature_c:[40,null,45,50],power_percent:[100,null,0,50],coverage_percent:[100,0,100,98],interval_s:300,revision:1}},async f=>{
 await f.page.goto(f.url);await f.page.waitForFunction(()=>document.querySelector('#chart-count').textContent==='3');
 const path=await f.page.locator('[data-series="hashrate"]').getAttribute('d');assert.equal((path.match(/M/g)||[]).length,2);assert.equal(/NaN|Infinity/.test(path),false);
 assert.equal(await f.page.locator('.plot-limit').count(),1);assert.match(await f.page.locator('#history-grid').textContent(),/Configured limit 65/);
 await f.page.locator('#hashrate-history').focus();await f.page.keyboard.press('Home');assert.match(await f.page.locator('#chart-inspection').textContent(),/1\.00 MH\/s/);
 await f.page.keyboard.press('ArrowRight');assert.equal(await f.page.locator('#chart-inspection strong').allTextContents().then(a=>a.join('')),'———');assert.equal(await f.page.locator('.plot-dot[visibility="visible"]').count(),0);
 await f.page.keyboard.press('ArrowRight');assert.match(await f.page.locator('#chart-inspection').textContent(),/0 H\/s/);assert.equal(await f.page.locator('.plot-dot[visibility="visible"]').count(),3);
 await f.page.keyboard.press('End');assert.match(await f.page.locator('#chart-inspection').textContent(),/Latest point.*500 kH\/s.*98 %/);
}));
check('live polls retain metric and chart nodes and a focused update button',()=>run({stats:{history:undefined,history_revision:1,github_update:{available:false,mode:1,checked:true}}},async f=>{
 await f.page.goto(f.url);await f.page.waitForFunction(()=>document.querySelector('#chart-count').textContent==='2');await f.page.waitForTimeout(80);
 await f.page.evaluate(()=>{window.__chart=document.querySelector('#hashrate-history svg');window.__rate=document.querySelector('#m .hero .v')});await f.page.locator('.gh-check').focus();
 f.state.stats.hashrate=1234567;await f.page.waitForFunction(()=>document.querySelector('#m .hero .v').textContent==='1.23 MH/s');
 assert.equal(await f.page.evaluate(()=>window.__chart===document.querySelector('#hashrate-history svg')&&window.__rate===document.querySelector('#m .hero .v')),true);assert.equal(await f.page.locator('.gh-check').evaluate(e=>e===document.activeElement),true);assert.equal(f.state.historyCalls,1);
}));
check('diagnostics load on demand and peer polling is paced independently',()=>run({realPolling:true,stats:{peers:['peer.test'],hashrate:1000}},async f=>{
 let calls=0;await f.page.route('http://peer.test/api/stats?history=0',route=>{calls++;return route.fulfill({json:{hashrate:2000,pool:{connected:true},shares:{accepted:3},best:{ever:1}}})});
 await f.page.goto(f.url);await f.page.waitForSelector('#p .t');await f.page.waitForTimeout(180);assert.equal(calls,1);assert.equal(f.state.requests.some(r=>r==='GET /api/diagnostics'),false);
 await f.page.locator('#diagnostics>summary').click();await f.page.waitForSelector('#profile-grid .t');assert.equal(f.state.requests.filter(r=>r==='GET /api/diagnostics').length,1);
 await f.page.locator('#diagnostics>summary').click();assert.equal(await f.page.locator('#diagnostics').getAttribute('open'),null);
}));
check('charts remain readable and bounded after mobile and desktop resizing',()=>run({stats:{history:undefined,history_revision:1}},async f=>{
 await f.page.goto(f.url);await f.page.waitForFunction(()=>document.querySelector('#chart-count').textContent==='2');
 for(const width of [320,375,768,1440]){await f.page.setViewportSize({width,height:850});await f.page.waitForTimeout(80);assert.equal(await f.page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);const view=await f.page.locator('#hashrate-history svg').getAttribute('viewBox');assert.ok(Number(view.split(' ')[2])>=240);assert.equal(await f.page.locator('path[data-series]').evaluateAll(nodes=>nodes.some(n=>/NaN|Infinity/.test(n.getAttribute('d')))),false)}
 assert.equal(f.state.historyCalls,1);
}));
check('pointer and touch inspection select actual points without changing polling',()=>run({stats:{history:undefined,history_revision:1}},async f=>{
 await f.page.goto(f.url);await f.page.waitForFunction(()=>document.querySelector('#chart-count').textContent==='2');
 const box=await f.page.locator('#hashrate-history').boundingBox();await f.page.mouse.move(box.x+62,box.y+60);await f.page.waitForTimeout(30);assert.match(await f.page.locator('#chart-inspection').textContent(),/1\.00 MH\/s/);
 await f.page.locator('#hashrate-history').dispatchEvent('pointerdown',{pointerType:'touch',clientX:box.x+box.width-20,clientY:box.y+60});await f.page.waitForTimeout(30);assert.match(await f.page.locator('#chart-inspection').textContent(),/900 kH\/s/);
 assert.equal(f.state.historyCalls,1);
}));
check('empty and single-point histories show honest states without invented samples',()=>run({stats:{history:undefined,history_revision:1},history:{hashrate:[],temperature_c:[],power_percent:[],coverage_percent:[],interval_s:300,revision:1}},async f=>{
 await f.page.goto(f.url);await f.page.waitForSelector('#hashrate-history .plot-empty');assert.equal(await f.page.locator('#download-history').isDisabled(),true);assert.match(await f.page.locator('#hashrate-history').textContent(),/first history point appears after five minutes/);
 f.state.history={hashrate:[0],temperature_c:[null],power_percent:[0],coverage_percent:[100],interval_s:300,revision:2};f.state.stats.history_revision=2;
 await f.page.waitForFunction(()=>document.querySelector('#chart-count').textContent==='1');assert.equal(await f.page.locator('#chart-average').textContent(),'0 H/s');assert.equal(await f.page.locator('#download-history').isEnabled(),true);assert.match(await f.page.locator('#history-grid').textContent(),/usable temperature sensor/);
}));

check('Gamma ASIC dashboard displays hardware diagnostics and TH/s history',()=>run({stats:{board:'Bitaxe-Gamma-601',hashrate:1.2e12,hashrate_live:1.19e12,history:undefined,history_revision:1,asic:{model:'BM1370',experimental:true,running:true,state:'Mining (experimental)',frequency_mhz:525,fan_rpm:5000,core_v:1.15,current_a:15,input_v:5.1,regulator_temp_c:55}},history:{hashrate:[1.1e12,1.2e12],temperature_c:[55,56],power_percent:[100,100],coverage_percent:[100,100],interval_s:300,revision:1}},async f=>{
 await f.page.goto(f.url);await f.page.waitForFunction(()=>document.querySelector('#chart-count').textContent==='2');
 assert.match(await f.page.locator('#m .hero .v').textContent(),/1\.20 TH\/s/);
 assert.match(await f.page.locator('#device-grid').textContent(),/ASIC · BM1370.*Mining.*Experimental.*525 MHz.*1\.150 V.*5000 RPM/s);
 assert.match(await f.page.locator('#hashrate-history').textContent(),/TH\/s/);
 assert.equal(await f.page.locator('#health').isHidden(),true);
}));
check('Gamma stopped ASIC overrides pool-online status and escapes fault text',()=>run({stats:{asic:{model:'BM1370',running:false,state:'Fan stopped <script>bad()</script>',frequency_mhz:0,fan_rpm:0,core_v:null,current_a:null,input_v:null,regulator_temp_c:null}}},async f=>{
 await f.page.goto(f.url);await f.page.waitForSelector('#device-grid .t');
 assert.equal(await f.page.locator('#health').textContent(),'Fan stopped <script>bad()</script>');
 assert.equal(await f.page.locator('#health').getAttribute('data-level'),'bad');
 assert.equal(await f.page.locator('#device-grid script').count(),0);
 assert.match(await f.page.locator('#device-grid').textContent(),/— V/);
}));

check('Gamma manual fan defaults to full speed and applies live on mobile',()=>run({gamma:true,mobile:true,postText:'{"restart":false,"changed":1}'},async f=>{
  await loaded(f);const slider=f.page.locator('#k_fanpct');
  assert.equal(await slider.inputValue(),'100');assert.equal(await slider.getAttribute('min'),'30');assert.equal(await slider.getAttribute('max'),'100');
  assert.equal(await f.page.locator('#k_fanauto').inputValue(),'false');assert.equal(await slider.isEnabled(),true);
  await slider.focus();await slider.press('Home');await slider.press('ArrowRight');
  assert.equal(await slider.inputValue(),'31');assert.equal(await f.page.locator('#fan-pct-value').textContent(),'31 %');
  await slider.evaluate(el=>{el.value='40';el.dispatchEvent(new Event('input',{bubbles:true}))});
  assert.equal(await f.page.locator('#save').textContent(),'Save');await f.page.click('#save');
  await f.page.waitForFunction(()=>document.querySelector('#msg').textContent==='Saved and applied.');
  const posted=JSON.parse(f.state.posts[0].body);assert.equal(posted.fanpct,40);assert.equal(posted.fanauto,false);
  assert.equal(await f.page.locator('.field.changed').count(),0);assert.equal(await f.page.locator('#save').isEnabled(),true);
  assert.equal(await f.page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
}));
check('Gamma automatic fan keeps its manual preset and discard restores its mode',()=>run({gamma:true,postText:'{"restart":false,"changed":1}'},async f=>{
  f.state.settings.values.fanpct=45;await loaded(f);await f.page.selectOption('#k_fanauto','true');
  assert.equal(await f.page.locator('#k_fanpct').isDisabled(),true);assert.equal(await f.page.locator('#save').textContent(),'Save');
  await f.page.click('#save');await f.page.waitForFunction(()=>document.querySelector('#msg').textContent==='Saved and applied.');
  const posted=JSON.parse(f.state.posts[0].body);assert.equal(posted.fanauto,true);assert.equal(posted.fanpct,45);
  await f.page.selectOption('#k_fanauto','false');assert.equal(await f.page.locator('#k_fanpct').isEnabled(),true);
  await f.page.click('#discard');assert.equal(await f.page.locator('#k_fanauto').inputValue(),'true');assert.equal(await f.page.locator('#k_fanpct').isDisabled(),true);
  assert.equal(await f.page.locator('#fan-pct-value').textContent(),'45 %');
}));
check('Gamma fan safety help and controls are translated in French',()=>run({gamma:true,language:'fr'},async f=>{
  await loaded(f);assert.equal(await f.page.locator('label[for="k_fanauto"]').textContent(),'Réglage du ventilateur');
  assert.equal(await f.page.locator('label[for="k_fanpct"]').textContent(),'Vitesse manuelle du ventilateur');
  assert.match(await f.page.locator('[data-key="fanpct"]').textContent(),/60 °C.*80 °C/);
  await f.page.fill('#k_maxtemp','0');assert.match(await f.page.locator('[data-key="maxtemp"]').textContent(),/protections matérielles.*restent actives/);
  await f.page.selectOption('#k_fanauto','true');assert.equal(await f.page.locator('#k_fanpct').isDisabled(),true);
}));
check('Gamma dashboard shows fan command RPM mode and safety override',()=>run({stats:{asic:{model:'BM1370',running:true,state:'Mining',fan_percent:65,fan_requested_percent:40,fan_auto:true,fan_override:false,fan_reason:'Automatic',fan_rpm:3200,regulator_temp_c:55}}},async f=>{
  await f.page.goto(f.url);await f.page.waitForFunction(()=>document.querySelector('#device-grid').textContent.includes('Fan: 65 % · 3200 RPM'));
  assert.match(await f.page.locator('#device-grid').textContent(),/Fan control: Automatic/);
  assert.equal(await f.page.locator('#device-grid a[href="/settings"]').textContent(),'Adjust fan');
  Object.assign(f.state.stats.asic,{fan_percent:100,fan_override:true,fan_reason:'Temperature safety'});
  await f.page.waitForFunction(()=>document.querySelector('#device-grid').textContent.includes('Cooling priority: Temperature safety'));
}));
check('NerdNOS settings do not expose a Gamma fan controller',()=>run({capabilities:{firmware_env:'nerdnos'}},async f=>{
  await loaded(f);assert.equal(await f.page.locator('#k_fanauto,#k_fanpct').count(),0);
}));

check('Gamma profile and core voltage default to ESP-Miner stock and save the selected values',()=>run({gamma:true},async f=>{
 await f.page.goto(f.url+'/settings');await f.page.waitForSelector('#k_asicmhz');
 assert.equal(await f.page.locator('#k_asicmhz').inputValue(),'525');
 assert.deepEqual(await f.page.locator('#k_asicmhz option').evaluateAll(nodes=>nodes.map(n=>Number(n.value))),[525,550,575,600,625,650,675,690,700,725,750,775,800]);
 assert.equal(await f.page.locator('#k_asicmv').inputValue(),'1150');
 assert.deepEqual(await f.page.locator('#k_asicmv option').evaluateAll(nodes=>nodes.map(n=>Number(n.value))),[1000,1060,1100,1150,1200,1250,1300]);
 assert.match(await f.page.locator('#performance').textContent(),/up to 690 MHz.*up to 800 MHz.*core voltage to the one that goes with it.*525 MHz fallback.*Default: 1150 mV.*1300 mV up to 800/s);
 await f.page.selectOption('#k_asicmhz','600');await f.page.selectOption('#k_asicmv','1200');await f.page.click('#save');
 await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('Saved'));
 const body=JSON.parse(f.state.posts.find(p=>p.url==='/api/settings').body);
 assert.equal(body.asicmhz,600);assert.equal(body.asicmv,1200);
}));
check('Gamma profile raises the core voltage to the one that goes with it, never lowers it',()=>run({gamma:true},async f=>{
 await f.page.goto(f.url+'/settings');await f.page.waitForSelector('#k_asicmhz');
 for(const [mhz,mv] of [['600','1150'],['650','1200'],['690','1250'],['800','1300']]){
  await f.page.selectOption('#k_asicmhz',mhz);assert.equal(await f.page.locator('#k_asicmv').inputValue(),mv);
 }
 await f.page.selectOption('#k_asicmhz','525');assert.equal(await f.page.locator('#k_asicmv').inputValue(),'1300');
 await f.page.selectOption('#k_asicmv','1200');await f.page.selectOption('#k_asicmhz','550');
 assert.equal(await f.page.locator('#k_asicmv').inputValue(),'1200');
 await f.page.selectOption('#k_asicmhz','775');await f.page.click('#save');
 await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('Saved'));
 const body=JSON.parse(f.state.posts.find(p=>p.url==='/api/settings').body);
 assert.equal(body.asicmhz,775);assert.equal(body.asicmv,1300);
}));
check('Gamma saved 690 MHz and 1250 mV stay selected and are explained in French',()=>run({gamma:true,language:'fr'},async f=>{
 Object.assign(f.state.settings.values,{asicmhz:690,asicmv:1250});
 await f.page.goto(f.url+'/settings');await f.page.waitForSelector('#k_asicmhz');
 assert.equal(await f.page.locator('#k_asicmhz').inputValue(),'690');assert.equal(await f.page.locator('#k_asicmv').inputValue(),'1250');
 assert.equal(await f.page.locator('label[for="k_asicmhz"]').textContent(),'Fréquence ASIC Gamma (MHz)');
 assert.equal(await f.page.locator('label[for="k_asicmv"]').textContent(),'Tension de cœur ASIC Gamma (mV)');
 assert.match(await f.page.locator('#performance').textContent(),/jusqu’à 690 MHz.*jusqu’à 800 MHz.*Par défaut : 525 MHz.*tension de cœur à celle qui va avec.*Par défaut : 1 150 mV/s);
}));
check('Gamma overclock fallback remains visible with requested clock and measured errors',()=>run({stats:{asic:{model:'BM1370',running:true,state:'Mining (525 MHz fallback profile)',frequency_mhz:525,profile_mhz:600,stock_fallback:true,fallback_reason:'High ASIC hardware error rate',hardware_error_percent:3.5,fan_rpm:5000,core_v:1.15,current_a:15,input_v:5.1,regulator_temp_c:55}}},async f=>{
 await f.page.goto(f.url);await f.page.waitForSelector('#device-grid .t');
 assert.match(await f.page.locator('#health').textContent(),/Overclock fallback/);
 assert.equal(await f.page.locator('#health').getAttribute('data-level'),'info');
 assert.match(await f.page.locator('#device-grid').textContent(),/525 MHz.*Requested profile: 600 MHz.*ASIC hardware errors: 3\.50 %/s);
}));

if(process.env.BDL_SCREENSHOT_DIR){
 const previewHistory={hashrate:[],temperature_c:[],power_percent:[],coverage_percent:[],interval_s:300,revision:1};
 for(let i=0;i<288;i++){const limited=i>=244&&i<252,gap=i>=262&&i<265;previewHistory.hashrate.push(gap?null:Math.round((limited?510000:997000)+17000*Math.sin(i/9)+8000*Math.cos(i/3)));previewHistory.temperature_c.push(gap?null:Math.round((51+8*Math.sin(i/19)+(limited?3:0))*10)/10);previewHistory.power_percent.push(gap?null:limited?50:100);previewHistory.coverage_percent.push(gap?0:100)}
 fs.mkdirSync(process.env.BDL_SCREENSHOT_DIR,{recursive:true});
 for(const [name,options,route]of [['desktop-en',{language:'en'},'/'],['dashboard',{language:'fr'},'/'],['mobile',{mobile:true,language:'fr'},'/'],['settings',{language:'fr'},'/settings'],['log',{language:'fr'},'/log']]){
  check('visual preview '+name,()=>run({...options,history:previewHistory,stats:{firmware:'2.0.0-beta.1',hashrate:1001240,hashrate_live:1004820,engines:[{name:'hw',rate:968500},{name:'sw',rate:27000},{name:'fill',rate:5740}],power_percent:100,power_limit_percent:100,throttle:'full',limitation_reason:'full',max_temp_c:65,uptime_s:18432,shares:{accepted:247,rejected:1},best:{session:0.0034,ever:1.28},history:undefined,history_revision:1,boot_id:'preview',github_update:{mode:2,checked:true,version:'2.0.0-beta.1',available:false},board:'M5StickC Plus2 · Demo data'}},async f=>{
   await f.page.goto(f.url+route);await f.page.waitForTimeout(200);
   await f.page.screenshot({path:path.join(process.env.BDL_SCREENSHOT_DIR,name+'.png'),fullPage:true});
  }));
 }
}

check('V2 overview prioritizes live state and keeps diagnostics out of the KPI row',()=>run({realPolling:true,stats:{board:'M5StickC Plus2',firmware:'2.0.0-beta.1',hashrate:1e6,hashrate_live:1e6,shares:{accepted:0,rejected:0}}},async f=>{
 await f.page.goto(f.url);await f.page.waitForSelector('#m .hero');
 assert.match(await f.page.locator('#miner-identity').textContent(),/M5StickC Plus2.*2\.0\.0-beta\.1/);
 assert.equal(await f.page.locator('#mining-state').textContent(),'Mining');
 assert.match(await f.page.locator('#m').textContent(),/No pool replies yet/);
 assert.equal(await f.page.locator('#pool-diagnostic-panel').getAttribute('open'),null);
 assert.ok(await f.page.locator('#m').evaluate(m=>m.getBoundingClientRect().top)<await f.page.locator('#performance-panel').evaluate(p=>p.getBoundingClientRect().top));
 await f.page.click('#open-pool-diagnostics');assert.equal(await f.page.locator('#check-pool').isVisible(),true);
}));

check('V2 French overview separates cooling stopped ASIC and pool socket states',()=>run({language:'fr',realPolling:true,stats:{hashrate:1e12,hashrate_live:1e12,shares:{accepted:9,rejected:1}}},async f=>{
 await f.page.goto(f.url);await f.page.waitForSelector('#m .hero');
 assert.match(await f.page.locator('#m').textContent(),/90,0 % Acceptation/);
 await f.page.evaluate(()=>{latestStats.asic={model:'BM1370',running:false,state:'Fan stopped'};show(latestStats)});
 assert.equal(await f.page.locator('#mining-state').textContent(),'ASIC arrêté');
 await f.page.evaluate(()=>{delete latestStats.asic;latestStats.limitation_reason='hot';show(latestStats)});
 assert.equal(await f.page.locator('#mining-state').textContent(),'Refroidissement');
}));

check('V2 diagnostics distinguish rare shares from incompatible ASIC jobs',()=>run({realPolling:true,stats:{pool_diagnostics:{code:'infrequent_shares',compatible:true,protocol:'Stratum V2 (Noise)',expected_share_seconds:42949672.96,accepted_samples:0,share_based_hashrate:null,version_rolling_bits:0,required_rolling_bits:0}}},async f=>{
 await f.page.goto(f.url);await f.page.click('#open-pool-diagnostics');await f.page.waitForSelector('#pool-diagnostic-values .t');
 assert.match(await f.page.locator('#pool-diagnostic-status').textContent(),/infrequent|rare|difficulty/i);
 await f.page.click('#check-pool');await f.page.waitForFunction(()=>!document.querySelector('#check-pool').disabled);
 assert.ok(f.state.requests.includes('GET /api/pool/diagnostics'));
 f.state.stats.pool_diagnostics.code='version_rolling_required';
 await f.page.click('#check-pool');await f.page.waitForFunction(()=>document.querySelector('#pool-diagnostic-status').textContent.toLowerCase().includes('version rolling'));
 assert.match(await f.page.locator('#pool-diagnostic-status').textContent(),/version rolling/i);
}));

check('V2 custom screen preserves mining page and saves a live metric order',()=>run({v2:true,postText:'{"restart":false,"changed":4}'},async f=>{
 await loaded(f);const pages=f.page.locator('[data-key=pages] .presets input');assert.equal(await pages.count(),8);
 assert.equal(await pages.first().isDisabled(),true);assert.equal(await pages.first().isChecked(),true);
 await pages.nth(1).uncheck();await f.page.selectOption('#k_screen0','3');await f.page.check('#k_customscreen');await f.page.selectOption('#k_channel','1');
 assert.equal(await f.page.locator('#save').textContent(),'Save');await f.page.click('#save');await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('applied'));
 const saved=JSON.parse(f.state.posts[0].body);assert.equal(saved.pages,253);assert.equal(saved.screen0,3);assert.equal(saved.channel,1);assert.equal(saved.customscreen,true);
}));

check('V2 headless search cannot reveal unsupported physical screen fields',()=>run({v2:true,capabilities:{display_pages:0}},async f=>{
 await loaded(f);await f.page.fill('#settings-search','screen');assert.equal(await f.page.locator('[data-key=customscreen]').isVisible(),false);
 await f.page.fill('#settings-search','');assert.equal(await f.page.locator('[data-key=pages]').isVisible(),false);
 assert.equal(await f.page.locator('#k_channel').isVisible(),true);
}));

check('V2 Gamma exposes three native pages and discards unsaved page/metric changes',()=>run({v2:true,gamma:true,language:'fr'},async f=>{
 await loaded(f);let pages=f.page.locator('[data-key=pages] .presets input');assert.equal(await pages.count(),3);
 await pages.nth(2).uncheck();await f.page.selectOption('#k_screen0','8');await f.page.click('#discard');
 assert.equal(await f.page.locator('#k_screen0').inputValue(),'1');assert.equal(await f.page.locator('[data-key=pages] .presets input').nth(2).isChecked(),true);
 assert.match(await f.page.locator('[data-key=channel]').textContent(),/Canal/);
}));

check('coin selection keeps payout and pool settings while choosing BCH Stratum V1',()=>run({coins:true},async f=>{
 await loaded(f);
 assert.equal(await f.page.locator('#k_coin').inputValue(),'0');
 await f.page.selectOption('#k_proto','1');
 await f.page.selectOption('#k_coin','1');
 assert.equal(await f.page.locator('#k_proto').inputValue(),'0');
 assert.equal(await f.page.locator('#k_proto2').inputValue(),'0');
 assert.equal(await f.page.locator('#k_proto option[value="1"]').isDisabled(),true);
 assert.equal(await f.page.locator('#k_pool').inputValue(),'pool.example');
 assert.equal(await f.page.locator('#k_addr').inputValue(),'address.worker');
 await f.page.fill('#k_pool','bch.example.org');await f.page.fill('#k_addr','bitcoincash:qpm2qsznhks23z7629mms6s4cwef74vcwvy22gdx6a.worker');
 await f.page.click('#save');await f.page.waitForFunction(()=>document.getElementById('save').disabled);
 await f.page.waitForTimeout(100);
 const body=JSON.parse(f.state.posts[0].body);assert.equal(body.coin,1);assert.equal(body.proto,0);assert.equal(body.pool,'bch.example.org');
}));
check('BCH dashboard names the coin and hides Bitcoin market data',()=>run({stats:{coin:'BCH',coin_name:'Bitcoin Cash',market:{updated_s:60,price:78000,currency:'USD',fees:[3,1,1],hashrate:1e21,mempool_tx:10,retarget:{change:1,blocks:10,progress:50}}}},async f=>{
 await f.page.goto(f.url+'/');await f.page.waitForSelector('.t');
 const tiles=await f.page.locator('body').innerText();
 assert.match(tiles,/Bitcoin Cash/);assert.match(tiles,/BCH/);assert.doesNotMatch(tiles,/BTC price|Fees sat\/vB|Next retarget/);
}));
check('coin selection and compatibility guidance are translated into French',()=>run({coins:true,language:'fr'},async f=>{
 await loaded(f);assert.match(await f.page.locator('[data-key=coin]').innerText(),/Cryptomonnaie/);
 assert.match(await f.page.locator('[data-key=coin]').innerText(),/La pool détermine/);
}));

for(const [id,ticker,name,address]of [[2,'BSV','Bitcoin SV','1BpEi6DfDAUFd7GtittLSdBeYJvcoaVggu'],[3,'XEC','eCash','ecash:qpm2qsznhks23z7629mms6s4cwef74vcwva87rkuu2'],[4,'DGB','DigiByte','DSC6tGC2NvN6AHio2p8pU4seTbPKDi7KZb'],[5,'PPC','Peercoin','PGWPnLCWjUMNfGSSu9zpLLnrbBfoXrkhmB']]){
 check('new SHA-256d coin '+ticker+' can be saved with V1 and its payout identity',()=>run({coins:true},async f=>{
  await loaded(f);await f.page.selectOption('#k_proto','1');await f.page.selectOption('#k_coin',String(id));
  assert.equal(await f.page.locator('#k_coin option').count(),6);
  assert.equal(await f.page.locator('#k_proto').inputValue(),'0');assert.equal(await f.page.locator('#k_proto option[value="1"]').isDisabled(),true);
  await f.page.fill('#k_pool',ticker.toLowerCase()+'.pool.example');await f.page.fill('#k_addr',address);
  await f.page.click('#save');await f.page.waitForFunction(()=>document.querySelector('#msg').textContent.includes('restarting'));
  const saved=JSON.parse(f.state.posts[0].body);assert.equal(saved.coin,id);assert.equal(saved.proto,0);assert.equal(saved.addr,address);
 }));
 check('new SHA-256d coin '+ticker+' shows its own name without Bitcoin market data',()=>run({stats:{coin:ticker,coin_name:name}},async f=>{
  await f.page.goto(f.url);await f.page.waitForSelector('#pool-grid .t');
  assert.match(await f.page.locator('#pool-grid').innerText(),new RegExp(ticker));assert.match(await f.page.locator('#pool-grid').innerText(),new RegExp(name));
  assert.doesNotMatch(await f.page.locator('#market-grid').innerText(),/BTC price|Next retarget/);
 }));
}

(async()=>{
  const executablePath=process.env.BDL_CHROMIUM_PATH||(fs.existsSync('/usr/bin/chromium')?'/usr/bin/chromium':undefined);
  browser=await chromium.launch({headless:true,executablePath,args:['--no-sandbox']});
  let failed=0;
  const selected=cases.filter(c=>!process.env.BDL_WEB_TEST_FILTER||c.name.includes(process.env.BDL_WEB_TEST_FILTER));
  try{for(const c of selected){try{await c.fn();console.log('PASS '+c.name)}catch(e){failed++;console.error('FAIL '+c.name+'\n'+e.stack)}}}
  finally{await browser.close()}
  console.log(`${selected.length-failed}/${selected.length} browser regressions passed`);process.exitCode=failed?1:0;
})().catch(e=>{console.error(e);process.exitCode=1});
