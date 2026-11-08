# Consulter ses mineurs à distance

Ouvre **[Mes mineurs](https://miner.bdl-pool.com/remote/)** depuis
ton téléphone, même hors du Wi-Fi de la maison. Les cartes envoient directement
leurs mesures en HTTPS vérifié. Aucun PC ou Raspberry Pi à laisser allumé,
aucun port de la box à ouvrir. Chaque carte doit utiliser **BDLMiner 2.5.4 ou
ultérieur** (2.3.0 avec une clé privée, voir plus bas) et avoir accès à Internet.

## Configuration

1. Utilise la [dernière version stable](https://github.com/bdlglua/BDLMinerV2/releases/latest).
   Installe la mise à jour signée depuis le tableau de bord local ou téléverse
   le firmware correspondant à ta carte.
2. Sur chaque mineur, ouvre **Réglages → Réseau → Suivi privé à distance**, donne-lui
   un nom puis active le suivi. Clique sur **Enregistrer et redémarrer**.
   Rien à copier : le mineur utilise l'adresse de paiement de **Réglages → Pool**
   (la partie avant un éventuel `.worker`).
3. Sur **[Mes mineurs](https://miner.bdl-pool.com/remote/)**, saisis la même adresse
   de portefeuille puis clique sur **Voir mes mineurs**. Tous les mineurs qui
   utilisent cette adresse apparaissent, quel que soit leur nom de worker. Coche
   **Retenir cette adresse sur cet appareil** pour ouvrir la page directement la
   prochaine fois.

Le premier envoi est prévu environ une minute après le démarrage, puis toutes
les cinq minutes. Le Wi-Fi, la synchronisation de l'heure et la mémoire
disponible pour HTTPS peuvent retarder un envoi. Le journal du mineur affiche
les messages `[remote]`.

Le suivi à distance est **désactivé par défaut**. Il est indépendant des
statistiques anonymes : tu peux activer l'un et désactiver l'autre.

## Adresse ou clé privée

- **Adresse de paiement (par défaut).** Comme la page de ton adresse sur la pool,
  **toute personne qui connaît l'adresse peut voir ces mesures**. Une adresse de
  paiement est déjà publique sur la blockchain et sur la plupart des pools ; la
  page ajoute seulement les noms et les mesures des mineurs. Un espace par adresse
  ne peut pas être effacé depuis la page ; les mesures expirent sept jours après
  le dernier envoi.
- **Clé d'accès privée (facultative).** Pour garder les mesures secrètes, une clé
  de 64 caractères créée avant la 2.5.4 fonctionne toujours : colle-la dans
  **Clé d'accès privée** sur chaque mineur, puis saisis la clé à la place de
  l'adresse sur **Mes mineurs**. Elle permet de consulter et d'effacer l'espace ;
  elle ne peut pas être récupérée en cas de perte. Quand une clé est réglée, le
  mineur l'utilise à la place de l'adresse.

## Les mesures

Tu retrouves le hashrate, les températures disponibles, les shares acceptées et
rejetées, l'uptime, la meilleure difficulté, la puissance effective, la version,
la cryptomonnaie et l'état de connexion à la pool. L'historique couvre **sept
jours**, avec des vues de 1 h, 6 h, 24 h et 7 j. Les interruptions et les
redémarrages laissent des coupures sur le graphique.

« En ligne » signifie que le dernier envoi date de moins de **11 minutes** ; son
âge est indiqué. Une carte hors ligne conserve sa dernière mesure sur sa carte,
mais ne contribue plus au total en ligne. La page consulte uniquement les
données : les réglages, mises à jour, redémarrages et calibrages restent locaux.

## Confidentialité et arrêt

Le contenu des envois contient le nom choisi et un identifiant pseudonyme calculé
sur la carte. L'adresse de paiement, le worker, l'adresse MAC brute, l'adresse IP
locale, la clé Wi-Fi, le mot de passe web et l'adresse d'une pool privée n'y
figurent jamais : l'adresse ou la clé voyage seulement dans l'en-tête
d'autorisation. Les pools publiques reconnues sont représentées par un groupe
fixe. Ces données ne rejoignent pas les statistiques publiques.

La page garde une clé en mémoire seulement et ne l'enregistre jamais. Elle
n'enregistre l'adresse dans le navigateur que si **Retenir cette adresse** est
coché ; **Déconnexion** l'oublie. Rien n'est placé dans les liens.

Le service Cloudflare sépare les espaces par adresse ou par clé, dont seule
l'empreinte sert à nommer le stockage. L'opérateur d'hébergement peut accéder aux
mesures stockées ; elles ne sont pas chiffrées de bout en bout. Les adresses des
requêtes servent seulement aux limites temporaires en mémoire. Les détails
techniques et l'API sont décrits dans le [guide complet](remote-access.md).

Pour arrêter les envois, désactive le suivi sur chaque carte : le bouton
s'applique immédiatement. Les anciennes mesures expirent après sept jours. Avec
une clé, **Gérer mes données privées → Effacer les données et révoquer la clé**
les supprime tout de suite et bloque définitivement la clé. Jusqu'à
**32 mineurs** peuvent partager un espace.
