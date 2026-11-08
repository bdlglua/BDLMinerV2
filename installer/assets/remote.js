// "My miners" page (remote.html): private remote monitoring, adapted from the
// statistics service's standalone viewer (BDLMiner-stats, src/remote-page.js).
// It follows the site's language (html[lang]) and theme (html[data-theme]).
'use strict';
(()=>{
const apiOrigin=document.querySelector('meta[name="bdl-remote-api"]')?.content||location.origin;
const SAVED='bdl-remote-address',KEY=/^[a-f0-9]{64}$/;
const root=document.getElementById('remote');if(!root)return;
const $=id=>document.getElementById('rm-'+id),FR={
"Performance":"Performances","Mining statistics":"Statistiques de minage","Export CSV":"Exporter CSV","Average hashrate":"Hashrate moyen","Peak hashrate":"Hashrate maximal","Average temperature":"Température moyenne","Reports received":"Relevés reçus","Hashrate":"Hashrate","Period average":"Moyenne de la période","Recent readings":"Derniers relevés","Time":"Heure","Minimum":"Minimum","Peak":"Maximum","Observed restarts":"Redémarrages observés","Variation":"Variation","Loading readings…":"Chargement des relevés…","History unavailable":"Historique indisponible","Statistics use received reports. Missing periods are left empty.":"Statistiques calculées sur les relevés reçus. Les périodes sans données restent vides.","No temperature readings.":"Aucun relevé de température.","reports":"relevés","readings":"relevés","Use left and right arrows to inspect readings.":"Utilisez les flèches gauche et droite pour consulter les relevés.",
'Home':'Accueil','Statistics':'Statistiques','My miners':'Mes mineurs','Sign out':'Déconnexion','Remote monitoring':'Suivi à distance','Your miners, wherever you are.':'Vos mineurs, où que vous soyez.','Each board sends its readings directly, every five minutes. No computer needs to stay on at home.':'Chaque carte envoie directement ses mesures toutes les cinq minutes. Aucun ordinateur ne doit rester allumé chez vous.','Online hashrate':'Hashrate en ligne','See your miners':'Voir vos mineurs','Enter the payout address your miners use. It is the one in Settings → Pool on each board.':'Saisissez l’adresse de paiement utilisée par vos mineurs. C’est celle de Réglages → Pool sur chaque carte.','Wallet address':'Adresse de portefeuille','View my miners':'Voir mes mineurs','Remember this address on this device':'Retenir cette adresse sur cet appareil',"Anyone who knows this address can see these readings, as on your pool's page. Nothing here can change a miner. A private access key created before BDLMiner 2.5.4 also works in this field.":'Toute personne qui connaît cette adresse peut voir ces mesures, comme sur la page de votre pool. Rien ici ne peut modifier un mineur. Une clé d’accès privée créée avant BDLMiner 2.5.4 fonctionne aussi dans ce champ.','Waiting for your first miner':'En attente de votre premier mineur','On each miner (BDLMiner 2.5.4 or newer), open Settings → Network → Private remote monitoring, enable it, then save and restart. The miner uses its payout address: nothing to copy. The first report arrives after about one minute.':'Sur chaque mineur (BDLMiner 2.5.4 ou ultérieur), ouvrez Réglages → Réseau → Suivi privé à distance, activez-le, puis enregistrez et redémarrez. Le mineur utilise son adresse de paiement : rien à copier. Le premier envoi arrive après environ une minute.','Setup guide':'Guide de configuration','Hashrate history':'Historique du hashrate','Manage my private data':'Gérer mes données privées','Revoke this key and permanently erase its readings and history. All paired miners must then use a new key.':'Révoquez cette clé et effacez définitivement ses mesures et son historique. Tous les mineurs associés devront ensuite utiliser une nouvelle clé.','Erase data and revoke key':'Effacer les données et révoquer la clé','Reports every five minutes; history kept for seven days.':'Envoi toutes les cinq minutes ; historique conservé sept jours.','Readings only: this page cannot change settings, restart a miner or install firmware.':'Consultation uniquement : cette page ne peut pas changer les réglages, redémarrer un mineur ou installer un firmware.','Setup and privacy':'Configuration et confidentialité','Online':'En ligne','Offline':'Hors ligne','Temperature':'Température','Accepted / rejected':'Acceptées / rejetées','Uptime':'Durée de marche','Power':'Puissance','Best difficulty':'Meilleure difficulté','Pool':'Pool','Connected':'Connectée','Disconnected':'Déconnectée','Last report':'Dernier envoi','Updated':'Actualisé','Enter the payout address set on your miners.':'Saisissez l’adresse de paiement réglée sur vos mineurs.','Connection failed. Last readings may be out of date.':'Connexion impossible. Les dernières mesures peuvent être anciennes.','This access key has been revoked':'Cette clé d’accès a été révoquée','Wallet address or private access key required':'Adresse de portefeuille ou clé d’accès privée requise','Only a private access key can erase readings':'Seule une clé d’accès privée peut effacer les mesures','Please wait before trying again':'Patientez avant de réessayer','Data erased and key revoked.':'Données effacées et clé révoquée.','Erase all private history and permanently revoke this key?':'Effacer tout l’historique privé et révoquer définitivement cette clé ?','Gaps indicate missing reports or restarts.':'Les interruptions indiquent des envois manquants ou des redémarrages.','Only one reading in this period. Reports arrive every five minutes.':'Un seul relevé sur cette période. Les envois arrivent toutes les cinq minutes.','No history yet.':'Pas encore d’historique.','Connection closed':'Connexion fermée'};
const siteLang=()=>document.documentElement.lang==='fr'?'fr':'en';
let lang=siteLang(),access=null,version=0,timer=null,fleet=null,series=null,historyVersion=0;
const t=s=>lang==='fr'?(FR[s]||s):s;
function stored(value){try{if(value===undefined)return localStorage.getItem(SAVED)||'';if(value)localStorage.setItem(SAVED,value);else localStorage.removeItem(SAVED)}catch{}return''}
function address(value){const a=String(value).trim().split('.')[0];return /^[A-Za-z0-9:_-]{4,100}$/.test(a)?a:''}
function translate(){for(const e of root.querySelectorAll('[data-i]'))e.textContent=t(e.dataset.i)}
function node(tag,text,cls){const el=document.createElement(tag);if(text!==undefined)el.textContent=text;if(cls)el.className=cls;return el}
function si(n){if(!Number.isFinite(n)||n<0)return'—';const units=['H/s','kH/s','MH/s','GH/s','TH/s','PH/s'];let i=0;while(n>=1000&&i<units.length-1){n/=1000;i++}return new Intl.NumberFormat(lang,{maximumFractionDigits:2}).format(n)+' '+units[i]}
function duration(n){if(n<60)return Math.round(n)+' s';if(n<3600)return Math.round(n/60)+' min';if(n<86400)return(n/3600).toFixed(1)+' h';return(n/86400).toFixed(1)+' d'}
function metric(box,label,value){const el=node('div',undefined,'metric');el.append(node('span',t(label)),node('strong',value));box.append(el)}
function lock(){root.classList.remove('on');version++;historyVersion++;access=null;fleet=null;clearHistory();clearTimeout(timer);$('key').value='';$('login').hidden=false;for(const id of ['logout','history','manage','empty'])$(id).hidden=true;$('miners').replaceChildren();$('history-miner').replaceChildren();$('total').textContent='—';$('updated').textContent='';const c=$('chart');c.getContext('2d').clearRect(0,0,c.width,c.height);$('notice').textContent='';translate()}
async function api(path,method='GET'){
 const epoch=version,current=access;if(!current)throw Error(t('Connection closed'));
 const response=await fetch(apiOrigin+path,{method,headers:{Authorization:(current.key?'Bearer ':'Address ')+current.value},credentials:'omit',redirect:'error',cache:'no-store',signal:AbortSignal.timeout(15000)});
 const data=response.status===204?null:await response.json();if(epoch!==version)throw Error(t('Connection closed'));
 if(!response.ok){if(response.status===401||response.status===410)lock();throw Error(t(data?.error||'HTTP '+response.status))}return data;
}
function render(data){root.classList.add('on');fleet=data;$('total').textContent=si(data.hashrate);$('updated').textContent=t('Updated')+' '+new Date(data.recorded_at).toLocaleTimeString(lang);$('login').hidden=true;$('logout').hidden=false;$('manage').hidden=!access?.key;$('empty').hidden=!!data.miners.length;$('history').hidden=!data.miners.length;const fragment=document.createDocumentFragment();
 for(const m of data.miners){const card=node('article',undefined,'miner'+(m.online?'':' offline'));card.dataset.id=m.id;const heading=node('div',undefined,'heading');heading.append(node('h2',m.name),node('span',t(m.online?'Online':'Offline'),'state'));card.append(heading,node('small',m.board+' · '+m.v+' · '+m.coin),node('div',si(m.hashrate),'rate'),node('small',t('Last report')+' · '+duration(m.age_s)));const metrics=node('div',undefined,'metrics');metric(metrics,'Temperature',Number.isFinite(m.temp_c)?m.temp_c.toFixed(1)+' °C':'—');metric(metrics,'Accepted / rejected',m.accepted+' / '+m.rejected);metric(metrics,'Uptime',duration(m.uptime_s));metric(metrics,'Power',m.power_percent+' %');metric(metrics,'Best difficulty',new Intl.NumberFormat(lang,{maximumFractionDigits:0}).format(m.best));metric(metrics,'Pool',t(m.pool_connected?'Connected':'Disconnected')+' · '+m.pool);card.append(metrics);fragment.append(card)}
 $('miners').replaceChildren(fragment);const chosen=$('history-miner').value;$('history-miner').replaceChildren(...data.miners.map(m=>{const el=node('option',m.name);el.value=m.id;return el}));if(data.miners.some(m=>m.id===chosen))$('history-miner').value=chosen;
}
const number=n=>new Intl.NumberFormat(lang,{maximumFractionDigits:1}).format(n);
const temperature=n=>Number.isFinite(n)?number(n)+' °C':'—';
const stamp=n=>new Date(n).toLocaleString(lang,{day:'2-digit',month:'short',hour:'2-digit',minute:'2-digit'});
let chartMetric='rate',hover=null,plot=null;
function clearHistory(){
 series=null;hover=null;plot=null;$('chart-tooltip').hidden=true;
 for(const id of ['stat-average','stat-maximum','stat-minimum','stat-temperature','stat-temperature-max','stat-reports','stat-restarts','stat-variation'])$(id).textContent='—';
 $('history-range').textContent='';$('history-rows').replaceChildren();$('export-history').disabled=true;
 const c=$('chart');c.getContext('2d').clearRect(0,0,c.width,c.height);
}
function historyStats(){
 const s=series?.summary;
 $('stat-average').textContent=si(s?.average);$('stat-maximum').textContent=si(s?.maximum);
 $('stat-minimum').textContent=t('Minimum')+' · '+si(s?.minimum);
 $('stat-temperature').textContent=temperature(s?.temperature_average);
 $('stat-temperature-max').textContent=t('Peak')+' · '+temperature(s?.temperature_maximum);
 $('stat-reports').textContent=s?number(s.reports):'—';
 $('stat-restarts').textContent=t('Observed restarts')+' · '+(s?number(s.restarts):'—');
 $('stat-variation').textContent=t('Variation')+' · '+(Number.isFinite(s?.variation_percent)?number(s.variation_percent)+' %':'—');
 $('history-range').textContent=s?.reports?stamp(s.first_at)+' → '+stamp(s.last_at):t('No history yet.');
 $('export-history').disabled=!series?.points.length;
 const rows=(series?.points||[]).slice(-8).reverse().map(p=>{
  const row=node('tr');for(const value of [stamp(p.t),si(p.rate),temperature(p.temp),number(p.samples??1)])row.append(node('td',value));return row;
 });$('history-rows').replaceChildren(...rows);
 root.querySelector('[data-metric="temp"]').disabled=!series?.points.some(p=>Number.isFinite(p.temp));
}
async function history(){
 const id=$('history-miner').value,hours=Number($('hours').value),epoch=version,seq=++historyVersion;if(!id)return;
 if(series?.miner!==id||series?.hours!==hours){clearHistory();$('history-note').textContent=t('Loading readings…');$('chart-empty').hidden=false;$('chart-empty').textContent=t('Loading readings…')}
 try{
  const data=await api('/api/remote/history?miner='+encodeURIComponent(id)+'&hours='+hours);
  if(epoch!==version||seq!==historyVersion)return;
  series={...data,miner:id,hours};hover=null;historyStats();draw();
 }catch(e){if(epoch===version&&seq===historyVersion){clearHistory();$('chart-empty').hidden=false;$('chart-empty').textContent=t('History unavailable');$('history-note').textContent=e.message}}
}
function draw(){
 const c=$('chart'),ratio=devicePixelRatio||1,w=c.clientWidth,h=c.clientHeight;if(!w||!series)return;
 c.width=w*ratio;c.height=h*ratio;
 const ctx=c.getContext('2d');ctx.scale(ratio,ratio);
 const pts=series.points,values=pts.map((p,i)=>({...p,index:i,value:chartMetric==='rate'?p.rate:p.temp})).filter(p=>Number.isFinite(p.value));
 $('history-note').textContent=t(!pts.length?'No history yet.':pts.length===1?'Only one reading in this period. Reports arrive every five minutes.':'Statistics use received reports. Missing periods are left empty.');
 $('chart-empty').hidden=!!values.length;
 if(!values.length){$('chart-empty').textContent=t(pts.length?'No temperature readings.':'No history yet.');$('chart-tooltip').hidden=true;plot=null;return}
 const interval=series.interval_ms,windowEnd=series.window_end||Date.now(),windowStart=series.window_start||windowEnd-series.hours*3600000;
 // Show the available history rather than compressing a new miner into the
 // final few pixels of a day. The selected period remains the maximum window.
 const start=Math.max(windowStart,pts[0].t),end=Math.max(windowEnd,start+1);
 const raw=values.flatMap(p=>chartMetric==='rate'?[p.min_rate??p.rate,p.max_rate??p.rate]:[p.temp,p.max_temp??p.temp]);
 const low=Math.min(...raw),high=Math.max(...raw),padding=Math.max((high-low)*.15,Math.abs(high)*.035,.01);
 const min=chartMetric==='rate'?Math.max(0,low-padding):low-padding,max=high+padding;
 const left=w<500?68:86,right=w-12,top=16,bottom=h-34;
 const x=p=>left+(right-left)*(p.t-start)/(end-start),y=value=>bottom-(bottom-top)*(value-min)/(max-min);
 let unit=0;const units=['H/s','kH/s','MH/s','GH/s','TH/s','PH/s'];while(high/1000**unit>=1000&&unit<units.length-1)unit++;
 const axisUnit=chartMetric==='rate'?1000**unit:1;
 const precision=Math.min(6,Math.max(0,Math.ceil(-Math.log10((max-min)/4/axisUnit))));
 const axisNumber=new Intl.NumberFormat(lang,{maximumFractionDigits:precision});
 const format=value=>axisNumber.format(value/axisUnit)+' '+(chartMetric==='rate'?units[unit]:'°C');
 const css=getComputedStyle(document.documentElement),colour=(name,fallback)=>css.getPropertyValue(name).trim()||fallback;
 ctx.font='11px system-ui';ctx.fillStyle=colour('--label','#777e90');ctx.strokeStyle=colour('--chart-grid','#26282f');ctx.lineWidth=1;
 for(let i=0;i<5;i++){
  const value=min+(max-min)*i/4,py=y(value);ctx.beginPath();ctx.moveTo(left,py);ctx.lineTo(right,py);ctx.stroke();ctx.fillText(format(value),0,py+4);
 }
 const ticks=w<500?3:6;
 for(let i=0;i<ticks;i++){
  const at=start+(end-start)*i/(ticks-1),px=left+(right-left)*i/(ticks-1);
  const label=new Date(at).toLocaleString(lang,end-start>86400000?{day:'2-digit',month:'short',hour:'2-digit'}:{hour:'2-digit',minute:'2-digit'});
  ctx.textAlign=i===0?'left':i===ticks-1?'right':'center';ctx.fillText(label,px,h-9);
 }ctx.textAlign='left';
 const color=chartMetric==='rate'?'#f7931a':'#5d9eff',rgb=chartMetric==='rate'?'247,147,26':'93,158,255';
 ctx.save();ctx.beginPath();ctx.rect(left,top-4,right-left,bottom-top+8);ctx.clip();
 const gradient=ctx.createLinearGradient(0,top,0,bottom);gradient.addColorStop(0,'rgba('+rgb+',.35)');gradient.addColorStop(1,'rgba('+rgb+',.025)');
 const runs=[];let run=[];
 for(const p of values){
  const previous=run[run.length-1];
  if(previous&&(p.index!==previous.index+1||p.mixed_boots||previous.mixed_boots||p.boot!==previous.boot||p.t-previous.t>interval*2.5)){runs.push(run);run=[]}
  run.push(p);
 }if(run.length)runs.push(run);
 for(const group of runs){
  if(group.length===1){
   const p=group[0],py=y(p.value);ctx.fillStyle=color;ctx.fillRect(x(p)-4,Math.min(py,bottom-3),8,Math.max(3,bottom-py));continue;
  }
  ctx.beginPath();ctx.moveTo(x(group[0]),bottom);for(const p of group)ctx.lineTo(x(p),y(p.value));ctx.lineTo(x(group[group.length-1]),bottom);ctx.closePath();ctx.fillStyle=gradient;ctx.fill();
  ctx.beginPath();group.forEach((p,i)=>i?ctx.lineTo(x(p),y(p.value)):ctx.moveTo(x(p),y(p.value)));ctx.strokeStyle=color;ctx.lineWidth=2;ctx.stroke();
 }
 const average=chartMetric==='rate'?series.summary?.average:series.summary?.temperature_average;
 if(Number.isFinite(average)){ctx.beginPath();ctx.setLineDash([5,5]);ctx.moveTo(left,y(average));ctx.lineTo(right,y(average));ctx.strokeStyle=colour('--label','#777e90');ctx.lineWidth=1;ctx.stroke();ctx.setLineDash([])}
 plot={values,x,y,left,right};
 const chosen=values.find(p=>p.index===hover);$('chart-tooltip').hidden=!chosen;
 if(chosen){
  const px=x(chosen),py=y(chosen.value);ctx.beginPath();ctx.setLineDash([3,4]);ctx.moveTo(px,top);ctx.lineTo(px,bottom);ctx.strokeStyle=colour('--text','#b1b5c3');ctx.stroke();ctx.setLineDash([]);ctx.beginPath();ctx.arc(px,py,4,0,Math.PI*2);ctx.fillStyle=color;ctx.fill();
  const tip=$('chart-tooltip');tip.querySelector('time').textContent=stamp(chosen.t);tip.querySelector('strong').textContent=chartMetric==='rate'?si(chosen.rate):temperature(chosen.temp);
  tip.querySelector('small').textContent=number(chosen.samples??1)+' '+t('reports')+' · '+(chartMetric==='rate'?temperature(chosen.temp):si(chosen.rate));
  tip.style.left=Math.max(0,Math.min(w-tip.offsetWidth,px+12))+'px';tip.style.top=Math.max(0,py-tip.offsetHeight-12)+'px';
 }
 ctx.restore();
 c.setAttribute('aria-label',t(chartMetric==='rate'?'Hashrate':'Temperature')+' · '+values.length+' '+t('readings')+'. '+t('Use left and right arrows to inspect readings.'));
}
function inspectChart(event){
 if(!plot)return;const x=event.clientX-$('chart').getBoundingClientRect().left;
 if(x<plot.left||x>plot.right){hover=null;draw();return}
 hover=plot.values.reduce((best,p)=>Math.abs(plot.x(p)-x)<Math.abs(plot.x(best)-x)?p:best).index;draw();
}
$('chart').addEventListener('pointermove',inspectChart);$('chart').addEventListener('pointerdown',inspectChart);
$('chart').addEventListener('pointerleave',()=>{hover=null;draw()});$('chart').addEventListener('blur',()=>{hover=null;draw()});
$('chart').addEventListener('keydown',e=>{
 if(!plot||!['ArrowLeft','ArrowRight','Home','End'].includes(e.key))return;e.preventDefault();
 const items=plot.values,index=items.findIndex(p=>p.index===hover);
 const next=e.key==='Home'?0:e.key==='End'?items.length-1:index<0?items.length-1:Math.max(0,Math.min(items.length-1,index+(e.key==='ArrowRight'?1:-1)));
 hover=items[next].index;draw();
});
for(const button of root.querySelectorAll('[data-metric]'))button.onclick=()=>{
 chartMetric=button.dataset.metric;hover=null;for(const other of root.querySelectorAll('[data-metric]'))other.setAttribute('aria-pressed',String(other===button));draw();
};
$('export-history').onclick=()=>{
 if(!series?.points.length)return;
 const rows=[['time_utc','hashrate_average_hs','hashrate_minimum_hs','hashrate_maximum_hs','temperature_average_c','temperature_maximum_c','reports','mixed_boots'],...series.points.map(p=>[new Date(p.t).toISOString(),p.rate,p.min_rate??p.rate,p.max_rate??p.rate,p.temp??'',p.max_temp??'',p.samples??1,!!p.mixed_boots])];
 const blob=new Blob([rows.map(row=>row.join(',')).join('\r\n')+'\r\n'],{type:'text/csv;charset=utf-8'}),url=URL.createObjectURL(blob),link=node('a');link.href=url;link.download='bdlminer-history-'+series.hours+'h.csv';link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
};
async function refresh(){const epoch=version;try{const data=await api('/api/remote');if(epoch!==version)return;$('notice').textContent='';render(data);await history()}catch(e){if(epoch===version||!access)$('notice').textContent=e.message;if(fleet&&epoch===version){$('updated').textContent=t('Connection failed. Last readings may be out of date.');$('total').textContent='—';for(const card of $('miners').children){card.classList.add('offline');card.querySelector('.state').textContent=t('Offline')}}}finally{if(access&&epoch===version){clearTimeout(timer);timer=setTimeout(refresh,30000)}}}
function open(next){version++;access=next;$('key').value='';void refresh()}
$('login-form').onsubmit=e=>{e.preventDefault();const value=$('key').value.trim();
 if(KEY.test(value)){stored(null);open({key:true,value});return}
 const a=address(value);if(!a){$('notice').textContent=t('Enter the payout address set on your miners.');return}
 stored($('remember').checked?a:null);open({key:false,value:a})};
$('logout').onclick=()=>{stored(null);lock()};
new MutationObserver(()=>{const next=siteLang();if(next!==lang){lang=next;translate();if(fleet)render(fleet);if(series)historyStats()}draw()}).observe(document.documentElement,{attributes:true,attributeFilter:['lang','data-theme']});
$('history-miner').onchange=history;$('hours').onchange=history;
$('revoke').onclick=async()=>{if(!access?.key||!confirm(t('Erase all private history and permanently revoke this key?')))return;try{await api('/api/remote','DELETE');lock();$('notice').textContent=t('Data erased and key revoked.')}catch(e){$('notice').textContent=e.message}};
new ResizeObserver(draw).observe($('chart'));translate();
const saved=address(stored());if(saved)open({key:false,value:saved});
})();
