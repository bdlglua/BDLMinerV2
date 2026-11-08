#!/usr/bin/env python3
"""Build the BDLMiner V2 Discord server: roles, categories, channels, forum tags,
a GitHub release webhook and the pinned welcome, rules, install and FAQ texts.

Once it has run, save it as a template in Discord (Server Settings > Server
Template > Generate): the discord.new/... link recreates the roles, channels and
permissions. Templates do not copy messages; this script posts them on your server.

  python3 create_server.py                  # show what would be created
  DISCORD_TOKEN=... DISCORD_GUILD_ID=... python3 create_server.py --apply
  ... --apply --clean                       # and remove the server's earlier channels
  ... --extras                              # complete an existing server (safe to repeat)

--extras finds the roles and channels by name and adds what is missing: security
settings, AutoMod rules against scams, the server icon, the welcome screen, the
onboarding question "which board?" and a permanent invite link.

Standard library only. The bot needs the Administrator permission on the server.
"""
import base64
import json
import os
import sys
import time
import urllib.error
import urllib.request

API = "https://discord.com/api/v10"
FLASHER = "https://miner.bdl-pool.com/"
REPO = "https://github.com/bdlglua/BDLMinerV2"

# Discord permission bits.
VIEW, SEND, REACT, HISTORY = 1 << 10, 1 << 11, 1 << 6, 1 << 16
EMBED, ATTACH, EXT_EMOJI, THREAD_SEND, PUBLIC_THREADS = 1 << 14, 1 << 15, 1 << 18, 1 << 38, 1 << 35
CONNECT, SPEAK, STREAM, NICK, COMMANDS = 1 << 20, 1 << 21, 1 << 9, 1 << 26, 1 << 31
ADMIN = 1 << 3
MODERATOR = ((1 << 1) | (1 << 2) | (1 << 13) | (1 << 27) | (1 << 34) | (1 << 40) | (1 << 7) | (1 << 17)
             | VIEW | SEND | REACT | HISTORY | EMBED | ATTACH | EXT_EMOJI | THREAD_SEND | PUBLIC_THREADS
             | CONNECT | SPEAK | STREAM | NICK | COMMANDS)
MEMBER = VIEW | SEND | REACT | HISTORY | EMBED | ATTACH | EXT_EMOJI | THREAD_SEND | PUBLIC_THREADS \
    | CONNECT | SPEAK | STREAM | NICK | COMMANDS

# Created from the highest to the lowest: each new role lands just above @everyone.
ROLES = [
    {"name": "👑 Admin", "color": 0xF7931A, "hoist": True, "permissions": ADMIN},
    {"name": "🛠️ Modérateur", "color": 0xE67E22, "hoist": True, "permissions": MODERATOR},
    {"name": "💻 Contributeur", "color": 0x9B59B6, "hoist": True, "permissions": MEMBER},
    {"name": "🧪 Testeur", "color": 0x3498DB, "hoist": True, "permissions": MEMBER, "mentionable": True},
    {"name": "🏆 Bloc trouvé", "color": 0xF1C40F, "hoist": True, "permissions": MEMBER},
    {"name": "⛏️ Mineur", "color": 0x2ECC71, "permissions": MEMBER},
    {"name": "🔔 Annonces", "color": 0x95A5A6, "permissions": 0, "mentionable": True},
    {"name": "📱 M5Stack", "color": 0x607D8B, "permissions": 0},
    {"name": "📺 T-Display", "color": 0x607D8B, "permissions": 0},
    {"name": "🟨 CYD", "color": 0x607D8B, "permissions": 0},
    {"name": "🧩 ESP32 sans écran", "color": 0x607D8B, "permissions": 0},
    {"name": "🟧 Bitaxe Gamma", "color": 0x607D8B, "permissions": 0},
    {"name": "🟦 NerdNOS", "color": 0x607D8B, "permissions": 0},
]

READ_ONLY = "read_only"  # everyone reads, only the team writes
STAFF = "staff"          # hidden from everyone but the team

BOARD_TAGS = [("M5StickC Plus2", "📱"), ("M5Stack", "📱"), ("T-Display", "📺"), ("CYD", "🟨"),
              ("ESP32 sans écran", "🧩"), ("Bitaxe Gamma", "🟧"), ("NerdNOS", "🟦"), ("Résolu", "✅")]

