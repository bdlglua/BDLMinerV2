# Rust firmware: CI and build troubleshooting

What each check of `firmware-rs/` runs, how it fails, and how to fix it.

## Checks

| Workflow | Job / step | Runs | Reproduce locally (from `firmware-rs/`) |
|---|---|---|---|
| `ci.yml` | `firmware-rs`: fmt | every push and pull request | `cargo fmt --all -- --check` and `cargo fmt --manifest-path esp/Cargo.toml -- --check` |
| `ci.yml` | `firmware-rs`: clippy | every push and pull request | `cargo clippy --locked --all-targets -- -D warnings` |
| `ci.yml` | `firmware-rs`: tests | every push and pull request | `cargo test --locked` |
| `ci.yml` | `firmware-rs`: reference set up to date | every push and pull request | `python3 core/tests/fixtures/make_golden.py > core/tests/fixtures/golden.json`, then `git diff --exit-code` |
| `ci.yml` | `firmware-rs`: host build | every push and pull request | `cargo build --locked --release -p bdlminer-host` |
| `security-audit.yml` | `cargo-audit` | changes to `firmware-rs/`, weekly | `cargo audit --file Cargo.lock` and `cargo audit --file esp/Cargo.lock` |
| `firmware-rs-esp.yml` | ESP32 / ESP32-S3 build | manual | `cd esp && cargo build --release` (needs `espup`) |

## Failures

| Failure | Cause and fix |
|---|---|
| `cargo fmt` diff | Run `cargo fmt --all` in `firmware-rs` (and `cargo fmt` in `esp`). |
| `--locked` refuses to update `Cargo.lock` | A `Cargo.toml` changed without its lockfile: run `cargo update -p <crate>` (in `esp`: `cargo +stable update`) and commit the lockfile. |
| `cargo audit` reports an advisory | Update the crate (`cargo update -p <crate>`); if no fixed version exists, document the exposure in the pull request before ignoring it. |
| `custom toolchain 'esp' ... is not installed` | Commands run inside `esp/` use the Xtensa toolchain: install it with `espup install`, or run from `firmware-rs/` (the host crates use stable Rust). |
| ESP build: `ldproxy` not found | `cargo install ldproxy`. |
| ESP build fails for an S3/C3 | Pass both the target and `MCU` (`MCU=esp32s3 cargo build --target xtensa-esp32s3-espidf`); ESP-IDF is built per chip. |
| ESP-IDF download or CMake errors | Delete `esp/.embuild` and `esp/target`, then build again (the first build downloads ESP-IDF v5.3 and needs Python 3 and git). |
| `golden.json` differs from its generator | The reference set (`firmware-rs/core/tests/fixtures/`) was edited by hand, or `make_golden.py` changed without regenerating: rerun the generator and commit both. |
| A `golden` test fails | The core disagrees with the independent Python oracle: the core is wrong (or the generator changed). Never edit `golden.json` to match the core. |
| End-to-end pool tests time out | They mine against local pools on 127.0.0.1 and need a few seconds of CPU; on a very slow runner run them alone: `cargo test -p bdlminer-app --test pool_e2e`. |

## Reference set and benchmark

`firmware-rs/core/tests/fixtures/golden.json` is a reference set computed by
`make_golden.py` with Python's hashlib and integers only, independently of
the Rust code: headers and their double SHA-256, the early-rejection filter
at its exact boundary, hash difficulties, compact targets (valid, negative,
zero and overflowing) and job-to-header assembly. `core/tests/golden.rs`
checks the core against all of it.

`cargo bench -p bdlminer-core --bench sha256d` measures the software
kernel's hashrate on one thread (set `BDL_BENCH_NONCES` for longer runs);
compare before and after a change to the kernel.

## Reading a failed run

- **fmt**: the log is the diff to apply; `cargo fmt --all` applies it.
- **clippy**: each error names the lint (`clippy::...`) and the line. Fix the
  code; allow a lint only with a comment explaining why (as
  `needless_range_loop` in `firmware-rs/core/src/lib.rs`, whose loops follow the
  published algorithms).
- **tests**: `cargo test --locked <test name> -- --nocapture` prints the
  firmware log lines (`[pool] ...`, `[miner] ...`) of the end-to-end tests,
  which show where a pool session stopped.
- **cargo-audit**: the report names the advisory (`RUSTSEC-YYYY-NNNN`), the
  crate, the patched versions and the dependency path. Updating the crate
  that pulls it in is usually enough.
- **ESP build**: the first error after `Building ESP-IDF` is the real one;
  later CMake errors follow from it.
