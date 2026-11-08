# Security policy

BDLMiner V2 runs on small ESP32 boards on home networks, installs its own updates
from GitHub and keeps a WiFi password, a payout address and optional notification
and MQTT credentials. Reports that help keep that safe are very welcome.

## Supported versions

| Version | Supported |
|---|---|
| Latest stable 2.x release | ✅ security fixes |
| older releases | ❌ — miners install the latest release by themselves (GitHub updates, setting `2`, the default) |

Fixes ship as a new release; the boards marked **experimental** (Bitaxe Gamma 601,
NerdNOS) receive the same fixes, on a best-effort basis for their hardware code.

## Reporting a vulnerability

**Please do not open a public issue, discussion or Discord message for a
vulnerability.** Report it privately through GitHub instead:

👉 **[Report a vulnerability](https://github.com/bdlglua/BDLMinerV2/security/advisories/new)**
(the repository's *Security* tab → *Report a vulnerability*).

Please include:

- the firmware version and board (`/api/stats` shows both), or the release file concerned;
- what an attacker needs (network position, the web password, physical access…);
- steps to reproduce, and what happens compared with what should;
- the impact as you understand it, and a proof of concept if you have one.

What you can expect:

- an acknowledgement within **7 days**;
- an assessment and a plan within **14 days**;
- a fixed release, then a published advisory crediting you (unless you prefer not to
  be named). Please keep the details private until the fixed release is out, or
  90 days, whichever comes first; we can agree on another date together.

## Scope

In scope:

- the firmware: the local web server and its API, settings, backup and restore,
  firmware upload, GitHub update discovery, download and **signature verification**,
  the firmware trial and rollback, Stratum V1, Stratum V2 with Noise, notifications
  (ntfy, webhook), MQTT / Home Assistant, the anonymous usage count and hashrate
  sharing;
- the release pipeline: release signing, `update.json`, the published files, and the
  web installer at <https://miner.bdl-pool.com/> (also served at
  <https://bdlglua.github.io/BDLMinerV2/>).

Out of scope:

- anything that requires physical or USB access to the board (it can always be
  re-flashed), or knowledge of the web password, except a way around it;
- denial of service by flooding the miner's small HTTP server from the local network;
- the pools, mempool.space, ntfy, Discord, Telegram or other third-party services;
- vulnerabilities in dependencies without an impact shown in BDLMiner (please report
  them upstream as well);
- social engineering, spam, and reports from automated scanners without a
  demonstrated issue.

## Security model and known limitations

These are deliberate design choices for a device on a home network, documented so
that reports can focus on what goes beyond them:

- **Only the miner's own pages can change it.** Settings, backups, firmware uploads,
  restarts and crash reports refuse a request sent by another site's page (its
  `Origin` header) or reaching the miner under a name of another site (its `Host`
  header: DNS rebinding). Since 1.9.6.
- **Passwords and the webhook URL never leave the miner** in the settings page: a set
  one reads as `••••••••`, kept as it is when the page saves. Backups hold them only
  when a web password is set. Since 1.9.6.
- **The web password is optional, and off by default.** Without one, anyone on the
  local network can change the settings (the payout address included), download
  backups (without the passwords) and crash reports, restart the
  miner and upload a firmware. Setting one protects all of that, through HTTP Basic
  authentication, which an attacker on the same network can still read (the dashboard
  is plain HTTP). Checking for an update never asks for it (one request every 30
  seconds at most); installing the latest **signed** GitHub release asks for it when
  one is set. Updating to 1.9.3 removes, once, a password set with an earlier version;
  one set afterwards is kept.
- **Read-only pages need no password**: the dashboard, `/api/stats`,
  `/api/history` and the live stream show the payout address, pool and network
  details to anyone on the local network. The log (`/api/log`), which also names
  networks, hosts and every error, uses the same host and origin checks as
  Settings and asks for the web password when one is set. `/api/stats` allows cross-origin reads so
  that one miner's dashboard can show the others.
- **Firmware from GitHub is trusted by its signature, not by TLS.** Every release
  firmware is signed with an ECDSA P-256 key kept outside the repository; the miner
  checks the signature against the public key built into it (`src/update_key.h`)
  before installing anything, then runs the new version on trial and returns to the
  previous one if it fails.
- **HTTPS connections verify the server's certificate and name** (ntfy and webhook
  notifications, market data, pool statistics, hashrate sharing, private monitoring,
  block confirmation) against about 40 public root authorities
  (`assets/tls/roots.pem`, packed into `src/tls_roots.h` by
  `tools/make_tls_roots.py`). A failure never falls back to an unverified connection,
  so a webhook URL or ntfy topic is not sent to an impostor; self-signed servers are
  refused (use an explicit `http://` URL for a webhook on a trusted local network).
  Verified HTTPS requires a synchronized clock. Since 2.3.3, after one minute
  without NTP, the miner can set the clock from a pool job. NTP corrects it when
  it responds. **Signed GitHub firmware updates** can retry without server
  verification if the certificate check fails or the clock is unset. The firmware
  signature is still checked before installation.
- **A firmware uploaded through the web page is not signature-checked**, so that
  people can install their own builds: only the web password, when set, protects it.
- **MQTT is plain TCP**; its user and password travel unencrypted to the broker.
- **Hashrate sharing is on by default**: every 5 minutes the version, the board (with
  the exact model on M5Stack boards) and the hashrate go to the BDLMiner statistics service. Pool sharing (`sharepool`, on by default from 2.0.3, with saved opt-outs preserved) adds only a fixed public pool group or `other`, never the configured host, port, Bitcoin address or worker (see the README). Public pool links are predefined website URLs, never report-provided addresses. Turn it
  off with the *Share my hashrate anonymously* setting.
- **Crash reports are memory images** and may contain secrets: downloading or erasing
  them requires the web password when one is set.
- **Private remote monitoring is opt-in.** From 2.3.0, a paired board can send
  selected readings over verified HTTPS to a key-isolated cloud space. Its
  256-bit access key allows reading and erasing that space and is masked as a
  secret setting. Personal names and pseudonymous miner IDs stay outside public
  aggregates. See [data, retention and revocation](docs/remote-access.md).

## Keeping your miner safe

- On a network you share, set a **web password** (setup portal or *Settings* → *Network*),
  different from your WiFi password.
- Keep the miner on your home or IoT network; **never forward its port 80** to the internet.
- Leave **GitHub updates** on, so security fixes reach the miner by themselves.
- Treat your **webhook URL**, **ntfy topic** and **backup files** as secrets: with a web
  password set, a backup contains every setting and password.