# type: 0 text, 2 voice, 4 category, 5 announcement, 15 forum.
CATEGORIES = [
    {"name": "📢 BIENVENUE", "access": READ_ONLY, "channels": [
        {"name": "👋-bienvenue", "topic": "Bienvenue sur le serveur de BDLMiner V2, le firmware de minage Bitcoin solo pour ESP32."},
        {"name": "📜-règles", "topic": "À lire avant de participer."},
        {"name": "📣-annonces", "type": 5, "topic": "Nouvelles versions et informations importantes."},
        {"name": "🔔-releases-github", "topic": "Publié automatiquement à chaque release GitHub.", "webhook": "GitHub"},
        {"name": "🎭-rôles", "topic": "Choisis ta carte et tes notifications."},
    ]},
    {"name": "📚 GUIDE", "access": READ_ONLY, "channels": [
        {"name": "📲-installation", "topic": f"Installer BDLMiner en 2 minutes depuis le navigateur : {FLASHER}"},
        {"name": "❓-faq", "topic": "Les questions qui reviennent souvent."},
        {"name": "⚙️-pools-et-réglages", "topic": "Quelle pool choisir, difficulté, notifications."},
        {"name": "🔗-liens", "topic": "GitHub, installateur web, documentation."},
    ]},
    {"name": "💬 COMMUNAUTÉ", "channels": [
        {"name": "💬-discussion", "topic": "Discussion générale autour de BDLMiner et du minage solo."},
        {"name": "📸-mes-miners", "topic": "Montre ton installation : photo, carte, hashrate."},
        {"name": "🏆-records-et-blocs", "topic": "Meilleures difficultés, shares 32/64 bits… et blocs trouvés !"},
        {"name": "🌍-english", "topic": "English-speaking members are welcome here."},
    ]},
    {"name": "🛠️ SUPPORT", "channels": [
        {"name": "🆘-aide", "type": 15, "tags": BOARD_TAGS,
         "topic": "Un sujet par problème. Indique ta carte, ta version et les lignes utiles du journal (/log)."},
        {"name": "🐛-bugs", "type": 15, "tags": BOARD_TAGS,
         "topic": "Un bug reproductible ? Version, carte, étapes, ce qui était attendu et ce qui s'est passé."},
        {"name": "💡-suggestions", "type": 15, "tags": [("Écran", "🖥️"), ("Tableau de bord", "📊"),
                                                       ("Notifications", "🔔"), ("Nouvelle carte", "🧩"),
                                                       ("Pools", "⛏️"), ("Accepté", "✅")],
         "topic": "Une idée pour BDLMiner ? Une suggestion par sujet."},
    ]},
    {"name": "⛏️ CARTES", "channels": [
        {"name": "📱-m5stack", "topic": "M5StickC Plus2, Core, Core2, CoreS3, AtomS3, Cardputer…"},
        {"name": "📺-t-display", "topic": "LilyGO T-Display et T-Display-S3."},
        {"name": "🟨-cyd", "topic": "ESP32-2432S028R « Cheap Yellow Display »."},
        {"name": "🧩-esp32-sans-écran", "topic": "ESP32, ESP32-S2, S3 et C3 sans écran."},
        {"name": "🟧-bitaxe-gamma", "topic": "Bitaxe Gamma 601 (BM1370), support expérimental."},
        {"name": "🟦-nerdnos", "topic": "NerdNOS (BM1397 + T-Display-S3), support expérimental : vos retours comptent !"},
    ]},
    {"name": "🔊 VOCAL", "channels": [
        {"name": "🎙️ Discussion", "type": 2},
        {"name": "🛠️ Entraide en direct", "type": 2},
    ]},
    {"name": "🔒 ÉQUIPE", "access": STAFF, "channels": [
        {"name": "🔒-équipe", "topic": "Coordination de l'équipe."},
        {"name": "🔒-modération", "topic": "Journal des actions de modération."},
    ]},
]

