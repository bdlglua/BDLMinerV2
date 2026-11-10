// Site behaviour: language, flasher, live statistics, chat. The page sets
// window.BDL_BOARDS and window.BDL_VERSION (filled in by tools/make_site_pages.py).
const BOARDS=window.BDL_BOARDS||[],VERSION=window.BDL_VERSION||'';
const STATS='https://api.bdl-pool.com:36018/api/summary';
const localStatsPreview=new URLSearchParams(location.search).get('statsApi')==='go'&&
 (location.protocol==='file:'||['localhost','127.0.0.1','[::1]'].includes(location.hostname));
const statsEndpoint=localStatsPreview?'http://127.0.0.1:8787/api/summary':STATS;
let poolNames={},poolLinks={};try{const data=JSON.parse(document.getElementById('site-data').textContent);poolNames=data.pool_names||{};poolLinks=data.pool_links||{}}catch{}
const FR={skip:'Aller au contenu',navFeatures:'Fonctions',navBoards:'Cartes',navInstall:'Flasher',navStats:'Statistiques',navCommunity:'Communauté',
navHome:'Accueil',navStart:'Bien démarrer',navDemo:'Démo',navMore:'Explorer',navFAQ:'FAQ',navNews:'Nouveautés',navSupport:'Soutenir',navPages:'Toutes les pages',
navRemote:'Mes mineurs',
heroPill:'Logiciel libre · mises à jour signées',famAll:'Toutes',famEsp32:'ESP32',famS3:'ESP32-S3',famS2c3:'S2 / C3',famAsic:'ASIC',famGroup:'Famille de cartes',searchLabel:'Rechercher une carte',searchBoard:'Rechercher une carte',noMatch:'Aucune carte ne correspond.',cardMiners:'{n} miners',cardMiner:'{n} miner',check1:'SHA-256 matériel sur ESP32, S2, S3 et C3',check2:'Stratum V1 et V2 chiffré (Noise)',check3:'Bitaxe Gamma jusqu’à 800 MHz',
heroKicker:'Firmware libre · ESP32 et ASIC',deviceCaption:'Statistiques sur l’écran du mineur',
worksWith:'Pools solo compatibles',marketTitle:'Réseau en direct',marketLead:'Hashrate partagé par carte · actualisé toutes les 20 secondes.',
marketAll:'Toutes les statistiques',colShare:'Part des miners',liveFleet:'Parc en direct',heroMiners:'miners en ligne',heroRecord:'Meilleure heure',
title:'Minage Bitcoin solo, <b>sur petite carte.</b>',
tagline:'Firmware libre pour mineurs ESP32 et Bitaxe.',
ctaInstall:'Installer',ctaHub:'Kit Linux',ctaBoards:'Cartes compatibles',version:'Firmware : '+VERSION,
featTitle:'Pensé pour le minage solo',
f1t:'SHA-256 matériel',f2t:'Stratum V1 et V2',f3t:'Dashboard web',f4t:'Mises à jour signées',
boardsTitle:'Cartes compatibles',boardsLead:'Firmware disponible et hashrate mesuré.',
installTitle:'Flasher ta carte',installLead:'Aucun logiciel à installer : Chrome ou Edge sur un ordinateur flashe la carte par USB.',
board:'Ta carte',install:'Flasher',unsupported:'Ce navigateur ne peut pas installer : utilise Chrome ou Edge sur un ordinateur.',
notallowed:'La page doit être ouverte en HTTPS.',erase:'Version '+VERSION+'. La première installation efface la carte (réglages et statistiques).',
s1:'Branche la carte à cet ordinateur avec un câble USB qui transmet les données.',
s2:'Choisis la carte, clique sur Flasher et sélectionne son port série.',
s3:'Au redémarrage, connecte-toi au WiFi BDLMinerV2-XXXX (mot de passe MineYourCoins) et entre ton WiFi, la pool et ton adresse BTC.',
after:'Ouvre ensuite le tableau de bord sur <b>http://bdlminer-xxxx.local</b>. Les mises à jour arrivent toutes seules.',
statsTitle:'Statistiques réseau',statsLead:'Données anonymisées · actualisation toutes les 20 secondes.',networkTotal:'Hashrate du parc',networkMiners:'Mineurs déclarant des mesures',networkAverage:'Moyenne actuelle par mineur',networkPeak:'Meilleure heure enregistrée',
poolOther:'Autre / pool privée',poolUnknown:'Pool non renseignée',byPool:'Par pool',colPool:'Pool',colCoin:'Cryptomonnaie',coinsTitle:'Par cryptomonnaie',
devicesTitle:'Appareils',online:'en ligne',offline:'hors ligne',
devOnline:'en ligne',devOffline:'hors ligne',devNone:'aucun pour l\'instant',devMore:'Voir les {n} autres cartes compatibles',devFewer:'Masquer les cartes sans miner',menu:'Menu',
tHashrate:'Hashrate total',tMiners:'Mineurs',tAverage:'Moyenne / mineur',tRecord:'Pic horaire',
chatTitle:'Discussion',chatOpen:'Ouvrir la discussion',themeDark:'Thème sombre',chatUnread:'non lus',chatClose:'Fermer la discussion',chatSettings:'Réglages',chatName:'Pseudo',chatNamePh:'Ton pseudo',
chatSave:'Enregistrer',chatNameHint:'Modifiable tous les 30 jours',chatWhen:'Modifiable le {d}',chatNow:'Modifiable maintenant',
chatCodeTitle:'Autre appareil',chatQrShow:'📱 Mon QR code',chatQrHide:'Masquer le QR code',chatQrAlt:'QR code pour ouvrir la discussion avec ton pseudo',chatLink:'Copier le lien',chatLinkCopied:'Lien copié : ouvre-le sur ton autre appareil. Ne le partage avec personne.',chatQrFailed:'Impossible d\'afficher le QR code pour l\'instant : utilise « Copier le lien ».',chatCodeHint:'🔒 Scanne-le avec ton téléphone. Ne le montre à personne.',
chatBack:'Retour à la discussion',chatMembers:'Membres en ligne',chatYou:'(toi)',chatGuests:'+ {n} visiteur(s) sans pseudo',chatNobody:'Personne avec un pseudo pour l\'instant.',chatNeedName:'Choisis d\'abord ton pseudo.',chatSaved:'Pseudo enregistré.',
chatMessage:'Message',chatTextPh:'Message…',chatSend:'Envoyer',chatAttach:'Photo ou vidéo',chatRecord:'Message vocal',chatRemove:'Retirer',
chatRules:'Messages, photos et vidéos publics, gardés 7 jours. Ne partage jamais de phrase de récupération, de clé privée ni de mot de passe.',
chatEmpty:'Aucun message pour l\'instant : dis bonjour !',chatTeam:'équipe',chatEdited:'modifié',chatEdit:'Modifier',chatDelete:'Supprimer',chatSure:'Sûr ?',chatBan:'Bannir',
chatSaveEdit:'Enregistrer',chatCancel:'Annuler',chatOffline:'Connexion au chat…',chatUploading:'Envoi du fichier…',chatRecording:'Enregistrement… {s} s (clique sur 🎤 pour arrêter)',
chatVoice:'Message vocal ({s} s)',chatPlay:'Écouter',chatPause:'Pause',chatPosition:'Position dans le message vocal',chatPhoto:'Photo',chatVideo:'Vidéo',chatNoMic:'Micro indisponible ou refusé.',
chatCodeSet:'Ton pseudo t\'a suivi sur cet appareil.',
chatErrors:{taken:'Ce pseudo est déjà pris.',wait:'Tu pourras changer de pseudo le {d}.',too_many:'Trop de nouveaux pseudos depuis ta connexion aujourd\'hui : réessaie demain.',
 name:'Choisis d\'abord ton pseudo.',reserved:'Ce pseudo est réservé à l\'équipe.',empty:'Le message est vide.',too_long:'Message trop long (500 caractères au plus).',
 blocked:'Message bloqué : il ressemble à une arnaque.',link:'Les liens ne sont permis que vers GitHub, Discord, Reddit et le site.',slow:'Doucement : un message toutes les 3 secondes.',
 banned:'Tu ne peux plus écrire dans ce chat.',forbidden:'Tu ne peux modifier ou supprimer que tes propres messages.',invalid:'Message invalide.',gone:'Ce message n\'existe plus.',
 media:'Fichier introuvable : renvoie-le.',media_type:'Format non accepté : photos, vidéos et sons seulement.',media_size:'Fichier trop lourd (photo 3 Mo, vocal 2 Mo, vidéo 8 Mo).',
 media_slow:'Pas plus de 10 fichiers par heure.',media_full:'Le chat est plein pour l\'instant : réessaie plus tard.',upload:'L\'envoi du fichier a échoué.'},
chatAdminOn:'Mode équipe activé.',chatAdminOff:'Clé refusée.',chatAdminSlow:'Trop d\'essais : réessaie dans une heure.',
live:'En direct',week:'7 jours',byBoard:'Par carte',byVersion:'Par version',colBoard:'Carte',colVersion:'Version',colMiners:'Miners',colHashrate:'Hashrate',noneNow:'Aucun miner en ce moment.',period:'Période',updated:'mis à jour il y a {s} s',now:'maintenant',
statsMore:'Données brutes : <a href="https://api.bdl-pool.com:36018/api/summary">JSON</a>.',
lotteryTitle:'Le minage solo est un loisir, pas un revenu.',
lottery:'À la vitesse d’un ESP32, trouver un bloc est extrêmement improbable.',
communityTitle:'Communauté',communityLead:'Questions, installations, meilleures shares et idées : viens dire bonjour.',
cDiscord:'Aide en direct, annonces et tickets de support.',cReddit:'r/bdlminer : montre ton installation et partage tes records.',
cGithub:'Code source, documentation, versions, signalement de bugs et idées.',
license:'BDLMiner V2 est un logiciel libre sous licence GNU GPL v3.0.',security:'Sécurité',accessibility:'Accessibilité',releases:'Versions',
altMining:'L\'écran du miner : 1,005 MH/s, 10827 shares, meilleure difficulté 7,82, 52,1 °C',altGraph:'Écran du graphique du hashrate',
altGlobal:'Écran du prix du Bitcoin et du réseau',altClock:'Écran de l\'horloge',screens:'Écrans',
noStats:'Pas encore de rapport : les miners partagent leur hashrate à partir de la version 1.9.0.',statsDown:'Statistiques indisponibles pour le moment.',
experimental:'expérimental',chartDay:'Hashrate total toutes les 5 minutes, 24 dernières heures',chartWeek:'Hashrate total par heure, 7 derniers jours',noChart:'La courbe se remplit au fil des rapports.'};
const EN={noMatch:'No board matches.',cardMiners:'{n} miners',cardMiner:'{n} miner',colCoin:'Cryptocurrency',coinsTitle:'By cryptocurrency',poolOther:'Other / private pool',poolUnknown:'Not shared',colPool:'Pool',chatUnread:'unread',devMore:'Show the {n} other supported boards',devFewer:'Hide the boards without a miner',chatYou:'(you)',chatGuests:'+ {n} visitor(s) without a name',chatNobody:'Nobody with a name yet.',chatWhen:'Changeable on {d}',chatNow:'Changeable now',chatNeedName:'Choose your name first.',chatSaved:'Name registered.',
chatEmpty:'No message yet: say hello!',chatTeam:'team',chatEdited:'edited',chatEdit:'Edit',chatDelete:'Delete',chatSure:'Sure?',chatBan:'Ban',chatSaveEdit:'Save',chatCancel:'Cancel',
chatOffline:'Connecting to the chat…',chatUploading:'Sending the file…',chatRecording:'Recording… {s} s (click 🎤 to stop)',chatVoice:'Voice message ({s} s)',chatPlay:'Play',chatPause:'Pause',chatPosition:'Position in the voice message',chatPhoto:'Photo',chatVideo:'Video',
chatNoMic:'Microphone unavailable or refused.',chatQrHide:'Hide the QR code',chatLinkCopied:'Link copied: open it on your other device. Share it with no one.',chatQrFailed:'The QR code cannot be shown for now: use “Copy the link”.',chatCodeSet:'Your name followed you to this device.',
chatErrors:{taken:'This name is already taken.',wait:'You can change your name on {d}.',too_many:'Too many new names from your connection today: try again tomorrow.',
 name:'Choose your name first.',reserved:'This name is kept for the team.',empty:'The message is empty.',too_long:'Message too long (500 characters at most).',
 blocked:'Message blocked: it looks like a scam.',link:'Links are only allowed to GitHub, Discord, Reddit and the site.',slow:'Easy: one message every 3 seconds.',
 banned:'You can no longer write in this chat.',forbidden:'You can only edit or delete your own messages.',invalid:'Invalid message.',gone:'This message no longer exists.',
 media:'File not found: send it again.',media_type:'Format not accepted: photos, videos and sounds only.',media_size:'File too large (photo 3 MB, voice 2 MB, video 8 MB).',
 media_slow:'No more than 10 files an hour.',media_full:'The chat is full for now: try again later.',upload:'Sending the file failed.'},
chatAdminOn:'Team mode on.',chatAdminOff:'Key refused.',chatAdminSlow:'Too many tries: try again in an hour.',colBoard:'Board',colVersion:'Version',colMiners:'Miners',colHashrate:'Hashrate',noneNow:'No miner right now.',devOnline:'online',devOffline:'offline',devNone:'none yet',noStats:'No reports yet: miners share their hashrate from version 1.9.0 on.',statsDown:'Statistics unavailable for now.',
experimental:'experimental',chartDay:'Total hashrate every 5 minutes, last 24 hours',chartWeek:'Total hashrate per hour, last 7 days',noChart:'The chart fills up as the reports come.',
updated:'updated {s} s ago',now:'now'};
Object.assign(FR,{"sPerformance": "PERFORMANCES DU PARC", "sHistory": "Statistiques de minage", "sAverage": "Hashrate moyen", "sMaximum": "Hashrate maximal", "sMiners": "Mineurs actifs moyens", "sSamples": "Mesures disponibles", "sMetric": "Mesure affichée", "sHashrate": "Hashrate", "sMean": "Moyenne de la période", "sNote": "Statistiques issues des mesures agrégées de cinq minutes. Les périodes manquantes restent vides ; la valeur instantanée est exclue des moyennes.", "sRecent": "Dernières mesures", "sTime": "Heure", "sSourceSamples": "Mesures sources", "sMinimum": "Minimum", "sPeakMiners": "Maximum", "sVariation": "Écart-type relatif", "sCoverage": "{n} % · {total} attendues", "sNoMeasurements": "Aucune mesure disponible sur cette période.", "sChartAria": "{metric}. Utilisez les flèches gauche et droite pour consulter les mesures.", "sInstant": "Valeur instantanée", "sHourMean": "Moyenne horaire", "sFiveMinute": "Moyenne sur 5 min"});
Object.assign(EN,{"sMinimum": "Minimum", "sPeakMiners": "Peak", "sVariation": "Relative deviation", "sCoverage": "{n}% · {total} expected", "sNoMeasurements": "No measurements available in this period.", "sChartAria": "{metric}. Use left and right arrows to inspect measurements.", "sInstant": "Instantaneous value", "sHourMean": "Hourly average", "sFiveMinute": "Five-minute average"});
document.querySelectorAll('[data-t]').forEach(e=>EN[e.dataset.t]=e.textContent);
document.querySelectorAll('[data-h]').forEach(e=>EN[e.dataset.h]=e.innerHTML);
document.querySelectorAll('[data-alt]').forEach(e=>EN[e.dataset.alt]=e.alt);
document.querySelectorAll('[data-al]').forEach(e=>EN[e.dataset.al]=e.getAttribute('aria-label'));
document.querySelectorAll('[data-ph]').forEach(e=>EN[e.dataset.ph]=e.placeholder);
Object.assign(FR,{sourceCode:'Code source sur GitHub ↗',firmwareRelease:'Firmware '+VERSION,statsLoading:'Chargement des statistiques…',statsRetained:'Dernières mesures conservées ; actualisation en attente.',statsSaved:'Dernières mesures'});
Object.assign(EN,{statsLoading:'Loading statistics…',statsRetained:'Showing the last readings; waiting for an update.',statsSaved:'Last readings'});
Object.assign(FR,{hashHistoryTitle:'Évolution du hashrate',screensTitle:'L’écran du mineur'});
let lang=(navigator.language||'').startsWith('fr')?'fr':'en';
try{lang=localStorage.getItem('lang')||lang}catch(e){}
const T=k=>(lang=='fr'?FR:EN)[k];
let screen='mining',summary=null;

