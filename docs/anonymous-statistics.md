# Anonymous statistics

## Anonymous usage count

To know how many miners run each version, 5 minutes after boot and then once a day the
miner downloads a 2-byte file of its own version's release, `ping-<board>.txt`. GitHub
counts the downloads of each release file, and `python3 tools/miner_stats.py` turns these
counts into active miners per version and board. Nothing about the miner is sent (no
address, network or identifier: only GitHub sees the download, like any other). It is on
by default; turn it off with the *Anonymous usage count* setting (`count`).

## Anonymous hashrate sharing

To know the total hashrate of BDLMiner miners, every 5 minutes (the first time 3
minutes after boot) the miner posts its firmware version, board, hashrate (the
average of the last minute, rounded to 3 digits), selected cryptocurrency, report
interval and, on M5Stack boards, exact model to the Supabase public statistics
function:

```json
{"v":"2.6.3","coin":"BTC","board":"m5stickc-plus2","hashrate":1.02e6,"every":300,"model":"M5StickC Plus2","pool":"public-pool.io"}
```

The endpoint is
`https://cqbvuzubjkxfsztzfzso.supabase.co/functions/v1/bdl-public-stats/report`.
Reports are sent over verified HTTPS without a device identifier or Supabase
credential. The database stores only the public payload and five-minute
aggregate samples; it does not receive a payout address, worker, private pool
hostname, miner name or device ID. A global admission limit bounds report volume.

**Share my pool anonymously** (`sharepool`) adds the public pool group. It is
**on by default from 2.0.3**, requires hashrate sharing and applies without
a restart in *Settings → Network → Anonymous statistics*. Disable it there to
hide the group. A saved disabled choice is kept, including one saved by 2.0.2,
where this option was introduced with a disabled default. Only the connected pool
is reported, including the backup when active. Recognised public hosts are mapped
to a fixed group; every other host becomes *Other / private pool* on the device.
The groups include Public Pool, Nerdminers.org, NerdMiner.io, CKPool, OCEAN,
Braiins Pool, ViaBTC, F2Pool, AntPool, BDL Pool, SoloPool.org, KanoPool,
AtlasPool, Solomining.de, Satoshi Radio, Noderunners, D-Central, Mining-Dutch,
Luxor, NiceHash, SECPOOL, SpiderPool, HMPool, Molepool and Peermining.

The configured hostname, port, Bitcoin address and worker are never sent. Older
versions, disabled pool sharing and disconnected pools appear as *Not shared*.
Public pool names link to a predefined official website. Private/other and
undisclosed groups have no link.

The public totals are shown at
**<https://miner.bdl-pool.com/stats.html>** (total hashrate, reporting miners,
history and breakdowns by board, version, cryptocurrency and pool). A paused
miner sends nothing. Anonymous sharing is on by default; turn it off with the
*Share my hashrate anonymously* setting (`sharehr`), which applies at once.

Firmware older than 2.6.3 still posts to the retired
`api.bdl-pool.com:36018/report` endpoint and is not forwarded to Supabase.
Update it to 2.6.3 or newer to appear in the new public statistics.

## Private remote monitoring

Optional *My miners* monitoring is separate from the anonymous aggregate. It
sends chosen names, pseudonymous device IDs and selected telemetry to the
separate Supabase `bdl-remote-stats` function. See
[remote monitoring and privacy](remote-access.md).

[Back to the project overview](../README.md#privacy).