MESSAGES = {
    "👋-bienvenue": [
        "# ⛏️ Bienvenue sur le serveur BDLMiner V2 !\n\n"
        "**BDLMiner V2** est un firmware libre (GPL-3.0) qui transforme une petite carte ESP32 en "
        "**mineur Bitcoin solo** : si ton miner trouve un bloc, la récompense va **directement à ton adresse**. "
        "C'est une loterie, mais une vraie : chaque hash a sa chance.\n\n"
        "**Pour commencer**\n"
        "1. Lis 📜-règles\n"
        "2. Choisis ta carte dans 🎭-rôles\n"
        "3. Installe le firmware avec 📲-installation\n"
        "4. Une question ? Ouvre un sujet dans 🆘-aide\n\n"
        "**Cartes prises en charge** : M5StickC Plus2 et M5Stack, LilyGO T-Display et T-Display-S3, "
        "CYD (ESP32-2432S028R), ESP32 / S2 / S3 / C3 sans écran, et en expérimental le Bitaxe Gamma 601 "
        "et la NerdNOS.\n\n"
        f"🔗 {REPO}",
    ],
    "📜-règles": [
        "# 📜 Règles du serveur\n\n"
        "1. **Respect** : pas d'insultes, de harcèlement ni de discriminations.\n"
        "2. **Pas d'arnaques** : aucun « investissement », cloud mining, airdrop ou trading ici. "
        "Personne de l'équipe ne te contactera en message privé pour de l'argent.\n"
        "3. **Ne partage jamais** ta phrase de récupération (seed), tes clés privées, tes mots de passe "
        "ou l'URL de ton webhook. Ton **adresse Bitcoin** de paiement, elle, peut être partagée.\n"
        "4. **Support dans 🆘-aide** : un sujet par problème, avec ta carte, ta version et le journal (`/log`).\n"
        "5. **Pas de spam** ni de publicité sans l'accord de l'équipe.\n"
        "6. **Français ou anglais** (🌍-english pour l'anglais).\n"
        "7. Les décisions de l'équipe de modération s'appliquent.\n\n"
        "Bon minage ! ⛏️",
    ],
    "🎭-rôles": [
        "# 🎭 Choisis tes rôles\n\n"
        "**Ta carte** (pour l'entraide) : 📱 M5Stack · 📺 T-Display · 🟨 CYD · 🧩 ESP32 sans écran · "
        "🟧 Bitaxe Gamma · 🟦 NerdNOS\n"
        "**🔔 Annonces** : être mentionné à chaque nouvelle version.\n"
        "**🧪 Testeur** : essayer les versions expérimentales (Gamma, NerdNOS…) et faire des retours.\n"
        "**🏆 Bloc trouvé** : réservé à ceux qui ont trouvé un bloc avec BDLMiner. Preuve à l'appui ! 😄\n\n"
        "_Les rôles se choisissent dans **Salons et rôles** (Intégration du serveur) ou avec un bot de rôles "
        "par réaction, selon la configuration de l'équipe._",
    ],
    "📲-installation": [
        "# 📲 Installation (2 minutes, sans rien installer)\n\n"
        f"1. Ouvre l'installateur web dans **Chrome ou Edge** : {FLASHER}\n"
        "2. Branche ta carte en **USB** (un câble qui transmet les données), choisis ta carte, clique sur "
        "**Installer**.\n"
        "3. Sur ton téléphone, connecte-toi au WiFi **`BDLMinerV2-XXXX`**, mot de passe **`MineYourCoins`**. "
        "La page de configuration s'ouvre (sinon : `http://192.168.4.1`).\n"
        "4. Choisis ton WiFi, entre ton **adresse Bitcoin** (tu peux ajouter un nom de worker : "
        "`adresse.nom`), et si tu veux un **mot de passe web** (facultatif), puis enregistre.\n"
        "5. Le miner redémarre et mine. Son tableau de bord : `http://<IP du miner>/` ou "
        "`http://bdlminer-xxxx.local/`.\n\n"
        "**Boutons** : maintiens le bouton principal **au démarrage** pour rouvrir la configuration, "
        "**5 secondes** pour tout réinitialiser.\n"
        "**Mises à jour** : automatiques depuis GitHub, signées, avec une période d'essai : "
        "si une version plante, l'ancienne revient toute seule.",
    ],
    "❓-faq": [
        "# ❓ FAQ\n\n"
        "**Est-ce que je vais trouver un bloc ?** Probablement pas : à 1 MH/s, environ 1 chance sur "
        "20 milliards par an. C'est une loterie ; le tableau de bord affiche ta chance par jour et par an.\n\n"
        "**Pourquoi mon worker n'apparaît pas sur la pool / est marqué « Offline » ?** Beaucoup de pools "
        "imposent une difficulté de 1 ou plus, et n'affichent un worker que s'il a envoyé une share récemment. "
        "À 1 MH/s, une share à difficulté 1 arrive environ toutes les 70 minutes. Le miner tourne bien ; "
        "prends une pool qui accepte une faible difficulté (voir ⚙️-pools-et-réglages).\n\n"
        "**C'est quoi la « période d'essai » ?** Après une mise à jour, la nouvelle version doit miner 3 minutes. "
        "Si elle plante ou n'y arrive pas, le miner revient seul à l'ancienne version.\n\n"
        "**Comment recevoir les notifications sur Discord ?** Crée un webhook dans un salon (Paramètres du "
        "salon › Intégrations › Webhooks), colle l'URL dans Réglages › Notifications › URL du webhook, puis "
        "clique sur « Envoyer une notification de test ».\n\n"
        "**Ça consomme combien ?** Une carte ESP32 consomme environ 0,5 à 1 W.",
    ],
    "⚙️-pools-et-réglages": [
        "# ⚙️ Pools et réglages\n\n"
        "**Pools solo conseillées pour les petites cartes**\n"
        "• `pool.nerdminers.org:3333` : accepte une faible difficulté, ton worker reste visible.\n"
        "• `public-pool.io:3333` : difficulté minimale 1 (worker visible seulement après une share).\n"
        "• ckpool (`eusolo.ckpool.org:3333`) : difficulté 10 000, inadaptée à 1 MH/s (aucune share visible).\n\n"
        "**Stratum V2** : pris en charge (chiffré), avec la clé d'autorité de la pool.\n"
        "**Réglages utiles** : puissance de minage, pause au-dessus d'une température, mode nuit, "
        "alertes de hashrate, résumé quotidien. Les nombres et interrupteurs s'appliquent sans redémarrage.\n"
        "**Home Assistant** : MQTT avec découverte automatique (capteurs, interrupteur de minage, puissance).",
    ],
    "🔗-liens": [
        "# 🔗 Liens\n\n"
        f"• **Code source et releases** : {REPO}\n"
        f"• **Installateur web** : {FLASHER}\n"
        f"• **Dernière version** : {REPO}/releases/latest\n"
        f"• **Signaler un bug sur GitHub** : {REPO}/issues",
    ],
}