function rate(h){const u=['H/s','kH/s','MH/s','GH/s','TH/s','PH/s','EH/s'];if(!(h>0))return'0 H/s';let i=0;while(h>=1000&&i<u.length-1){h/=1000;i++}
 return h.toLocaleString(lang=='fr'?'fr-FR':'en-US',{maximumFractionDigits:h>=100?0:h>=10?1:2})+' '+u[i]}

function showScreen(name){if(!document.getElementById('screen'))return;screen=name;const suffix=lang=='fr'?'-fr':'';
 document.getElementById('screen').src='assets/screen-'+name+suffix+'.png';
 const big=document.getElementById('screen');big.alt=T({mining:'altMining',graph:'altGraph',global:'altGlobal',clock:'altClock'}[name]);
 document.querySelectorAll('[data-screen]').forEach(b=>{b.setAttribute('aria-pressed',b.dataset.screen==name);b.querySelector('img').src='assets/screen-'+b.dataset.screen+suffix+'.png'})}
let rotate=!document.getElementById('screen')||matchMedia('(prefers-reduced-motion: reduce)').matches?null:setInterval(()=>{const order=['mining','graph','global','clock'];showScreen(order[(order.indexOf(screen)+1)%order.length])},4000);
document.querySelectorAll('[data-screen]').forEach(b=>b.onclick=()=>{clearInterval(rotate);rotate=null;showScreen(screen==b.dataset.screen?'mining':b.dataset.screen)});

function boards(){const ul=document.getElementById('board-list');if(!ul)return;ul.innerHTML='';
 for(const b of BOARDS){const li=document.createElement('li');const exp=/experimental/i.test(b.name);li.className=exp?'exp':'';
  li.textContent=exp?b.name.replace(/\s*\(experimental[^)]*\)/i,'')+' ('+T('experimental')+')':b.name;ul.appendChild(li)}}

