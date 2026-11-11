const FR={'Dashboard':'Tableau de bord','Settings':'Réglages','Log':'Journal','Hashrate':'Hashrate','Shares':'Shares',
'Explorer':'Explorateur',
'Seen in the Bitcoin best chain':'Vu dans la chaîne principale Bitcoin','confirmations':'confirmations','Outside the explorer best chain':'Hors de la chaîne principale selon l’explorateur','Not indexed by the explorer yet':'Pas encore indexé par l’explorateur','Explorer check failed':'Échec de la vérification sur l’explorateur','Reported by':'Rapporté par','Last checked':'Dernière vérification','Pool payout is separate':'Le paiement dépend de la pool',
'rejected':'rejetées','pool diff':'diff pool','Best difficulty':'Meilleure difficulté','best ever':'record','32-bit shares':'Shares 32 bits','64-bit shares':'Shares 64 bits','Since restart':'Depuis le redémarrage','years':'ans',
'1 every':'1 toutes les','on average':'en moyenne','Block chance':'Chance de bloc','day':'jour','year':'an','Block candidates':'Blocs candidats',
'Local candidates; chain confirmation unknown':'Candidats locaux ; confirmation sur la chaîne inconnue','Queued with priority':'En file prioritaire','Submitted to pool':'Envoyé à la pool','Share accepted by pool':'Share acceptée par la pool','Share rejected by pool':'Share rejetée par la pool','No pool answer':'Aucune réponse de la pool','Obsolete work':'Travail obsolète','Candidate queue full: not sent':'File de candidats pleine : non envoyé','Unknown submission status':'État de transmission inconnu',
'Saved from a previous session':'Sauvegardé lors d’une session précédente','lifetime':'depuis le début','Total hashes':'Hashes totaux','uptime':'en marche depuis','Block height':'Hauteur de bloc','net diff':'diff réseau',
'Lifetime':'Depuis le début','hashes':'hashes','shares':'shares','running':'en marche','Hashrate history':'Historique du hashrate',
'5-min averages, last':'moyennes sur 5 min, depuis','avg':'moy.','range':'de','first point after':'premier point après','min of mining':'min de minage',
'Seen by the pool':'Vu par la pool','best':'meilleure','worker(s)':'worker(s)','min ago':'min','no stats API found for this pool':'aucune API de statistiques trouvée pour cette pool','via':'via','estimated from accepted shares':'estimé d\'après les shares acceptées','since restart':'depuis le redémarrage','waiting for accepted shares':'en attente de shares acceptées',
'Best shares':'Meilleures shares','BTC price':'Prix du BTC','sats':'sats','Fees sat/vB':'Frais sat/vB','fast / 30 min / 1 h':'rapide / 30 min / 1 h',
'Network hashrate':'Hashrate du réseau','tx in mempool':'tx dans la mempool','Next retarget':'Prochain ajustement','in':'dans','blocks':'blocs','done':'fait',
'Pool (backup)':'Pool (secours)','Pool':'Pool','Payout address':'Adresse de paiement','not a valid Bitcoin address':'adresse Bitcoin invalide',
'Device':'Appareil','night mode':'mode nuit','last reset':'dernier redémarrage','WiFi':'WiFi','Miners':'Miners','All miners':'Tous les miners',
'Share my pool anonymously':'Partager ma pool anonymement',
'On by default. Turn off to hide your pool group. Requires hashrate sharing. Other pools are grouped as private/other. Never sends the hostname, port, Bitcoin address or worker.':'Activé par défaut. Désactive ce réglage pour masquer ta pool. Nécessite le partage du hashrate. Les autres pools sont regroupées dans « Autre / pool privée ». Aucun nom d’hôte, port, adresse Bitcoin ni worker n’est envoyé.',
'miners':'miners','no answer':'pas de réponse','online':'en ligne','ONLINE':'EN LIGNE','OFFLINE':'HORS LIGNE','NO CONNECTION':'PAS DE CONNEXION',
'Updated':'Mis à jour','market data':'données marché','min old':'min','mining paused':'minage en pause','power limited':'puissance limitée','battery saver: half speed':'économie de batterie : mi-régime',
'Recent blocks':'Derniers blocs','mining now':'en cours de minage','your best':'ta meilleure','ago':'il y a','Firmware update':'Mise à jour du firmware',
'Update available':'Mise à jour disponible','Install':'Installer','up to date':'à jour',
'Save and restart':'Enregistrer et redémarrer','Backup':'Sauvegarde','Download a backup':'Télécharger une sauvegarde','Restore a backup':'Restaurer une sauvegarde',
'Update':'Mettre à jour','Not a backup file':'Ce n\'est pas un fichier de sauvegarde','Restore this backup? The miner restarts afterwards.':'Restaurer cette sauvegarde ? Le miner redémarre ensuite.',
'Saved, restarting… reload in 15 s':'Enregistré, redémarrage… recharge dans 15 s','Upload failed':'Échec de l\'envoi',
'Refreshes every 5 s. Times are since the last start.':'Actualisé toutes les 5 s. Heures depuis le dernier démarrage.',
'backup_help':'Le fichier de sauvegarde contient tous les réglages (les mots de passe et le webhook seulement si un mot de passe web est défini : garde-le alors privé) et les statistiques à vie : meilleure difficulté, les 10 meilleures shares, hashes, shares, blocs, durée de fonctionnement. Le restaurer sur un autre miner lui donne aussi ces statistiques.',
'Display':'Affichage','Notifications':'Notifications','Safety':'Sécurité','Network':'Réseau'};
// Settings labels by key.
Object.assign(FR,{
 'Cryptocurrency':'Cryptomonnaie',
 'BTC / BCH setup guide':'Guide de configuration BTC / BCH',
 'Payout address or pool account':'Adresse de paiement ou compte de pool',
 "Use the selected coin's address or the account name required by your pool. A worker name can follow: address.worker":"Utilise une adresse de la monnaie choisie ou le compte demandé par la pool. Tu peux ajouter un worker : adresse.worker",
 'SHA-256d only. Choose main and backup pools for the same coin and use its payout address or pool account. The pool determines which chain is mined. BCH uses Stratum V1; Bitcoin market data is unavailable for BCH.':'SHA-256d uniquement. Choisis des pools principale et de secours pour la même monnaie, avec son adresse de paiement ou son compte de pool. La pool détermine la chaîne minée. BCH utilise Stratum V1 ; les données du marché Bitcoin sont indisponibles en BCH.',
 'Bitcoin market data is unavailable for this coin.':'Les données du marché Bitcoin sont indisponibles pour cette monnaie.',
 'Check the selected coin and the pool account requirements.':'Vérifie la monnaie choisie et le compte demandé par la pool.',
 'Unsupported cryptocurrency':'Cryptomonnaie non prise en charge',
 'BCH requires Stratum V1':'BCH nécessite Stratum V1',
 'Choose a pool for the selected cryptocurrency':'Choisis une pool pour la cryptomonnaie sélectionnée',
 'Use an address for the selected cryptocurrency':'Utilise une adresse de la cryptomonnaie sélectionnée'
});
const FR_SET={asicmhz:'Fréquence ASIC Gamma (MHz)',asicmv:'Tension de cœur ASIC Gamma (mV)',pool:'Hôte de la pool',port:'Port de la pool',addr:'Adresse BTC (éventuellement adresse.worker)',pass:'Mot de passe de la pool',
diff:'Difficulté de share suggérée',pool2:'Pool de secours (vide = aucune)',port2:'Port de la pool de secours',
proto:'Protocole du pool principal',proto2:'Protocole du pool de secours',sv2key:'Clé d’autorité V2 du pool principal',sv2key2:'Clé d’autorité V2 du pool de secours',uaauto:'Mode du user agent',ua:'User agent manuel',
rotateh:'Alterner pool principale / secours toutes les N heures (0 = seulement en cas de panne)',bright:'Luminosité (%)',rot:'Rotation (0-3)',
autorot:'Rotation avec l\'accéléromètre (appareils M5)',lang:'Langue : 0 anglais, 1 français',theme:'Thème (0 orange, 1 vert, 2 bleu, 3 violet, 4 rose)',
cycle:'Écran suivant toutes les N secondes (0 = non)',nightfrom:'Mode nuit à partir de (heure 0-23, 255 = non)',nightto:'Mode nuit jusqu\'à (heure 0-23)',
beep:'Bip à chaque share 32 bits',tz:'Fuseau horaire (POSIX TZ, ex. CET-1CEST,M3.5.0,M10.5.0/3 ou UTC0)',fiat:'Devise du prix (USD, EUR, GBP, CAD, CHF, AUD, JPY)',
ntfy:'Sujet ntfy (vide = non)',ntfysrv:'Serveur ntfy',notify32:'Notifier aussi chaque share 32 bits',hook:'URL de webhook : POST JSON pour Discord, Telegram… (vide = non)',alertpct:'Alerter si le hashrate reste sous ce % de sa normale (0 = non)',alertmin:'… pendant ce nombre de minutes',power:'Puissance de minage sur USB et batterie (0–100 %, 0 = pause)',maxtemp:'Pause du minage au-dessus de cette température en °C (0 = non)',
rebooth:'Redémarrer après N heures sans share acceptée (0 = non)',saver:'Économie de batterie : écran faible et minage à mi-régime sur batterie',
wifi2:'WiFi de secours 1 : nom',wpass2:'WiFi de secours 1 : mot de passe',wifi3:'WiFi de secours 2 : nom',wpass3:'WiFi de secours 2 : mot de passe',
mqtt:'Serveur MQTT pour Home Assistant (vide = non)',mqttport:'Port MQTT',mqttuser:'Utilisateur MQTT',mqttpass:'Mot de passe MQTT',
peers:'Autres miners du tableau de bord (IP ou noms, séparés par des virgules)',webpass:'Mot de passe web (réglages et mise à jour depuis le tableau de bord)',
update:'Mises à jour depuis GitHub : 0 non, 1 vérifier et proposer, 2 installer automatiquement',
count:'Compteur anonyme d\'utilisation : une fois par jour, télécharge un fichier de 2 octets de la release de cette version (rien n\'est envoyé)'};
Object.assign(FR,{
 'Update channel':'Canal de mise à jour','Stable':'Stable','Beta':'Bêta',
 'Beta versions receive the same signature checks and rollback protection.':'Les versions bêta conservent la vérification des signatures et le retour arrière.',
 'Custom mining screen':'Écran de minage personnalisé','Choose the four metrics and their order on the mining screen.':'Choisis les quatre mesures et leur ordre sur l’écran de minage.',
 'First screen metric':'Première mesure','Second screen metric':'Deuxième mesure','Third screen metric':'Troisième mesure','Fourth screen metric':'Quatrième mesure',
 'Enabled screens':'Écrans activés','The mining screen remains available.':'L’écran de minage reste disponible.',
 'The mining screen must remain enabled':'L’écran de minage doit rester activé','Mining':'Minage','Clock':'Horloge','Bitcoin':'Bitcoin','Network':'Réseau','Device':'Appareil','Records':'Records','Blocks':'Blocs','Power / thermal':'Puissance et température','Pool / records':'Pool et records','ASIC frequency':'Fréquence ASIC','Fan RPM':'Ventilateur (tr/min)',
 'beta channel unavailable: %s':'canal bêta indisponible : %s','beta release requires the beta channel':'cette version exige le canal bêta',
 'Pool diagnostics':'Diagnostic du pool','Check pool':'Vérifier le pool','Checking pool…':'Vérification du pool…',
 'Pool ready':'Pool compatible','Waiting for mining work':'En attente de travail de minage',
 'Version rolling is required by this ASIC':'Cet ASIC exige le version rolling',
 'Mining with infrequent pool shares':'Minage avec des shares peu fréquentes',
 'Invalid pool difficulty':'Difficulté du pool invalide','Expected share interval':'Intervalle moyen des shares',
 'Share-based rate':'Débit estimé par les shares','At least 3 accepted shares and 60 seconds are needed':'Il faut au moins 3 shares acceptées et 60 secondes',
 'Accepted samples':'Shares utilisées','Rejected share causes':'Causes des rejets','stale':'obsolète','duplicate':'doublon',
 'low_difficulty':'difficulté insuffisante','authorization':'authentification','invalid':'invalide','unknown':'inconnue',
 'Share intervals are statistical means, not deadlines. Share-based rates fluctuate with luck and pool difficulty.':'Les intervalles sont des moyennes statistiques, sans échéance garantie. Le débit estimé varie avec la chance et la difficulté du pool.'
});
Object.assign(FR,{'Mining overview':'Vue du minage','Firmware':'Firmware','Connecting…':'Connexion…','Pool acceptance':'Acceptation par le pool','No pool replies yet':'Aucune réponse du pool pour le moment','Updating firmware':'Mise à jour du firmware','Cooling down':'Refroidissement','ASIC stopped':'ASIC arrêté','Waiting for pool':'En attente du pool','Starting mining':'Démarrage du minage'});
function t(s){return LANG=='fr'&&FR[s]||s}
FR['ASIC mining stays paused for at least 2 minutes, then resumes once cooled by 5 °C.']='Le minage ASIC reste en pause au moins 2 minutes, puis reprend une fois la température abaissée de 5 °C.';
// Numbers and labels as the page's language writes them: « 1,00 MH/s », « Pool : … ».
function fix(v,n){const s=v.toFixed(n);return LANG=='fr'?s.replace('.',','):s}
const COLON=LANG=='fr'?'\u00a0: ':': ',LOCALE=LANG=='fr'?'fr-FR':'en-US';