def say(text):
    print(text, flush=True)


class Discord:
    def __init__(self, token):
        self.token = token

    def call(self, method, path, body=None):
        data = json.dumps(body).encode() if body is not None else None
        for _ in range(8):
            req = urllib.request.Request(API + path, data=data, method=method, headers={
                "Authorization": "Bot " + self.token,
                "Content-Type": "application/json",
                "User-Agent": "DiscordBot (https://github.com/bdlglua/BDLMinerV2, 1.0)",
            })
            try:
                with urllib.request.urlopen(req, timeout=30) as res:
                    raw = res.read()
                    return json.loads(raw) if raw else None
            except urllib.error.HTTPError as e:
                payload = e.read().decode(errors="replace")
                if e.code == 429:  # rate limited: wait as told
                    time.sleep(float(json.loads(payload).get("retry_after", 1)) + 0.2)
                    continue
                raise RuntimeError(f"{method} {path}: HTTP {e.code} {payload}") from None
        raise RuntimeError(f"{method} {path}: still rate limited")


def overwrites(access, everyone, staff_roles):
    if access == READ_ONLY:
        return [{"id": everyone, "type": 0, "allow": str(VIEW | HISTORY | REACT), "deny": str(SEND | PUBLIC_THREADS)}] + \
               [{"id": r, "type": 0, "allow": str(SEND), "deny": "0"} for r in staff_roles]
    if access == STAFF:
        return [{"id": everyone, "type": 0, "allow": "0", "deny": str(VIEW)}] + \
               [{"id": r, "type": 0, "allow": str(VIEW | SEND | HISTORY), "deny": "0"} for r in staff_roles]
    return []


