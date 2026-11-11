# NerdNOS — experimental native BDLMiner

The [NerdNOS](https://github.com/benjamin-wilson/NerdNOS) is an add-on board with one
**BM1397** ASIC (the Antminer S17 chip, as on the Bitaxe Max) that carries a LilyGO
**T-Display-S3**. The T-Display-S3 runs BDLMiner: it shows the usual screens and web
dashboard and drives the ASIC. Its stock firmware announces 150–200 GH/s for about
8 W.

**No physical NerdNOS was available during implementation.** The ASIC protocol is
tested on a PC (command vectors, a chip-side replay of every job's midstates, a real
proof of work from the genesis block), but start-up, real hashrate and accepted
shares still need to be confirmed on a board. Reports are welcome.

## What BDLMiner does with the board

| Function | Pin | As in the NerdNOS firmware |
|---|---|---|
| ASIC UART1 | TX GPIO17, RX GPIO18 | 115200 baud for start-up, then 1 562 500 |
| ASIC reset | GPIO12 | high holds the chip in reset on this board |
| Core supply enable | GPIO13 | the buck converter; off whenever mining stops |
| Temperature | GPIO2 (ADC1) | 10 kΩ NTC, beta 3380, next to the ASIC |
| Buttons, screen | GPIO14, GPIO0, LCD | as on any T-Display-S3 |

These pins, the start-up sequence, the PLL, the job format and the NTC formula were
checked again against that firmware in v1.9.10: they match it (BDLMiner takes the
version mask from the pool where the stock firmware assumes 1fffe000, and verifies every
result by hashing). These pins come from the NerdNOS firmware
([WantClue/NerdMiner_v2](https://github.com/WantClue/NerdMiner_v2/tree/nerdnos), branch
`nerdnos`, revision `22588347597d`), the code that ships on the boards; the KiCad files in the hardware
repository name different header pins, probably another revision.

- **Start-up**: the chip stays unpowered and in reset until the pool sends work. Then
  BDLMiner powers it, releases reset, checks that exactly one BM1397 answers, writes
  the stock register values, sets the PLL and switches to the fast UART speed.
- **Frequency**: 300 MHz at 100 % power, like the stock firmware. The *Mining power*
  setting scales it (minimum 50 MHz); the core voltage is fixed by the board.
- **Work**: a new job every 30 ms, each with four SHA-256 midstates (four rolled
  versions), so the pool must support **version rolling** (`mining.configure`).
- **Results**: the chip reports hashes of difficulty 64 and above (lower when the pool
  difficulty is between 16 and 64). Every one is verified by hashing it on the ESP32
  before it counts: the hashrate comes from these verified results, corrupted frames
  count as hardware errors and are never submitted.
- **Heat**: *Pause mining above* defaults to **75 °C** for this board; the stock firmware
  cuts power at 80 °C. Since 2.3.4 the driver does the same whatever that setting (even
  0 = off): the ASIC stops at **80 °C** on the NTC and restarts once it reads 70 °C or less. Since v1.9.10 there are no 50 % / 25 % steps below the limit (each
  power change restarts the ASIC), and once hot it stays paused at least two minutes.
  The NTC reads with the ASIC off as well. The hard stop needs a reading: when the NTC
  gives none for 10 s (open or shorted sensor), the ASIC is stopped whatever the setting,
  and starts again by itself once the sensor reads.
- **Watchdog**: no verified result for a minute restarts the chip.
- **Pool**: defaults to `public-pool.io:3333`, Stratum V1, suggested difficulty
  **1024**. Since v1.9.8 a pool difficulty below 16 no longer stops the ASIC: its filter
  stays at 16 and only those shares are sent (block odds unchanged). Only a pool without
  version rolling keeps it off.
- **Updates**: GitHub updates are offered (setting `1`) rather than installed
  automatically, as for the other experimental board.

## Reliability corrections in v1.9.13

- The dashboard result counter is published on every verified result, rather
  than waiting for a 50-result error-statistics window.
- A lost pool job interrupts startup within its next short supervision wait,
  holding reset and cutting ASIC power instead of finishing initialization.

Both cases are covered by the native backend test. The default ASIC frequency,
NTC conversion and work/nonce accounting are unchanged. Gamma's EMC2101 fan
controls are not exposed for this backend.

## Install

Use a USB supply rated for at least **2 A** (the ASIC draws about 8 W).

1. Download **`nerdnos-factory.bin`** from the latest
   [release](https://github.com/bdlglua/BDLMinerV2/releases/latest) and flash it at
   **0x0**, or select **NerdNOS** in the [web flasher](https://miner.bdl-pool.com/flash.html).
   To build it yourself:

   ```sh
   pio run -e nerdnos -t upload
   pio device monitor -b 115200
   ```

2. Connect to `BDLMinerV2-XXXX`, password `MineYourCoins`, open `http://192.168.4.1`,
   and set WiFi, your payout address and a web password.
3. The log (`/log`) shows `[nerdnos] BM1397 at 300 MHz` once the chip runs; the
   dashboard's *ASIC · BM1397* tile shows its state, frequency, verified results and
   hardware error rate.

Factory flashing erases the board's previous firmware and settings. Do not flash
this image onto a plain T-Display-S3: use `lilygo-t-display-s3` there.
