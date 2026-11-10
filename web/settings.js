let S=null,busy=false,restarting=false,notificationPending=false,notificationMessage='';const $=id=>document.getElementById(id);
function showNotificationMessage(text){notificationMessage=text;const out=$('notify-result');if(out)out.textContent=text}
function poolURLChanged(key){
 const input=$('k_'+key),value=input.value.trim();
 if(!value.startsWith('stratum2+tcp://'))return;
 try{
  const url=new URL(value);
  if(!url.hostname||url.username||url.password||url.search||url.hash||(url.port&&Number(url.port)<1))throw Error();
  const suffix=key==='pool2'?'2':'';
  input.value=url.hostname;$('k_proto'+suffix).value='1';
  if(url.port)$('k_port'+suffix).value=url.port;
  if(url.pathname.length>1)$('k_sv2key'+suffix).value=url.pathname.slice(1);
  input.setCustomValidity('');dirtyState();sync();
 }catch(e){input.setCustomValidity(t('Enter a valid value.'))}
}
$('bh').textContent=LANG==='fr'?t('backup_help'):'The backup contains all settings and lifetime statistics. Passwords and the webhook are only in it when a web password is set: keep it private then.';
function sync(){
 const off=busy||restarting||!S;
 for(const id of ['save','backup','restore','restart','discard'])$(id).disabled=off;
 const usb=S?.capabilities?.ota===false;
 $('upload').disabled=off||usb||!$('fw').files.length;
 if(usb)$('firmware-help').textContent=t('This board requires a USB firmware update.');
 $('retry').disabled=busy||restarting;
 document.querySelectorAll('#f input,#f select,#f button,#fw,#imp').forEach(i=>i.disabled=busy||restarting||i.dataset.fixed==='true');
 if($('notify-test'))$('notify-test').disabled=off||notificationPending;
 if($('k_fanpct'))$('k_fanpct').disabled=off||$('k_fanauto')?.value==='true';
 if($('k_ua'))$('k_ua').required=true;
 for(const key of ['proto','proto2'])for(const option of $('k_'+key)?.options||[])option.disabled=!!$('k_coin')&&$('k_coin').value!=='0'&&option.value==='1';
 for(const [protocol,key]of [['proto','sv2key'],['proto2','sv2key2']])if($('k_'+key)){$('k_'+key).disabled=off||$('k_'+protocol)?.value!=='1';$('k_'+key).required=$('k_'+protocol)?.value==='1';$('k_'+key).maxLength=64}
}
// Presentation only: titles, hints, units and choices for the settings the
// miner describes in /api/settings. Unknown keys keep the miner's own label.
const GROUPS={Pool:['pool','Where your miner sends its work and how it introduces itself.'],Display:['display','The miner\'s screen: brightness, colours, night mode and clock.'],Notifications:['notifications','Phone notifications, webhook and alerts.'],Safety:['safety','Temperature, power and automatic restarts.'],Network:['network','Fallback WiFi, Home Assistant, other miners, password and updates.']};
const SETTINGS_ICONS={
 pool:'<circle cx="12" cy="12" r="3"/><circle cx="5" cy="5" r="2"/><circle cx="19" cy="5" r="2"/><circle cx="5" cy="19" r="2"/><circle cx="19" cy="19" r="2"/><path d="m6.5 6.5 3.4 3.4m4.2 4.2 3.4 3.4m0-11-3.4 3.4m-4.2 4.2-3.4 3.4"/>',
 display:'<rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8m-4-4v4"/>',
 notifications:'<path d="M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9m-8 12a2 2 0 0 0 4 0"/>',
 safety:'<path d="m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6l8-3Z"/><path d="m8 12 3 3 5-6"/>',
 network:'<path d="M2 8a16 16 0 0 1 20 0M5 12a11 11 0 0 1 14 0m-11 4a6 6 0 0 1 8 0"/><circle cx="12" cy="20" r="1"/>',
 maintenance:'<path d="m14 6 4 4 3-3a7 7 0 0 1-9 9l-6 6-4-4 6-6a7 7 0 0 1 9-9l-3 3Z"/>',
 backup:'<path d="M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5"/>',
 firmware:'<path d="M12 16V4m-5 5 5-5 5 5M4 16v5h16v-5"/>',
 restart:'<path d="M3 11a9 9 0 1 1 2 7M3 4v7h7"/>',
 settings:'<circle cx="12" cy="12" r="3"/><path d="M12 3v3m0 12v3M3 12h3m12 0h3M6 6l2 2m8 8 2 2M6 18l2-2m8-8 2-2"/>'
};
function settingsIcon(name){
 const mark=document.createElement('span');mark.className='group-icon';mark.setAttribute('aria-hidden','true');
 const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');
 for(const[key,value]of Object.entries({viewBox:'0 0 24 24',fill:'none',stroke:'currentColor','stroke-width':'1.7','stroke-linecap':'round','stroke-linejoin':'round',focusable:'false'}))svg.setAttribute(key,value);
 svg.innerHTML=Object.hasOwn(SETTINGS_ICONS,name)?SETTINGS_ICONS[name]:SETTINGS_ICONS.settings;
 mark.appendChild(svg);return mark;
}
document.querySelectorAll('[data-icon]').forEach(node=>node.replaceWith(settingsIcon(node.dataset.icon)));
const SECTIONS={pool:'Main pool',ua:'Identity',pool2:'Backup pool',bright:'Screen',nightfrom:'Night mode',tz:'Clock and price',ntfy:'ntfy',hook:'Webhook',alertpct:'Alerts',maxtemp:'Heat and power',fanauto:'Cooling',rebooth:'Recovery',wifi2:'Fallback WiFi',mqtt:'Home Assistant (MQTT)',peers:'Web pages',update:'Updates',count:'Anonymous statistics'};
SECTIONS.remote='Private remote monitoring';
const FIELDS={
 coin:['Cryptocurrency','SHA-256d only. Choose main and backup pools for the same coin and use its payout address or pool account. The pool determines which chain is mined. BCH, BSV, XEC, DGB and PPC use Stratum V1. Peercoin uses proof of work only. DGB requires a SHA-256d port. Bitcoin market data is unavailable for other coins.'],
 pool:['Pool host','A pasted stratum+tcp:// or stratum2+tcp:// address is split for you.'],port:['Pool port'],
 addr:['Payout address or pool account','Use the selected coin\'s address or the account name required by your pool. A worker name can follow: address.worker'],pass:['Pool password','Usually x.'],
 diff:['Suggested difficulty','Share difficulty asked of the pool; it may choose another.'],proto:['Protocol'],
 sv2key:['Stratum V2 authority key','Given by the pool for encrypted V2 connections.'],
 ua:['User agent','Choose the identity sent to the pool. BDLMinerV2 follows the installed firmware version.'],
 pool2:['Backup pool host','Used when the main pool keeps failing. Empty = none.'],port2:['Backup pool port'],proto2:['Backup protocol'],
 sv2key2:['Backup Stratum V2 authority key'],rotateh:['Alternate pools every','0 = switch only when the main pool fails.','h'],
 bright:['Brightness','','%'],rot:['Rotation'],autorot:['Follow the accelerometer','M5 devices with a motion sensor: the picture turns with the device.'],
 lang:['Language','Screen and web pages.'],theme:['Colour theme','Accent colour of the screen.'],
 cycle:['Next screen every','0 = only with the button.','s'],nightfrom:['Night mode from','Screen off and no beep. A button press lights it for 30 s.'],
 nightto:['Night mode until'],beep:['Beep on a 32-bit share'],tz:['Time zone','POSIX TZ, e.g. CET-1CEST,M3.5.0,M10.5.0/3 or UTC0'],fiat:['Price currency'],
 ntfy:['ntfy topic','A long, hard-to-guess name. Empty = off.'],ntfysrv:['ntfy server'],notify32:['Also every 32-bit share'],
 hook:['Webhook URL','JSON POST for Discord, Telegram… It holds a token: keep it private. Empty = off.'],
 summary:['Daily summary at','Average hashrate, best share, shares and pool connection of the past day. Off = none.'],
 alertpct:['Hashrate alert below','Share of the usual full-power hashrate. 0 = off.','%'],alertmin:['… for at least','','min'],
 maxtemp:['Pause mining above','0 = no thermal limit.','°C'],power:['Mining power','On USB and battery. 0 = paused.','%'],
 fanauto:['Fan control','Manual or temperature-controlled cooling. Changes apply without restarting the ASIC.'],
 fanpct:['Manual fan speed','Used in manual mode. The percentage is a control setting; RPM shows measured speed.','%'],
 rebooth:['Restart after no accepted share for','0 = off.','h'],saver:['Battery saver','On battery: dim the screen and mine at half speed.'],
 wifi2:['Fallback WiFi 1','Network name.'],wpass2:['Fallback WiFi 1 password'],wifi3:['Fallback WiFi 2','Network name.'],wpass3:['Fallback WiFi 2 password'],
 mqtt:['MQTT broker','Your Home Assistant broker. Empty = off.'],mqttport:['MQTT port'],mqttuser:['MQTT user'],mqttpass:['MQTT password'],
 peers:['Other miners','Shown on the dashboard: IP addresses or names, comma separated.'],
 webpass:['Web password','Optional. Asked before changing settings or installing firmware from these pages. Empty = no password: anyone on your network can.'],
 update:['Updates from GitHub','Signed releases of this firmware.'],count:['Anonymous usage count','Once a day, downloads a 2-byte file of this version\'s release. Nothing is sent.'],sharehr:['Share my hashrate anonymously','Every 5 minutes, sends the firmware version, the board, the cryptocurrency and the hashrate, to count the total of all BDLMiner miners live. No address, name or identifier.'],
 sharepool:['Share my pool anonymously','On by default. Turn off to hide your pool group. Requires hashrate sharing. Other pools are grouped as private/other. Never sends the hostname, port, Bitcoin address or worker.']};