Object.assign(FR,{
 '525 MHz (stock)':'525 MHz (origine)',
 'Profiles above 525 MHz are experimental: up to 690 MHz as in ESP-Miner, then overclocking up to 800 MHz. Default: 525 MHz. Choosing a profile raises the core voltage to the one that goes with it. Instability triggers a 525 MHz fallback, kept until the frequency or voltage changes.':'Les profils au-dessus de 525 MHz sont expérimentaux : jusqu’à 690 MHz comme dans ESP-Miner, puis overclocking jusqu’à 800 MHz. Par défaut : 525 MHz. Choisir un profil monte la tension de cœur à celle qui va avec. En cas d’instabilité, repli à 525 MHz, gardé jusqu’à un changement de fréquence ou de tension.',
 'Default: 1150 mV, as ESP-Miner. Goes with the frequency: 1150 mV up to 600 MHz, 1200 mV up to 675, 1250 mV up to 725, 1300 mV up to 800. Higher voltage means more heat and power; watch the temperature. Safety stops still apply.':'Par défaut : 1 150 mV, comme ESP-Miner. Va avec la fréquence : 1 150 mV jusqu’à 600 MHz, 1 200 mV jusqu’à 675, 1 250 mV jusqu’à 725, 1 300 mV jusqu’à 800. Plus de tension, c’est plus de chaleur et de consommation : surveille la température. Les arrêts de sécurité restent actifs.',
 '1150 mV (stock)':'1150 mV (origine)',
 'Gamma frequency must be 525-800 MHz in 25 MHz steps, or 690 MHz':'La fréquence du Gamma doit être de 525 à 800 MHz par pas de 25 MHz, ou 690 MHz',
 'Gamma core voltage must be 1000, 1060, 1100, 1150, 1200, 1250 or 1300 mV':'La tension de cœur du Gamma doit être 1000, 1060, 1100, 1150, 1200, 1250 ou 1300 mV',
 'Requested profile':'Profil demandé','ASIC hardware errors':'Erreurs matérielles ASIC','Stock fallback':'Repli à la fréquence d’origine',
 'Overclock fallback: mining at the stock profile.':'Repli de l’overclock : minage à la fréquence d’origine.',
 'Mining (525 MHz fallback profile)':'Minage à 525 MHz (repli)',
 'High ASIC hardware error rate':'Taux élevé d’erreurs matérielles ASIC',
 'ASIC error telemetry unavailable':'Mesure des erreurs ASIC indisponible',

 'Experimental hardware support':'Prise en charge matérielle expérimentale',
 'Fan':'Ventilateur','Regulator':'Régulateur','Input':'Entrée',
 'Mining (experimental)':'Minage (expérimental)',
 'Fan stopped; restart required':'Ventilateur arrêté ; redémarrage nécessaire',
 'ASIC overtemperature; restart required':'Surchauffe ASIC ; redémarrage nécessaire',
 'ASIC temperature sensor fault; restart required':'Sonde ASIC en défaut ; redémarrage nécessaire',
 'Regulator temperature fault; restart required':'Température du régulateur en défaut ; redémarrage nécessaire',
 'Regulator output overcurrent; restart required':'Surintensité en sortie du régulateur ; redémarrage nécessaire',
 'Regulator reports a fault; restart required':'Le régulateur signale un défaut ; redémarrage nécessaire',
 'ASIC voltage fault; restart required':'Tension de cœur ASIC en défaut ; redémarrage nécessaire',
 '5V supply fault; restart required':'Alimentation 5 V en défaut ; redémarrage nécessaire',
 'ASIC not responding; restart required':'L’ASIC ne répond plus ; redémarrage nécessaire',
 'ASIC counter stalled; restart required':'Compteur de l’ASIC bloqué ; redémarrage nécessaire',
 'Restarting the ASIC':'Redémarrage de l’ASIC',
 'BM1370 initialization failed':'Échec de l’initialisation du BM1370',
 'Temperature sensor not reading: ASIC stopped':'Sonde de température sans mesure : ASIC arrêté',
 'read by this browser':'lu par ce navigateur',
 'Pool needs version rolling':'Le pool doit accepter le version rolling',
 'Paused / waiting for pool':'En pause / en attente du pool',
 'Ready / waiting for pool':'Prêt / en attente du pool',
 'Last fault':'Dernier défaut',
 'Regulator or sensors not found: plug in the 5V supply':'Régulateur ou capteurs introuvables : branche l’alimentation 5 V',
 'Waiting for the 5V supply (barrel jack)':'En attente de l’alimentation 5 V (prise jack)',
 'Waiting for the fan to turn':'En attente du ventilateur',

 'automatic':'automatique',
 'manual':'manuel','Automatic':'Automatique','Manual':'Manuel',
 'Choose automatic or manual user agent below. Each pool has its own protocol and authority key.':'Choisis le user agent automatique ou manuel ci-dessous. Chaque pool a son protocole et sa clé d’autorité.',
 'Stratum V2 needs the pool\'s V2 port and authority key. It does not change share difficulty.':'Stratum V2 nécessite le port V2 et la clé d’autorité du pool. La difficulté des shares reste décidée par le pool.',
 'User agent must be at most 64 characters':'Le user agent ne doit pas dépasser 64 caractères',
 'A manual user agent is required':'Un user agent manuel est obligatoire',
 'A valid Stratum V2 authority key is required':'Une clé d’autorité Stratum V2 valide est obligatoire',
 'SV2 authority key missing/invalid':'Clé d’autorité V2 absente ou invalide',
 'SV2 waiting for synchronized time':'V2 attend la synchronisation de l’heure',
 'SV2 allocation failed':'Mémoire insuffisante pour V2',
 'SV2 encrypted handshake':'Établissement de la connexion V2 chiffrée',
 'SV2 Noise handshake failed':'Échec de la négociation V2 Noise',
 'SV2 server verification failed':'Échec de la vérification du serveur V2',
 'SV2 invalid or unauthenticated frame':'Message V2 invalide ou non authentifié',
 'SV2 protocol refused/invalid':'Protocole V2 refusé ou invalide',
 'SV2 setup/channel timed out':'Délai dépassé pour l’ouverture du canal V2',
 'Pool work timed out':'Délai dépassé pour recevoir le travail de la pool',
 'SV2 work timed out':'Délai dépassé pour recevoir le travail V2 de la pool',
 'The Stratum user agent is selected automatically for each pool.':'Le user agent Stratum est choisi automatiquement pour chaque pool.'
});

