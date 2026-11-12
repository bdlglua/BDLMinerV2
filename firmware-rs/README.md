# BDLMiner V2 in Rust

A rewrite of the BDLMiner V2 firmware (`src/`, C++/Arduino) in Rust. The C++
firmware remains the released one until this build has been validated on
hardware: see [Status](#status).

| Crate | What it holds | Runs on |
|---|---|---|
| [`core`](core) `bdlminer-core` | Port of `src/core`: SHA-256d kernel with midstate and early rejection, header/merkle building, difficulty and targets, Stratum V1 state machine, Stratum V2 extended channel and Noise_NX (secp256k1 EllSwift + ChaCha20-Poly1305), payout addresses of the six SHA-256d coins, BM1370/BM1397 wire protocols, thermal/fan/safety policies, release versions, image markers | anywhere |
| [`app`](app) `bdlminer-app` | Port of the hardware-independent firmware: settings table, validation and the two-record store, shared miner state (job board, share queues, block candidates, statistics, history, persistence), pool task with failover/rotation, supervision and alerts, notifications, the dashboard's JSON API and pages (`web/`) | anywhere with `std` |
| [`host`](host) `bdlminer-host` | The firmware on a PC: software SHA-256d threads, pool sessions, the local dashboard | Linux, macOS, Windows |
| [`esp`](esp) `bdlminer-esp` | The ESP32 firmware: NVS, WiFi with setup portal, SNTP, pinned mining threads with the hardware SHA engine and the Xtensa assembly kernels ([`src/hwsha.rs`](esp/src/hwsha.rs)), the BM1370/BM1397 ASIC drivers, ESP-IDF HTTP server, HTTPS notifications | ESP32, ESP32-S3, ESP32-C3 |

Everything portable is in `core` and `app` and is tested on a PC; `esp` only
binds ESP-IDF services to the traits of `app::platform`, and plugs the
hardware SHA engine into the mining loop (`app::miner::Engine`).

## Hardware SHA and assembly

`esp/src/hwsha.rs` is a Rust port of the C++ firmware's `src/hw_sha.cpp`
(register access, engine locks, kernel selection and self-tests). Its
Xtensa kernels are the C++ firmware's `hw_sha_classic.S`, `hw_sha_fill.S` and
`sw_sha_prefilter.S`, assembled with `global_asm!`: `esp/build.rs` does what
the C preprocessor did for them (their `#define`s and target guard), and the
data of the idle-cycle hash is `core::fill` (from `src/core/fill.cpp`). At boot the
same self-tests as the C++ firmware choose what runs:

- the last core: the hardware miner (`hw`, priority 19, alone on its core,
  pausing one tick every 10 s); while the peripheral is busy (HTTPS
  notification) or, on the original ESP32, for pool targets above the fixed
  16-bit filter, it hashes in software in the engine's nonce order;
- original ESP32 (revision 3 and later): a second work hashed in the idle
  cycles of the hardware loop, counted as engine `fill`;
- the other core: the software miner (`sw`), with the assembly prefilter on
  the ESP32 and ESP32-S3 when it passes its self-test, else the Rust kernel.

## Compatibility with the C++ firmware

- Settings: same keys, order, limits, validation messages and stored record
  (`bdlminer`/`cfgA`/`cfgB`: header + JSON, CRC-32). An update keeps the
  configuration.
- Statistics and block candidate proofs: same record layouts (`stats`/`statA`,
  `statB`, `blockA`, `blockB`). The 24 h hashrate history is now kept in NVS
  (`stats`/`hist`) instead of the `spiffs` partition, so it starts empty after
  the switch.
- WiFi: the network saved by the C++ portal (in the WiFi driver's NVS) is used.
  Without one, the setup portal (`BDLMinerV2-XXXX`, http://192.168.4.1) works
  as WiFiManager's did: it opens by itself on a phone that joins it (every DNS
  name and page lead to it) and lists the networks in range.
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

Install the Xtensa toolchain with [espup](https://github.com/esp-rs/espup)
(`espup install --toolchain-version 1.97.0.0`, as CI: 1.99.0.0 does not
build std for ESP-IDF; the PC workspace
uses Rust 1.99.1, see `rust-toolchain.toml`),
plus `ldproxy` and `espflash`, then:

```bash
cd firmware-rs/esp
cargo build --release
```

```bash
cargo run --release
```

M5StickC Plus2 (holds the power pin on GPIO4 as soon as the app starts, and
the image's ESP-IDF bootloader skips the image check at power-on so that it
starts in time after the power button or the USB cable; reads the IMU temperature, and
draws the C++ firmware's screens with its own renderer, `src/ui_view.cpp` on
M5GFX, built by `esp/components/bdl_ui`; button A: next page, held 5 s:
factory reset; button B: brightness):

```bash
cargo build --release --features m5stickc-plus2
```

Without an ESP toolchain, run the manual workflow *Rust ESP32 firmware*
(`.github/workflows/firmware-rs-esp.yml`): its artifacts hold one image per
board (`bdlminer-esp-<board>.bin`, ESP-IDF's bootloader built from
`esp/sdkconfig.defaults`, the partition table and the app) to flash at 0x0. ASIC boards (Bitaxe
Gamma 601, NerdNOS) are not supported yet: keep the C++ firmware on them.

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
Dependabot proposes weekly updates of the host workspace; the `esp` crate
is updated by hand (its toolchain and ESP-IDF updates need an ESP32 build). The ESP32 build has its own manual workflow,
`firmware-rs-esp.yml`, with the third-party toolchain action pinned by commit.

Common CI and build failures, the reference set and the benchmark:
[docs/firmware-rs-ci-troubleshooting.md](../docs/firmware-rs-ci-troubleshooting.md).

## Status

Done and tested on a PC: everything in `core` and `app` (the screen
renderer included, on in-memory panels of every board size) and `host`.

Built by the workflow for every board, Rust from end to end: the hardware
SHA engine and its Xtensa kernels (`esp/src/hwsha.rs`, `global_asm!`), the
Bitaxe Gamma 601 (BM1370 + TPS546 + EMC2101) and NerdNOS (BM1397) drivers
(`esp/src/gamma601.rs`, `nerdnos.rs`), the panels (`esp/src/lcd.rs`). Run on
hardware so far: the M5StickC Plus2 (before the renderer and the SHA engine
moved to Rust). The M5Stack image keeps M5GFX (C++) for its board detection
and panel drivers only (`esp/components/bdl_ui`); its pages are drawn in
Rust like the others.

Ported from the C++ firmware since the first builds, run by the HTTPS task
(`esp/src/main.rs`) unless noted: firmware updates (dashboard upload,
signed GitHub releases of the Rust firmware, trial boot and rollback:
`app/src/update.rs`, `app/src/multipart.rs`, `esp/src/ota.rs`), market data
and the pool's statistics (`app/src/market.rs`), remote monitoring
(`remote.rs`), block confirmation checks (`block_status.rs`), the anonymous
statistics report (`share.rs`), MQTT/Home Assistant (`mqtt.rs`, its own
task), the daily summary (`summary.rs`), crash reports (`crash.rs`,
`esp/src/crash.rs`), mDNS, the live dashboard stream (`/api/events`,
`esp/src/http.rs`), the beeper (`esp/src/beeper.rs`) and the clock set from
the pool's job time when NTP does not answer.

GitHub updates only install `<env>-firmware.bin` from `rust-v*` releases
(`<env>` ends in `-rust`), signed by the release key and naming this board
and that version in the image: the C++ and Rust firmwares never install each
other by themselves.