def plan():
    say("Rôles :")
    for role in ROLES:
        say(f"  {role['name']}")
    for cat in CATEGORIES:
        say(f"{cat['name']}" + (f"  [{cat['access']}]" if cat.get("access") else ""))
        for ch in cat["channels"]:
            kind = {0: "texte", 2: "vocal", 5: "annonces", 15: "forum"}[ch.get("type", 0)]
            extra = f", {len(ch['tags'])} étiquettes" if ch.get("tags") else ""
            extra += f", webhook {ch['webhook']}" if ch.get("webhook") else ""
            extra += f", {len(MESSAGES[ch['name']])} message(s) épinglé(s)" if ch["name"] in MESSAGES else ""
            say(f"  #{ch['name']} ({kind}{extra})")
    for name, texts in MESSAGES.items():
        for text in texts:
            assert len(text) <= 2000, f"{name}: {len(text)} caractères"
    say("\nTous les messages font moins de 2000 caractères. Ajoute --apply avec DISCORD_TOKEN et DISCORD_GUILD_ID pour créer le serveur.")


def apply(token, guild, clean):
    d = Discord(token)
    info = d.call("GET", f"/guilds/{guild}")
    community = "COMMUNITY" in info.get("features", [])
    say(f"Serveur : {info['name']}" + (" (Communauté)" if community else " (sans Communauté : forums en salons texte)"))
    existing = d.call("GET", f"/guilds/{guild}/channels")  # removed at the end with --clean
    everyone = guild
    d.call("PATCH", f"/guilds/{guild}/roles/{everyone}", {"permissions": str(MEMBER)})
    roles = {}
    for role in ROLES:
        made = d.call("POST", f"/guilds/{guild}/roles", {
            "name": role["name"], "color": role["color"], "hoist": role.get("hoist", False),
            "mentionable": role.get("mentionable", False), "permissions": str(role["permissions"])})
        roles[role["name"]] = made["id"]
        say(f"  rôle {role['name']}")
    staff = [roles["👑 Admin"], roles["🛠️ Modérateur"]]
    channels, hooks = {}, []
    for position, cat in enumerate(CATEGORIES):
        over = overwrites(cat.get("access"), everyone, staff)
        made = d.call("POST", f"/guilds/{guild}/channels", {
            "name": cat["name"], "type": 4, "position": position, "permission_overwrites": over})
        say(cat["name"])
        for ch in cat["channels"]:
            body = {"name": ch["name"], "type": ch.get("type", 0), "parent_id": made["id"],
                    "permission_overwrites": over}
            if ch.get("topic") and body["type"] != 2:
                body["topic"] = ch["topic"]
            if ch.get("tags"):
                body["available_tags"] = [{"name": n, "emoji_name": e} for n, e in ch["tags"]]
            try:
                created = d.call("POST", f"/guilds/{guild}/channels", body)
            except RuntimeError as e:
                # Announcement and forum channels need a Community server: fall back to text.
                if body["type"] not in (5, 15):
                    raise
                say(f"  #{ch['name']} : {'annonces' if body['type'] == 5 else 'forum'} refusé "
                    "(serveur non Communauté), créé en salon texte")
                body["type"] = 0
                body.pop("available_tags", None)
                created = d.call("POST", f"/guilds/{guild}/channels", body)
            channels[ch["name"]] = created["id"]
            say(f"  #{ch['name']}")
            if ch.get("webhook"):
                hook = d.call("POST", f"/channels/{created['id']}/webhooks", {"name": ch["webhook"]})
                hooks.append((ch["name"], f"https://discord.com/api/webhooks/{hook['id']}/{hook['token']}"))
    for name, texts in MESSAGES.items():
        for text in texts:
            msg = d.call("POST", f"/channels/{channels[name]}/messages", {"content": text})
            d.call("PUT", f"/channels/{channels[name]}/pins/{msg['id']}")
        say(f"  messages publiés et épinglés dans #{name}")
    # A Community server needs a rules and an updates channel: point them at the new
    # ones before the old ones can go. Joins are announced in the welcome channel.
    settings = {"system_channel_id": channels["👋-bienvenue"]}
    if community:
        settings.update(rules_channel_id=channels["📜-règles"], public_updates_channel_id=channels["🔒-modération"])
    d.call("PATCH", f"/guilds/{guild}", settings)
    say("  salons de règles, de mises à jour Discord et d'arrivées définis")
    if clean:
        for ch in existing:
            try:
                d.call("DELETE", f"/channels/{ch['id']}")
                say(f"  ancien salon supprimé : #{ch['name']}")
            except RuntimeError as e:
                say(f"  ancien salon gardé : #{ch['name']} ({e})")
    say("\nTerminé.")
    for name, url in hooks:
        say(f"\nWebhook de #{name} (garde-le privé) :\n  {url}/github\n"
            "  À coller dans GitHub › Settings › Webhooks › Add webhook › Payload URL, "
            "Content type application/json, événement « Releases ».")
    say("\nEnsuite : place le rôle 👑 Admin au-dessus du rôle du bot, puis Paramètres du serveur › "
        "Modèle de serveur › Générer un modèle pour obtenir le lien discord.new/…")


