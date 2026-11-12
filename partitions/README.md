Partition tables of Arduino-ESP32 (identical in 2.0.17 and 3.3.12), kept in the
project: pioarduino's ESP-IDF rebuild (custom_sdkconfig) looks for them here.
The layouts are the ones flashed boards already have, so an update over WiFi
from 2.4.x keeps every setting and needs no USB reflash.

`rust_4MB.csv` is the Rust firmware's (firmware-rs) layout for 4 MB boards with
a screen: one app slot large enough for its image, NVS at the same place.