Object.assign(FR,{
 'Connection failed. Please try again.':'Connexion impossible. Réessaie.',
 'Request timed out. Please try again.':'Le délai de réponse est dépassé. Réessaie.',
 'Invalid response from the miner.':'Réponse du miner invalide.',
 'Authentication required. Reload this page and sign in.':'Authentification requise. Recharge la page et reconnecte-toi.',
 'Loading settings…':'Chargement des réglages…','Retry':'Réessayer',
 'Enter a valid value.':'Saisis une valeur valide.',
 'The temperature must be 0 or at least 30 °C.':'La température doit être 0 ou au moins 30 °C.',
 'The hour must be 0–23 or 255 to disable night mode.':'L’heure doit être comprise entre 0 et 23, ou 255 pour désactiver le mode nuit.',
 'The difficulty must be greater than zero.':'La difficulté doit être supérieure à zéro.',
 'Backup restored. The miner is restarting…':'Sauvegarde restaurée. Le miner redémarre…',
 'Select a firmware file first.':'Sélectionne d’abord un fichier de firmware.',
 'Use a firmware .bin file, not a factory image.':'Utilise le fichier firmware .bin, pas l’image factory.',
 'Invalid firmware file.':'Fichier de firmware invalide.',
 'Upload complete. Validating firmware…':'Envoi terminé. Validation du firmware…',
 'Firmware updated. The miner is restarting…':'Firmware mis à jour. Le miner redémarre…',
 'Upload timed out. Please try again.':'Le délai d’envoi est dépassé. Réessaie.',
 'Upload cancelled.':'Envoi annulé.',
 'Installing…':'Installation…','Installation requested.':'Installation demandée.',
 'GitHub updates':'Mises à jour GitHub','Check now':'Vérifier maintenant',
 'Checking…':'Vérification…','Check requested.':'Vérification demandée.',
 'Updates disabled in settings.':'Mises à jour désactivées dans les réglages.',
 'Waiting for the first check.':'En attente de la première vérification.',
 'Latest release checked.':'Dernière release vérifiée.',
 'Could not refresh the log.':'Impossible d’actualiser le journal.'
});
Object.assign(FR,{
 'Unsupported backup version':'Version de sauvegarde non prise en charge',
 'Invalid backup settings':'Réglages de sauvegarde invalides',
 'Missing backup statistics':'Statistiques absentes de la sauvegarde',
 'Invalid or incomplete backup statistics':'Statistiques de sauvegarde invalides ou incomplètes',
 'Invalid text setting':'Valeur de texte invalide',
 'Setting outside its allowed range':'Valeur hors de la plage autorisée',
 'Temperature must be 0 or at least 30 C':'La température doit être 0 ou au moins 30 °C',
 'Night hours must be 0-23 or 255':'Les heures doivent être comprises entre 0 et 23, ou 255',
 'Difficulty must be a finite positive number':'La difficulté doit être un nombre positif valide',
 'Invalid checkbox setting':'Valeur de case à cocher invalide',
 'Pool host and payout address cannot be empty':'L’hôte de la pool et l’adresse de paiement sont obligatoires',
 'No settings to save':'Aucun réglage à enregistrer',
 'No firmware file uploaded':'Aucun fichier de firmware envoyé',
 'Only one firmware file is allowed':'Un seul fichier de firmware est autorisé',
 'Empty firmware file':'Le fichier de firmware est vide',
 'Firmware update or restart pending; try again afterwards':'Une mise à jour ou un redémarrage est en cours. Réessaie ensuite'
});
function errorText(e){return e&&e.message?e.message:t('Connection failed. Please try again.')}
async function request(url,options={},format='json'){
 const {timeout=8000,...init}=options,c=new AbortController();
 const timer=setTimeout(()=>c.abort(),timeout);
 try{
  const r=await fetch(url,{cache:'no-store',...init,signal:c.signal});
  const body=await r.text();
  if(r.status===401)throw Error(t('Authentication required. Reload this page and sign in.'));
  if(!r.ok)throw Error(t(body)||'HTTP '+r.status);
  if(format==='text')return body;
  try{return JSON.parse(body)}catch(e){throw Error(t('Invalid response from the miner.'))}
 }catch(e){
  if(e.name==='AbortError')throw Error(t('Request timed out. Please try again.'));
  if(e instanceof TypeError)throw Error(t('Connection failed. Please try again.'));
  throw e;
 }finally{clearTimeout(timer)}
}

