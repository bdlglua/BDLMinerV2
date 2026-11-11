# Gamma 601: regulator shutdown reported as an ASIC voltage fault

Source reviewed: published firmware 2.5.8 at
`c50609d8483f413e2c6fbb1d5b24899d0180674e`.

## Evidence from the supplied device log

The device starts the BM1370 at 525 MHz / 1150 mV at uptime 00:18. At 00:46 it
reports:

| Field | Value |
|---|---:|
| Regulator STATUS_WORD | `0x4851` |
| Input voltage | 5.38 V |
| ASIC core voltage | 0.027 V |
| Output current | -0.3 A |
| ASIC temperature | 72.6 C |
| Regulator temperature | 62.5 C |
| Fan speed | 8837 RPM |

`0x4851` includes `IOUT_OC` (`0x0010`, output overcurrent fault), `OFF`
(`0x0040`) and the output-current summary (`0x4000`). The regulator reports a
fault and an already-disabled output; the low measured voltage is consistent
with that shutdown. The sampled current is not a measurement of the peak that
triggered the latched fault.

The earlier `0x2841` / 0.451 V fault printed at boot is explicitly the persisted
last fault from before that boot, not a new failure during startup. The active
fault at 00:46 is separate. The following watch message rounds 72.6 C to 73 C
and reports a thermal pause with a 60 C resume threshold. A preceding 97.4 C
diode sample was marked unconfirmed by the existing three-sample filter.

The owner reports using the original Gamma supply. That identifies the supply
as original; it does not establish its present electrical condition. Fan RPM
establishes rotation, not heatsink contact or actual cooling performance.

## Confirmed diagnostic defect and correction

`bm1370::Health::fault()` tested the core-voltage window before the regulator
fault status. Once the regulator had shut down, the resulting near-zero voltage
therefore masked the recorded hardware fault behind `ASIC voltage fault`.

The correction checks regulator faults before the core-voltage window and
reports `Regulator output overcurrent; restart required` for `IOUT_OC`. The
status mask, current/voltage/thermal thresholds, reset, power shutdown, full fan
and requirement for an operator restart are retained. A low core voltage with
no regulator fault still produces the ASIC voltage fault. Expected OFF status
while intentionally unpowered remains non-fatal. French translations expose
the distinction in the dashboard.

This improves fault attribution. It does not repair or suppress the reported
regulator shutdown and makes no claim to cure real heating under load.

## Reference comparison and remaining uncertainty

ESP-Miner reference revision
[`7972147f593dd8b1fcc51124417a024429ca8da1`](https://github.com/bitaxeorg/ESP-Miner/tree/7972147f593dd8b1fcc51124417a024429ca8da1)
uses the same `IOUT_OC` and `OFF` bits, default current warning/fault limits of
25 A / 30 A and `IOUT_OC_FAULT_RESPONSE = 0xc0` (shutdown without retries).
BDLMiner configures those same current limits and response for Gamma 601.
No current-limit configuration mismatch was established by this comparison.

The log alone cannot distinguish a load/current transient, electrical or cooling
problem, or another firmware/hardware interaction. Physical checks should
include the original 5 V supply and connector under load and heatsink mounting
with power disconnected. Repeated restarts are not a diagnostic substitute for
investigating the reported overcurrent fault.

## Validation

- The driver regression using `0x4851` and 0.027 V failed before the correction
  because it returned the generic ASIC voltage diagnosis; it passes afterward.
- Portable interlock cases cover the supplied trace, fault retention while off,
  intentionally disabled output and actual undervoltage without a status fault.
- The complete native suite passes, including the portable interlock tests and
  the real driver exercised against simulated I2C.
- All 122 firmware-browser regressions pass, including French overcurrent
  display while the pool remains connected.
- The `bitaxe-gamma-601` cross-build succeeds.

Publication of this correction is tracked in the
[2.5.9 release notes](../releases/v2.5.9.md). The published 2.5.8 tag is retained
unchanged.