# Scams seen on crypto servers: fake support in DMs, airdrops, wallet "validation",
# seed phrase requests, guaranteed profits. Wildcards match inside words.
SCAM_KEYWORDS = [
    "*seed phrase*", "*phrase de récupération*", "*recovery phrase*", "*secret phrase*", "*private key*",
    "*clé privée*", "*airdrop*", "*giveaway*", "*dm me*", "*message me*", "*écris-moi en privé*",
    "*contacte-moi en mp*", "*en mp pour*", "*support ticket*", "*open a ticket*", "*ouvre un ticket*",
    "*wallet connect*", "*walletconnect*", "*validate your wallet*", "*synchronize your wallet*",
    "*rectify*", "*investment plan*", "*double your*", "*doubler tes*", "*guaranteed profit*",
    "*profit garanti*", "*free btc*", "*btc gratuit*", "*discord.gift*", "*dlscord*", "*steamcommunlty*",
]
WELCOME = [("📜-règles", "Les règles du serveur", "📜"), ("📲-installation", "Installer BDLMiner en 2 minutes", "📲"),
           ("❓-faq", "Les questions fréquentes", "❓"), ("🆘-aide", "Poser une question", "🆘"),
           ("📸-mes-miners", "Montrer son installation", "📸")]
BOARD_OPTIONS = [("📱 M5Stack", "M5StickC Plus2, Core, CoreS3, Atom…", "📱", "📱-m5stack"),
                 ("📺 T-Display", "T-Display et T-Display-S3", "📺", "📺-t-display"),
                 ("🟨 CYD", "ESP32-2432S028R", "🟨", "🟨-cyd"),
                 ("🧩 ESP32 sans écran", "ESP32, S2, S3, C3", "🧩", "🧩-esp32-sans-écran"),
                 ("🟧 Bitaxe Gamma", "Bitaxe Gamma 601", "🟧", "🟧-bitaxe-gamma"),
                 ("🟦 NerdNOS", "BM1397 + T-Display-S3", "🟦", "🟦-nerdnos")]
ICON = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "web", "icon-512.png")


def plan_extras():
    say("Réglages : vérification moyenne, filtre de contenu pour tous, notifications « mentions uniquement », icône ₿.")
    say(f"AutoMod : mots-clés d'arnaque ({len(SCAM_KEYWORDS)}), spam de mentions (5 max + mise en sourdine 10 min), contenu suspect.")
    say("Écran de bienvenue : " + ", ".join(n for n, _, _ in WELCOME))
    say("Intégration : « Quelle carte utilises-tu ? » → " + ", ".join(r for r, _, _, _ in BOARD_OPTIONS) +
        " ; « Que veux-tu recevoir ? » → 🔔 Annonces, 🧪 Testeur")
    say("Lien d'invitation permanent sur #👋-bienvenue.")


_snowflakes = 0


def snowflake():
    """A Discord-style id: milliseconds since 2015 in the high bits, a counter below."""
    global _snowflakes
    _snowflakes += 1
    return str(((int(time.time() * 1000) - 1420070400000) << 22) | (_snowflakes & 0xfff))


