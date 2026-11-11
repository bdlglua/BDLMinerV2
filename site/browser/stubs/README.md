# ESP RAM stubs

Unmodified binary text/data sections extracted from the previously vendored
`esptool-js` ESP32, ESP32-S2, ESP32-S3 and ESP32-C3 target stub definitions
(`installer/assets/vendor/esp-web-tools-10.4.0`). Metadata records each load and
entry address. The original Apache-2.0 licence is retained alongside the blobs.

These programs run temporarily in ESP RAM during flashing. All browser protocol,
chip validation, image checks, transport and user interaction are implemented in
Rust (`../src/flash.rs`). No esptool JavaScript executes in the website.
