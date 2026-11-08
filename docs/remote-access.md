# Remote monitoring

[Français](remote-access.fr.md)

View your miners from a phone or another network at
**[My miners](https://miner.bdl-pool.com/remote.html)**.
The boards send readings directly over verified HTTPS: no computer, Raspberry Pi,
VPN, port forwarding or local hub needs to stay running. The miners need Wi-Fi
with Internet access and **BDLMiner 2.5.4 or newer** (2.3.0 with a private key,
see below).

## Set up

1. Use the [latest stable release](https://github.com/bdlglua/BDLMinerV2/releases/latest).
   Install the signed update from the local dashboard or upload the matching
   board firmware.
2. Open each miner's local page. In **Settings → Network → Private remote monitoring**,
   choose a name such as `M5 bureau` or `Gamma atelier` and enable monitoring.
   **Save and restart** once. Nothing to copy: the miner uses the payout address
   set in **Settings → Pool** (the part before any `.worker`).
3. Open **[My miners](https://miner.bdl-pool.com/remote.html)**, enter the same wallet
   address and choose **View my miners**. Every miner using that address appears,
   whatever its worker name. Tick **Remember this address on this device** to
   open the page directly next time.

The first report is due about one minute after boot; later reports arrive every
five minutes. A missing clock synchronization, Wi-Fi connection or TLS memory
delays an attempt. See the miner's log for `[remote]` messages.

Remote monitoring is optional and disabled by default. It works independently of
anonymous hashrate/pool sharing: turning one service off does not change the other.

## Address or private key

- **Payout address (default).** Like your pool's address page, **anyone who knows
  the address can see these readings**. A payout address is already public on the
  blockchain and on most pools; the page only adds the miners' names and readings.
  An address space cannot be erased through the page; readings expire after seven
  days once miners stop reporting.
- **Private access key (optional).** To keep the readings secret, a 64-character
  key created before 2.5.4 still works: paste it in **Private access key** on every
  miner, and type the key instead of the address on **My miners**. Anyone with the
  key can view and erase that space; the service cannot recover a lost key. When a
  key is set, the miner uses it instead of the address.

## What you can see

Each card shows the miner's chosen name, board, firmware, cryptocurrency,
hashrate, temperature when available, accepted/rejected shares, uptime, best
difficulty, effective mining power and pool connection/public group. Paused
miners send a zero rate; disconnected pools are shown separately.

Readings and chart history are kept for **seven days**. History windows are one,
six or 24 hours and seven days. Missing reports and changed boot IDs make chart
gaps. A miner is considered online while its last report is at most **11 minutes**
old; the age is shown on its card. An offline miner's last known rate is retained
on its card and excluded from the total online rate. This is periodic telemetry,
not a continuous connection or a confirmation of pool payment/block discovery.

The viewer has no commands to change settings, restart, calibrate or update a
miner. Those operations stay on the local miner page.

## Privacy and removal

- Report bodies contain a pseudonymous device ID (derived from the address or key
  and the board) and the chosen name. They never contain the payout address,
  worker, raw MAC, LAN address, Wi-Fi credentials, web password or configured
  private pool host. The address or key only travels in the authorization header.
  A recognized public pool is represented by a fixed group; other pools are `other`.
- The page keeps a key in memory only and never stores it. It stores the address
  in this browser only when **Remember this address** is ticked; **Sign out**
  forgets it. Nothing is put in links, analytics or the public statistics.
- On the miner, a key uses the existing secret-setting storage and masking. As with
  other secrets, protected backups can contain it. Naming a miner or adding a key
  requires a restart; the enable/disable switch applies immediately.
- Reports use CA and hostname verification, with no insecure fallback and no
  redirects. A failed report waits for the next attempt; it does not restart the
  miner or stop mining. The public statistics retain their existing behavior.
- The Cloudflare-hosted service isolates each address or key in its own Durable
  Object named by a SHA-256 digest. The hosting operator can access stored
  readings; this is not end-to-end encryption. Request addresses are used only for
  temporary in-memory rate limits.
- To stop sending, disable monitoring on each miner. Existing readings expire
  after seven days. With a key, **Manage my private data → Erase data and revoke
  key** removes them immediately and permanently revokes the key.

A space supports **32 miners**. The backend accepts at most one report per miner
per minute; the firmware normally sends one every five minutes. A temporary
admission limit also bounds requests from one Internet address.

## API

On `https://bdlminer-stats.bdlminer-bot.workers.dev`, with
`Authorization: Address <payout-address>` or `Authorization: Bearer <private-key>`:

- `GET /api/remote`: your latest readings and their age.
- `GET /api/remote/history?miner=<id>&hours=24`: up to about 480 chart buckets;
  `hours` accepts `1`, `6`, `24` or `168`.
- `DELETE /api/remote` (key only): erase the space and permanently revoke its key.

These endpoints require authorization even when public statistics are open and use
`Cache-Control: no-store`. Browser reads are allowed only from the GitHub Pages
viewer origin (and the legacy service origin); no credentials are sent
automatically. `POST /api/remote/report` is used only by miners and is strictly
separated from the anonymous `/report` endpoint.