// Public totals: historical period indicators exclude the instantaneous live value.
let range='day',statsMetric='hashrate',statsHover=null,statsPlot=null;
const statsHours={hour:1,six:6,day:24,week:168};
const statsNumber=n=>Number.isFinite(n)?n.toLocaleString(lang==='fr'?'fr-FR':'en-US',{maximumFractionDigits:1}):'—';
const statsRate=n=>Number.isFinite(n)?rate(n):'—';
const statsStamp=t=>new Date(t).toLocaleString(lang==='fr'?'fr-FR':'en-US',{day:'2-digit',month:'short',hour:'2-digit',minute:'2-digit'});
function statsSeries(){
 if(!summary)return[];
 const hours=statsHours[range],end=summary.at,from=end-hours*3600000;
 const hourly=p=>({...p,at:p.end_at??p.at+3600000,interval_ms:3600000});
 const source=hours===168?[...(summary.history||[]).map(hourly),...(summary.current_hour?[hourly(summary.current_hour)]:[]),...(summary.recent||[]).filter(p=>p.live)]:summary.recent||[];
 return source.filter(p=>Number.isFinite(p.at)&&p.at>from&&p.at<=end&&Number.isFinite(p.hashrate)&&p.hashrate>=0&&Number.isFinite(p.average_miners??p.miners)&&(p.average_miners??p.miners)>=0)
  .map(p=>({...p,miners:p.average_miners??p.miners,interval_ms:p.interval_ms||(hours===168?3600000:300000),samples:p.live?0:p.samples??1})).sort((a,b)=>a.at-b.at);
}
function statsPeriod(points){
 const p=summary?.periods?.[statsHours[range]],set=(id,value)=>document.getElementById(id).textContent=value;
 set('st-period-average',statsRate(p?.average_hashrate));set('st-period-maximum',statsRate(p?.maximum_hashrate));
 set('st-period-minimum',T('sMinimum')+' · '+statsRate(p?.minimum_hashrate));
 set('st-period-variation',T('sVariation')+' · '+(Number.isFinite(p?.variation_percent)?statsNumber(p.variation_percent)+' %':'—'));
 set('st-period-miners',statsNumber(p?.average_miners));set('st-period-peak-miners',T('sPeakMiners')+' · '+statsNumber(p?.maximum_miners));
 set('st-period-samples',statsNumber(p?.samples));
 const coverage=p?.expected_samples?Math.min(100,p.samples/p.expected_samples*100):null;
 set('st-period-coverage',Number.isFinite(coverage)?T('sCoverage').replace('{n}',statsNumber(coverage)).replace('{total}',statsNumber(p.expected_samples)):'—');
 set('st-period-dates',p?.samples?statsStamp(p.first_at)+' → '+statsStamp(p.last_at):points.length?statsStamp(points[0].at)+' → '+statsStamp(points.at(-1).at):T('sNoMeasurements'));
 const rows=points.slice(-8).reverse().map(point=>{
  const row=document.createElement('tr');
  for(const text of [statsStamp(point.at)+(point.live?' · '+T('live'):''),statsRate(point.hashrate),statsNumber(point.miners),point.live?'—':statsNumber(point.samples)]){
   const cell=document.createElement('td');cell.textContent=text;row.appendChild(cell);
  }return row;
 });document.getElementById('st-readings').replaceChildren(...rows);
}
function chart(update=true){
 const svg=document.getElementById('st-chart');if(!svg)return;
 const points=statsSeries(),period=summary?.periods?.[statsHours[range]],tip=document.getElementById('st-tooltip'),empty=document.getElementById('st-chart-empty');
 if(update)statsPeriod(points);
 const W=Math.max(280,Math.round(svg.clientWidth||960)),H=svg.clientWidth<500?280:320,left=W<500?70:86,right=W-12,top=16,bottom=H-34;
 svg.setAttribute('viewBox','0 0 '+W+' '+H);tip.hidden=true;empty.hidden=!!points.length;
 svg.setAttribute('aria-label',T('sChartAria').replace('{metric}',T(statsMetric==='hashrate'?'sHashrate':'tMiners')));
 if(!points.length){svg.replaceChildren();empty.textContent=T('sNoMeasurements');statsPlot=null;return}
 const palette=getComputedStyle(document.documentElement);
 const color=palette.getPropertyValue(statsMetric==='hashrate'?'--accent':'--blue').trim();
 svg.style.setProperty('--plot-color',color);
 const start=Math.max(summary.at-statsHours[range]*3600000,points[0].at),end=Math.max(summary.at,start+1),values=points.map(p=>p[statsMetric]);
 const extrema=statsMetric==='hashrate'?[period?.minimum_hashrate,period?.maximum_hashrate]:[period?.minimum_miners,period?.maximum_miners];
 const low=Math.min(...values,...extrema.filter(Number.isFinite)),high=Math.max(...values,...extrema.filter(Number.isFinite));
 const padding=Math.max((high-low)*.15,high*.035,high?0.01:1),min=Math.max(0,low-padding),max=high+padding;
 const x=p=>left+(right-left)*(p.at-start)/(end-start),y=value=>bottom-(bottom-top)*(value-min)/(max-min),xy=p=>x(p).toFixed(2)+' '+y(p[statsMetric]).toFixed(2);
 let unit=0;const units=['H/s','kH/s','MH/s','GH/s','TH/s','PH/s','EH/s'];while(statsMetric==='hashrate'&&high/1000**unit>=1000&&unit<units.length-1)unit++;
 const scale=statsMetric==='hashrate'?1000**unit:1,precision=Math.min(6,Math.max(0,Math.ceil(-Math.log10((max-min)/4/scale))));
 const axisNumber=new Intl.NumberFormat(lang==='fr'?'fr-FR':'en-US',{maximumFractionDigits:precision});
 let grid='';
 for(let i=0;i<5;i++){
  const value=min+(max-min)*i/4,py=y(value).toFixed(2),label=axisNumber.format(value/scale)+(statsMetric==='hashrate'?' '+units[unit]:'');
  grid+='<line class="grid-h" x1="'+left+'" x2="'+right+'" y1="'+py+'" y2="'+py+'"/><text x="0" y="'+(Number(py)+4)+'">'+label+'</text>';
 }
 const ticks=W<500?3:6,locale=lang==='fr'?'fr-FR':'en-US';
 for(let i=0;i<ticks;i++){
  const at=start+(end-start)*i/(ticks-1),px=left+(right-left)*i/(ticks-1);
  const label=new Date(at).toLocaleString(locale,end-start>86400000?{day:'2-digit',month:'short',hour:'2-digit'}:{hour:'2-digit',minute:'2-digit'});
  grid+='<text x="'+px.toFixed(2)+'" y="'+(H-9)+'" text-anchor="'+(!i?'start':i===ticks-1?'end':'middle')+'">'+label+'</text>';
 }
 const groups=[];let group=[];
 for(const p of points.filter(p=>!p.live)){
  const previous=group.at(-1);
  if(previous&&(p.missing||previous.missing||p.at-previous.at>Math.max(p.interval_ms,previous.interval_ms)*2.5)){groups.push(group);group=[]}
  group.push(p);
 }if(group.length)groups.push(group);
 let shapes='';
 for(const run of groups){
  if(run.length===1){const p=run[0],py=y(p[statsMetric]);shapes+='<rect x="'+(x(p)-4).toFixed(2)+'" y="'+Math.min(py,bottom-3).toFixed(2)+'" width="8" height="'+Math.max(3,bottom-py).toFixed(2)+'" fill="'+color+'"/>';continue}
  const line=run.map((p,i)=>(i?'L':'M')+xy(p)).join(' ');
  shapes+='<path d="'+line+' L'+x(run.at(-1)).toFixed(2)+' '+bottom+' L'+x(run[0]).toFixed(2)+' '+bottom+' Z" fill="url(#st-area-gradient)"/><path class="line" d="'+line+'"/>';
 }
 const mean=statsMetric==='hashrate'?period?.average_hashrate:period?.average_miners;
 if(Number.isFinite(mean))shapes+='<line class="mean" x1="'+left+'" x2="'+right+'" y1="'+y(mean).toFixed(2)+'" y2="'+y(mean).toFixed(2)+'"/>';
 const live=points.find(p=>p.live),last=points.filter(p=>!p.live).at(-1);
 if(live){
  if(last&&!last.missing&&live.at-last.at<=last.interval_ms*2.5)shapes+='<path class="live-tail" d="M'+xy(last)+' L'+xy(live)+'"/>';
  shapes+='<circle class="point" cx="'+x(live).toFixed(2)+'" cy="'+y(live[statsMetric]).toFixed(2)+'" r="4"/>';
 }
 const chosen=points[statsHover];
 if(chosen)shapes+='<line class="crosshair" x1="'+x(chosen).toFixed(2)+'" x2="'+x(chosen).toFixed(2)+'" y1="'+top+'" y2="'+bottom+'"/><circle class="point" cx="'+x(chosen).toFixed(2)+'" cy="'+y(chosen[statsMetric]).toFixed(2)+'" r="4"/>';
 svg.innerHTML='<defs><linearGradient id="st-area-gradient" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="'+color+'" stop-opacity=".4"/><stop offset="100%" stop-color="'+color+'" stop-opacity="0"/></linearGradient><clipPath id="st-plot-clip"><rect x="'+left+'" y="'+(top-4)+'" width="'+(right-left)+'" height="'+(bottom-top+8)+'"/></clipPath></defs>'+grid+'<g clip-path="url(#st-plot-clip)">'+shapes+'</g>';
 statsPlot={points,x,left,right,W};
 if(chosen){
  tip.hidden=false;tip.querySelector('time').textContent=statsStamp(chosen.at)+(chosen.live?' · '+T('live'):'');
  tip.querySelector('strong').textContent=statsMetric==='hashrate'?statsRate(chosen.hashrate):statsNumber(chosen.miners)+' '+T('tMiners').toLowerCase();
  tip.querySelector('small').textContent=(chosen.live?T('sInstant'):T(chosen.interval_ms===3600000?'sHourMean':'sFiveMinute'))+' · '+(statsMetric==='hashrate'?statsNumber(chosen.miners)+' '+T('tMiners').toLowerCase():statsRate(chosen.hashrate));
  tip.style.left=Math.max(0,Math.min(W-tip.offsetWidth,x(chosen)+12))+'px';tip.style.top=Math.max(0,y(chosen[statsMetric])-tip.offsetHeight-12)+'px';
 }
}
document.querySelectorAll('[data-range]').forEach(button=>button.onclick=()=>{
 range=button.dataset.range;statsHover=null;document.querySelectorAll('[data-range]').forEach(other=>other.setAttribute('aria-pressed',String(other===button)));chart();
});
document.querySelectorAll('[data-st-metric]').forEach(button=>button.onclick=()=>{
 statsMetric=button.dataset.stMetric;statsHover=null;document.querySelectorAll('[data-st-metric]').forEach(other=>other.setAttribute('aria-pressed',String(other===button)));chart(false);
});
const statsChart=document.getElementById('st-chart');
if(statsChart){
 const inspect=event=>{
  if(!statsPlot)return;const rect=statsChart.getBoundingClientRect(),px=(event.clientX-rect.left)*statsPlot.W/rect.width;
  const next=px<statsPlot.left||px>statsPlot.right?null:statsPlot.points.reduce((best,p,i)=>Math.abs(statsPlot.x(p)-px)<Math.abs(statsPlot.x(statsPlot.points[best])-px)?i:best,0);
  if(next!==statsHover){statsHover=next;chart(false)}
 };
 statsChart.addEventListener('pointermove',inspect);statsChart.addEventListener('pointerdown',inspect);
 for(const name of ['pointerleave','blur'])statsChart.addEventListener(name,()=>{statsHover=null;chart(false)});
 statsChart.addEventListener('keydown',event=>{
  if(!statsPlot||!['ArrowLeft','ArrowRight','Home','End'].includes(event.key))return;event.preventDefault();
  statsHover=event.key==='Home'?0:event.key==='End'?statsPlot.points.length-1:statsHover===null?statsPlot.points.length-1:Math.max(0,Math.min(statsPlot.points.length-1,statsHover+(event.key==='ArrowRight'?1:-1)));chart(false);
 });

}
addEventListener('resize',()=>chart(false));
let fetchedAt=0,statsFailed=false,statsLoading=false;
function statsConnection(){
 const age=summary?Date.now()-summary.at:Infinity;
 const fresh=!!summary&&!statsFailed&&age>=-60000&&age<=60000&&Date.now()-fetchedAt<=60000;
 const message=summary?T('statsRetained'):T(statsFailed?'statsDown':'statsLoading');
 const text=(element,value)=>{if(element&&element.textContent!==value)element.textContent=value};
 const label=document.getElementById('st-live-label'),live=document.getElementById('st-live');
 text(label,fresh?T('live'):summary?T('statsSaved'):T(statsFailed?'statsDown':'statsLoading'));
 if(live){live.classList.toggle('is-stale',!fresh);live.querySelector('.dot').className='dot '+(fresh?'on':'none')}
 const updated=document.getElementById('st-updated');
 text(updated,summary?T('updated').replace('{s}',Math.max(0,Math.round(age/1000))):'');
 const home=document.getElementById('hm-status');if(home){home.hidden=fresh;text(home,fresh?'':message)}
 const status=document.getElementById('st-status');
 if(status){if(fresh&&!(summary.miners===0&&!summary.history?.length)){const html=T('statsMore');if(status.innerHTML!==html)status.innerHTML=html}else text(status,fresh?T('noStats'):message)}
}
setInterval(statsConnection,1000);