const recommendedMv=mhz=>mhz<=600?1150:mhz<=675?1200:mhz<=725?1250:1300;
const hours=[[255,'Off'],...Array.from({length:24},(_,h)=>[h,String(h).padStart(2,'0')+':00'])];
const CHOICES={proto:[[0,'Stratum V1'],[1,'Stratum V2 (Noise)']],proto2:[[0,'Stratum V1'],[1,'Stratum V2 (Noise)']],
 fanauto:[['false','Manual'],['true','Automatic']],
 asicmhz:[525,550,575,600,625,650,675,690,700,725,750,775,800].map(mhz=>[mhz,mhz===525?'525 MHz (stock)':mhz+' MHz']),
 asicmv:[1000,1060,1100,1150,1200,1250,1300].map(mv=>[mv,mv===1150?'1150 mV (stock)':mv+' mV']),
 lang:[[0,'English'],[1,'Français']],rot:[[0,'Portrait'],[1,'Landscape'],[2,'Portrait, upside down'],[3,'Landscape, upside down']],
 theme:[[0,'Orange'],[1,'Green'],[2,'Blue'],[3,'Purple'],[4,'Pink']],nightfrom:hours,nightto:hours,
 summary:hours,update:[[0,'Off'],[1,'Check and offer'],[2,'Install automatically']],fiat:['USD','EUR','GBP','CAD','CHF','AUD','JPY'].map(c=>[c,c])};
