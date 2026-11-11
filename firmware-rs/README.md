# BDLMiner V2 in Rust

A rewrite of the BDLMiner V2 firmware (`src/`, C++/Arduino) in Rust. The C++
firmware remains the released one until this build has been validated on
hardware: see [Status](#status).

| Crate | What it holds | Runs on |
|---|---|---|
| [`core`](core) `bdlminer-core` | Port of `src/core`: SHA-256d kernel with midstate and early rejection, header/merkle building, difficulty and targets, Stratum V1 state machine, Stratum V2 extended channel and Noise_NX (secp256k1 EllSwift + ChaCha20-Poly1305), payout addresses of the six SHA-256d coins, BM1370/BM1397 wire protocols, thermal/fan/safety policies, release versions, image markers | anywhere |
| [`app`](app) `bdlminer-app` | Port of the hardware-independent firmware: settings table, validation and the two-record store, shared miner state (job board, share queues, block candidates, statistics, history, persistence), pool task with failover/rotation, supervision and alerts, notifications, the dashboard's JSON API and pages (`web/`) | anywhere with `std` |
| [`host`](host) `bdlminer-host` | The firmware on a PC: software SHA-256d threads, pool sessions, the local dashboard | Linux, macOS, Windows |
| [`esp`](esp) `bdlminer-esp` | The ESP32 firmware: NVS, WiFi with setup portal, SNTP, pinned mining threads, ESP-IDF HTTP server, HTTPS notifications | ESP32, ESP32-S3, ESP32-C3 |

Everything portable is in `core` and `app` and is tested on a PC; `esp` only
binds ESP-IDF services to the traits of `app::platform`.

## Compatibility with the C++ firmware

- Settings: same keys, order, limits, validation messages and stored record
  (`bdlminer`/`cfgA`/`cfgB`: header + JSON, CRC-32). An update keeps the
  configuration.
- Statistics and block candidate proofs: same record layouts (`stats`/`statA`,
  `statB`, `blockA`, `blockB`). The 24 h hashrate history is now kept in NVS
  (`stats`/`hist`) instead of the `spiffs` partition, so it starts empty after
  the switch.
- WiFi: the network saved by the C++ portal (in the WiFi driver's NVS) is used.
- Dashboard: the same pages and scripts from `web/`, the same JSON API
  (`/api/stats`, `/api/history`, `/api/settings`, `/api/backup`, `/metrics`...),
  so the hub and other miners' dashboards keep working.
- Stratum: identical messages, user agents, failover and timeouts.

## Build and test (PC)

```bash
cd firmware-rs
cargo test
cargo clippy --all-targets -- -D warnings
```

The tests include the C++ host tests' vectors (SHA-256d, block 125552 and
Peercoin genesis headers, Stratum edge cases, BIP173/BIP350/CashAddr/DigiByte/
Peercoin addresses, BM1370/BM1397 wire vectors, thermal and fan policies,
Stratum V2 frames) and end-to-end runs of the miner against local Stratum V1
and Stratum V2 (Noise) pools that re-verify every submitted share.

The Noise implementation can be checked against another initiator, such as
a small driver around the C++ `sv2::Noise` (protocol in the example's header):

```bash
cargo run --example noise_interop -p bdlminer-core -- <initiator binary>
```

## Run on a PC

```bash
cargo run --release -p bdlminer-host -- --user <payout address>.<worker> --pool public-pool.io:21496
```

Then open <http://127.0.0.1:8080/>. Options: `--threads N`, `--http ADDR:PORT`,
`--data DIR` (settings and statistics), `--difficulty D`, `--coin BCH`,
`--sv2-key KEY` (Stratum V2).

## Build for an ESP32

Install the Xtensa toolchain with [espup](https://github.com/esp-rs/espup),
plus `ldproxy` and `espflash`, then:

```bash
cd firmware-rs/esp
cargo build --release
```

```bash
cargo run --release
```

The first build downloads and compiles ESP-IDF v5.3. `cargo run` flashes with
the C++ firmware's partition table (`partitions/min_spiffs.csv`). For an
ESP32-S3 add `--target xtensa-esp32s3-espidf` (and `MCU=esp32s3`), for an
ESP32-C3 `--target riscv32imc-esp-espidf` (`MCU=esp32c3`).

## CI and troubleshooting

The `firmware-rs` job of `.github/workflows/ci.yml` runs, on every push and
pull request: `cargo fmt --check` (workspace and `esp`), `cargo clippy -D
warnings`, the tests and a release build of `bdlminer-host`. The security
audit workflow, `security-audit.yml`, runs `cargo audit` (RustSec advisories)
on both lockfiles on every change to
`firmware-rs/`, and weekly, so new advisories surface without a push.
Dependabot proposes weekly updates of both lockfiles. The ESP32 build has its own manual workflow,
`firmware-rs-esp.yml`, with the third-party toolchain action pinned by commit.

Common CI and build failures, with their fix: [TROUBLESHOOTING.md](TROUBLESHOOTING.md).

## Status

Done and tested on a PC: everything in `core`, `app` and `host`.

Written but not yet built or run on hardware (no ESP toolchain where it was
written): the `esp` crate. Expect compile fixes on its first build.

Not ported yet (the C++ firmware keeps them):

- Hardware SHA-256 engines (`hw_sha.cpp`, the assembly prefilter and the
  classic ESP32 idle-cycle fill): the Rust build mines with the portable
  software kernel on every core, so ESP32 hashrates are lower.
- ASIC boards: the Bitaxe Gamma 601 (BM1370 + TPS546 + EMC2101) and NerdNOS
  (BM1397) drivers. Their wire protocols, safety limits and fan policy are in
  `core` and tested; the UART/I2C drivers and board bring-up are not.
- Displays and buttons (M5StickC Plus2, T-Display, CYD...), screens, beeper.
- Firmware updates: web upload (`/update`), signed GitHub updates, trial boot
  and rollback.
- MQTT/Home Assistant, market data (mempool.space), pool statistics, block
  confirmation checks, anonymous statistics sharing, remote monitoring,
  daily summary notification, crash reports, mDNS, live dashboard stream
  (`/api/events`: the pages poll instead).