def fix_community(d, guild, existing, channels):
    """Channels made before the server became a Community server: the
    announcement channel and the forums fell back to text channels."""
    spec = {ch["name"]: ch for cat in CATEGORIES for ch in cat["channels"]}
    # First: Discord keeps its rules and updates channels as text channels, and
    # may have been given #📣-annonces when Community was turned on.
    d.call("PATCH", f"/guilds/{guild}", {"rules_channel_id": channels["📜-règles"],
                                          "public_updates_channel_id": channels["🔒-modération"]})
    say("Salons de règles et de mises à jour Discord : 📜-règles et 🔒-modération")
    for c in existing:
        want = spec.get(c["name"], {}).get("type", 0)
        if want == 5 and c["type"] == 0:
            try:
                d.call("PATCH", f"/channels/{c['id']}", {"type": 5})
                say(f"#{c['name']} : converti en salon d'annonces")
            except RuntimeError as e:
                say(f"#{c['name']} : conversion refusée, gardé en salon texte ({e})")
        elif want == 15 and c["type"] == 0:
            if c.get("last_message_id"):
                say(f"#{c['name']} : contient déjà des messages, gardé en salon texte")
                continue
            ch = spec[c["name"]]
            made = d.call("POST", f"/guilds/{guild}/channels", {
                "name": c["name"], "type": 15, "parent_id": c.get("parent_id"), "position": c.get("position", 0),
                "topic": ch.get("topic", ""), "permission_overwrites": c.get("permission_overwrites", []),
                "available_tags": [{"name": n, "emoji_name": e} for n, e in ch.get("tags", [])]})
            try:
                d.call("DELETE", f"/channels/{c['id']}")
            except RuntimeError as e:
                d.call("DELETE", f"/channels/{made['id']}")  # keep a single channel of that name
                say(f"#{c['name']} : remplacement refusé, gardé en salon texte ({e})")
                continue
            channels[c["name"]] = made["id"]
            say(f"#{c['name']} : remplacé par un forum")
    ours = set(spec) | {cat["name"] for cat in CATEGORIES}
    others = [c["name"] for c in existing if c["name"] not in ours]
    if others:
        say("Salons qui ne viennent pas du script (à supprimer à la main s'ils ne servent pas) : "
            + ", ".join("#" + n for n in others))