const THEMES=['#f7931a','#38d07a','#3b9cf6','#a66bf5','#ec5aa0'];
Object.assign(FIELDS,{remote:['Enable remote monitoring','Optional. Every five minutes, sends this miner’s readings under its payout address. Open the remote dashboard and enter the same address: nothing to copy. Anyone who knows the address can see them, as on a pool page.'],remotekey:['Private access key (optional)','Leave empty to use the payout address. A key created before BDLMiner 2.5.4 keeps your readings in a secret space instead; use the same key on every miner.'],rname:['Miner name','The name shown in your remote dashboard. Empty uses the board model. Up to 48 bytes.']});
CHOICES.coin=[[0,'Bitcoin (BTC)'],[1,'Bitcoin Cash (BCH)'],[2,'Bitcoin SV (BSV)'],[3,'eCash (XEC)'],[4,'DigiByte · SHA-256d (DGB)'],[5,'Peercoin (PPC) · PoW']];
Object.assign(FIELDS,{channel:['Update channel','Beta versions receive the same signature checks and rollback protection.'],
 customscreen:['Custom mining screen','Choose the four metrics and their order on the mining screen.'],
 screen0:['First screen metric'],screen1:['Second screen metric'],screen2:['Third screen metric'],screen3:['Fourth screen metric'],pages:['Enabled screens','The mining screen remains available.']});
