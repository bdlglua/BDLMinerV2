# Contributing to BDLMiner V2

Thanks for helping! Bug reports, tests on real boards, translations, documentation
and code are all welcome. By taking part you agree to the
[code of conduct](CODE_OF_CONDUCT.md).

- **A question, or help to install?** Ask on the [BDLMiner Discord](https://discord.gg/S8kSgD6Xe9).
- **A bug or an idea?** Open an [issue](https://github.com/bdlglua/BDLMinerV2/issues/new/choose):
  the forms ask for the board, the firmware version and the log, which is most of
  what is needed to help.
- **A security problem?** Do not open an issue: see [SECURITY.md](SECURITY.md).
- **Tested an experimental board** (Bitaxe Gamma 601, NerdNOS)? Reports from real
  hardware are especially valuable, even when everything works.

## Building

Install [PlatformIO](https://platformio.org/) (Core 6.2.0, as the CI uses; see
[development.md](docs/development.md#build-and-flash) for the first build), then:

```sh
pio run -e m5stickc-plus2                 # build one board
pio run -e m5stickc-plus2 -t upload       # build and flash it over USB
pio device monitor -b 115200              # serial log
```

Every board is a PlatformIO environment in [`platformio.ini`](platformio.ini); the
[board reference](docs/hardware/supported-boards.md) lists them. `FW_VERSION` there is the version
the build reports.

## Project layout

- `src/core/` — portable code with no Arduino dependency (SHA-256, Stratum V1/V2,
  work building, ASIC wire protocols), unit-tested on a PC;
- `src/` — the firmware: mining tasks, network, web server, screen, settings,
  boards (`board.cpp`), ASIC backends (`gamma601.cpp`, `nerdnos.cpp`);
- `web/` — the dashboard, settings and log pages. Edit these files: they are packed
  into `src/web_assets.generated.h` (do not edit it) by `tools/make_web.py` at
  every build;
- `test/host/` — host tests, `test/web/` — browser tests, `test/tools/` — tests
  of the release and packing scripts;
- `tools/` — release, installer, previews, fonts and icons;
- `docs/` — release notes and hardware notes.

## Testing

Please run what your change touches; the CI runs all of it on every pull request.

```sh
# Host tests (C++ and Python): protocols, mining, settings, web delivery...
pio pkg install -e esp32-headless         # once: ArduinoJson for the host tests
./test/host/run.sh                        # needs g++ and libsodium

# Release and packing scripts
python3 -m unittest discover -s test/tools -v

# Browser tests of the web pages (Node 24)
npm ci --prefix test/web
test/web/node_modules/.bin/playwright install chromium
npm test --prefix test/web

# Screen renderer on every display size (PNG previews in tools/preview/)
pio pkg install -e m5stickc-plus2         # once: M5GFX for the previews
python3 tools/preview_display.py --check  # without --check it also writes the PNGs
```

A change to firmware behaviour should come with a test when the code can run on a
PC (`src/core/`, or a backend with stubs, as `test/host/test_nerdnos.cpp` does). If
you tested on a real board, say which one and what you saw in the pull request.

## Code style

- Match the code around your change: naming, formatting, comment density.
- Comments say *why*, not *what*: a hardware quirk, a protocol rule, a past bug.
- Code, comments, commit messages and documentation are in **English**.
- Every new text shown to users gets a **French** translation too: the web pages'
  `FR` dictionary in [`web/common.js`](web/common.js), the screen's table in
  [`src/i18n.cpp`](src/i18n.cpp).
- Keep the firmware small: the 4 MB boards share their flash between two firmware
  slots, and the M5Stack Core build is above 90 % of its slot.

## Commits and pull requests

- One topic per pull request, based on `main`.
- Commit messages follow the existing history: `feat: …`, `fix: …`, `docs: …`,
  `test: …`, `build: …`, `chore: …`, with a short body explaining why.
- Update the README or `docs/` when behaviour or settings change.
- Adding a board: a `[env:…]` in `platformio.ini`, its pins in `src/board.cpp`,
  an entry in `site/src/package.rs` and in the CI build matrix
  (`.github/workflows/ci.yml`), a row in the README table, and hardware notes in
  `docs/hardware/` for anything experimental.

## Releases

Releases are made by the maintainers: pushing a `vX.Y.Z` tag builds every board,
signs the firmware with the release key (kept outside the repository) and publishes
the release and the web installer. The release notes are `docs/releases/vX.Y.Z.md`; a French
translation, `docs/releases/vX.Y.Z.fr.md`, is what the community Discord bot shows to
French speakers (read from `main`, so it can follow the release). Pull requests do not need to change
`FW_VERSION` or the release notes.

## License

BDLMiner V2 is released under the [GNU GPL v3.0](LICENSE). By contributing, you
agree that your contribution is released under the same license.