// Every supported board: green when some are online now, red when the only
// ones seen went offline (the service estimates them: reports carry no
// identifier), grey when none was seen yet.
const SHORT={'m5stickc-plus2':['M5StickC Plus2'],'m5stack-esp32':['M5Stack (ESP32)'],'m5stack-s3':['M5Stack (ESP32-S3)'],
 'lilygo-t-display':['LilyGO T-Display'],'lilygo-t-display-s3':['LilyGO T-Display-S3'],'esp32-2432s028r':['Cheap Yellow Display'],
 'esp32-headless':['ESP32 without a screen','ESP32 sans écran'],'esp32s3-headless':['ESP32-S3 without a screen','ESP32-S3 sans écran'],
 'esp32s2-headless':['ESP32-S2 without a screen','ESP32-S2 sans écran'],'esp32c3-headless':['ESP32-C3 without a screen','ESP32-C3 sans écran'],
 'bitaxe-gamma-601':['Bitaxe Gamma 601'],'nerdnos':['NerdNOS']};
let fleetAll=false;
function fleet(){const ul=document.getElementById('st-fleet');ul.innerHTML='';
 const seen=new Map((summary?.fleet||[]).map(f=>[f.board,f]));
 const known=new Set(BOARDS.map(b=>b.env));
 const label=env=>{const n=SHORT[env];return n?(lang=='fr'&&n[1]||n[0]):env};
 const rows=[...BOARDS.map(b=>b.env),...[...seen.keys()].filter(k=>!known.has(k))]
  .map(env=>({env,name:label(env),online:seen.get(env)?.online||0,offline:seen.get(env)?.offline||0}))
  .sort((a,b)=>b.online-a.online||b.offline-a.offline);
 // Boards never seen yet stay folded behind a button, unless none was seen at all.
 const seenRows=rows.filter(r=>r.online||r.offline),hidden=rows.length-seenRows.length;
 const more=document.getElementById('st-more');more.hidden=!seenRows.length||!hidden;
 more.textContent=T(fleetAll?'devFewer':'devMore').replace('{n}',hidden);more.setAttribute('aria-expanded',fleetAll);
 let on=0,off=0;
 for(const r of rows){on+=r.online;off+=r.offline;if(seenRows.length&&!fleetAll&&!r.online&&!r.offline)continue;
  const li=document.createElement('li'),dot=document.createElement('span'),name=document.createElement('span'),count=document.createElement('span');
  const state=r.online?'on':r.offline?'off':'none';li.className=state;
  dot.className='dot '+state;dot.setAttribute('role','img');dot.setAttribute('aria-label',T({on:'devOnline',off:'devOffline',none:'devNone'}[state]));
  name.className='name';name.textContent=r.name;count.className='count';
  // The exact models online, for the firmware that serves several (M5Stack).
  const plain=x=>String(x).toLowerCase().replace(/[^a-z0-9]/g,'');
  const models=(summary?.models||[]).filter(m=>m.board===r.env&&m.model&&plain(m.model)!==plain(r.env)&&plain(m.model)!==plain(r.name))
   .map(m=>m.model+(m.miners>1?' ×'+m.miners:''));
  if(models.length){const sub=document.createElement('span');sub.className='models';sub.textContent=models.join(', ');name.appendChild(sub)}
  const parts=[];if(r.online)parts.push('<b>'+r.online+'</b> '+T('devOnline'));if(r.offline)parts.push('<b>'+r.offline+'</b> '+T('devOffline'));
  count.innerHTML=parts.length?parts.join(' · '):T('devNone');
  li.append(dot,name,count);ul.appendChild(li)}
 document.getElementById('st-on').textContent=summary?on:'–';document.getElementById('st-off').textContent=summary?off:'–'}

// The miners online now, by board and by firmware version.
function table(id,rows,head,label,website){const el=document.getElementById(id);el.innerHTML='';
 if(!rows.length){const td=document.createElement('td');td.className='empty';td.textContent=T('noneNow');el.insertRow().appendChild(td);return}
 const h=el.createTHead().insertRow();
 for(const [k,cls] of [[head,''],['colMiners','num'],['colHashrate','num']]){const th=document.createElement('th');th.textContent=T(k);th.className=cls;h.appendChild(th)}
 const body=el.createTBody();
 for(const r of rows){const tr=body.insertRow();
  for(const [v,cls] of [[label(r.name),''],[r.miners.toLocaleString(lang=='fr'?'fr-FR':'en-US'),'num'],[rate(r.hashrate),'num']]){const td=tr.insertCell();td.textContent=v;td.className=cls;
   const url=!cls&&website?.(r.name);if(url){const a=document.createElement('a');a.href=url;a.target='_blank';a.rel='noopener noreferrer';a.textContent=v;
    const address=document.createElement('small');address.className='pool-site';address.textContent=url;a.appendChild(address);td.replaceChildren(a)}}}}
function breakdowns(){
 const family=env=>{const n=SHORT[env];return n?(lang=='fr'&&n[1]||n[0]):env};
 // By exact model when the miners tell it (from 1.9.4), by firmware otherwise.
 // Rows with the same name (a model that repeats its board's name) are one.
 const rows=new Map();
 for(const m of summary?.models||(summary?.boards||[]).map(b=>({board:b.name,model:null,miners:b.miners,hashrate:b.hashrate}))){
  const label=m.model||family(m.board),key=label.toLowerCase().replace(/[^a-z0-9]/g,''),row=rows.get(key)||{name:label,miners:0,hashrate:0};
  row.miners+=m.miners;row.hashrate+=m.hashrate;rows.set(key,row)}
 table('st-boards',[...rows.values()].sort((a,b)=>b.hashrate-a.hashrate),'colBoard',n=>n);
 table('st-versions',summary?.versions||[],'colVersion',v=>v);
 table('st-coins',summary?.coins||[],'colCoin',v=>v);
 const pools=summary?.pools??(summary?.miners?[{name:'unknown',miners:summary.miners,hashrate:summary.hashrate}]:[]);
 table('st-pools',pools,'colPool',key=>key==='unknown'?T('poolUnknown'):Object.hasOwn(poolNames,key)?poolNames[key]:T('poolOther'),
  key=>Object.hasOwn(poolNames,key)&&Object.hasOwn(poolLinks,key)?poolLinks[key]:null)}

// A small line of points (the total hashrate) for the cards of the home page.
function sparkline(points){const values=points.map(p=>p.hashrate).filter(v=>v>=0);if(values.length<2)return'';
 const W=120,H=36,max=Math.max(...values),min=Math.min(...values),span=max-min||1;
 const d=values.map((v,i)=>(i?'L':'M')+(W*i/(values.length-1)).toFixed(1)+' '+(H-3-(H-6)*(v-min)/span).toFixed(1)).join(' ');
 return '<path class="spark-area" d="'+d+' L'+W+' '+H+' L0 '+H+' Z"/><path class="spark-line" d="'+d+'"/>'}
