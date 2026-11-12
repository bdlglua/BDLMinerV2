# BDLMiner website in Rust

`site/src/` builds all eleven public pages, the USB installer, stable/beta release
channels, isolated demos and anonymous statistics snapshots. `site/browser/src/`
implements the browser application in Rust compiled to WebAssembly: languages,
navigation, Canvas charts, statistics, private viewer, chat and Web Serial.
HTML templates and CSS remain the presentation formats. The only published
JavaScript is generated `wasm-bindgen` bindings and a generated module loader.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.105 --locked
cargo run --locked --release --manifest-path site/Cargo.toml
cargo test --locked --manifest-path site/Cargo.toml
cargo test --locked --manifest-path site/browser/Cargo.toml
cargo run --locked --manifest-path site/Cargo.toml -- --check
# Requires gh authenticated for the release repository:
cargo run --locked --release --manifest-path site/Cargo.toml -- channels --repository bdlglua/BDLMinerV2 --output dist
```

The newest Rust firmware release (`rust-v*`, firmware-rs) gets its own
installer under `rust/`, built from its full images; a missing or unreadable
Rust release leaves it out without holding up the stable and beta channels.
Publishing a Rust release starts this site's workflow.

The normal build regenerates tracked `installer/site/` templates and ignored
`installer/assets/wasm/`. Publication fills channel metadata and generates
SHA-256 manifests directly from release factory images. A staged export preserves
the previous directory on failure. Firmware signatures and release assets remain
unchanged. Browser tests use intentionally invalid, non-flashable fixtures;
`test_flash.cjs` uses deterministic wire-simulator images exclusively on loopback.

```sh
npm ci --prefix test/web
npm test --prefix test/web
node test/web/test_hub.cjs
node test/web/test_flash.cjs
```

The flash simulator exercises the actual Rust SLIP/ROM/stub protocol on ESP32,
S2, S3 and C3, chip detection before erase, chunk writes, checksum verification,
USB/UART reset and cleanup. Physical hardware validation is still required.
RAM stub provenance and its licence are in `browser/stubs/`.

Demos reuse the firmware HTML/CSS with a Rust in-memory model. Their CSP permits
only same-origin resources, blocking access to real LAN miners. Firmware and its
local UI are separate from the public websites. `stats-wasm/` remains an
independent numerical benchmark; it is not a second application runtime.

The native APIs live in the `bdl-pool/services`, `BDLMiner-stats` and
`BDLMiner-chat` repositories. Deploying static WASM does not restart these services;
see each repository's activation and rollback procedure.
