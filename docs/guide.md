# BDLMiner V2 user guide

Everything about installing, setting up and using BDLMiner V2. The [README](../README.md)
has the short version; [development.md](development.md) covers building, releases and
the code.

## Install a release (no build tools)

**Easiest:** open **https://miner.bdl-pool.com/flash.html** in Chrome or Edge on a
computer, plug the board in with a data USB cable, pick it in the list and click
*Install* (it flashes the latest release; the first install erases the board).

Each [release](https://github.com/bdlglua/BDLMinerV2/releases) also has two files per board
(`<board>` = an environment in the [board reference](hardware/supported-boards.md),
e.g. `m5stickc-plus2`):

* **`<board>-factory.bin`**: complete image for a first install, flashed at address
  **0x0**. In Chrome or Edge: https://espressif.github.io/esptool-js/ → *Connect* →
  address `0x0`, the file → *Program*. Or `esptool.py write_flash 0x0 <board>-factory.bin`.
  It erases the settings, WiFi and statistics.
* **`<board>-firmware.bin`**: update keeping settings and statistics: on the dashboard
  (*Settings* → *Firmware update*), or `esptool.py write_flash 0x10000 <board>-firmware.bin`.

`SHA256SUMS.txt` lists their checksums, and `<board>-firmware.bin.sig` the signature
that updates from GitHub check.

## Headless boards

Boards without a display report everything on the serial monitor and the
[web dashboard](#web-dashboard):

* **Setup**: the serial monitor prints the access point name and password to join
  (no QR code); then follow [First-time setup](#first-time-setup).
* **Button**: BOOT (GPIO0, GPIO9 on ESP32‑C3). Hold it during boot to reopen the
  portal, or for 5 s while running to erase all settings.
* **Serial port**: on ESP32‑S3 and ESP32‑C3 boards, the log comes out on the native
  USB port (boards with only that one, such as the Super Mini) and on UART0 as well
  (DevKits, through their USB‑serial chip), since 1.9.10.
* **Status LED**: steady while connecting to the pool, blinking once per second while
  mining. Only `esp32-headless` defines one by default (GPIO2); for the other boards,
  or if your LED is elsewhere, add `-DLED_PIN=<gpio>` to `build_flags`
  (plus `-DLED_ACTIVE_LOW` if it lights when the pin is low).

## First-time setup

1. On first boot the board opens a Wi‑Fi access point
   **`BDLMinerV2-XXXX`**, password **`MineYourCoins`** (boards with a display show a QR code to scan).
2. Open **http://192.168.4.1**, pick your Wi‑Fi network and fill in:
   * **Cryptocurrency**: the setup portal uses `0` BTC (default), `1` BCH,
     `2` BSV, `3` XEC, `4` DGB (SHA-256d) and `5` PPC (proof of work).
     The local Settings → Pool page has a named dropdown. Use pool endpoints
     for the same chain for both the primary and backup, and a compatible
     payout address or pool account. The pool serves the chain configured on
     its side. DGB requires a SHA-256d port, and PPC is mining only.
     See [coin and pool setup](mining-coins.md);
   * **Pool host / port**: `public-pool.io` / `21496` is the compiled default.
     For low-hashrate ESP32 feedback, see [Pools for ESP32](pools.md),
     or use your own pool. A pasted
     `stratum+tcp://host:port` is accepted: the prefix is dropped and the port taken;
   * **Backup pool host / port** (optional): used when the main pool fails, see
     [Backup pool](#backup-pool);
   * **Payout address or pool account**: an address for the selected coin, optionally
     followed by `.workerName`, or the username required by an account-based pool.
     BTC/DGB/PPC check their own Base58Check and Bech32/Bech32m prefixes; BCH/XEC
     check their own CashAddr prefixes and legacy encodings. BSV uses legacy
     Base58Check. Unrecognized addresses
     show a warning because some pools require account names;
   * **Pool password**: usually `x`;
   * **Suggested difficulty (V1)**: share difficulty requested from the pool,
     `0.0002` by default; the pool may enforce a higher minimum. V2 uses the target
     assigned to its mining channel;
   * **Main / backup protocol**: Stratum V1 by default and for all non-BTC
     profiles. BTC can also use native Stratum V2 with Noise and a pool
     authority key. See [Stratum V1 and Stratum V2](#stratum-v1-and-stratum-v2);
   * **User agent**: select `NerdMinerV2` or `BDLMinerV2/<version>` from the two-option
     menu. The selected identity is sent on both main and backup pools;
   * **Beep on a 32-bit share**: `1` (default) beeps when a hash with 32 leading zero
     bits (difficulty ≥ 1) is found, `0` stays silent. M5 devices use their buzzer or
     speaker (StickC Plus2: GPIO2); on other boards add `-DBUZZER_PIN=<gpio>` to
     `build_flags` for a passive buzzer;
   * **Timezone**: POSIX TZ string, defaults to Paris time
     `CET-1CEST,M3.5.0,M10.5.0/3` (`UTC0`, `EST5EDT,M3.2.0,M11.1.0`, `JST-9`…);
   * **Price currency**: currency for the BTC price, `USD` by default
     (`USD`, `EUR`, `GBP`, `CAD`, `CHF`, `AUD`, `JPY`);
   * **Language**: `0` English (default), `1` French, for the screen and the web dashboard;
   * **Web password** (optional): asked (user `admin`) before the
     [web dashboard](#web-dashboard) changes a setting or the firmware. Empty, the
     default: no password, anyone on your network can. Updating to 1.9.3 removes, once,
     a password set with an earlier version; one set afterwards is kept. The portal
     never shows the current password: leave the field empty to keep it, type a new
     one to replace it (the dashboard's settings remove it).
3. The board restarts and connects automatically. Open its local dashboard using
   the IP on the Network screen or serial monitor. If the chosen pool provides a
   public dashboard, search there for your Bitcoin address or worker as well.

If nothing is saved within **3 minutes**, the portal closes and the board restarts
(it reopens the portal on the next boot as long as Wi‑Fi is not configured).

To reopen the portal: **hold button A during boot** (2 s).

**The portal's password once the miner is set up.** `MineYourCoins` is the password
of a miner that has no payout address yet. A miner that has one opens its portal with
**a password of its own** (ten letters and digits), so that nobody nearby can
reconfigure it while your network is down: read it on the miner's screen (and its QR
code) when the portal opens, or beforehand in the dashboard under *Settings › Network*
(below *Fallback WiFi 1*); it is also printed on the USB serial console. Without it,
erasing everything (below) brings `MineYourCoins` back.
To erase everything (Wi‑Fi + settings): **hold A for 5 s** while running.

## Backup pool

When a backup pool is set and the main pool fails **3 times in a row** (unreachable,
or the connection drops before authorization), the miner switches to the backup with
the same address and password. After **30 minutes** on the backup it tries the main
pool again. With **Alternate main / backup pool every N hours** (web settings) both
pools get N hours in turn instead. While on the backup, the Network screen highlights it (orange pool name,
or a *Backup* label), the status reads `Mining (backup pool)` and the dashboard shows
*Pool (backup)*.

<a id="automatic-pool-identity-and-stratum-versions"></a>

## User agent

The setup portal and local dashboard offer exactly two identities:
**NerdMinerV2** and **BDLMinerV2/<installed version>** (for example,
`BDLMinerV2/2.1.1`). Select the one accepted by your pool, then save and restart.
There is no mode selector or text field. Both main/backup pools and Stratum V1/V2
use the selected identity. Refusals do not silently change it; the ordinary
reconnect/failover policy still applies.

The default is the native BDLMiner identity. A saved `NerdMinerV2` selection is
preserved across restarts and updates; native identities follow the firmware
version. Old custom/obsolete identities migrate to the current native option.
The obsolete `uaauto: false` key is retained only in saved records/backups for
compatibility with rollback firmware; it is not shown as a user setting.
User agent selection does not choose the cryptocurrency or override pool difficulty.

## Stratum V1 and Stratum V2

Select the protocol in **Settings → Pool → Main pool protocol** and, when used,
**Backup pool protocol**. The dropdowns are independent. In the boot setup portal,
`0` means V1 and `1` means V2. Save and restart to apply changes.

| Setting | Stratum V1 | Stratum V2 |
|---|---|---|
| Connection | Stratum JSON over TCP | Binary mining messages over encrypted Noise |
| Pool port | Use the pool's V1 port | Use the pool's V2 port |
| Authority key | Not used | Required; Base58Check or 32-byte hexadecimal key |
| Clock synchronization | Not required for a server certificate | Required to verify certificate validity |
| Worker identity | Configured address/worker, with the V1 password | Configured address/worker; the V1 password is not sent |
| Existing installations | Remain on V1 after updating | Explicitly select V2 or apply a V2 preset |

Native **Stratum V2** uses a Noise_NX handshake (secp256k1 EllSwift, BIP340
authority signature and ChaCha20-Poly1305), binary frames and an extended mining
channel. Each pool has its own protocol, port and authority key. V2 requires a
configured Base58Check authority key (or its 32-byte hexadecimal form) and a
synchronized clock; invalid signatures, expired certificates and unauthenticated
messages cause a reconnect. It never silently downgrades the selected endpoint
to V1. A separately configured backup pool can explicitly use V1 or V2.

Gamma and NerdNOS ASICs require version rolling. Since 1.9.15 the client explicitly
requests the V2 `REQUIRES_VERSION_ROLLING` feature: an accepting pool must allow
rolling in every job. A fixed-version requirement or a job without rolling is
rejected and counts towards configured backup failover. Software ESP32 miners
can still accept fixed-version work. No version bits are changed without the
pool's permission, and the selected V2 endpoint is never silently changed to V1.

In 1.9.14 an ASIC may remain connected at zero hashrate with **Pool needs version
rolling** because that feature was not requested. To resume on that version,
explicitly select V1 on a compatible pool/port, such as `public-pool.io:3333`,
keep the Bitcoin address and save/restart. The V2 port `23330` cannot be used
with the V1 setting. A retained history graph does not imply current hashing.

For [Public Pool](https://web.public-pool.io/) over V2, set `public-pool.io`, port
`23330`, Stratum V2 and the published authority key
`9c4zpyJ2ndm4e8sP2uNc1VNCGxYjqaxWS6wUCjk8zFj6njFquH6` (pasting
`stratum2+tcp://public-pool.io:23330/<key>` into the pool host fills all three).
Use your Bitcoin address (optionally `.worker`) as the worker identity and verify
current connection details on the pool's site if its endpoint/key changes.
The protocol and user agent do not override pool difficulty or increase the
device's hashing power. V2 channel identity uses the configured address/worker;
the V1 password setting is not sent as a V2 authentication field.

V2 uses the existing SHA-256d kernels and a unique extranonce per work unit.
Exact 256-bit share targets are retained rather than rounded to floating-point
difficulty. New previous-block notifications invalidate old work; target changes
are checked again before submission. Batched acknowledgments count only pending
shares, and duplicate/unknown acknowledgments do not increment counters.
Frames are limited to 16 KiB, with up to eight cached jobs and 32 KiB of cached
coinbase data. Unsupported/oversized jobs reconnect with a diagnostic. The
firmware does not implement Job Declaration, Template Distribution or a V2 pool
server; see the [Stratum V2 specification](https://stratumprotocol.org/).

Validation includes an independent Noise responder, official authority-key
vectors, certificate/AEAD/replay failure cases, truncated-frame checks under
sanitizers, independent coinbase/Merkle/header verification, and the production
network session over fragmented local sockets. A live Public Pool connection
and physical-device throughput still require hardware/network validation.

## Buttons and screens

| | M5StickC Plus2 | T‑Display / T‑Display‑S3 | CYD | headless |
|---|---|---|---|---|
| Next screen | A (front) | right button (GPIO35 / GPIO14) | BOOT | — |
| Brightness 100→60→30→10→off | B (side) or Power | BOOT button | — | — |
| Setup portal (hold at boot) | A | right button | BOOT | BOOT |
| Reset (5 s) | A | right button | BOOT | BOOT |

The CYD with two USB ports carries an ST7789 display instead of the ILI9341: since
1.9.10 the firmware recognises it at boot and inverts its colours.

Touch screens (CYD, M5Stack Core2 / CoreS3): a tap shows the next screen, a press of
one second changes the brightness.

Screens, in order:

1. **Mining**: live hashrate, temperature, effective power, accepted/rejected shares,
   best session difficulty and pool difficulty;
2. **Clock**: local time/date (NTP), BTC price, block height, hashrate and accepted shares;
3. **Bitcoin**: BTC price, next difficulty adjustment, pending transactions,
   network hashrate and fees (fast / 30 min / 1 h);
4. **Network**: block height, next halving, network difficulty, total hashes, pool, IP/RSSI;
5. **Device**: board, firmware, free RAM, payout address and active hashing engines;
6. **Hashrate**: graph over the last ~40 minutes (one point every 10 s), min / avg / max;
7. **Records**: best difficulty ever and this session, 32/64-bit counters,
   lifetime hashes and running time, with block totals and annual block odds in the footer;
8. **Blocks**: the block being mined, then the last 3 blocks found on the network, with
   their pool, age and your best share while each one was being mined.

When a locally verified hash meets the network target, a full-screen
**BLOCK CANDIDATE** alert with that work's height
stays until a button is pressed (that press only closes it); it lights the screen
even in night mode. Accepted shares flash `+1 SHARE` in the header; 32‑bit shares also light the red LED of M5
devices for a second (not in night mode). Check **Block candidates** in the
dashboard for the latest candidate's hash and pool submission status. A pool
accepting its share does not establish Bitcoin chain confirmation or payment.
Web settings add: 5 colour themes, automatic screen change every N seconds, rotation
following the accelerometer (M5 devices with an IMU: the two landscape ways), and a night
mode (screen off, no beep, from one hour to another; a button press lights it for 30 s).

Clock / Global data comes from the public [mempool.space](https://mempool.space) API
(refreshed every 10 minutes over HTTPS) and the time from `pool.ntp.org`.

## Physical display

The eight physical screens show **one big number per page**, readable from across
the room: live hashrate, local time, Bitcoin price, block height, IP address,
average hashrate (with its graph), best difficulty ever, and the block being mined.
Three secondary readings sit under the big value (side by side on landscape panels,
stacked on portrait ones), for example shares, best difficulty and temperature on the
mining page. A thin header shows a Bitcoin coin, the page name and the miner state:
`LIVE`, `OFFLINE`, `+1 SHARE`, or `COOLING`, `PAUSED`, `LIMITED`, `BATTERY` when
mining is limited. Dots at the bottom identify the page.

The big value uses the largest antialiased face that fits the panel, up to 80
pixels on 320x240 screens, with its unit beside it or, on narrow portrait panels,
under it. French and English, the five accent themes, buttons, rotation, night
mode and brightness settings are retained. Missing temperatures display `--`.

Only changed fields are redrawn. The renderer reuses one 8 KiB strip buffer
instead of allocating sprites for every field or a full-screen framebuffer. If this
allocation fails, clipped direct drawing is used. The graph redraws when a new
sample arrives, preserves elapsed time and gaps, and uses a zero baseline. Text uses
light antialiasing compressed in flash (4-bit coverage, run-length encoded); there is
no image, decoded bitmap, font-loading heap, filesystem or animation loop.

Color-screen fonts are drawn at their native size with 16 coverage levels;
the firmware does not resample the completed text or apply a blur pass.
Exported overview sheets use exact integer enlargement, preserving those
original pixels rather than adding another smoothing filter. The Gamma's
128×32 OLED is monochrome: it uses a native 6×8 pixel font, without per-pixel
gray levels for the color screens' antialiasing.

| Resolution | Boards |
|---|---|
| 240×135 | M5StickC Plus2 / Plus, Cardputer, T‑Display |
| 320×170 | T‑Display‑S3 |
| 320×240 | M5Stack Core / Core2 / Tough / CoreS3, CYD |

All sizes use the same renderer in `src/ui_view.cpp`; `src/ui.cpp` supplies actual
device readings. To test and generate previews using that renderer and its real
LovyanGFX fonts:

```sh
pio pkg install -e m5stickc-plus2
python3 tools/preview_display.py --check
pip install pillow
python3 tools/preview_display.py
```

Generate the embedded fonts with `python3 tools/pack_display_fonts.py`; their
source font and license are also included in that directory. These preparation
steps require Pillow; ordinary firmware builds do not. Overview sheets use a
filtered enlargement for inspection; panel PNGs retain their native resolution.
Smoothing cannot increase the number of physical pixels in the LCD.

The PNGs in `tools/preview/` use fictional demonstration readings; they are not
photographs or measured performance results. Native checks cover eight pages,
seven panel shapes, both languages, identical strip/direct output, unchanged-frame
caching, long text, missing samples, timer wrap and restoration after an update.

## Web dashboard

Once connected, the board serves a dashboard at **http://<board IP>/** or
**http://bdlminer-xxxx.local/** (mDNS; `xxxx` = last digits of the MAC address, shown in
the serial monitor):

The dashboard is live: the miner pushes its statistics every 2 seconds to up to two
open dashboards (Server-Sent Events on `/api/events`); any other page, or a browser
that loses the stream, falls back to asking every 10 seconds. An *Integrations* tile
shows MQTT (connected or not), the last notification sent and how each destination
answered, and where the pool statistics come from. The pool tile adds the time
connected since restart, reconnections and switches to the backup pool.

Settings that are numbers or switches (brightness, rotation, theme, language, screen
cycle, night mode, beep, alerts, maximum temperature, mining power, restart after no
share, battery saver, GitHub updates, usage count, hashrate sharing) apply at once,
without a restart;
the button then reads **Save**. Text settings, the pool, WiFi and MQTT still restart
the miner (**Save and restart**), since other tasks use them.

The dashboard can be added to a phone's home screen (web app manifest and icon):
it then opens full screen like an app. Browsers only offer a full *install* over
HTTPS; *Add to Home screen* works on the local address.

* **Dashboard**: hashrate per engine, shares, best difficulties, 32‑bit and 64‑bit shares and their
  average interval, block chance per day and per year, the 10 best shares ever with
  their date, what the pool sees (hashrate, best share, workers, from the pool's
  statistics API, found automatically; see below), market data,
  lifetime totals, the last blocks of the network (pool, transactions, your best share
  for each), a 24‑hour chart (kept across restarts: saved every 30 minutes and before
  each restart, the time the miner was off shown as a gap), the last 20 shares sent
  with the pool's answer (*Recent shares*), an *Update available* tile (see
  [Updates from GitHub](#updates-from-github)), the reason of the last restart, and night / heat /
  battery state. With **Other miners** set, a *Miners* section shows each of them and the
  total hashrate (each miner's `/api/stats` allows the pages of your local network to
  read it; a page of any other site gets the readings without the payout address, the
  WiFi name, the addresses or the crash report).
  The responsive interface separates mining, pool, Bitcoin network, device health and
  GitHub updates. It shows both the 60-second average and the live 10-second hashrate,
  configured/effective duty cycle and the reason for a limit or pause.
  The 64-bit share counter counts fully verified hashes with at least 64 leading
  zero bits, since restart. At 1 MH/s their expected interval is about 585,000 years;
  this is a statistical average. Also exposed as `/api/stats.over64bits`, the
  `bdlminer_shares_64bit_total` Prometheus metric and MQTT/Home Assistant `shares64`.
* **Settings** (asks for the web password, user `admin`, when one is set): every setting,
  grouped (pool, display, notifications, safety, network); *Save and restart*.
  **Backup**: *Download a backup* gives a JSON file with every setting (passwords
  included: keep it private) and the lifetime statistics (best difficulty, the 10 best
  shares, hashes, shares, blocks, running time); *Restore a backup* puts them back, on
  this miner after a factory reset or on another one (settings-only files of earlier
  versions are accepted too). Unsupported backup versions and incomplete or invalid
  statistics are rejected before anything is restored. Also at `/api/backup`
  (GET / POST). **Firmware update**
  takes the `firmware.bin` of this board (`.pio/build/<env>/firmware.bin`): no cable
  needed. M5Stack Core and CYD boards installed before 1.6.6 have a single firmware
  slot and need one USB reinstall with a 1.6.6 or later factory image first.
  Settings can be searched, passwords revealed locally, and mining power selected
  with **Full performance (100%)**, **Balanced (75%)** or **Quiet (50%)** presets.
  Presets only edit the form; **Save and restart** applies them. Existing user limits
  are kept on upgrade. Restart, backup and firmware controls are grouped in maintenance.
  Saves use two complete CRC-protected records and verify the written record before
  scheduling a restart. A failed save reports an error and retains the previous valid
  record. Export a JSON backup before downgrading: older firmware reads legacy keys.
* **Log**: the last few KB of the serial log, refreshed every 5 s, with filtering,
  pause/resume and a download of the original unfiltered text.
* **/api/stats** (JSON), **/api/log** (text) and **/metrics** (Prometheus).
  The log uses the same host and origin checks as Settings. Open it from the
  miner's own page and enter the web password if one is set.

All styles and scripts are served by the miner itself: no CDN, external font or
JavaScript framework is required. `tools/make_web.py` packs the sources in `web/`
into gzip assets in flash during every PlatformIO build. Content-versioned asset URLs
allow browser caching without stale assets after a firmware update. Hidden browser
tabs pause polling, and resumed tabs never overlap their refresh requests.

The dashboard uses `/api/stats?history=0` for live values and requests
`/api/history` only when its revision changes. This keeps the 288 history points
out of every three-second refresh. History aligns hashrate with sampled temperature
and effective power, includes observation coverage and can be downloaded as CSV.
Temperature averages use valid sensor samples; missing readings stay empty.
Use the 1 h, 6 h and 24 h controls to select a window locally; CSV exports that
window. The browser remembers the selected window. Charts include axes, a zero
baseline for hashrate, a configured-temperature-limit line and linked inspection
with the pointer, touch or arrow/Home/End keys. Times are relative to the latest
recorded point; the headline chart average uses the valid displayed samples.
Missing points keep their positions and break the lines. Empty and single-point
histories are shown without invented data.

Live updates retain metric and graph elements instead of rebuilding the dashboard
on every poll. Other miners refresh every 15 seconds; the local miner stays on its
three-second polling interval. Diagnostics load when their section is opened.

**Diagnostics & measurements**, below the main dashboard, shows time spent in worker batches, deliberate rest,
waiting for jobs and safety/OTA pauses, plus TLS handshake counts and durations.
These are elapsed times, not CPU-utilization measurements; the classic ESP32 can
continue hardware hashing during TLS. Profiling runs between batches, never inside
the nonce loop. `/api/diagnostics` exposes the counters.

**Ten-minute measurement** computes hashrate from actual total-hash and device-uptime
deltas, recording temperature and effective-power observations. It exports a JSON
report with before/after profiling counters and their differences, without passwords.
Keep the tab visible: hiding it or detecting a different boot cancels the measurement.
Sampling gaps over 15 seconds are marked, and temperatures are observed values rather
than a guarantee of the physical peak. No telemetry history is written to flash.

```sh
curl http://bdlminer-xxxx.local/api/stats
curl http://bdlminer-xxxx.local/metrics
```

`/api/stats` fields: hashrates in H/s (`engines`: `hw`, `sw`, and `fill` for the
idle-cycle software hash), fees in sat/vB, `market.updated_s` / `pool_view.updated_s` =
seconds since the last refresh (`-1` if none yet), `history.hashrate` up to 288
5‑minute averages, `top` = best shares (`d` difficulty, `t` Unix time or 0),
`throttle` = `full`, `half` (battery saver), `reduced` (power/thermal limit) or
`paused` (heat, lost supervision, sensor failure, a zero power setting, or OTA).
`power_percent` is the current mining duty cycle, `power_limit_percent` the configured
limit. `temp_c` = `null` without a usable sensor; `temperature_source` identifies the
session's sensor (`imu`, `chip`, or `unavailable`). The duty cycle is a limit on mining
activity, not a measurement of electrical power.
`hashrate_live` is the last 10-second average. `max_temp_c` is the configured thermal
threshold; `limitation_reason` distinguishes `full`, `user`, `user_pause`, `thermal`,
`hot`, `sensor`, `battery`, `supervision` and `ota`. `task_failures` is a diagnostic
bitmask (miner=1, network=2, market/updates=4, MQTT=8, notifications=16, web=32).
`github_update.supported` reports whether an OTA partition exists; USB-only boards
link to the release instead of offering an installation that cannot succeed.
The authenticated `POST /api/restart` preserves settings and lifetime statistics.
`boot_id` identifies a boot so a reconnect cannot silently mix measurements across
restarts. `history_revision` changes when a new history point is added;
`github_update.retry_in_s` is the current installation retry countdown.
`pool.pending_shares` counts submissions awaiting a reply. Only outstanding request
IDs can affect accepted/rejected counters; duplicate and unknown replies are ignored.
`pool.user_agent` reports the identity being sent, `pool.user_agent_auto` reports
the configured mode, and `pool.protocol` reports `Stratum V1` or `Stratum V2 (Noise)`.
These are also shown in the dashboard's Pool card.

> **Security:** the dashboard, `/api/stats` and the log have no authentication: anyone on
> your local network can read them (your BTC address is public on the pool anyway).
> Settings, backups and firmware updates are open too unless you set a web password; it
> is then sent with HTTP basic authentication, which is not encrypted on the local network.
> Changes are only accepted from the miner's own pages, opened by its IP address, a local
> name (`bdlminer-xxxx.local`, `.lan`, `.home`…) or the name your router gives it
> (`bdlminer-xxxx.fritz.box`). After three requests under a name that is not the miner's,
> that last form is refused for an hour (a web page trying names to reach the miner):
> open the miner by its IP address or `.local` name meanwhile.

## What the pool sees

The miner finds its pool's statistics API by itself, for any pool: it tries
ckpool's `/users/<address>` (also served as `/api/miner?address=<address>`, as on
nerdminer.io) and public-pool's `/api/client/<address>` on the pool's
host, on its parent domain (`eu.pool.example` → `pool.example`) and on the usual
ports (443, 40557; 3334 and 80 over plain HTTP for a pool on the local network), and
recognises the answer by its JSON. The address that answered is kept, across restarts,
and shown under the *Seen by the pool* tile; if it stops answering, the search starts
again. Most pools only publish an address once it has an accepted share, so a search
that found an API not knowing the address yet (HTTP 404) runs again after the miner's
next accepted share; any other fruitless search runs again every 6 hours. Each failed
attempt is logged with its cause. Meanwhile the tile estimates the hashrate from the
accepted shares. When a known API stops answering (an outage on the pool's side), the tile says so, shows the estimate, and the miner
asks again every 10 minutes.

public-pool.io serves its statistics API (port 40557) over **TLS 1.3 only**. Since
2.5.0 the firmware speaks TLS 1.2 and 1.3 and reads it itself; up to 2.4.2 (TLS 1.2
only) it logged `refused the TLS 1.2 handshake`. When the miner finds no statistics
API, whatever the cause, the dashboard asks
`https://<pool host>:40557/api/client/<address>` itself, from your browser (which
speaks TLS 1.3; the API allows any origin), at most every 10 minutes: the tile shows
the pool's figures marked *read by this browser*, or the estimate if that fails too.
Only the address is sent, not the worker name. Mining and shares are not affected.

A pool may list no worker at all for this miner: public-pool, for one, now imposes a
minimum difficulty of 1 (it ignores the suggested difficulty) and lists a worker only
if it sent a share in the last 10 minutes. At about 1 MH/s that is one share every 70
minutes, so the worker appears only briefly after each one, while the pool still
counts its shares and hourly hashrate. The tile explains it when it happens. Pools
that honour a low difficulty, such as pool.nerdminers.org, keep the worker listed. To
give it a name, enter the address as `address.name`.

## Notifications

Install the [ntfy](https://ntfy.sh) app, subscribe to a topic of your own (a long,
hard-to-guess name: anyone who knows it can read it) and enter it in the web settings
(*ntfy topic*; another ntfy server can be set). The miner then sends: block candidate,
new best difficulty ever (from difficulty 1, at most every 10 minutes), offline for
10 minutes and back online, too hot / cooled down, restarted after a crash or a
watchdog, a firmware rolled back after a failed trial, and optionally every 32‑bit share.

**Daily summary**: at the hour set in *Daily summary at* (20:00 by default, *Off* to
stop it), a notification gives the day's average hashrate, best share and record,
accepted and rejected shares and the time connected to the pool (or since the start,
when the miner started during the day).

**Alerts**: when the hashrate stays below the *% of normal* setting (50 % by default,
0 = off) for the *minutes* setting (15), with the pool connected and mining at full
power, the miner sends *Hashrate low*, then *Hashrate back to normal*. "Normal" is the
median of the last 24 hours at full power (at least 30 minutes of it). Over the same
span, 5 or more rejected shares making at least 10 % of the shares send *Shares rejected*.

**Webhook**: every notification is also posted as JSON to *Webhook URL* (`http://` or
`https://`; it holds a token, so it is a hidden field). The body carries `content`
(Discord), `text` (Telegram's `sendMessage`, with `chat_id` in the URL), and
`title`, `message`, `priority`, `tags`, `miner`, `firmware` for anything else
(Home Assistant, n8n, Gotify proxies...). ntfy and the webhook work alone or together.
For Discord, use *Channel settings › Integrations › Webhooks › Copy URL*. After saving,
**Send a test notification** (under the webhook field) shows the result of each
destination: OK, an HTTP code, no connection, or *certificate not trusted*.

For HTTPS notifications, the miner verifies the server's hostname and certificate
chain against the roots in `assets/tls/roots.pem`. Self-signed ntfy and webhook
certificates are refused (*certificate not trusted*). For a webhook on a trusted
local network, you can explicitly configure an `http://` URL. See the
[security model](../SECURITY.md#security-model-and-known-limitations) for the
separate transport exception for signed GitHub firmware updates.

Notifications are sent from the task that already fetches market data, so they cost
no memory of their own. Each one is tried up to 3 times, 30 seconds apart; while there
is not enough free memory for a TLS connection (54 KB, with a 24 KB block), no WiFi,
or no synchronized clock yet for certificate validation, it waits up to 10
minutes without using up those tries. Since 2.3.3, after one minute without
NTP, a pool job can supply the time. NTP corrects the clock when it responds. A job time
older than the firmware's own build is never used: it cannot be the real time, and would
make an expired certificate look valid.

## Private remote monitoring

Starting with 2.3.0, a paired miner can send selected readings over verified
HTTPS to a private, read-only dashboard. Pairing is off by default and works
independently of anonymous statistics. The remote dashboard keeps seven days
of history and works without a local hub or port forwarding. See the
[setup, data and revocation guide](remote-access.md).

## Home Assistant

Set an MQTT broker in the web settings (host, port, user, password). The miner announces
itself through MQTT discovery (device *BDLMiner bdlminer-xxxx*: hashrate, temperature,
shares, best difficulties, 32‑bit shares, blocks, uptime, pool status) and publishes
every 30 s on `bdlminer/<name>/state`, with `bdlminer/<name>/status` = `online` /
`offline`. `/metrics` serves Prometheus scrapers.

It can also be driven from Home Assistant: a *Mining* switch (off = power 0 %, on =
back to the previous power), *Power* (0–100 %) and *Brightness* (0–100 %) numbers and a
*Restart* button, on `bdlminer/<name>/set/<mining|power|brightness|restart>`. Power is
saved like the web setting; brightness works like the button.

## Anonymous pool statistics

From 2.0.2, **Settings → Network → Anonymous statistics → Share my pool anonymously**
adds the connected pool's public group to anonymous hashrate reports. Introduced
as default-off in 2.0.2, it is on by default from 2.0.3 for new devices and records
without the setting. Saved disabled choices are preserved. The setting applies
without a restart and can be turned off independently.
**Share my hashrate anonymously** must also be enabled. When a backup pool is
active, its group is used instead of the main pool's.

Only Public Pool, Nerdminers.org, NerdMiner.io, CKPool, OCEAN, Braiins Pool, ViaBTC,
F2Pool and AntPool are named. Other hosts become **Other / private pool** before
leaving the board. No configured hostname, port, Bitcoin address or worker is sent.
Older firmware, a disabled sharing option or a disconnected pool appears as
**Not shared**. The public tables show miner counts and combined hashrate per
group; they cannot diagnose an individual board or reveal its settings.
Public pool names include a link and visible URL to the pool's predefined official
website. Private/other and undisclosed groups have no link.

## Updates from GitHub

Thirty seconds after boot and then every hour (5 minutes after a failed check), the
miner looks at the latest release of
this repository. When it is newer than its own firmware, it installs it by itself (the
*Updates from GitHub* setting, `2` by default) and the screen shows the progress. At `1`
it only offers it: an *Update available* tile with an **Install** button on the dashboard
(and a notification); `0` turns the check off.
The dashboard always shows the GitHub update status, including connection errors,
disabled updates and checks waiting for WiFi or enough free memory. **Check now**
requests a fresh check without restarting the miner; it needs no password (installing
does, when one is set). New releases include a small
`update.json` discovery file, avoiding GitHub's shared unauthenticated API quota;
the releases API remains a fallback for older releases.
Failed installation downloads retry after 5 minutes, then 10, 20 and at most 30
minutes. The dashboard shows the countdown and **Retry now**. A refused signature
or a board without an OTA slot does not enter this automatic retry loop.
The downloaded firmware is only installed if its ECDSA P‑256 signature matches the release
key built into the firmware (`src/update_key.h`), so a modified file is refused. For that
reason **Install** only asks for the web password when one is set. Settings
and statistics are kept. Like web updates, it needs two firmware slots: every board has
them from 1.6.6 on. M5Stack Core and CYD boards installed before 1.6.6 had a single slot;
reinstall them once by USB (or the web flasher) with the 1.6.6 factory image to get two.
If an older firmware cannot reach GitHub to update itself, install the matching
`<board>-firmware.bin` once through *Settings → Firmware update* (or by USB on a
board without two app slots). Future checks then use the updated discovery code.

**Trial and automatic rollback** (from 1.6.8): a newly installed firmware, from GitHub
or the web page, starts on trial. It is kept once it has run 3 minutes connected to
the pool and hashing (shares are too rare at difficulty 1 to be the proof). If it
crashes or hangs before that, or does not get there within 20 minutes (40 without
WiFi), the previous firmware comes back by itself and a notification is sent; rolled
back twice, a version is no longer installed automatically (the update tile says why
if it is offered). Any restart during the trial brings the previous firmware back,
but one caused by a power cut, the reset button or a USB serial connection is not
held against the new version: it is installed again, without a notification (after
three such interruptions, they count as failures). The *Device* tile shows *Firmware on trial* meanwhile.

## Crash reports

After a crash the panic handler keeps a core dump in flash. At the next start the
log and a *Last crash* tile on the dashboard show its summary (task, address,
the exception by name, backtrace; an `abort()`, failed assert or watchdog is named as
such rather than by the "StoreProhibited at 0x00000000" ESP-IDF uses to stop); **Download report** saves the full ELF for
`esp-coredump` / GDB with the matching `firmware.elf`, and **Erase** clears it
(both ask for the web password when one is set). The miner also keeps the summaries of its last 5
crashes, with the version that crashed and the date, so a report replaced by a later
crash still leaves a trace; **Erase** clears this history too.

## Safety, power and networks

* **Mining power on USB and battery**: the web setting *Mining power* limits the
  mining duty cycle to 0–100 %, default **100 %** for new configurations. At 100 % the miners run at full
  speed; 0 pauses them. Lower settings trade hashrate for idle time without changing
  the clock timing of the hardware SHA loops. The limit also applies on USB.
  Previously saved limits are preserved. To restore full requested speed after an
  earlier 75% default, choose **Full performance** and save. Around 1 MH/s is a
  hardware-dependent target on compatible ESP32 revision 3 boards, not a guaranteed
  sustained rate: temperature, power, pool availability and sensor health still apply.
* **Too hot**: *Pause mining above this temperature* defaults to **75 °C** on the
  boards that read the chip's own sensor (T-Display, T-Display-S3, Cheap Yellow
  Display, boards without a screen) and **65 °C** on M5Stack devices, whose IMU can
  read the board next to the battery, and on ASIC boards (0 = off). Since 2.5.3 a
  chip-sensor board still at the former 65 °C default is raised once to 75 °C:
  its die runs warmer than the board, and at 65 °C an ESP32-S3 mining at full speed
  stayed on the 50 % step (176 instead of 351 kH/s on a T-Display-S3). Any other
  saved threshold, including 0, is preserved; check this setting after upgrading. Within 10 °C of the threshold, mining is
  limited to 50 %; within 5 °C, to 25 %. At the threshold, miners pause until at
  least 5 °C below. These limits never increase a lower user-selected duty cycle.
  IMU and chip sensors measure different components: choose a threshold for the
  particular board and sensor, and verify it on hardware.
* **Unavailable temperature**: with thermal protection enabled, a board that has
  never produced a usable temperature is limited to 25 %. Losing a previously
  working sensor pauses mining; an unreadable sample cannot clear a heat pause.
  Once selected, the physical sensor stays the same until reboot, so a failed hot
  IMU cannot be replaced silently by a cooler chip reading.
  HTTP runs in its own task so slow requests do not block the screen or sensor reads.
  Each worker also stops at its next batch boundary if supervision has not refreshed
  its permission for over 3 seconds, including during a slow HTTP request. It resumes
  after fresh supervision allows it. This guard also applies when the thermal
  threshold is disabled. Firmware updates pause all miners and only one web/GitHub
  installation can own the flash writer at a time.
* **Battery saver**: on battery the screen dims to 20 % and the miners work at most
  half of the time, or less if the power/thermal limit requires it.
  Boards without a power chip (StickC Plus2) are judged by the battery voltage
  (below ~4.05 V: on battery), so it can also kick in while a low battery charges.
* **Watchdogs**: the board restarts after 30 minutes without a pool connection, and,
  if set, after N hours without an accepted share. The reason is shown after the
  restart (dashboard, log, notification).
* **Statistics on flash**: lifetime totals, best difficulty and the 10 best shares are
  saved every 10 minutes (at once for a block, within a minute for a new best) and just
  before every restart the miner makes itself, as one record written in turn to two
  slots, each with a sequence number and a CRC (the scheme of NerdMiner Modded): at boot
  the newest valid one is used, so a power cut during a save only loses that save.
  Statistics survive a factory reset.
* **Fallback WiFi**: two more networks can be set in the web settings; they are tried
  when the portal's network cannot be joined, at boot and after a minute of lost WiFi.

## Serial monitor (115200 baud)

```
[cfg] address type P2WPKH
[miner] hardware SHA loop: fast
[miner] hardware SHA: self-test OK
[miner] miner0 engine=hw core=1
[miner] miner1 engine=sw core=0
[pool] job 265612f height=972641 clean=1 diff=1
[web] dashboard: http://192.168.1.20/ or http://bdlminer-a1b2.local/
[market] 5/5 ok | 1 BTC = 72543 EUR | fees 3/2/1 | net 1.04e+21 H/s | retarget +3.50% in 1067
[stats] 48.12 kH/s | shares 0 ok / 0 rejected | best 0.0213 (ever 0.0213) | blocks 0 | Mining
```

## Pool shares and local records

| Counter | What it measures |
|---|---|
| Accepted shares | Submitted proofs that the pool has acknowledged as accepted. |
| Rejected shares | Submissions the pool has explicitly rejected. Check the log for its reason. |
| Pending shares | Submissions still waiting for a pool response. |
| 32-bit shares | Locally verified hashes at difficulty 1 or higher. |
| 64-bit shares | Locally verified hashes with at least 64 leading zero bits. |
| Best difficulty | The strongest locally verified hash in this session or across saved records. |
| Block candidates | Locally verified hashes meeting the work's network target; not confirmed blocks or payouts. |

Candidates use reserved priority storage, separate from ordinary queued shares.
The dashboard shows the latest candidate, its original height, full
block identifier and submission status: queued, submitted, share accepted,
share rejected, unanswered, stale or candidate queue full. Recent records are
bounded in RAM; the latest hash/height/state is also saved in alternating,
CRC-checked NVS extension slots without changing the existing statistics format.
After reboot it is labelled as a previous-session proof and is never resubmitted
or announced as newly found. Interrupted queued/submitted work becomes stale
or unanswered. Statistics backup files keep their original layout; candidate
evidence stays in device-local NVS. An unanswered
submission has an unknown result, including if the connection closed after send.
For a local/saved candidate, the background task queries mempool.space using
certificate/hostname-verified HTTPS, validates the reported height and reads the
chain tip for confirmations. The dashboard identifies this external observation,
its age and any error; it is not an ESP32 full-chain validation or a payout
guarantee. A 404 means not yet indexed. Errors leave current chain acceptance
unknown, and a later reorganization can reverse an earlier observation. There
are no explorer requests without a candidate; retries back off automatically.

For API compatibility, `blocks` and `lifetime.blocks` keep their existing field
names and represent local candidates. The optional `block_candidate` object in
`/api/stats` has `height`, `hash`, `status`, `previous_session`, `chain_source`,
`confirmations`, `chain_checked_s` and `chain_confirmed` (true/false for a valid
explorer observation, null when unknown or the latest check failed). Optional
`chain_not_found` and `chain_error` describe an unindexed hash or failed check.
Prometheus exposes `bdlminer_block_candidates_total`; the previous
`bdlminer_blocks_found_total` remains an alias for existing dashboards.

An accepted-share counter at zero does not by itself mean hashing has stopped.
Pool acceptance depends on the assigned difficulty, a valid connection and a
reply. Local best-difficulty and 32/64-bit records are independent of that reply.
The session counters restart at boot; the Lifetime card keeps saved accepted shares.

The approximate average time to find a share is
`difficulty × 2^32 / hashrate_in_H_per_second`. At **1 MH/s**, examples are:

| Assigned difficulty | Average time per share |
|---|---|
| `0.001` | 4.3 seconds |
| `1` | About 1 hour 12 minutes |
| `65536` | About 8.9 years |

These are averages, not deadlines or claims about a particular pool's current
settings. Check **pool diff** in the dashboard. A pool intended for ASIC miners
can assign a difficulty far beyond an ESP32's rate. Changing the user agent or
switching from V1 to V2 does not make hashes stronger or force a lower share difficulty.

## Good to know

* The pool controls the assigned share difficulty. A difficulty of **1** means
  about 4.3 billion hashes per share on average. The locally computed best
  difficulty can change before the pool accepts a share.
* The all-time best difficulty is saved to flash (at most once per minute), the lifetime
  totals (hashes, accepted shares, blocks) every 10 minutes, and a found block at once.
  A factory reset keeps them.
* ESP32‑C6/H2: no build yet (Arduino-ESP32 3.x supports them since 2.5.0, but no
  environment is defined or tested).