CHOICES.channel=[[0,'Stable'],[1,'Beta']];
for(const key of ['screen0','screen1','screen2','screen3'])CHOICES[key]=[[1,'Hashrate'],[2,'Accepted shares'],[3,'Temperature'],[4,'Best difficulty'],[5,'uptime'],[6,'Pool'],[7,'ASIC frequency'],[8,'Fan RPM']];
function help(text){const p=document.createElement('p');p.className='field-help';p.textContent=text;return p}
function render(data){
 const agents=data.capabilities?.user_agents;
 if(!Array.isArray(agents)||agents.length!==2||agents[0]!=='NerdMinerV2'||!/^BDLMinerV2\//.test(agents[1]))throw Error(t('Invalid response from the miner.'));
 CHOICES.ua=agents.map(value=>[value,value]);
 data={...data,schema:data.schema.filter(([key])=>key!=='uaauto')};
 const f=$('f');f.replaceChildren();$('settings-nav').replaceChildren();const groups=new Map();let section=null;
 for(const[k,type,group,label,limits]of data.schema){
  let fs=groups.get(group);
  if(!fs){
   groups.set(group,fs=document.createElement('fieldset'));fs.id=group==='Safety'?'performance':'group-'+group.toLowerCase();
   const [icon,about]=Object.hasOwn(GROUPS,group)?GROUPS[group]:['settings',''],lg=document.createElement('legend'),mark=settingsIcon(icon);
   lg.append(mark,t(group));fs.appendChild(lg);
   if(about){const p=document.createElement('p');p.className='group-about';p.textContent=t(about);fs.appendChild(p)}
   f.appendChild(fs);section=null;
   const link=document.createElement('a');link.href='#'+fs.id;link.dataset.group=fs.id;
   const linkIcon=mark.cloneNode(true);link.append(linkIcon,t(group));link.onclick=()=>{$('settings-search').value='';filterSettings()};$('settings-nav').appendChild(link);
  }
  if(SECTIONS[k]||!section||section.parentNode!==fs){
   section=document.createElement('div');section.className='subsection';
   if(SECTIONS[k]){const h=document.createElement('h3');h.textContent=t(SECTIONS[k]);section.appendChild(h)}
   fs.appendChild(section);
  }
  const [title,hint,unit]=FIELDS[k]||[LANG=='fr'&&FR_SET[k]||label],options=CHOICES[k],value=data.values[k];
  const l=document.createElement('label'),i=document.createElement(options?'select':'input');l.textContent=t(title);i.id='k_'+k;l.htmlFor=i.id;
  if(options){
   for(const[v,name]of options){const option=document.createElement('option');option.value=String(v);option.textContent=t(name);i.appendChild(option)}
   const current=String(k==='ua'?(value==='NerdMinerV2'?value:agents[1]):value??(k==='asicmhz'?525:k==='asicmv'?1150:k==='fanauto'?false:0));
   if(![...i.options].some(o=>o.value===current)){const option=document.createElement('option');option.value=current;option.textContent=current;i.appendChild(option)}
   i.value=current;if(k==='asicmhz'||k==='asicmv')i.required=true;
  }else if(type=='bool'){i.type='checkbox';i.className='switch';i.setAttribute('role','switch');i.checked=value}
  else{
   i.type=type=='secret'?'password':type=='text'?'text':'number';i.value=value??'';
   if(type=='number'||type=='decimal'){i.required=true;i.step=type=='decimal'?'any':'1';i.min=limits?.min??0;if(limits?.max!=null)i.max=limits.max;i.inputMode=type=='decimal'?'decimal':'numeric'}
   if(k==='pool'||k==='addr')i.required=true;
  if(k==='rname')i.maxLength=48;
  if(k==='remotekey'){i.maxLength=64;i.autocomplete='off'}
   if(type=='text'||type=='secret'){i.spellcheck=false;i.autocapitalize='off'}
  }
  i.addEventListener('input',()=>{i.setCustomValidity('');dirtyState();if(options)sync()});
  if(options)i.addEventListener('change',()=>{dirtyState();sync()});
  if(k==='pool'||k==='pool2')i.addEventListener('change',()=>poolURLChanged(k));
  // The voltage that goes with a profile (bm1370::recommendedMv): raised, never lowered.
  if(k==='asicmhz')i.addEventListener('change',()=>{const mv=$('k_asicmv'),need=recommendedMv(Number(i.value));if(mv&&Number(mv.value)<need){mv.value=String(need);dirtyState();sync()}});
  if(k==='coin')i.addEventListener('change',()=>{
   if(i.value!=='0')for(const key of ['proto','proto2'])if($('k_'+key))$('k_'+key).value='0';
   dirtyState();sync();
  });
  const row=document.createElement('div');row.className='field'+(type==='bool'?' boolean':'');row.dataset.key=k;
  const text=document.createElement('div');text.className='field-text';text.appendChild(l);
  if(hint){const p=document.createElement('p');p.className='field-hint';p.id='h_'+k;p.textContent=t(hint);text.appendChild(p);i.setAttribute('aria-describedby',p.id)}
  row.dataset.search=(text.textContent+' '+label+' '+k).toLocaleLowerCase();row.appendChild(text);
  const control=document.createElement('div');control.className='field-control';control.appendChild(i);row.appendChild(control);
  if(k==='fanpct'){
   i.type='range';control.classList.add('fan-speed');const output=document.createElement('output');output.id='fan-pct-value';output.htmlFor=i.id;
   const update=()=>{output.value=i.value+' %';i.setAttribute('aria-valuetext',output.value)};i.addEventListener('input',update);update();control.appendChild(output);
  }else if(unit){const u=document.createElement('span');u.className='unit';u.setAttribute('aria-hidden','true');u.textContent=unit;control.classList.add('with-unit');control.appendChild(u)}
  if(k==='theme'){const swatch=document.createElement('span');swatch.className='swatch';swatch.setAttribute('aria-hidden','true');const paint=()=>swatch.style.background=THEMES[Number(i.value)]||THEMES[0];i.addEventListener('change',paint);paint();control.prepend(swatch)}
  if(type==='secret'){const show=document.createElement('button');show.type='button';show.className='secondary secret-toggle';show.textContent=t('Show');show.setAttribute('aria-label',t('Show')+' '+l.textContent);show.onclick=()=>{const visible=i.type==='password';i.type=visible?'text':'password';show.textContent=t(visible?'Hide':'Show');show.setAttribute('aria-label',t(visible?'Hide':'Show')+' '+l.textContent)};control.appendChild(show)}
  if(k==='pool')row.appendChild(help(t('Choose the user agent below. Each pool has its own protocol and authority key.')));
  if(k==='coin'){
   const guide=document.createElement('a');guide.href='https://github.com/bdlglua/BDLMinerV2/blob/main/docs/mining-coins.md';guide.target='_blank';guide.rel='noopener noreferrer';guide.textContent=t('SHA-256d coin setup guide');
   const note=help('');note.appendChild(guide);row.appendChild(note);
  }
  if(k==='proto'||k==='proto2')row.appendChild(help(t('Stratum V2 needs the pool\'s V2 port and authority key. It does not change share difficulty.')));
  if(k==='asicmhz')row.appendChild(help(t('Profiles above 525 MHz are experimental: up to 690 MHz as in ESP-Miner, then overclocking up to 800 MHz. Default: 525 MHz. Choosing a profile raises the core voltage to the one that goes with it. Instability triggers a 525 MHz fallback, kept until the frequency or voltage changes.')));
  if(k==='asicmv')row.appendChild(help(t('Default: 1150 mV, as ESP-Miner. Goes with the frequency: 1150 mV up to 600 MHz, 1200 mV up to 675, 1250 mV up to 725, 1300 mV up to 800. Higher voltage means more heat and power; watch the temperature. Safety stops still apply.')));
  if(k==='fanauto')row.appendChild(help(t('Default: manual 100%. Startup, stopped ASIC, missing temperature or heat safety force full cooling.')));
  if(k==='fanpct')row.appendChild(help(t('Minimum 30%. Safety forces 100% at ASIC 60 °C or regulator 80 °C until both cool by 5 °C. A lower thermal limit raises cooling earlier. Fan-stop protection remains active.')));
  if(k==='power'){const h=document.createElement('div');h.className='field-help';h.textContent=t('At 100%, mining runs at full available speed. Temperature protection can still reduce it.');const presets=document.createElement('div');presets.className='presets';for(const [name,v]of [['Full performance',100],['Balanced',75],['Quiet',50]]){const button=document.createElement('button');button.type='button';button.className='secondary';button.id='preset-'+v;button.textContent=t(name)+' · '+v+' %';button.onclick=()=>{i.value=v;i.dispatchEvent(new Event('input',{bubbles:true}))};presets.appendChild(button)}h.appendChild(presets);row.appendChild(h)}
  if(k==='remote'){const link=document.createElement('a');link.href='https://miner.bdl-pool.com/remote.html';link.target='_blank';link.rel='noopener noreferrer';link.textContent=t('Open my remote dashboard');row.appendChild(link)}
  if(k==='maxtemp'){const h=help('');const update=()=>{const limit=Number(i.value),stepped=data.capabilities?.thermal_steps!==false;h.textContent=limit>=30?(stepped?t('Throttling starts at')+' '+(limit-10)+' °C. ':'')+t('Mining pauses at')+' '+limit+' °C.'+(stepped?'':' '+t('ASIC mining stays paused for at least 2 minutes, then resumes once cooled by 5 °C.')):limit===0?t(data.capabilities?.fan_control?'User temperature limit is disabled; hardware protection and cooling safety remain active.':'Thermal protection is disabled.'):''};i.addEventListener('input',update);update();row.appendChild(h)}
  if(k==='hook'){const h=help(t('Discord: channel settings › Integrations › Webhooks › Copy URL. Telegram: https://api.telegram.org/bot<token>/sendMessage?chat_id=<chat>. Save and restart first, then:'));const test=document.createElement('button');test.type='button';test.className='secondary';test.id='notify-test';test.textContent=t('Send a test notification');test.onclick=testNotification;const out=document.createElement('span');out.id='notify-result';out.textContent=notificationMessage;out.setAttribute('role','status');out.setAttribute('aria-live','polite');const line=document.createElement('div');line.className='presets';line.append(test,out);h.appendChild(line);row.appendChild(h)}
  section.appendChild(row);
 }
 S=data;screenControls(data);$('msg').textContent='';filterSettings();dirtyState();watchSections();
}
function screenControls(data){
 const count=data.capabilities?.display_pages??8;
 for(const key of ['customscreen','screen0','screen1','screen2','screen3','pages']){
  const input=$('k_'+key);if(input){input.closest('.field').dataset.unsupported=count?'false':'true';input.closest('.field').hidden=!count;}
 }
 for(const key of ['screen0','screen1','screen2','screen3'])for(const option of $('k_'+key)?.options||[]){if(option.value==='7')option.disabled=!data.capabilities?.asic;if(option.value==='8')option.disabled=!data.capabilities?.fan_control;}
 const input=$('k_pages');if(!input||!count)return;
 input.type='hidden';const row=input.closest('.field'),label=row.querySelector('label');
 label.removeAttribute('for');label.id='screen-pages-label';
 const group=document.createElement('div');group.className='presets';group.setAttribute('role','group');group.setAttribute('aria-labelledby',label.id);
 const names=count===3?['Mining','Power / thermal','Pool / records']:['Mining','Clock','Cryptocurrency','Network','Device','Hashrate','Records','Blocks'];
 names.forEach((name,n)=>{const l=document.createElement('label'),box=document.createElement('input');box.type='checkbox';box.checked=n===0||!!(Number(input.value)&(1<<n));box.disabled=n===0;box.dataset.fixed=String(n===0);
  box.onchange=()=>{input.value=Number(input.value)^(1<<n);input.dispatchEvent(new Event('input',{bubbles:true}))};l.append(box,' '+t(name));group.appendChild(l)});
 input.parentNode.appendChild(group);
}
async function load(){
 if(busy||restarting)return;
 busy=true;S=null;sync();$('msg').textContent=t('Loading settings…');$('retry').hidden=true;
 try{
  const data=await request('/api/settings');
  if(!data||!Array.isArray(data.schema)||!data.values)throw Error(t('Invalid response from the miner.'));
  render(data);
 }catch(e){$('msg').textContent=errorText(e);$('retry').hidden=false}
 finally{busy=false;sync()}
}
function discardChanges(){if(S&&!busy&&!restarting){render(S);sync()}}
// Highlights the group being read in the side navigation.
let sectionObserver=null;
function watchSections(){
 if(!('IntersectionObserver'in window))return;
 if(sectionObserver)sectionObserver.disconnect();
 sectionObserver=new IntersectionObserver(entries=>{for(const e of entries)if(e.isIntersecting)document.querySelectorAll('#settings-nav a').forEach(a=>a.classList.toggle('active',a.dataset.group===e.target.id))},{rootMargin:'-20% 0px -70% 0px'});
 document.querySelectorAll('#f fieldset').forEach(fs=>sectionObserver.observe(fs));
}
function values(){
 const v={};
 if(S.schema.some(([key])=>key==='ua'))v.uaauto=false;
 for(const[k,type]of S.schema){
  const i=$('k_'+k);v[k]=fieldValue(k,type);
  if((k==='pool'||k==='addr')&&!v[k].trim())i.setCustomValidity(t('Enter a valid value.'));
  if(type==='decimal'&&!(v[k]>0))i.setCustomValidity(t('The difficulty must be greater than zero.'));
  if(k==='maxtemp'&&v[k]>0&&v[k]<30)i.setCustomValidity(t('The temperature must be 0 or at least 30 °C.'));
  if((k==='nightfrom'||k==='nightto')&&v[k]>23&&v[k]!==255)i.setCustomValidity(t('The hour must be 0–23 or 255 to disable night mode.'));
 }
 return v;
}
async function saveSettings(event){
 if(event)event.preventDefault();if(busy||restarting||!S)return;
 const v=values();
 if(!$('f').checkValidity()){$('settings-search').value='';filterSettings();$('f').reportValidity();return}
 busy=true;sync();
 try{
  const answer=await request('/api/settings',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(v)},'text');
  let result=null;try{result=JSON.parse(answer)}catch(e){}
  if(result&&result.restart===false){
   // Applied by the miner at once: the page stays usable.
   const language=v.lang!==undefined&&v.lang!==S.values.lang;S.values=v;dirtyState();
   $('msg').textContent=t(result.changed?'Saved and applied.':'Nothing to save.');
   if(language)setTimeout(()=>location.reload(),600);
  }else{restarting=true;$('msg').textContent=t('Saved, restarting… reload in 15 s')}
 }
 catch(e){$('msg').textContent=errorText(e)}finally{busy=false;sync()}
}
async function imp(input){
 const file=input.files[0];if(!file||busy||restarting)return;
 busy=true;sync();
 try{
  const raw=await file.text();let parsed;
  try{parsed=JSON.parse(raw)}catch(e){throw Error(t('Not a backup file'))}
  if(!parsed||typeof parsed!=='object'||Array.isArray(parsed))throw Error(t('Not a backup file'));
  if(!confirm(t('Restore this backup? The miner restarts afterwards.')))return;
  // Send original text: parsing/stringifying would round uint64 hash totals.
  await request('/api/backup',{method:'POST',headers:{'Content-Type':'application/json'},body:raw},'text');
  restarting=true;$('msg').textContent=t('Backup restored. The miner is restarting…');
 }catch(e){$('msg').textContent=errorText(e)}finally{input.value='';busy=false;sync()}
}
async function uploadFirmware(){
 if(busy||restarting||!S)return;
 const file=$('fw').files[0];if(!file){$('up').textContent=t('Select a firmware file first.');return}
 busy=true;sync();
 try{
  if(!/\.bin$/i.test(file.name)||/factory(?:\.bin)?$/i.test(file.name))throw Error(t('Use a firmware .bin file, not a factory image.'));
  if(!file.size||new Uint8Array(await file.slice(0,1).arrayBuffer())[0]!==0xE9)throw Error(t('Invalid firmware file.'));
  const x=new XMLHttpRequest(),data=new FormData();data.append('firmware',file);
  x.upload.onprogress=e=>{if(e.lengthComputable)$('up').textContent=e.loaded===e.total?t('Upload complete. Validating firmware…'):Math.round(e.loaded*100/e.total)+' %'};
  const finish=message=>{$('up').textContent=message;busy=false;sync()};
  x.onload=()=>{
   if(x.status>=200&&x.status<300){restarting=true;finish(t('Firmware updated. The miner is restarting…'))}
   else finish(x.status===401?t('Authentication required. Reload this page and sign in.'):t(x.responseText)||t('Upload failed'));
  };
  x.onerror=()=>finish(t('Upload failed'));x.ontimeout=()=>finish(t('Upload timed out. Please try again.'));x.onabort=()=>finish(t('Upload cancelled.'));
  x.open('POST','/update');x.timeout=120000;x.send(data);
 }catch(e){$('up').textContent=errorText(e);busy=false;sync()}
}
load();