// Home: the live fleet by board, as exchange-style cards and a market table,
// both filtered by board family (tabs), the table also by name (search).
const FAMILY={'m5stickc-plus2':'esp32','m5stack-esp32':'esp32','lilygo-t-display':'esp32','esp32-2432s028r':'esp32','esp32-headless':'esp32',
 'm5stack-s3':'s3','lilygo-t-display-s3':'s3','esp32s3-headless':'s3','esp32s2-headless':'s2c3','esp32c3-headless':'s2c3','bitaxe-gamma-601':'asic','nerdnos':'asic'};
const familyOf=env=>FAMILY[env]||(/gamma|bitaxe|nerdnos/.test(env)?'asic':/s3/.test(env)?'s3':/s2|c3/.test(env)?'s2c3':'esp32');
const homeFilter={cards:'all',table:'all',query:''};
function homeMarket(){const table=document.getElementById('home-market');if(!table)return;
 const locale=lang=='fr'?'fr-FR':'en-US',set=(id,v)=>{const e=document.getElementById(id);if(e)e.textContent=v};
 set('hm-total',summary?rate(summary.hashrate||0):'–');set('hm-miners',summary?(summary.miners||0).toLocaleString(locale):'–');
 const spark=document.getElementById('hm-spark');if(spark)spark.innerHTML=sparkline(summary?.recent||[]);
 const name=env=>{const n=SHORT[env];return n?(lang=='fr'&&n[1]||n[0]):env};
 const famLabel=f=>T({esp32:'famEsp32',s3:'famS3',s2c3:'famS2c3',asic:'famAsic'}[f]);
 const minerTotal=summary?.miners||0,pct=x=>(x*100).toLocaleString(locale,{maximumFractionDigits:x<.1?2:1})+' %';
 const all=(summary?.boards||[]).filter(b=>b.miners>0).map(b=>({...b,family:familyOf(b.name),label:name(b.name)})).sort((a,b)=>b.hashrate-a.hashrate);
 const icon=r=>{const c=document.createElement('span');c.className='chip-icon';c.dataset.family=r.family;c.setAttribute('aria-hidden','true');
  c.textContent=r.label.replace(/[^A-Za-z0-9]/g,'').slice(0,2).toUpperCase();return c};
 // Cards: the four busiest boards of the chosen family.
 const cards=document.getElementById('hm-cards');
 if(cards){cards.innerHTML='';const list=all.filter(r=>homeFilter.cards==='all'||r.family===homeFilter.cards).slice(0,4);
  if(!list.length){const p=document.createElement('p');p.className='ticker-empty';p.textContent=!summary?'–':all.length?T('noMatch'):T('noneNow');cards.appendChild(p)}
  list.forEach((r,i)=>{const a=document.createElement('a'),head=document.createElement('span'),title=document.createElement('span'),tag=document.createElement('span');
   a.className='ticker';a.href=BOARDS.some(b=>b.env===r.name)?'flash.html?board='+encodeURIComponent(r.name):'stats.html';
   head.className='ticker-head';title.className='ticker-name';title.textContent=r.label;tag.className='ticker-tag';tag.textContent=famLabel(r.family);
   head.append(icon(r),title,tag);
   const value=document.createElement('strong');value.className='ticker-value';value.textContent=rate(r.hashrate);
   const foot=document.createElement('span'),count=document.createElement('span'),badge=document.createElement('span');foot.className='ticker-foot';
   count.textContent=T(r.miners===1?'cardMiner':'cardMiners').replace('{n}',r.miners.toLocaleString(locale));badge.className='chg up';badge.textContent=pct(minerTotal?r.miners/minerTotal:0);
   foot.append(count,badge);a.append(head,value,foot);cards.appendChild(a)})}
 // Table: every board online in the chosen family that matches the search.
 const q=homeFilter.query.trim().toLowerCase();
 const rows=all.filter(r=>(homeFilter.table==='all'||r.family===homeFilter.table)&&(!q||r.label.toLowerCase().includes(q)||r.name.includes(q)));
 const body=table.tBodies[0];body.innerHTML='';
 if(!rows.length){const td=body.insertRow().insertCell();td.colSpan=6;td.className='empty';td.textContent=!summary?'–':all.length&&(q||homeFilter.table!=='all')?T('noMatch'):T('noneNow');return}
 rows.forEach((r,i)=>{const tr=body.insertRow(),share=minerTotal?r.miners/minerTotal:0;
  const rank=tr.insertCell();rank.className='rank';rank.textContent=i+1;
  const cell=tr.insertCell(),wrap=document.createElement('span'),label=document.createElement('span'),tag=document.createElement('small');
  label.textContent=r.label;tag.textContent=famLabel(r.family);wrap.className='board';wrap.append(icon(r),label,tag);cell.appendChild(wrap);
  const hash=tr.insertCell();hash.className='num strong';hash.textContent=rate(r.hashrate);
  const miners=tr.insertCell();miners.className='num col-miners';miners.textContent=r.miners.toLocaleString(locale);
  const part=tr.insertCell(),bar=document.createElement('span'),fill=document.createElement('i'),value=document.createElement('span');
  const shareBox=document.createElement('div');part.className='col-share';shareBox.className='share';bar.className='bar';fill.style.width=Math.max(2,share*100).toFixed(1)+'%';bar.appendChild(fill);value.textContent=pct(share);shareBox.append(bar,value);part.appendChild(shareBox);
  const act=tr.insertCell();act.className='act';
  if(BOARDS.some(b=>b.env===r.name)){const a=document.createElement('a');a.className='btn-sm';a.href='flash.html?board='+encodeURIComponent(r.name);a.textContent=T('navInstall');act.appendChild(a)}})}
document.querySelectorAll('.family-tabs').forEach(group=>group.addEventListener('click',e=>{const b=e.target.closest('button[data-family]');if(!b)return;
 homeFilter[group.dataset.target]=b.dataset.family;group.querySelectorAll('button').forEach(o=>o.setAttribute('aria-pressed',o===b));homeMarket()}));
if(document.getElementById('hm-search'))document.getElementById('hm-search').addEventListener('input',e=>{homeFilter.query=e.target.value;homeMarket()});
function stats(){homeMarket();statsConnection();document.dispatchEvent(new Event('bdl-public-stats'));if(!document.getElementById('stats'))return;const set=(id,v)=>document.getElementById(id).textContent=v;fleet();breakdowns();
 if(!summary){chart();return}
 set('st-hashrate',rate(summary.hashrate||0));set('st-miners',(summary.miners||0).toLocaleString(lang=='fr'?'fr-FR':'en-US'));
 set('st-average',summary.miners?rate(summary.hashrate/summary.miners):'–');set('st-record',summary.record?rate(summary.record.hashrate):'–');
 chart()}
async function refresh(){
 if(statsLoading||(!document.getElementById('stats')&&!document.getElementById('home-market')))return;
 statsLoading=true;
 try{
  const r=await fetch(statsEndpoint,{cache:'no-store',credentials:'omit',signal:AbortSignal.timeout(10000)});if(!r.ok)throw Error('Statistics unavailable');
  const next=await r.json();
  if(!next||!Number.isFinite(next.at)||!Number.isFinite(next.hashrate)||next.hashrate<0||!Number.isInteger(next.miners)||next.miners<0||
   ['boards','versions','coins','pools','fleet','recent','history'].some(key=>next[key]!=null&&(!Array.isArray(next[key])||next[key].some(row=>!row||typeof row!=='object'))))throw Error('Invalid statistics');
  summary=next;statsFailed=false;statsHover=null;fetchedAt=Date.now();stats();
 }catch(e){statsFailed=true;statsConnection()}
 finally{statsLoading=false}
}

function apply(){document.documentElement.lang=lang;
 document.querySelectorAll('[data-t]').forEach(e=>e.textContent=T(e.dataset.t));
 document.querySelectorAll('[data-h]').forEach(e=>e.innerHTML=T(e.dataset.h));
 document.querySelectorAll('[data-al]').forEach(e=>e.setAttribute('aria-label',T(e.dataset.al)));
 document.querySelectorAll('[data-alt]').forEach(e=>e.alt=T(e.dataset.alt));
 document.querySelectorAll('[data-ph]').forEach(e=>e.placeholder=T(e.dataset.ph));
 chatRender();chatMe();chatMembers();
 document.getElementById('lang').textContent=lang=='fr'?'English':'Français';
 showScreen(screen);boards();stats();document.dispatchEvent(new Event('bdl-language-change'))}
// The phone menu: opens under the bar, closes on a link, Escape or a tap elsewhere.
{const menu=document.getElementById('menu'),links=document.getElementById('site-links'),more=document.getElementById('nav-more');
 const set=open=>{links.classList.toggle('open',open);menu.setAttribute('aria-expanded',open);menu.textContent=open?'✕':'☰'};
 menu.onclick=e=>{e.stopPropagation();set(!links.classList.contains('open'))};
 links.addEventListener('click',e=>{if(e.target.closest('a'))set(false)});
 document.addEventListener('click',e=>{if(!e.target.closest('nav'))set(false);if(more&&!more.contains(e.target))more.open=false});
 document.addEventListener('keydown',e=>{if(e.key!=='Escape')return;if(more?.open){more.open=false;more.querySelector('summary').focus();return}if(links.classList.contains('open')){set(false);menu.focus()}})}
if(document.getElementById('st-more'))document.getElementById('st-more').onclick=()=>{fleetAll=!fleetAll;fleet()};
{const theme=document.getElementById('theme'),root=document.documentElement;
 const update=()=>{theme.setAttribute('aria-pressed',root.dataset.theme==='dark');document.querySelector('meta[name="theme-color"]')?.setAttribute('content',getComputedStyle(root).getPropertyValue('--bg').trim())};update();
 theme.onclick=()=>{const light=root.dataset.theme==='dark';root.dataset.theme=light?'light':'dark';update();chart(false);
  try{localStorage.setItem('theme',light?'light':'dark')}catch(e){}}}