def extras(token, guild):
    d = Discord(token)
    info = d.call("GET", f"/guilds/{guild}")
    community = "COMMUNITY" in info.get("features", [])
    roles = {r["name"]: r["id"] for r in d.call("GET", f"/guilds/{guild}/roles")}
    existing = d.call("GET", f"/guilds/{guild}/channels")
    channels = {c["name"]: c["id"] for c in existing}
    missing = [n for n in ("👋-bienvenue", "🔒-modération", "📜-règles") if n not in channels]
    if missing or "👑 Admin" not in roles:
        sys.exit("Rôles ou salons du script introuvables : lance d'abord --apply.")
    staff = [roles["👑 Admin"], roles["🛠️ Modérateur"]]
    if community:
        fix_community(d, guild, existing, channels)

    settings = {"verification_level": 2, "explicit_content_filter": 2, "default_message_notifications": 1}
    with open(ICON, "rb") as f:
        settings["icon"] = "data:image/png;base64," + base64.b64encode(f.read()).decode()
    if community:
        settings["description"] = "Firmware de minage Bitcoin solo pour ESP32 : installation, aide, records."
    d.call("PATCH", f"/guilds/{guild}", settings)
    say("Réglages de sécurité, notifications et icône appliqués.")

    alert = {"type": 2, "metadata": {"channel_id": channels["🔒-modération"]}}
    block = {"type": 1, "metadata": {"custom_message":
             "Message bloqué : il ressemble à une arnaque courante. L'équipe ne te contactera jamais en privé."}}
    wanted = [
        {"name": "Anti-arnaque (mots-clés)", "event_type": 1, "trigger_type": 1,
         "trigger_metadata": {"keyword_filter": SCAM_KEYWORDS}, "actions": [block, alert]},
        {"name": "Spam de mentions", "event_type": 1, "trigger_type": 5,
         "trigger_metadata": {"mention_total_limit": 5, "mention_raid_protection_enabled": True},
         "actions": [block, alert, {"type": 3, "metadata": {"duration_seconds": 600}}]},
        {"name": "Contenu suspect", "event_type": 1, "trigger_type": 3, "actions": [block, alert]},
    ]
    have = {r["name"] for r in d.call("GET", f"/guilds/{guild}/auto-moderation/rules")}
    for rule in wanted:
        if rule["name"] in have:
            say(f"AutoMod « {rule['name']} » : déjà là")
            continue
        rule.update(enabled=True, exempt_roles=staff)
        try:
            d.call("POST", f"/guilds/{guild}/auto-moderation/rules", rule)
            say(f"AutoMod « {rule['name']} » : créée")
        except RuntimeError as e:
            say(f"AutoMod « {rule['name']} » : refusée ({e})")

    if not community:
        say("Écran de bienvenue et intégration : réservés aux serveurs Communauté, ignorés.")
    else:
        try:
            d.call("PATCH", f"/guilds/{guild}/welcome-screen", {
                "enabled": True, "description": "Le firmware de minage Bitcoin solo pour ESP32. Commence ici :",
                "welcome_channels": [{"channel_id": channels[n], "description": t, "emoji_name": e}
                                     for n, t, e in WELCOME if n in channels]})
            say("Écran de bienvenue configuré.")
        except RuntimeError as e:
            say(f"Écran de bienvenue refusé ({e})")
        prompts = [
            {"type": 0, "title": "Quelle carte utilises-tu ?", "single_select": False, "required": False,
             "in_onboarding": True, "options": [
                 {"title": title, "description": text, "emoji_name": emoji, "role_ids": [roles[title]],
                  "channel_ids": [channels[chan]] if chan in channels else []}
                 for title, text, emoji, chan in BOARD_OPTIONS if title in roles]},
            {"type": 0, "title": "Que veux-tu recevoir ?", "single_select": False, "required": False,
             "in_onboarding": True, "options": [
                 {"title": "🔔 Annonces", "description": "Être mentionné à chaque nouvelle version",
                  "emoji_name": "🔔", "role_ids": [roles["🔔 Annonces"]], "channel_ids": []},
                 {"title": "🧪 Testeur", "description": "Essayer les versions expérimentales",
                  "emoji_name": "🧪", "role_ids": [roles["🧪 Testeur"]], "channel_ids": []}]},
        ]
        # Discord wants an id on every prompt and option: keep the ones already
        # on the server (same titles), make new snowflakes for the rest.
        current = d.call("GET", f"/guilds/{guild}/onboarding")
        known = {p["title"]: p for p in current.get("prompts", [])}
        for prompt in prompts:
            old = known.get(prompt["title"], {})
            prompt["id"] = old.get("id") or snowflake()
            old_options = {o["title"]: o["id"] for o in old.get("options", [])}
            for option in prompt["options"]:
                option["id"] = old_options.get(option["title"]) or snowflake()
        # Discord wants every public channel either by default or offered by a question:
        # all of them by default, except the board channels the first question offers.
        offered = {chan for _, _, _, chan in BOARD_OPTIONS}
        defaults = [channels[ch["name"]] for cat in CATEGORIES if cat.get("access") != STAFF
                    for ch in cat["channels"] if ch["name"] not in offered and ch["name"] in channels]
        try:
            d.call("PUT", f"/guilds/{guild}/onboarding", {"prompts": prompts, "default_channel_ids": defaults,
                                                          "enabled": True, "mode": 0})
            say("Intégration configurée : les nouveaux choisissent leur carte en arrivant.")
        except RuntimeError as e:
            say(f"Intégration refusée ({e})")

    invites = d.call("GET", f"/guilds/{guild}/invites")
    permanent = next((i for i in invites if i.get("max_age") == 0 and i.get("max_uses") == 0), None)
    if not permanent:
        permanent = d.call("POST", f"/channels/{channels['👋-bienvenue']}/invites",
                           {"max_age": 0, "max_uses": 0, "unique": False})
    say(f"\nLien d'invitation permanent : https://discord.gg/{permanent['code']}")
    say("Pense aussi à la double authentification de ton compte (obligatoire pour modérer un serveur bien protégé).")


def main():
    args = sys.argv[1:]
    if "--apply" not in args and "--extras" not in args:
        plan()
        say("")
        plan_extras()
        return
    token, guild = os.environ.get("DISCORD_TOKEN"), os.environ.get("DISCORD_GUILD_ID")
    if not token or not guild:
        sys.exit("DISCORD_TOKEN et DISCORD_GUILD_ID sont nécessaires.")
    if "--apply" in args:
        apply(token, guild, "--clean" in args)
    if "--extras" in args:
        extras(token, guild)


if __name__ == "__main__":
    main()