function fieldValue(k,type){const input=$('k_'+k);return type==='bool'?(input.tagName==='SELECT'?input.value==='true':input.checked):type==='number'||type==='decimal'?Number(input.value):input.value}
function dirtyState(){
 if(!S)return;
 let count=0,restart=false;const live=S.live||[];
 for(const[k,type]of S.schema){const changed=fieldValue(k,type)!==S.values[k];$('k_'+k).closest('.field').classList.toggle('changed',changed);if(changed){count++;if(!live.includes(k))restart=true}}
 $('dirty').textContent=count?count+' '+t(count>1?'unsaved changes':'unsaved change')+' · '+t(restart?'needs a restart':'applied right away'):t(live.length?'Display, alerts and power apply right away; other changes restart the miner.':'Changes take effect after restart.');
 $('save').textContent=t(count&&!restart?'Save':'Save and restart');
 document.querySelector('.save-bar').dataset.dirty=count>0;$('discard').hidden=!count;
}
function filterSettings(){
 const query=$('settings-search').value.trim().toLocaleLowerCase();let visible=0;
 document.querySelectorAll('#f fieldset').forEach(fs=>{let found=0;fs.querySelectorAll('.subsection').forEach(sub=>{let inside=0;sub.querySelectorAll('.field').forEach(row=>{row.hidden=row.dataset.unsupported==='true'||!row.dataset.search.includes(query);if(!row.hidden)inside++});sub.hidden=!inside;found+=inside});fs.hidden=!found;visible+=found});
 $('settings-empty').hidden=!!visible||!S;
}
$('settings-search').addEventListener('input',filterSettings);
// The miner sends in the background (with retries): wait for its result.
async function testNotification(){
 const button=$('notify-test');if(busy||restarting||notificationPending||button.disabled)return;
 notificationPending=true;button.disabled=true;showNotificationMessage(t('Sending…'));
 try{
  const queued=await request('/api/notify-test',{method:'POST'});
  if(!Number.isInteger(queued?.id)||queued.id<=0)throw Error(t('Invalid response from the miner.'));
  for(let n=0;n<50;n++){
   await new Promise(r=>setTimeout(r,2000));
   const r=await request('/api/notify-test?id='+queued.id);
   if(r.id===queued.id){showNotificationMessage('ntfy: '+t(r.ntfy)+' · webhook: '+t(r.webhook));return}
  }
  showNotificationMessage(t('No result yet: see the log page.'));
 }catch(e){showNotificationMessage(errorText(e))}
 finally{notificationPending=false;sync()}
}
async function restartMiner(){
 if(busy||restarting||!S||!confirm(t('Restart this miner?')))return;
 busy=true;sync();
 try{await request('/api/restart',{method:'POST'},'text');restarting=true;$('msg').textContent=t('Restarting…')}
 catch(e){$('msg').textContent=errorText(e)}finally{busy=false;sync()}
}