document.getElementById('lang').onclick=()=>{lang=lang=='fr'?'en':'fr';try{localStorage.setItem('lang',lang)}catch(e){}apply()};

const sel=document.getElementById('board'),btn=document.getElementById('installer');
if(sel&&btn){
for(const b of BOARDS){const o=document.createElement('option');o.value=b.env;o.textContent=b.name;sel.appendChild(o)}
// The chosen board is remembered: a reload or the next visit keeps it. The button
// gets it as an attribute too: esp-web-tools may load after this script, and its
// element then starts with an empty manifest property.
const requested=new URLSearchParams(location.search).get('board');
if(BOARDS.some(b=>b.env===requested))sel.value=requested;
else try{const saved=localStorage.getItem('board');if(BOARDS.some(b=>b.env===saved))sel.value=saved}catch(e){}
sel.onchange=()=>{const m='manifest-'+sel.value+'.json';btn.setAttribute('manifest',m);btn.manifest=m;
 try{localStorage.setItem('board',sel.value)}catch(e){}};sel.onchange();
}
// --- Live chat (BDLMiner-chat Worker) ---
const CHAT_HOST='bdlminer-chat.bdlminer-bot.workers.dev';
const CHAT='wss://'+CHAT_HOST+'/ws';
const store={get(k){try{return localStorage.getItem(k)}catch(e){return null}},set(k,v){try{v==null?localStorage.removeItem(k):localStorage.setItem(k,v)}catch(e){}}};
const chat={ws:null,messages:[],online:0,unread:0,open:false,admin:false,retry:1000,connected:false,me:null,pending:null,attach:null,rec:null,settings:false,busy:false};
const $c=id=>document.getElementById(id);
// The browser's secret code: the server keeps the name registered with it (its hash).
function chatCode(){let c=store.get('chatCode');
 if(!c||!/^[A-Za-z0-9_-]{16,128}$/.test(c)){const b=new Uint8Array(24);crypto.getRandomValues(b);
  c=btoa(String.fromCharCode(...b)).replace(/\+/g,'-').replace(/\//g,'_').replace(/=+$/,'');store.set('chatCode',c)}return c}
function chatDate(t){return new Date(t).toLocaleDateString(lang=='fr'?'fr-FR':'en-US',{day:'numeric',month:'short',year:'numeric'})}
function chatTime(t){return new Date(t).toLocaleTimeString(lang=='fr'?'fr-FR':'en-US',{hour:'2-digit',minute:'2-digit'})}
function chatNote(text){$c('chat-note').textContent=text||''}
function chatError(key,until){chatNote((T('chatErrors')[key]||key).replace('{d}',until?chatDate(until):''))}
// Links become clickable (the server only lets the project's own through); the rest stays text.
function chatText(el,text){for(const part of text.split(/(https?:\/\/[^\s]+)/g)){
 if(/^https?:\/\//.test(part)){const a=document.createElement('a');a.href=part;a.textContent=part;a.target='_blank';a.rel='noopener noreferrer nofollow ugc';el.appendChild(a)}
 else el.appendChild(document.createTextNode(part))}}
// Voice messages: a round orange play button, the message's own waveform
// (orange as it plays) and its time; one plays at a time.
const VOICE_BARS=36,PLAY='<svg viewBox="0 0 24 24" fill="currentColor"><path d="M7 4.5v15l13-7.5z"/></svg>',PAUSE='<svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 4h4v16H6zm8 0h4v16h-4z"/></svg>';
let voicePlaying=null;
function voiceClock(s){s=Math.max(0,Math.round(s||0));return Math.floor(s/60)+':'+String(s%60).padStart(2,'0')}
// The peaks of the decoded sound; made-up but steady bars when it cannot be decoded.
async function voicePeaks(source,seed){let ctx;try{const buf=source instanceof Blob?await source.arrayBuffer():await (await fetch(source)).arrayBuffer();
  ctx=new (window.AudioContext||window.webkitAudioContext)();const audio=await ctx.decodeAudioData(buf);
  const data=audio.getChannelData(0),step=Math.floor(data.length/VOICE_BARS)||1,peaks=[];
  for(let i=0;i<VOICE_BARS;i++){let m=0;for(let j=i*step;j<(i+1)*step&&j<data.length;j+=16)m=Math.max(m,Math.abs(data[j]));peaks.push(m)}
  const top=Math.max(...peaks)||1;return peaks.map(p=>.15+.85*p/top)}
 catch(e){let h=[...String(seed)].reduce((a,c)=>a*31+c.charCodeAt(0)>>>0,7);return Array.from({length:VOICE_BARS},()=>{h=(h*1103515245+12345)>>>0;return .25+.75*((h>>>16)%100)/100})}
 finally{if(ctx)try{await ctx.close()}catch(e){}}}
function voicePlayer(src,seed){const box=document.createElement('div');box.className='voice';
 const btn=document.createElement('button');btn.type='button';btn.className='vplay';btn.innerHTML=PLAY;btn.setAttribute('aria-label',T('chatPlay'));
 const wave=document.createElement('div');wave.className='vwave';wave.tabIndex=0;wave.setAttribute('role','slider');wave.setAttribute('aria-label',T('chatPosition'));
 const time=document.createElement('span');time.className='vtime';time.textContent='0:00';
 const audio=new Audio();audio.preload='metadata';audio.src=src instanceof Blob?URL.createObjectURL(src):src;
 const bars=[];for(let i=0;i<VOICE_BARS;i++){const b=document.createElement('i');b.style.height='30%';bars.push(b);wave.appendChild(b)}
 voicePeaks(src,seed).then(p=>p.forEach((h,i)=>bars[i].style.height=Math.round(h*100)+'%'));
 // MediaRecorder files may not know their length until played through.
 const length=()=>isFinite(audio.duration)&&audio.duration>0?audio.duration:0;
 const draw=()=>{const d=length(),f=d?audio.currentTime/d:0;bars.forEach((b,i)=>b.classList.toggle('on',(i+.5)/VOICE_BARS<=f));
  time.textContent=audio.paused&&!audio.currentTime?voiceClock(d):voiceClock(audio.currentTime);
  wave.setAttribute('aria-valuemin','0');wave.setAttribute('aria-valuemax',String(Math.round(d)));wave.setAttribute('aria-valuenow',String(Math.round(audio.currentTime)));
  wave.setAttribute('aria-valuetext',voiceClock(audio.currentTime)+' / '+voiceClock(d))};
 const state=()=>{const on=!audio.paused;box.classList.toggle('playing',on);btn.innerHTML=on?PAUSE:PLAY;btn.setAttribute('aria-label',T(on?'chatPause':'chatPlay'))};
 audio.addEventListener('loadedmetadata',()=>{if(audio.duration===Infinity){audio.currentTime=1e9;audio.addEventListener('durationchange',()=>{audio.currentTime=0;draw()},{once:true})}draw()});
 audio.addEventListener('timeupdate',draw);audio.addEventListener('play',state);audio.addEventListener('pause',state);
 audio.addEventListener('ended',()=>{audio.currentTime=0;state();draw()});
 btn.onclick=()=>{if(audio.paused){if(voicePlaying&&voicePlaying!==audio)voicePlaying.pause();voicePlaying=audio;audio.play()}else audio.pause()};
 const seek=t=>{const d=length();if(d){audio.currentTime=Math.min(d,Math.max(0,t));draw()}};
 wave.onclick=e=>{const r=wave.getBoundingClientRect();seek((e.clientX-r.left)/r.width*length())};
 wave.onkeydown=e=>{if(e.key==='ArrowRight'){e.preventDefault();seek(audio.currentTime+2)}else if(e.key==='ArrowLeft'){e.preventDefault();seek(audio.currentTime-2)}
  else if(e.key===' '||e.key==='Enter'){e.preventDefault();btn.click()}};
 box.append(btn,wave,time);return box}
function chatMedia(m){const box=document.createElement('div');box.className='media';const src='https://'+CHAT_HOST+'/media/'+m.id;
 if(m.kind==='image'){const a=document.createElement('a');a.href=src;a.target='_blank';a.rel='noopener';
  const img=document.createElement('img');img.src=src;img.loading='lazy';img.alt=T('chatPhoto');a.appendChild(img);box.appendChild(a)}
 else if(m.kind==='video'){const v=document.createElement('video');v.src=src;v.controls=true;v.preload='metadata';v.playsInline=true;box.appendChild(v)}
 else box.appendChild(voicePlayer(src,m.id));
 return box}
// A button that asks once more before acting.
function chatConfirm(label,action){const b=document.createElement('button');b.type='button';b.className='danger';b.textContent=T(label);let armed=0;
 b.onclick=()=>{if(armed){clearTimeout(armed);action();return}b.textContent=T('chatSure');armed=setTimeout(()=>{armed=0;b.textContent=T(label)},3000)};return b}
function chatItem(m){const li=document.createElement('li');li.dataset.id=m.id;
 const meta=document.createElement('div');meta.className='meta';
 const who=document.createElement('span');who.className='who';who.textContent=m.name;
 who.style.color=m.team?'var(--accent)':'hsl('+[...m.name].reduce((h,c)=>h*31+c.charCodeAt(0)>>>0,7)%360+' 70% 70%)';meta.appendChild(who);
 if(m.team){const b=document.createElement('span');b.className='team';b.textContent=T('chatTeam');meta.appendChild(b)}
 const time=document.createElement('time');time.dateTime=new Date(m.t).toISOString();time.textContent=chatTime(m.t);meta.appendChild(time);
 if(m.edited){const e=document.createElement('span');e.className='edited';e.textContent='('+T('chatEdited')+')';meta.appendChild(e)}
 if(m.own||chat.admin){const acts=document.createElement('span');acts.className='acts';
  if(m.own){const e=document.createElement('button');e.type='button';e.textContent=T('chatEdit');e.onclick=()=>chatEditing(li,m);acts.appendChild(e)}
  acts.appendChild(chatConfirm('chatDelete',()=>chatSend({type:'delete',id:m.id})));
  if(chat.admin&&!m.own)acts.appendChild(chatConfirm('chatBan',()=>chatSend({type:'ban',id:m.id})));
  meta.appendChild(acts)}
 li.appendChild(meta);
 if(m.text){const text=document.createElement('div');text.className='text';chatText(text,m.text);li.appendChild(text)}
 if(m.media)li.appendChild(chatMedia(m.media));
 return li}
function chatEditing(li,m){const old=li.querySelector('.text');const box=document.createElement('div');box.className='editing';
 const ta=document.createElement('textarea');ta.maxLength=500;ta.value=m.text;ta.rows=2;
 const row=document.createElement('div');row.className='row';
 const save=document.createElement('button');save.type='button';save.className='btn-s';save.textContent=T('chatSaveEdit');
 const cancel=document.createElement('button');cancel.type='button';cancel.className='btn-s ghost';cancel.textContent=T('chatCancel');
 save.onclick=()=>chatSend({type:'edit',id:m.id,text:ta.value});cancel.onclick=()=>chatRender();
 ta.addEventListener('keydown',e=>{if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();save.click()}if(e.key==='Escape'){e.stopPropagation();cancel.click()}});
 row.append(cancel,save);box.append(ta,row);if(old)old.replaceWith(box);else li.insertBefore(box,li.querySelector('.media'));ta.focus()}
function chatRender(){const log=$c('chat-log');if(!log)return;const near=log.scrollHeight-log.scrollTop-log.clientHeight<80;log.innerHTML='';
 if(!chat.messages.length){const li=document.createElement('li');li.className='empty';li.textContent=chat.connected?T('chatEmpty'):T('chatOffline');log.appendChild(li)}
 for(const m of chat.messages)log.appendChild(chatItem(m));
 if(near)log.scrollTop=log.scrollHeight;$c('chat-online').textContent=chat.connected?chat.online:'–';chatBadge()}
function chatBadge(){const b=$c('chat-unread');b.hidden=!chat.unread||chat.open;b.textContent=chat.unread>9?'9+':chat.unread;
 $c('chat-bubble').setAttribute('aria-label',T('chatOpen')+(chat.unread&&!chat.open?' ('+chat.unread+' '+T('chatUnread')+')':''))}
// Messages read: the newest id seen with the chat open, kept by the browser.
// Unread are the newer ones from others; a first visit counts the last 24 hours.
function chatSeen(){const ids=chat.messages.map(m=>m.id);if(ids.length)store.set('chatSeen',String(Math.max(...ids,+store.get('chatSeen')||0)))}
function chatUnread(){const seen=+store.get('chatSeen')||0,day=Date.now()-864e5;
 chat.unread=chat.open?0:chat.messages.filter(m=>!m.own&&(seen?m.id>seen:m.t>day)).length;chatBadge()}
function chatSend(value){if(chat.ws&&chat.ws.readyState===1){chat.ws.send(JSON.stringify(value));return true}chatNote(T('chatOffline'));return false}
// Settings: the name, locked once registered until it may change again.
function chatMe(){const input=$c('chat-name'),me=chat.me;if(!input)return;
 const named=!!(me&&me.name),free=!named||chat.admin||Date.now()>=me.changeAt;
 if(named&&document.activeElement!==input)input.value=me.name;else if(!named&&!input.value)input.value=store.get('chatName')||'';
 input.readOnly=!free;$c('chat-save').hidden=!free;
 $c('chat-when').textContent=!named?T('chatNameHint'):free?T('chatNow'):T('chatWhen').replace('{d}',chatDate(me.changeAt))}
// The panel shows one view: the messages ('log'), the settings or the members.
function chatView(v){if(v!=='settings'&&window.chatQr)chatQr(false);chat.view=v;chat.settings=v==='settings';
 $c('chat-settings').hidden=v!=='settings';$c('chat-members').hidden=v!=='members';
 for(const el of [$c('chat-log'),$c('chat-form'),$c('chat').querySelector('.chat-rules')])el.hidden=v!=='log';
 $c('chat-gear').setAttribute('aria-pressed',v==='settings');$c('chat-people').setAttribute('aria-pressed',v==='members')}
function chatShowSettings(on){chatView(on?'settings':'log');chatMe();if(chatTouch)return;if(on)$c('chat-name').focus();else $c('chat-text').focus()}
function chatShowMembers(on){chatView(on?'members':'log');chatMembers();if(!chatTouch)(on?$c('chat-members-back'):$c('chat-text')).focus()}
// Members online: a green dot each, the team's badge, "(you)", and the visitors without a name.
function chatMembers(){const ul=$c('chat-member-list');if(!ul)return;ul.innerHTML='';const p=chat.presence||{members:[],guests:0};
 for(const m of p.members){const li=document.createElement('li');const dot=document.createElement('span');dot.className='dot on';dot.setAttribute('aria-hidden','true');
  const name=document.createElement('span');name.textContent=m.name;li.append(dot,name);
  if(m.team){const b=document.createElement('span');b.className='team';b.textContent=T('chatTeam');li.appendChild(b)}
  if(chat.me&&chat.me.name===m.name){const me=document.createElement('span');me.className='me';me.textContent=T('chatYou');li.appendChild(me)}
  ul.appendChild(li)}
 $c('chat-guests').textContent=(!p.members.length?T('chatNobody')+' ':'')+(p.guests?T('chatGuests').replace('{n}',p.guests):'')}
function chatAttachClear(){if(chat.attach?.url)URL.revokeObjectURL(chat.attach.url);chat.attach=null;$c('chat-attach').hidden=true}
function chatAttachShow(a){chatAttachClear();chat.attach=a;const box=$c('chat-attach');box.querySelectorAll('img,video,audio').forEach(e=>e.remove());
 box.querySelectorAll('.voice').forEach(e=>e.remove());a.url=URL.createObjectURL(a.blob);
 if(a.kind==='audio'){box.insertBefore(voicePlayer(a.blob,'preview'),box.firstChild);$c('chat-attach-name').textContent=''}
 else{const el=document.createElement(a.kind==='image'?'img':'video');el.src=a.url;if(a.kind==='video'){el.muted=true;el.playsInline=true}else el.alt='';
  box.insertBefore(el,box.firstChild);$c('chat-attach-name').textContent=a.label}
 box.hidden=false;$c('chat-text').focus()}
// Photos are made smaller in the browser (1600 px, about 0.85 quality); a GIF stays as it is, to keep moving.
async function chatShrink(file){if(file.type==='image/gif'||!window.createImageBitmap)return file;
 try{const bmp=await createImageBitmap(file);const scale=Math.min(1,1600/Math.max(bmp.width,bmp.height));
  const c=document.createElement('canvas');c.width=Math.round(bmp.width*scale);c.height=Math.round(bmp.height*scale);c.getContext('2d').drawImage(bmp,0,0,c.width,c.height);
  const blob=await new Promise(r=>c.toBlob(r,'image/jpeg',0.85));return blob&&blob.size<file.size?blob:file}catch(e){return file}}
async function chatUpload(a){const r=await fetch('https://'+CHAT_HOST+'/api/media?kind='+a.kind,{method:'POST',body:a.blob,
  headers:{'X-Chat-Code':chatCode(),'Content-Type':a.blob.type||'application/octet-stream'}});
 let j={};try{j=await r.json()}catch(e){}if(!r.ok||!j.media)throw Error(j.error||'upload');return j.media}
// Voice messages: the microphone until 🎤 again, 60 seconds at most.
async function chatRecord(){const btn=$c('chat-rec');
 if(chat.rec){chat.rec.stop();return}
 let stream;try{stream=await navigator.mediaDevices.getUserMedia({audio:true})}catch(e){chatNote(T('chatNoMic'));return}
 const type=['audio/webm;codecs=opus','audio/ogg;codecs=opus','audio/mp4'].find(t=>window.MediaRecorder&&MediaRecorder.isTypeSupported(t));
 let rec;try{rec=new MediaRecorder(stream,type?{mimeType:type,audioBitsPerSecond:32000}:{})}catch(e){stream.getTracks().forEach(t=>t.stop());chatNote(T('chatNoMic'));return}
 const parts=[],start=Date.now();chat.rec=rec;btn.setAttribute('aria-pressed','true');
 const tick=setInterval(()=>{const s=Math.round((Date.now()-start)/1000);chatNote(T('chatRecording').replace('{s}',s));if(s>=60)rec.stop()},500);
 rec.ondataavailable=e=>{if(e.data.size)parts.push(e.data)};
 rec.onstop=()=>{clearInterval(tick);stream.getTracks().forEach(t=>t.stop());chat.rec=null;btn.setAttribute('aria-pressed','false');chatNote('');
  const blob=new Blob(parts,{type:(rec.mimeType||'audio/webm').split(';')[0]});
  if(blob.size)chatAttachShow({kind:'audio',blob,label:T('chatVoice').replace('{s}',Math.max(1,Math.round((Date.now()-start)/1000)))})};
 rec.start()}
function chatConnect(){let ws;try{ws=new WebSocket(CHAT)}catch(e){return}chat.ws=ws;
 ws.onopen=()=>{chat.connected=true;chat.retry=1000;chatNote('');const key=store.get('chatKey');if(key)ws.send(JSON.stringify({type:'auth',key}));
  ws.send(JSON.stringify({type:'identify',code:chatCode()}))};
 ws.onmessage=e=>{let m;try{m=JSON.parse(e.data)}catch(x){return}
  if(m.type==='hello'){chat.messages=m.messages;chat.online=m.online;chatRender();if(chat.open)chatSeen();chatUnread()}
  else if(m.type==='mine'){for(const x of chat.messages)if(m.ids.includes(x.id))x.own=true;chatRender();chatUnread()}
  else if(m.type==='online'){chat.online=m.count;$c('chat-online').textContent=m.count}
  else if(m.type==='presence'){chat.presence=m;chatMembers()}
  else if(m.type==='message'){if(chat.messages.some(x=>x.id===m.message.id))return;chat.messages.push(m.message);if(chat.messages.length>100)chat.messages.shift();
   if(!chat.open&&!m.message.own)chat.unread++;else if(chat.open)chatSeen();const log=$c('chat-log');const near=log.scrollHeight-log.scrollTop-log.clientHeight<80;
   if(log.querySelector('.empty'))chatRender();else{log.appendChild(chatItem(m.message));
    while(log.children.length>100){const old=log.firstElementChild;
     if(old.querySelector('.voice.playing')&&voicePlaying){voicePlaying.pause();voicePlaying=null}old.remove()}
    if(near||m.message.own)log.scrollTop=log.scrollHeight}chatBadge()}
  else if(m.type==='edited'){const i=chat.messages.findIndex(x=>x.id===m.message.id);if(i>=0){chat.messages[i]=m.message;chatRender()}}
  else if(m.type==='removed'){chat.messages=chat.messages.filter(x=>!m.ids.includes(x.id));chatRender()}
  else if(m.type==='you'){const was=chat.me&&chat.me.name;chat.me=m;if(m.name)store.set('chatName',m.name);chatMe();
   if(m.name&&chat.settings&&was!==m.name){chatNote(T('chatSaved'));chatShowSettings(false)}
   if(chat.pending&&m.name){const p=chat.pending;chat.pending=null;chatSend(p)}
   if(chat.imported){chat.imported=false;if(!chat.open)chatToggle(true);if(m.name){chatShowSettings(false);chatNote(T('chatCodeSet'))}}}
  else if(m.type==='error'){chat.pending=null;chatError(m.error,m.until)}
  else if(m.type==='auth'){chat.admin=m.ok;if(!m.ok&&m.error!=='slow')store.set('chatKey',null);chatNote(T(m.ok?'chatAdminOn':m.error==='slow'?'chatAdminSlow':'chatAdminOff'));chatRender();chatMe()}};
 ws.onclose=()=>{chat.connected=false;chat.ws=null;chatRender();setTimeout(chatConnect,chat.retry);chat.retry=Math.min(chat.retry*2,30000)}}
// On a phone, focusing the message field would open the keyboard at once and
// push the chat's header, with its close button, off the screen.
const chatTouch=matchMedia('(pointer: coarse)').matches;
// Keeps the sheet within the visible part of a phone's screen when the keyboard
// or the browser's bars take some of it.
function chatFit(){const el=$c('chat'),vv=window.visualViewport;
 if(!vv||!chat.open||innerWidth>520){el.style.height=el.style.bottom='';return}
 el.style.bottom=Math.max(0,innerHeight-vv.height-vv.offsetTop)+'px';el.style.height=Math.round(Math.min(vv.height*.92,innerHeight*.85))+'px'}
if(window.visualViewport){visualViewport.addEventListener('resize',chatFit);visualViewport.addEventListener('scroll',chatFit)}
addEventListener('resize',chatFit);
function chatToggle(open){chat.open=open;$c('chat').hidden=!open;$c('chat-bubble').setAttribute('aria-expanded',open);chatFit();
 if(open){chat.unread=0;chatSeen();chatBadge();const log=$c('chat-log');log.scrollTop=log.scrollHeight;
  if(!(chat.me&&chat.me.name))chatShowSettings(true);else if(!chatTouch)$c('chat-text').focus();else $c('chat-close').focus({preventScroll:true})}
 else $c('chat-bubble').focus()}
$c('chat-bubble').onclick=()=>chatToggle(!chat.open);
$c('chat-close').onclick=()=>chatToggle(false);
$c('chat-gear').onclick=()=>chatShowSettings(!chat.settings);
$c('chat-back').onclick=()=>chatShowSettings(false);
$c('chat-people').onclick=()=>chatShowMembers(chat.view!=='members');
$c('chat-members-back').onclick=()=>chatShowMembers(false);
$c('chat').addEventListener('keydown',e=>{if(e.key==='Escape'){if(chat.view&&chat.view!=='log')chatShowSettings(false);else chatToggle(false)}});
const chatSaveName=()=>{const n=$c('chat-name').value.trim();if(!n){chatError('name');return}
 // The team can sign in from here too, before taking a reserved name.
 const login=n.match(/^\/login\s+(\S+)$/);if(login){store.set('chatKey',login[1]);chatSend({type:'auth',key:login[1]});$c('chat-name').value='';return}
 if(chat.me&&n===chat.me.name){chatShowSettings(false);return}chatSend({type:'register',name:n})};
$c('chat-save').onclick=chatSaveName;
$c('chat-name').addEventListener('keydown',e=>{if(e.key==='Enter'){e.preventDefault();if(!e.target.readOnly)chatSaveName()}});
// Another device: a QR code (or link) to this page whose fragment carries the
// secret code. A fragment is never sent to any server; the page takes it out
// of the address at once.
const chatLinkTo=()=>location.origin+location.pathname+'#chat-code='+chatCode();
let qrHide=0;
function chatQrLib(){if(window.qrcode)return Promise.resolve();return new Promise((ok,fail)=>{const s=document.createElement('script');
 s.src='https://unpkg.com/qrcode-generator@1.4.4/qrcode.js';s.onload=ok;s.onerror=fail;document.head.appendChild(s)})}
async function chatQr(show){const box=$c('chat-qr'),btn=$c('chat-qr-show');clearTimeout(qrHide);
 if(!show){box.hidden=true;$c('chat-qr-img').innerHTML='';btn.textContent=T('chatQrShow');btn.setAttribute('aria-expanded','false');return}
 try{await chatQrLib()}catch(e){chatNote(T('chatQrFailed'));return}
 const qr=qrcode(0,'M');qr.addData(chatLinkTo());qr.make();const n=qr.getModuleCount(),q=4;let d='';
 for(let y=0;y<n;y++)for(let x=0;x<n;x++)if(qr.isDark(y,x))d+='M'+(x+q)+' '+(y+q)+'h1v1h-1z';
 $c('chat-qr-img').innerHTML='<svg viewBox="0 0 '+(n+2*q)+' '+(n+2*q)+'" shape-rendering="crispEdges" aria-hidden="true"><rect width="100%" height="100%" fill="#fff"/><path d="'+d+'" fill="#0b0d12"/></svg>';
 box.hidden=false;btn.textContent=T('chatQrHide');btn.setAttribute('aria-expanded','true');
 // Shown only as long as needed.
 qrHide=setTimeout(()=>chatQr(false),90000)}
$c('chat-qr-show').onclick=()=>chatQr($c('chat-qr').hidden);
$c('chat-link').onclick=async()=>{try{await navigator.clipboard.writeText(chatLinkTo());chatNote(T('chatLinkCopied'))}catch(e){chatNote(chatLinkTo())}};
// Opened from such a QR code or link: keep the code, clean the address, open the chat.
{const m=location.hash.match(/^#chat-code=([A-Za-z0-9_-]{16,128})$/);
 if(m){store.set('chatCode',m[1]);history.replaceState(null,'',location.pathname+location.search);chat.imported=true}}
$c('chat-file').onclick=()=>$c('chat-file-input').click();
$c('chat-file-input').onchange=async e=>{const f=e.target.files[0];e.target.value='';if(!f)return;
 const kind=f.type.startsWith('image/')?'image':f.type.startsWith('video/')?'video':null;if(!kind){chatError('media_type');return}
 const blob=kind==='image'?await chatShrink(f):f;if(blob.size>(kind==='image'?3:8)*1048576){chatError('media_size');return}
 chatAttachShow({kind,blob,label:f.name||T(kind==='image'?'chatPhoto':'chatVideo')})};
$c('chat-attach-drop').onclick=chatAttachClear;
$c('chat-rec').onclick=chatRecord;
$c('chat-text').addEventListener('keydown',e=>{if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();$c('chat-form').requestSubmit()}});
$c('chat-text').addEventListener('input',e=>{e.target.style.height='auto';e.target.style.height=e.target.scrollHeight+'px'});
// "/login <key>" turns team mode on for this browser, "/logout" off; never sent as a message.
$c('chat-form').onsubmit=async e=>{e.preventDefault();if(chat.busy)return;const t=$c('chat-text'),text=t.value.trim();if(!text&&!chat.attach)return;
 const login=text.match(/^\/login\s+(\S+)$/);
 if(login){store.set('chatKey',login[1]);chatSend({type:'auth',key:login[1]});t.value='';return}
 if(text==='/logout'){store.set('chatKey',null);chat.admin=false;chatRender();chatMe();t.value='';return}
 if(!(chat.me&&chat.me.name)){chatNote(T('chatNeedName'));chatShowSettings(true);return}
 const say={type:'say',text};
 if(chat.attach){chat.busy=true;$c('chat-send').disabled=true;chatNote(T('chatUploading'));
  try{say.media={id:(await chatUpload(chat.attach)).id};chatNote('')}catch(err){chatError(err.message);return}
  finally{chat.busy=false;$c('chat-send').disabled=false}}
 if(chatSend(say)){t.value='';t.style.height='auto';chatAttachClear()}};
chatConnect();

apply();refresh();setInterval(()=>{if(!document.hidden)refresh()},20000);
document.addEventListener('visibilitychange',()=>{if(!document.hidden)refresh()});