document.addEventListener('DOMContentLoaded',()=>{document.documentElement.lang=LANG;document.querySelectorAll('[data-t]').forEach(e=>e.textContent=t(e.dataset.t))});

Object.assign(FR,{
 'Skip to content':'Aller au contenu','Workspace':'Espace de travail','Served by your miner':'Hébergé par ton miner','Local control. No cloud required.':'Contrôle local, sans cloud.',
 'LOCAL':'LOCAL','MINING OVERVIEW':'VUE DU MINAGE','Your miner, at a glance.':'Ton miner, en un coup d’œil.','Live performance, pool activity and device health.':'Performances en direct, activité de la pool et état de la carte.',
 'Tune performance':'Régler la puissance','Connecting to your miner…':'Connexion à ton miner…','Pool & wallet':'Pool et portefeuille','Your connection to the network':'Ta connexion au réseau','Bitcoin network':'Réseau Bitcoin','Public market data':'Données publiques du marché','Device health':'État de la carte','Temperature, memory and connectivity':'Température, mémoire et connexion',
 'MAKE IT YOURS':'À TON IMAGE','Settings & maintenance':'Réglages et maintenance','Tune your miner. Your settings stay on this device.':'Règle ton miner. Tes paramètres restent sur cette carte.','Find a setting':'Trouver un réglage','Search settings…':'Rechercher un réglage…','No matching settings.':'Aucun réglage correspondant.',
 'Changes take effect after restart.':'Les changements s’appliquent après redémarrage.','Select the firmware .bin for your board. Settings and statistics are kept.':'Choisis le firmware .bin de ta carte. Les réglages et les statistiques sont conservés.','Firmware file':'Fichier de firmware','Restart miner':'Redémarrer le miner','Restart the device while keeping your settings and statistics.':'Redémarre la carte en conservant ses réglages et ses statistiques.','Restart this miner?':'Redémarrer ce miner ?','Restarting…':'Redémarrage…',
 'UNDER THE HOOD':'DIAGNOSTIC','Activity log':'Journal d’activité','Connection events, shares and device diagnostics.':'Connexions, shares et diagnostics de la carte.','Pause':'Pause','Resume':'Reprendre','Download log':'Télécharger le journal','Filter the log':'Filtrer le journal','Search log entries…':'Rechercher dans le journal…','DEVICE OUTPUT':'JOURNAL DE LA CARTE','No matching entries.':'Aucune entrée correspondante.',
 'Full performance':'Pleine puissance','Balanced':'Équilibré','Quiet':'Discret','Configured limit':'Limite configurée','Effective power':'Puissance effective','Temperature':'Température','Memory':'Mémoire','Live (10 s)':'En direct (10 s)','1-minute average':'Moyenne sur 1 minute','Hardware SHA':'SHA matériel','Software SHA':'SHA logiciel','Download via USB':'Télécharger pour USB','This board requires a USB firmware update.':'Cette carte nécessite une mise à jour du firmware par USB.',
 'At 100%, mining runs at full available speed. Temperature protection can still reduce it.':'À 100 %, le minage utilise toute la vitesse disponible. La protection thermique peut encore la réduire.',
 'Saved settings are kept when upgrading. Select Full performance to request 100%.':'Les réglages existants sont conservés à la mise à jour. Choisis Pleine puissance pour demander 100 %.',
 'No usable temperature sensor.':'Aucun capteur de température utilisable.','Temperature protection is limiting mining.':'La protection thermique limite le minage.','Temperature protection has paused mining.':'La protection thermique a mis le minage en pause.','Mining is paused by your power setting.':'Le minage est en pause avec ton réglage de puissance.','Mining is paused during a firmware update.':'Le minage est en pause pendant la mise à jour du firmware.','Mining is waiting for fresh supervision.':'Le minage attend une nouvelle mesure de supervision.','Battery saver is limiting mining.':'L’économie de batterie limite le minage.','Mining is limited by your power setting.':'Ton réglage de puissance limite le minage.','Waiting for a pool connection.':'En attente d’une connexion à la pool.','Public market data is temporarily unavailable. Mining can continue.':'Les données du marché sont temporairement indisponibles. Le minage peut continuer.',
 'GitHub updates are disabled or busy':'Les mises à jour GitHub sont désactivées ou occupées','GitHub updates are disabled in settings':'Les mises à jour GitHub sont désactivées dans les réglages','An update is being installed':'Une mise à jour est en cours d’installation'
});
function translatePage(){
 document.documentElement.lang=LANG;
 document.querySelectorAll('[data-t]').forEach(e=>e.textContent=t(e.dataset.t));
 document.querySelectorAll('[data-placeholder]').forEach(e=>e.placeholder=t(e.dataset.placeholder));
 const page=location.pathname==='/settings'?'Settings':location.pathname==='/log'?'Log':'Dashboard';
 document.getElementById('page-title').textContent=t(page);
 document.querySelectorAll('[data-nav]').forEach(a=>{if(a.dataset.nav===location.pathname)a.setAttribute('aria-current','page')});
}
document.addEventListener('DOMContentLoaded',translatePage);
// Page visibility and online events share one scheduler: a slow request can
// never overlap the next refresh, including when a hidden tab is reopened.
function startPolling(task,delay){
 let timer=0,running=false,stopped=false;
 async function run(){
  if(stopped||running)return;
  clearTimeout(timer);
  if(document.hidden){timer=setTimeout(run,delay);return}
  running=true;
  try{await task()}finally{running=false;if(!stopped)timer=setTimeout(run,delay)}
 }
 const wake=()=>{if(!document.hidden)run()};
 document.addEventListener('visibilitychange',wake);window.addEventListener('online',wake);run();
 return {refresh:run,stop(){stopped=true;clearTimeout(timer);document.removeEventListener('visibilitychange',wake);window.removeEventListener('online',wake)}};
}
Object.assign(FR,{"Where your miner sends its work and how it introduces itself.":"Où votre miner envoie son travail et comment il se présente.","The miner's screen: brightness, colours, night mode and clock.":"L’écran du miner : luminosité, couleurs, mode nuit et horloge.","Phone notifications, webhook and alerts.":"Notifications sur téléphone, webhook et alertes.","Temperature, power and automatic restarts.":"Température, puissance et redémarrages automatiques.","Fallback WiFi, Home Assistant, other miners, password and updates.":"WiFi de secours, Home Assistant, autres miners, mot de passe et mises à jour.","Main pool":"Pool principale","Identity":"Identité","Backup pool":"Pool de secours","Screen":"Écran","Night mode":"Mode nuit","Clock and price":"Heure et prix","Webhook":"Webhook","Alerts":"Alertes","Heat and power":"Chaleur et puissance","Recovery":"Récupération","Fallback WiFi":"WiFi de secours","Home Assistant (MQTT)":"Home Assistant (MQTT)","Web pages":"Pages web","Updates":"Mises à jour","Pool host":"Adresse de la pool","A pasted stratum+tcp:// or stratum2+tcp:// address is split for you.":"Une adresse stratum+tcp:// ou stratum2+tcp:// collée est découpée pour vous.","Pool port":"Port de la pool","Bitcoin address":"Adresse Bitcoin","Your payout address. A worker name can follow: address.worker":"Votre adresse de paiement. Un nom de worker peut suivre : adresse.worker","Pool password":"Mot de passe de la pool","Usually x.":"En général x.","Suggested difficulty":"Difficulté suggérée","Share difficulty asked of the pool; it may choose another.":"Difficulté des shares demandée à la pool ; elle peut en choisir une autre.","Protocol":"Protocole","Stratum V2 authority key":"Clé d’autorité Stratum V2","Given by the pool for encrypted V2 connections.":"Fournie par la pool pour les connexions V2 chiffrées.","User agent":"User agent","How the miner names itself to the pool.":"Le nom sous lequel le miner se présente à la pool.","Manual user agent":"User agent manuel","Up to 64 characters.":"Jusqu’à 64 caractères.","Backup pool host":"Adresse de la pool de secours","Used when the main pool keeps failing. Empty = none.":"Utilisée quand la pool principale échoue. Vide = aucune.","Backup pool port":"Port de la pool de secours","Backup protocol":"Protocole de secours","Backup Stratum V2 authority key":"Clé d’autorité Stratum V2 de secours","Alternate pools every":"Alterner les pools toutes les","0 = switch only when the main pool fails.":"0 = changer seulement si la pool principale échoue.","Brightness":"Luminosité","Rotation":"Orientation","Follow the accelerometer":"Suivre l’accéléromètre","M5 devices with a motion sensor: the picture turns with the device.":"Appareils M5 avec capteur de mouvement : l’image tourne avec l’appareil.","Language":"Langue","Screen and web pages.":"Écran et pages web.","Colour theme":"Couleur du thème","Accent colour of the screen.":"Couleur d’accent de l’écran.","Next screen every":"Écran suivant toutes les","0 = only with the button.":"0 = seulement avec le bouton.","Night mode from":"Mode nuit à partir de","Screen off and no beep. A button press lights it for 30 s.":"Écran éteint et pas de bip. Un appui sur un bouton l’allume 30 s.","Night mode until":"Mode nuit jusqu’à","Beep on a 32-bit share":"Bip sur une share 32 bits","Time zone":"Fuseau horaire","POSIX TZ, e.g. CET-1CEST,M3.5.0,M10.5.0/3 or UTC0":"TZ POSIX, par ex. CET-1CEST,M3.5.0,M10.5.0/3 ou UTC0","Price currency":"Devise du prix","ntfy topic":"Sujet ntfy","A long, hard-to-guess name. Empty = off.":"Un nom long et difficile à deviner. Vide = désactivé.","ntfy server":"Serveur ntfy","Also every 32-bit share":"Aussi chaque share 32 bits","Webhook URL":"URL du webhook","JSON POST for Discord, Telegram… It holds a token: keep it private. Empty = off.":"POST JSON pour Discord, Telegram… Elle contient un jeton : gardez-la privée. Vide = désactivé.","Hashrate alert below":"Alerte hashrate sous","Share of the usual full-power hashrate. 0 = off.":"Part du hashrate habituel à pleine puissance. 0 = désactivé.","… for at least":"… pendant au moins","Pause mining above":"Pause du minage au-dessus de","0 = no thermal limit.":"0 = pas de limite thermique.","Mining power":"Puissance de minage","On USB and battery. 0 = paused.":"Sur USB et batterie. 0 = en pause.","Restart after no accepted share for":"Redémarrer sans share acceptée pendant","0 = off.":"0 = désactivé.","Battery saver":"Économie de batterie","On battery: dim the screen and mine at half speed.":"Sur batterie : écran atténué et minage à mi-vitesse.","Fallback WiFi 1":"WiFi de secours 1","Network name.":"Nom du réseau.","Fallback WiFi 1 password":"Mot de passe du WiFi de secours 1","Fallback WiFi 2":"WiFi de secours 2","Fallback WiFi 2 password":"Mot de passe du WiFi de secours 2","MQTT broker":"Broker MQTT","Your Home Assistant broker. Empty = off.":"Le broker de votre Home Assistant. Vide = désactivé.","MQTT port":"Port MQTT","MQTT user":"Utilisateur MQTT","MQTT password":"Mot de passe MQTT","Other miners":"Autres miners","Shown on the dashboard: IP addresses or names, comma separated.":"Affichés sur le tableau de bord : adresses IP ou noms, séparés par des virgules.","Web password":"Mot de passe web","Optional. Asked before changing settings or installing firmware from these pages. Empty = no password: anyone on your network can.":"Facultatif. Demandé avant de changer les réglages ou d’installer un firmware depuis ces pages. Vide = pas de mot de passe : n’importe qui sur ton réseau peut le faire.","Updates from GitHub":"Mises à jour depuis GitHub","Signed releases of this firmware.":"Versions signées de ce firmware.","Anonymous usage count":"Comptage anonyme d’utilisation","Once a day, downloads a 2-byte file of this version's release. Nothing is sent.":"Une fois par jour, télécharge un fichier de 2 octets de la version installée. Rien n’est envoyé.","Share my hashrate anonymously":"Partager mon hashrate anonymement","Every 5 minutes, sends the firmware version, the board, the cryptocurrency and the hashrate, to count the total of all BDLMiner miners live. No address, name or identifier.":"Toutes les 5 minutes, envoie la version du firmware, la carte, la cryptomonnaie et le hashrate, pour compter en direct le total de tous les miners BDLMiner. Ni adresse, ni nom, ni identifiant.","Anonymous statistics":"Statistiques anonymes","Off":"Désactivé","English":"English","Français":"Français","Portrait":"Portrait","Landscape":"Paysage","Portrait, upside down":"Portrait, à l’envers","Landscape, upside down":"Paysage, à l’envers","Orange":"Orange","Green":"Vert","Blue":"Bleu","Purple":"Violet","Pink":"Rose","Check and offer":"Vérifier et proposer","Install automatically":"Installer automatiquement","Mining":"Minage","System":"Système","Integrations":"Intégrations","Entries":"Entrées","Warnings":"Alertes","Covers":"Période","Last start":"Dernier démarrage","LIVE":"EN DIRECT","PAUSED":"EN PAUSE","Maintenance":"Maintenance","Discard changes":"Annuler les modifications","All":"Tout","Categories":"Catégories","unsaved change":"modification non enregistrée"});
Object.assign(FR,{'Show':'Voir','Hide':'Masquer','unsaved changes':'modifications non enregistrées','largest block':'plus grand bloc'});
Object.assign(FR,{'A service could not start: not enough memory. Restart the miner.':'Un service n’a pas pu démarrer par manque de mémoire. Redémarre le miner.'});
Object.assign(FR,{'Throttling starts at':'Le ralentissement commence à','Mining pauses at':'Le minage s’arrête à','Thermal protection is disabled.':'La protection thermique est désactivée.'});
Object.assign(FR,{
 'Settings could not be saved. Previous configuration kept; try again.':'Les réglages n’ont pas pu être enregistrés. La configuration précédente est conservée. Réessaie.',
 'Settings saved, but statistics could not be restored. Retry before restarting.':'Réglages enregistrés, mais statistiques non restaurées. Réessaie avant de redémarrer.',
 'Next retry in':'Prochaine tentative dans','Retry now':'Réessayer maintenant','awaiting response':'en attente de réponse',
 'Temperature & power history':'Historique température et puissance','Download CSV':'Télécharger le CSV','Five-minute sample averages. Gaps mean missing observations.':'Moyennes des mesures sur cinq minutes. Les interruptions indiquent des observations manquantes.',
 'No observations yet.':'Pas encore de mesures.','Mining measurements':'Mesures du minage','Batch times are elapsed worker time, not CPU usage. TLS handshakes do not pause hardware mining on classic ESP32.':'Les durées des lots mesurent le temps écoulé du moteur, pas son utilisation CPU. Les connexions TLS ne suspendent pas le minage matériel de l’ESP32 classique.',
 'TLS handshakes':'Connexions TLS','Elapsed time':'Durée cumulée','failures':'échecs','Mining worker':'Moteur','Batch time':'Temps dans les lots de calcul','Rest':'Repos','Waiting for work':'Attente de travail','Paused':'Pause',
 'Ten-minute measurement':'Mesure sur dix minutes','Measure actual hashes over device uptime, with temperature and effective power samples. Keep this tab visible.':'Mesure les hashes réellement calculés pendant le temps de fonctionnement de la carte, avec des relevés de température et de puissance effective. Garde cet onglet visible.',
 'Start measurement':'Démarrer la mesure','Finish now':'Terminer maintenant','Download report':'Télécharger le rapport','Preparing measurement…':'Préparation de la mesure…','Measurement running…':'Mesure en cours…','Measurement cancelled. Keep this tab visible.':'Mesure annulée. Garde cet onglet visible.',
 'Miner restarted. Start a new measurement.':'Le miner a redémarré. Lance une nouvelle mesure.','Waiting for a new sample.':'En attente d’un nouveau relevé.','Measured hashrate':'Hashrate mesuré','Temperature samples contain gaps.':'Les relevés de température comportent des interruptions.'
});
Object.assign(FR,{
 'PERFORMANCE HISTORY':'SUIVI DES PERFORMANCES','Every hash, in perspective.':'Le minage dans la durée.',
 'Average of displayed samples':'Moyenne des points affichés','Minimum':'Minimum','Maximum':'Maximum','Samples':'Points',
 'Latest point':'Dernier point','Displayed history':'Historique affiché','Sampling interval':'Pas des mesures','Observation coverage':'Couverture des mesures',
 'History period':'Période de l’historique','Device overview':'État de la carte',
 'Five-minute averages. Time is relative to the latest recorded point. Use arrow keys to inspect.':'Moyennes sur cinq minutes. Temps relatif au dernier point enregistré. Utilise les flèches du clavier pour consulter les valeurs.',
 'Gaps mean missing observations.':'Les coupures indiquent des mesures manquantes.',
 'A usable temperature sensor is required.':'Un capteur de température utilisable est nécessaire.',
 'The first history point appears after five minutes of mining.':'Le premier point apparaît après cinq minutes de minage.',
 'Mining records':'Records du minage','Shares, luck and lifetime totals':'Shares, probabilités et compteurs cumulés',
 'Diagnostics & measurements':'Diagnostics et mesures','TLS, mining workers and ten-minute report':'TLS, moteurs de minage et rapport sur dix minutes'
});
Object.assign(FR,{'Discord: channel settings › Integrations › Webhooks › Copy URL. Telegram: https://api.telegram.org/bot<token>/sendMessage?chat_id=<chat>. Save and restart first, then:':'Discord : paramètres du salon › Intégrations › Webhooks › Copier l’URL. Telegram : https://api.telegram.org/bot<jeton>/sendMessage?chat_id=<chat>. Enregistrez et redémarrez d’abord, puis :','Send a test notification':'Envoyer une notification de test','Sending…':'Envoi…','No result yet: see the log page.':'Pas encore de résultat : voir la page du journal.','off':'désactivé','no connection':'pas de connexion','no WiFi or memory':'pas de WiFi ou de mémoire','certificate not trusted':'certificat non reconnu','Set an ntfy topic or a webhook URL, save, then try again':'Indiquez un sujet ntfy ou une URL de webhook, enregistrez, puis réessayez','Test notification queued':'Notification de test envoyée','Crash history':'Historique des plantages','Verified ASIC results':'Résultats ASIC vérifiés','No worker listed: the pool shows a worker only for a few minutes after each share, and at difficulty':'Aucun worker listé : la pool n’affiche un worker que quelques minutes après chaque share, et à la difficulté','this miner sends one every':'ce miner en envoie une toutes les','Recent shares':'Shares récentes','Accepted':'Acceptée','Rejected':'Rejetée','Waiting':'En attente','No answer':'Sans réponse','diff':'diff','backup pool':'pool de secours','Daily summary at':'Résumé quotidien à','Average hashrate, best share, shares and pool connection of the past day. Off = none.':'Hashrate moyen, meilleure share, shares et connexion à la pool de la journée. Désactivé = aucun.','The summary hour must be 0-23 or 255':'L’heure du résumé doit être comprise entre 0 et 23, ou 255','the pool\'s statistics API is not answering':'l’API de statistiques de la pool ne répond pas','connected':'connecté','reconnection':'reconnexion','reconnections':'reconnexions','switch to backup':'bascule sur la pool de secours','switches to backup':'bascules sur la pool de secours','Integrations':'Intégrations','none sent yet':'aucune envoyée pour l’instant','Pool statistics':'Statistiques de la pool','searching…':'recherche…','check':'à vérifier','disconnected':'déconnecté','Saved and applied.':'Enregistré et appliqué.','Nothing to save.':'Rien à enregistrer.','needs a restart':'redémarrage nécessaire','applied right away':'appliqué tout de suite','Display, alerts and power apply right away; other changes restart the miner.':'Affichage, alertes et puissance s’appliquent tout de suite ; les autres changements redémarrent le miner.','Save':'Enregistrer','date unknown':'date inconnue','Last crash':'Dernier plantage','The webhook URL must start with http:// or https://':'L’URL du webhook doit commencer par http:// ou https://','Download report':'Télécharger le rapport','Erase':'Effacer','Firmware on trial: confirmed after 3 minutes of mining':'Version en essai : confirmée après 3 minutes de minage','No crash report':'Aucun rapport de plantage','Crash report erased':'Rapport de plantage effacé','The crash report could not be erased':'Le rapport de plantage n’a pas pu être effacé'});
Object.assign(FR,{'Other miners refresh every 15 seconds.':'Les autres miners sont actualisés toutes les 15 secondes.','accepted':'acceptées','One recorded sample':'Un point enregistré'});
Object.assign(FR,{'Notifications are busy; try again shortly':'Les notifications sont occupées ; réessaie dans un instant','Invalid notification test ID':'Identifiant de test de notification invalide'});
Object.assign(FR,{
 'Cooling':'Refroidissement','Fan control':'Réglage du ventilateur','Manual fan speed':'Vitesse manuelle du ventilateur',
 'Cooling priority':'Ventilation prioritaire','Adjust fan':'Régler le ventilateur',
 'Startup or stopped ASIC':'Démarrage ou ASIC arrêté','Temperature safety':'Sécurité thermique','Temperature sensor unavailable':'Température indisponible',
 'User temperature limit is disabled; hardware protection and cooling safety remain active.':'La limite de température utilisateur est désactivée ; les protections matérielles et la sécurité du refroidissement restent actives.',
 'Manual or temperature-controlled cooling. Changes apply without restarting the ASIC.':'Ventilation manuelle ou selon la température. Les changements s’appliquent sans redémarrer l’ASIC.',
 'Used in manual mode. The percentage is a control setting; RPM shows measured speed.':'Utilisé en mode manuel. Le pourcentage est une consigne ; les RPM indiquent la vitesse mesurée.',
 'Default: manual 100%. Startup, stopped ASIC, missing temperature or heat safety force full cooling.':'Par défaut : manuel à 100 %. Le démarrage, l’ASIC arrêté, une température indisponible ou la sécurité thermique imposent la ventilation maximale.',
 'Minimum 30%. Safety forces 100% at ASIC 60 °C or regulator 80 °C until both cool by 5 °C. A lower thermal limit raises cooling earlier. Fan-stop protection remains active.':'Minimum 30 %. La sécurité impose 100 % dès 60 °C sur l’ASIC ou 80 °C sur le régulateur, jusqu’à ce que les deux soient refroidis de 5 °C. Une limite thermique plus basse augmente la ventilation plus tôt. La protection contre l’arrêt du ventilateur reste active.'
});

