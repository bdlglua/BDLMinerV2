# BDLMiner V2

The stable firmware version is **2.6.1**. See the release notes in
[English](../releases/v2.6.1.md) or [French](../releases/v2.6.1.fr.md).
The seven V2 features were developed on `v2/development`; the owner-operated
acceptance results below identify the beta versions actually measured.
The [previous stable V1 release](https://github.com/bdlglua/BDLMinerV2/releases/tag/v1.9.15)
remains available.

## M5 / Gamma test kit

Download the [2.6.1 Linux kit](https://github.com/bdlglua/BDLMinerV2/releases/download/v2.6.1/BDLMiner-2.6.1-Linux-kit.zip)
and extract the complete ZIP. It contains the optional fleet hub, host tools,
M5StickC Plus2 and Gamma 601
firmware/factory images, their checksums and the read-only
readiness checker. It does not install firmware automatically.

For development builds, successful V2 [CI runs](https://github.com/bdlglua/BDLMinerV2/actions/workflows/ci.yml?query=branch%3Av2%2Fdevelopment)
also provide a `v2-linux-test-kit` artifact. CI previews are unsigned; the public
release kit records verified firmware signatures in `kit.json`.

From the extracted folder on a Linux computer on the miners' LAN:

```bash
sh check-miners.sh
sh start-hub.sh
```

The readiness check saves `bdlminer-v2-check.json`. Exit code 2 / `review` means a
miner needs attention: inspect its firmware, trial, connection, health and mining
checks. A paused miner or a board still running V1 does not get a V2-ready pass.
No settings are changed by this check. The hub opens at **http://127.0.0.1:8787**.
Edit `bdlhub.local.json` if the supplied addresses differ from your network.

For a manual OTA trial, use the matching `firmware/<board>-firmware.bin` in the
existing local Settings page. `*-factory.bin` is for USB installation/recovery.
Keep your working firmware and private settings backup. Development CI artifacts
are unsigned and cannot be installed by automatic GitHub OTA. The kit records
signature availability in `kit.json`; it does not represent a published release.

Maintainers can build the same kit from verified local release images:

```bash
python3 tools/make_beta_bundle.py --archive .pio/BDLMiner-V2-Linux-test-kit.zip
```

The builder checks chip/target/version, binary hashes, factory application and
both OTA slots. The ZIP contains only checksum-listed distribution files;
runtime databases/caches, private keys and local reports are excluded.

## Four hardware checks on the owner's cards

The owner's 2026-11-02 09:48:37 UTC snapshot shows **both devices on confirmed
beta.3**, mining at 1.005929 MH/s on the M5 and 1.069368 TH/s on Gamma. The owner
also confirmed the gap/icon corrections on both local dashboards. This snapshot
does not observe the full beta.3 OTA transition or establish hour-long stability.

Beta.4 includes the boot-selected S2/S3/C3 transfer optimization and host observers
for the remaining tests. See [beta.4 changes](../releases/v2.0.0-beta.4.md) and
the ordered protocol in [English](hardware-validation.md) or
[French](hardware-validation.fr.md). The owner's Gamma beta.3 hour completed at
1.071274 TH/s over 3,604 device seconds, with +141 accepted shares and no new
rejection. One recovered HTTP timeout keeps its result `review`; health during
that gap is unobserved. A subsequent complete beta.3-to-beta.4 OTA observation
shows a new boot, pending trial and confirmation on that boot, at 1.073901 TH/s
with six accepted shares and no rejection. The post-OTA check passed over 122 s
at 1.073214 TH/s, with +3 accepted shares, no new rejection or reported read error
and an unchanged stored-fault timestamp. The subsequent beta.4 instrumented hour,
reported at 13:23:53 UTC, **passes** over 3,604 device seconds at 1.070391 TH/s:
715 error-free stats reads, unchanged boot `93c65b31`, +119 accepted shares and
no new rejection. HTTP latency averages 47.898 ms, maximum 987.599 ms, under
operator-declared dashboard-closed/hub-stopped conditions. It does not establish
the cause of the old beta.3 timeout; that original report stays `review`.
M5 USB reset on `/dev/ttyACM0` is verified.
M5 physical beta.4 rollback followed by normal beta.4 reinstallation/confirmation
is also observed, ending at 1.003914 MH/s / 47 C IMU on boot `ab2a323b`.
Its short check passes over 122 s at 1.004855 MH/s, with no reported read error.
A first pool-mode network attempt failed to observe pool disconnection and ended
with local-API timeouts. The owner used a router blacklist; a later snapshot shows
restored mining on the same boot. The matching Wi-Fi/local-connectivity test now
passes with operator-confirmed blacklisting/restoration, unchanged boot `ab2a323b`
and a ten-second resumed-counter rate of 1.0052 MH/s. The first failed report is
retained. The low 60-tick average immediately after interruption includes the
intentional pause; live rate has returned to about 1 MH/s.
**For reproduction, test M5 rollback before normal confirmation.** S2/S3/C3/NerdNOS measurements remain
pending actual board availability.

Extract the new kit into a separate directory, retaining configuration/settings
backups. The readiness launcher expects the firmware version of the extracted
kit (2.6.1 for the current stable kit). A card running an older version reports
`review` until its update completes.
Stop the old hub before starting the new one
on port 8787. Use only `*-firmware.bin` through the local dashboard; factory images
are for USB. Observers do not upload firmware or change settings; an OTA observer
only requests a physical reset when `--reset-command` is explicitly supplied.
The network observer records manual operator actions and requires attribution. Signed active
bench operations remain separate and require explicit arguments.

## Local dashboard and mining work

The embedded dashboard now opens with the device identity and actual mining
state, hashrate, pool acceptance and device overview. Detailed pool diagnostics
open on demand below the history chart. Existing history inspection/export,
block records, thermal/fan information and EN/FR support remain available.
Everything is served by the miner, with no CDN or extra background device polling.

The optional fleet hub distinguishes the configured power limit from effective
power after thermal/duty limiting. Unsaved edits survive polling. Outages mark
cached data as stale and disable controls; late fleet/chart responses cannot
overwrite newer selections. Power/channel writes refresh the affected miner's
cached reading, so the hub can show the applied setting.

Portable SHA-256d reuses the first digest's words and specializes the fixed
32-byte second hash. NerdNOS verifies each returned proof once through the shared
ASIC adapter, preserving generation, mask, hardware difficulty and share/block
checks. The per-nonce software scan, hardware assembly kernels, clocks, voltage
and cooling/thermal interlocks are unchanged. A repeatable native comparison is:

```bash
python3 tools/benchmark_sha256d.py --baseline 3eb9b5a --output native-sha-benchmark.json
```

This measures portable SHA-256d on the computer running the tool. It does not
establish ESP32 hardware MH/s, ASIC silicon TH/s or a temperature reduction.
Development after the original beta.3 kit also adds an S2/S3/C3 transfer kernel:
boot checks full digests and keeps it only when a local comparison is faster.
The classic ESP32 loop remains intact. No physical S2/S3/C3 is available for this
change, so no measured gain or 1 MH/s claim is made; see the
[board SHA review](../audits/2026-11-02-resumable-sha.md).
The [beta readiness record](../audits/2026-11-02-v2-beta-readiness.md) separates
software evidence, owner-operated M5/Gamma dashboard-load measurements and the
remaining board tests. The M5 held 1 MH/s with its local page open; Gamma's
open/closed difference was -0.74% in one short comparison. This does not establish
long-duration stability or validate the other boards. A later M5 counter/health
check passed for 3,604 seconds at 1.004141 MH/s; the Gamma hour-long measurement
did not start because its initial bench health prerequisite failed. After the
host bench-status correction, the owner's Gamma passed a 61-second check at
1.076066 TH/s while retaining status `0x0002` as a warning. Its hour-long check
remains incomplete: the next run stopped on an HTTP read error after about 21
minutes, with 1.072458 TH/s across the available counter window and 46 additional
accepted shares / no rejection. The failure does not locate the cause of the
interrupted read.

## View your miners away from home

[Remote monitoring](../remote-access.md) lets each board send its readings
directly to the cloud; from 2.5.4 you open them with your payout address (a private
key from 2.3.0 still works). No local computer or hub is needed. Monitoring is opt-in; the optional local fleet hub below remains available.

## Fleet dashboard on Linux / Raspberry Pi

Python 3.10 or newer is sufficient; no pip dependencies or cloud account are
needed. Run these commands from the repository root:

```bash
cp tools/bdlhost/example.json bdlhub.local.json
python3 tools/bdlhub.py --config bdlhub.local.json
```

Open **http://127.0.0.1:8787**. The example already contains the owner's M5
(`192.168.1.40`) and Gamma (`192.168.1.135`); edit it for another network or add
a NerdNOS. The hub stores seven days of history in `bdlhub.sqlite` by default.
Polling is every ten seconds, with at most four simultaneous miner requests.
Retention and polling can be configured (`retention_days`: 1–90;
`poll_interval_s`: 5–300). Miner IDs must stay unique and stable.

Each card shows actual local rates, shares, temperature, uptime and available
ASIC telemetry. Offline cards retain the last reading with an explicit offline
state; they do not contribute to total online hashrate. Chart gaps distinguish
missing samples and reboots. Local rates, estimates reported by a pool API and
share-based estimates are labeled separately. English and French are available.
Commands include power, pool diagnostics, update channel and signed updates.
The miner's own settings page remains accessible from each card.

For a password-protected miner, configure an environment variable name, not the
password itself:

```json
{"id":"gamma","name":"Gamma 601","url":"http://192.168.1.135","password_env":"GAMMA_WEB_PASSWORD"}
```

Set that variable in the shell/service environment. Authentication uses the
miner's existing `admin` web account. Passwords, Wi-Fi SSIDs and payout addresses
are excluded from fleet JSON, stored history and bench reports. Requests never
follow redirects to another device.

The default listener is local to the computer. An SSH tunnel can expose it to
another computer without changing that listener:

```bash
ssh -L 8787:127.0.0.1:8787 user@raspberrypi
```

For direct LAN hosting, set `BDLHUB_TOKEN` to at least 24 random characters, add
the actual hub `host:port` to `allowed_hosts` in the configuration and start with
`--bind 0.0.0.0`. Enter the token on the hub login form; it is held in browser
memory. Device credentials stay on the host. Host/origin checks and authenticated
commands protect the service; there is no browser-supplied arbitrary device URL.

## One-click pool diagnostics and honest statistics

The embedded dashboard includes **Check pool**, backed by
`GET /api/pool/diagnostics`. It distinguishes disconnected, waiting for work,
incompatible ASIC version rolling, invalid difficulty, rare shares and ready
mining. This inspects the currently configured pool session; it does not probe
other pools, change the endpoint, downgrade V2 or promise that a disconnected
endpoint supports a protocol.

The mean share interval is `assigned difficulty × 2^32 / local hashrate`. It is
a statistical mean, not a deadline. At 1 MH/s and difficulty 10,000, the mean is
about **497 days**. Zero accepted pool shares in that situation does not imply
that SHA-256d is broken. ASIC work additionally requires the advertised version
rolling bits (Gamma: eight; NerdNOS: two).

The share-based estimate uses credited difficulty, not the luck of the submitted
hash: `accepted work × 2^32 / session seconds`. It stays unavailable until at
least three accepted samples and sixty seconds. It includes session downtime and
can differ substantially from the local rate, especially with few shares. The
dashboard also lists stale, duplicate, low-difficulty, authorization, invalid and
unknown rejection counts. These counters describe the current session; existing
lifetime records and block-candidate persistence retain their format.

## Gamma calibration

The hub's calibration command tests **525, 550, 575 and 600 MHz**. The standalone
tool permits explicit trials up to 800 MHz in 25 MHz increments:

```bash
python3 tools/calibrate_gamma.py --config bdlhub.local.json --miner gamma \
  --frequencies 525,550,575,600 --output gamma-calibration.json
```

Calibration requires a confirmed firmware, a mining BM1370, connected pool and
full power. Each frequency gets at least 60 seconds of warmup and 120 seconds of
measurement (default 180), with telemetry checked every two seconds throughout.
Rates come from hash-counter deltas and device elapsed time, rather than the
nominal frequency or dashboard smoothing. The recommendation chooses the lowest
tested frequency within 3% of the best measured rate. It is a short trial, not
proof of long-term stability or an electrical efficiency measurement.

No voltage setting is changed. Calibration stops on missing telemetry, ASIC or
pool stoppage, reboots, throttling/fallback, fan/regulator faults, hardware errors
above 0.5%, ASIC temperature reaching the lower of 63 °C or the configured limit
minus 2 °C, regulator temperature reaching 85 °C, or unsafe supply/core readings.
Gamma firmware interlocks remain active. Reported ASIC current is **core-side**,
not 5 V input current.

The original frequency, fan mode and fan preset are restored and read back after
completion, cancellation or failure. A recommendation is not automatically
applied; apply it later in the miner's existing ASIC settings if desired. Keep
the hub running until restoration finishes. Ctrl+C and SIGTERM request a graceful
stop. A durable checkpoint is saved before the first change: after a host crash,
the hub attempts restoration on its next start using the same database, miner ID
and URL. It blocks another calibration until that checkpoint is cleared by a
verified restore. The CLI keeps `<output>.recovery.json`; rerun the same command
or use `--restore` to restore it without beginning new trials. A powered-off host
cannot send a restore command while it is offline.

## Physical screen configuration

In the miner's local **Settings → Display**, enable the custom mining screen and
choose the four metrics in their desired order: hashrate, accepted shares,
temperature, best difficulty, uptime, pool connection, supported ASIC frequency
or Gamma fan RPM. Enable/disable the other pages independently. The mining page
always remains enabled. Changes apply live and can be discarded before saving.

Gamma keeps its native **128×32**, one-bit OLED layout with four eight-pixel rows;
long rows use the existing bounded paging. Color screens keep their existing
native resolution and font renderer. Block-candidate and fault/update messages
retain priority over custom content. Headless boards hide physical display
controls. Existing settings migrate with defaults; custom screens are off by
default.

## Stable and beta updates

The new **Update channel** setting defaults to **Stable**. Beta is an explicit
opt-in; changing channels clears queued discovery/install work and requests fresh
metadata. Beta discovery uses `updates/beta.json` on the repository's Pages site.
A missing beta pointer produces a clear retryable error rather than changing the
selected channel. Stable discovery refuses prerelease versions. Semantic version
ordering handles `beta.2 < beta.11 < rc.1 < stable`.

Both channels use the existing ECDSA-P256 firmware signature verification and OTA
trial/bootloader rollback. Switching from a beta to Stable does not force an
older firmware downgrade; it offers a later stable version when available.
V2 channel controls require V2 firmware; V1 devices can still appear in the hub
and keep their existing stable updater.

The `Release V2` workflow builds all twelve boards in parallel without signing
secrets. The aggregate job checks every image's chip, target/version markers,
ESP checksum/SHA-256, factory application, two OTA slots and input checksums,
then signs centrally and atomically assembles the release. Beta tags produce
GitHub prereleases with `make_latest=false`; draft preparation branches never
deploy Pages. The root web installer stays on the highest published stable
version; `/beta/` is opt-in. A published stable version also supersedes its beta
on the beta channel. Building or pushing this development branch publishes
neither a release nor a beta update pointer.

## Hardware bench

Run on the same LAN as the boards:

```bash
python3 tools/hardware_suite.py --config bdlhub.local.json --miner m5 --output m5-suite.json
python3 tools/hardware_suite.py --config bdlhub.local.json --miner gamma --output gamma-suite.json
```

The default suite records real counter rates and health for two minutes and
checks pool diagnostics. Unsupported/unrequested physical display, fault and
OTA tests are explicitly **skipped**. No simulated result counts as a hardware
pass. For the actual browser closed/open comparison, the existing
`tools/check_hardware.py measure` command still opens the dashboards and asks the
operator to keep them in visible windows.

Counter/health failures retain the first/last available readings, sample count,
elapsed time and failure stage. Health failures name the failed checks; an
unreachable device retains its last successful sample without labelling that
sample unhealthy. Gamma's regulator check matches the firmware shutdown bits
(`0x007c` while powered). Other STATUS_WORD bits, including CML (`0x0002`), are
retained in `regulator_warning_bits` and `warnings`; a nonzero word alone does
not fail the bench. Missing/malformed status and actual shutdown faults still
fail, as do the existing temperature, voltage, fan and pool checks. The active
frequency-calibration supervisor keeps its stricter preconditions.

Statistics GET reads retry transient timeout/connection errors at most twice,
with two seconds between attempts. Use `--read-retries 0` for immediate failure
on a read error. No setting/update request is retried; authentication/TLS errors,
invalid responses, observed health faults, reboots and counter resets still fail.
Recovered read errors remain visible in `read_error_count`, `read_errors` (up to
32 events) and `read_recoveries`. A completed counter window with read gaps has
status **`review`**, exit code **2**, and unobserved health during those gaps. It
does not count as a clean stability pass. Persistent read failure produces
`failed`, exit 1, retaining the available `observed_counter_window` when valid.
Exit 0 means the requested cases completed without a failed/review case; explicitly
skipped physical/OTA/fault cases remain skipped. Update both
`tools/hardware_suite.py` and `tools/bdlhost/client.py` when updating an older kit.

For the owner's recovered Gamma timeout, the read-only
[`trace_hardware.py` protocol](hardware-validation.md#instrumented-gamma-hour)
adds HTTP phase timings and bounded diagnostic context after a recovered error,
while retaining these exact timeout/review criteria. The owner's beta.4 hour now
passes under its declared isolated conditions. [Published board research](../audits/2026-11-02-board-hashrate-research.md)
documents external S3/C3/NerdNOS figures and their limits; it does not substitute
for our pending S2/S3/C3/NerdNOS hardware measurements.

Optional OTA needs `--firmware <env>-firmware.bin --signature <file>.sig --version
<version>`. The bench independently checks the release signature, ESP digest,
target and version before uploading, then requires observing a pending trial and
confirmation. `--ota-mode rollback --reset-command '<physical reset command>'`
tests real bootloader rollback. A web restart is unsuitable because it confirms
the trial first. Commands are argument vectors executed without a shell; use a
local executable script for multi-step hardware operations.

Network recovery testing requires `--fault-kind pool` or `wifi`, plus both
`--disconnect-command '<command>'` and `--restore-command '<command>'` for the
operator's router or hardware fixture. Restoration runs even if the interruption
fails. A pass requires observing the actual outage and healthy resumed mining;
missing commands produce a skip. The bench never saves an invalid Wi-Fi network
or changes pool credentials to manufacture a disconnect.

For manual interruptions without router automation, the new read-only observer
is available as `python3 tools/monitor_network.py --config bdlhub.local.json
--miner m5 --kind wifi --output network-m5.json`. It first requires confirmed,
healthy mining. Transport loss alone does not prove Wi-Fi loss: a pass also needs
operator attribution, unchanged identity/boot and ten seconds of advancing
healthy reconnected counters. Without attribution the outcome is `review`/exit 2.
Keep the observer computer connected and restore the connection as instructed;
this observer cannot operate or restore a router. The ordered hardware protocol
explains how to avoid interrupting Gamma's simultaneous hour-long test.

V2 has automated loopback/browser/rendering tests, owner-operated M5/Gamma
readiness and short dashboard-load measurements, and a passed one-hour M5
counter/health check. The corrected Gamma host check also passed for 61 seconds.
A later beta.3 hour completed at 1.071274 TH/s but remains `review` after one
recovered HTTP timeout. Gamma beta.4 OTA trial/confirmation and a clean
3,604-second instrumented hour at 1.070391 TH/s are now observed; that hour used
operator-declared dashboard-closed/hub-stopped conditions and had 715 error-free
stats reads. Longer-duration operation and physical-screen acceptance remain
to be established; the other
physical boards remain unvalidated. V1 hardware evidence does not establish V2
hardware validation. See the [implementation record](implementation.md) and
[beta readiness audit](../audits/2026-11-02-v2-beta-readiness.md) for the scope of
the recorded checks.
