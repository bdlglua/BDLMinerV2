# BDLMiner website in Rust

The Rust binary renders public pages from shared HTML templates to the tracked
`installer/site/` export. It uses only the Rust standard library and requires no
Node, React or Next.js to render the website.

```sh
cargo test --locked --manifest-path site/Cargo.toml
cargo run --locked --release --manifest-path site/Cargo.toml
cargo run --locked --manifest-path site/Cargo.toml -- --check
```

Edit `site/templates/pages/` for page content, and the shared document,
navigation, footer, chat and head templates for common elements. The typed route
catalog in `site/src/main.rs` selects chart, flasher and remote-viewer assets.
Bindings are checked at build time and text metadata is HTML-escaped.

The release packager still fills version, board and site-data placeholders and
publishes stable/beta channels. Both website publication workflows run Rust
before packaging. `dist/` is their output, keeping `site/` sources intact.

Browser features remain in `installer/assets/*.js`: Web Serial/ESP Web Tools,
statistics, Chart.js, language selection, chat and the remote viewer. Node is
needed for the existing Playwright tests and optional statistics benchmark,
not page generation. Firmware and Python release/API services are separate.

The independent Rust/WebAssembly statistics project remains in `stats-wasm/`
with its existing build and benchmark paths.