Object.assign(FR,{
 'Choose the identity sent to the pool. BDLMinerV2 follows the installed firmware version.':'Choisis l’identité envoyée à la pool. BDLMinerV2 suit la version du firmware installé.',
 'Choose the user agent below. Each pool has its own protocol and authority key.':'Choisis le user agent ci-dessous. Chaque pool utilise son propre protocole et sa propre clé d’autorité.',
 'Choose one of the available user agents':'Choisis l’un des user agents proposés'
});

Object.assign(FR,{
 'This cryptocurrency requires Stratum V1':'Cette cryptomonnaie nécessite Stratum V1',
 'Peercoin (PPC) · PoW':'Peercoin (PPC) · preuve de travail',
 'DGB requires a SHA-256d pool':'DGB nécessite une pool SHA-256d',
 'SHA-256d coin setup guide':'Guide des cryptomonnaies SHA-256d',
 'SHA-256d only. Choose main and backup pools for the same coin and use its payout address or pool account. The pool determines which chain is mined. BCH, BSV, XEC, DGB and PPC use Stratum V1. Peercoin uses proof of work only. DGB requires a SHA-256d port. Bitcoin market data is unavailable for other coins.':'SHA-256d uniquement. Choisis des pools principale et de secours pour la même monnaie, avec son adresse ou son compte de pool. La pool détermine la chaîne minée. BCH, BSV, XEC, DGB et PPC utilisent Stratum V1. Peercoin utilise uniquement la preuve de travail. DGB nécessite un port SHA-256d. Les données du marché Bitcoin sont indisponibles pour les autres monnaies.'
});

