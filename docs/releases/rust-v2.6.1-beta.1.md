# BDLMiner V2 in Rust, 2.6.1 beta 1

A first beta of the Rust rewrite of the firmware (`firmware-rs/`), for
testing. It is a separate prerelease: the C++ firmware stays the released
one, and its installer and updates never offer these images.

The miner, the pool sessions, the dashboard and the setup portal, the hardware
SHA engine with its assembly kernels, the screens, and the Bitaxe Gamma 601
and NerdNOS ASIC drivers are written in Rust. The M5Stack image keeps M5GFX
for its board detection and panel drivers; everything it shows is drawn in
Rust.

## Images

One image per board, to flash at 0x0 over USB (bootloader, partition table
and application), for example with https://espressif.github.io/esptool-js/ or

    espflash write-bin 0x0 BDLMiner-rust-2.6.1-beta.1-<board>.bin

`SHA256SUMS.txt` lists their checksums.

| Image | Board |
|---|---|
| `m5stickc-plus2` | M5StickC Plus2 |
| `m5stack-esp32`, `m5stack-s3` | M5Stack devices (Core/Basic/Gray/Fire, Core2, Tough; CoreS3, AtomS3...) |
| `lilygo-t-display`, `lilygo-t-display-s3` | LilyGO T-Display and T-Display-S3 |
| `esp32-2432s028r` | ESP32-2432S028R (CYD) |
| `bitaxe-gamma-601` | Bitaxe Gamma 601 (BM1370) |
| `nerdnos` | NerdNOS (BM1397) |
| `esp32`, `esp32s3`, `esp32s2`, `esp32c3` | Development boards without a screen |

The settings keep the C++ firmware's format and place: flashing a Rust image
keeps the configuration, and going back to the C++ firmware keeps it too.

## Same features as the C++ firmware

Firmware updates (upload from the dashboard, signed GitHub releases, trial
and automatic rollback), market data (price, fees, retarget, recent blocks),
the pool's statistics, block confirmation checks, remote monitoring ("My
miners"), the anonymous public statistics, MQTT/Home Assistant, the daily
summary, crash reports, mDNS (`http://<name>.local/`), the live dashboard and
the beeper.

## Updates

The next Rust betas install from the dashboard (GitHub update, or upload of
`<board>-rust-firmware.bin` from the release). A new firmware starts on
trial: it is kept after a few minutes connected to the pool and mining, else
the previous one comes back by itself. The C++ firmware's updates never
offer these images, and the Rust firmware only installs Rust releases: to
go back to the C++ firmware, flash it over USB.

## Testing

Built by the workflow for every board; the portable code (mining, pools,
settings, dashboard, screen layout) is tested on a PC. On hardware, only the
M5StickC Plus2 ran an earlier build. Check the boot log for
`hardware SHA: self-test OK` and, on the Bitaxe Gamma 601, for
`regulator and sensors configured`. Report problems with the boot log, read
with a serial monitor that leaves DTR and RTS inactive (the M5StickC Plus2
turns off otherwise), e.g.

    python -m serial.tools.miniterm --dtr 0 --rts 0 /dev/ttyACM0 115200
