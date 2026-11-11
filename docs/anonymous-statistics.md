# Anonymous statistics

## Anonymous usage count

To know how many miners run each version, 5 minutes after boot and then once a day the
miner downloads a 2-byte file of its own version's release, `ping-<board>.txt`. GitHub
counts the downloads of each release file, and `python3 tools/miner_stats.py` turns these
counts into active miners per version and board. Nothing about the miner is sent (no
address, network or identifier: only GitHub sees the download, like any other). It is on
by default; turn it off with the *Anonymous usage count* setting (`count`).

## Anonymous hashrate sharing

To know the total hashrate of the BDLMiner miners, every 5 minutes (the first time 3
minutes after boot) the miner posts its firmware version, its board, its hashrate (the
average of the last minute, rounded to 3 digits), the selected cryptocurrency, the interval until its next report and,
on M5Stack boards, their exact model (such as `M5Stack Core2`) to the
[BDLMiner statistics service](https://github.com/bdlglua/BDLMiner-stats):

```json
{"v": "2.2.0", "coin": "BTC", "board": "m5stack-esp32", "hashrate": 1.02e6, "every": 300, "model": "M5Stack Core2", "pool": "public-pool.io"}
```

**Share my pool anonymously** (`sharepool`) adds the public pool group. It is
**on by default from 2.0.3**, requires hashrate sharing and applies without
a restart in *Settings → Network → Anonymous statistics*. Disable it there to
hide the group. A saved disabled choice is kept, including one saved by 2.0.2,
where this option was introduced with a disabled default. Only the connected pool
is reported, including the backup when active. Recognised public hosts are mapped
to a fixed group; every other host becomes *Other / private pool* on the device.
The groups are Public Pool, Nerdminers.org, NerdMiner.io, CKPool, OCEAN, Braiins Pool,
ViaBTC, F2Pool and AntPool; from 2.6.1 also BDL Pool, SoloPool.org,
KanoPool, AtlasPool, Solomining.de, Satoshi Radio, Noderunners, D-Central, Mining-Dutch,
Luxor, NiceHash, SECPOOL, SpiderPool, HMPool, Molepool and Peermining.
The configured hostname, port, Bitcoin address and worker are never sent. Older
versions, disabled pool sharing and disconnected pools appear as *Not shared*.
Public pool names link to a predefined official website, with its URL shown below
the name. Private/other and undisclosed groups have no link.

No Bitcoin address, worker, private hostname, name or identifier is sent. From
2.5.7, reports go directly to `https://api.bdl-pool.com:36018/report`, with
certificate and hostname verification. The collector adds the reports up live
and keeps a five-minute sample of the total for 35 days. It applies server-wide
admission limits behind the TCP relay and does not store client IP addresses.
The totals are public:
**<https://miner.bdl-pool.com/stats.html>** (total hashrate, miners, 7-day
chart, by board, version, cryptocurrency and pool). A paused miner sends
nothing. It is on by default; turn it off with the *Share my hashrate anonymously*
setting (`sharehr`), which applies at once.

Older firmware still uses a compatibility Cloudflare Worker, which forwards
reports to the server and remains subject to its own request allowance.
Updating to 2.5.7 removes that intermediary for anonymous reports and optional
[personal monitoring](remote-access.md).

[Back to the project overview](../README.md#privacy).