Object.assign(FR,{'Public market and explorer data are not provided for this coin. Pool job data is shown.':'Les données publiques du marché et de l’explorateur ne sont pas fournies pour cette monnaie. Les données des jobs de la pool sont affichées.'});
Object.assign(FR,{'Private remote monitoring':'Suivi privé à distance','Enable remote monitoring':'Activer le suivi à distance','Optional. Every five minutes, sends this miner’s readings under its payout address. Open the remote dashboard and enter the same address: nothing to copy. Anyone who knows the address can see them, as on a pool page.':'Facultatif. Envoie toutes les cinq minutes les mesures de ce mineur sous son adresse de paiement. Ouvrez le dashboard distant et saisissez la même adresse : rien à copier. Toute personne qui connaît l’adresse peut les voir, comme sur une page de pool.','Private access key (optional)':'Clé d’accès privée (facultative)','Leave empty to use the payout address. A key created before BDLMiner 2.5.4 keeps your readings in a secret space instead; use the same key on every miner.':'Laissez vide pour utiliser l’adresse de paiement. Une clé créée avant BDLMiner 2.5.4 garde plutôt vos mesures dans un espace secret ; utilisez la même clé sur chaque mineur.','Miner name':'Nom du mineur','The name shown in your remote dashboard. Empty uses the board model. Up to 48 bytes.':'Le nom affiché dans votre dashboard distant. Vide : modèle de la carte. Jusqu’à 48 octets.','Open my remote dashboard':'Ouvrir mon dashboard distant','The remote access key must contain 64 lowercase hexadecimal characters':'La clé d’accès distant doit contenir 64 caractères hexadécimaux minuscules','The remote miner name must be at most 48 bytes':'Le nom du mineur distant doit contenir au plus 48 octets','Set a payout address before enabling remote monitoring':'Indiquez une adresse de paiement avant d’activer le suivi à distance'});
