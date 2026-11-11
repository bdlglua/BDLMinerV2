# Firmware review: settings and Gamma thermal supervision

Reviewed the firmware sources at `v2.5.7`
(`b81bc72c0110492a0e28fbeb934afaad4353d3c8`) and current `main`
(`0685ca92735e84335de317923cd7978930079dbf`). The firmware sources in
`src/` and `web/` are unchanged between those revisions before this fix.

The review prioritized settings validation/persistence, the Gamma and NerdNOS
drivers, cooling, thermal supervision, mining work/results, network framing,
TLS, updates and rollback. Host and browser regressions exercise these paths;
they do not constitute a physical hardware certification or an exhaustive
proof that the repository has no other defects.

## Confirmed: an unpowered ASIC can latch a false overtemperature fault

`gamma601::collect()` previously ignored an unrouted diode while the ASIC was
powered, but accepted that same input while the ASIC was off. The analog mux
cannot be trusted before its core is powered and the routing register is
initialized. A floating input can nevertheless decode as a finite temperature.

Reproduction with the actual driver and simulated I2C: leave the ASIC unpowered,
return 35 C from the board sensor and 106 C from the external input, then take
three samples. The unmodified driver latches `ASIC overtemperature; restart
required`. This can prevent startup while waiting for a pool or turn a normal
thermal pause into a fault requiring a restart.

The fix accepts the ASIC diode only when `powered && diodeReady`. While off,
the existing EMC2101 board sensor supplies the temperature. Full fan cooling,
regulator/supply checks and the ASIC's active temperature/fan interlocks remain
in force. New regressions cover repeated false readings before startup and
after a pause, successful subsequent startup and observable board temperature.

## Fan settings at 65 C

Manual mode is deliberately overridden at ASIC 60 C or regulator 80 C, until
the ASIC is at most 55 C and the regulator at most 75 C. A lower configured
thermal limit makes cooling intervene earlier. This is not a failed settings
save: the requested manual percentage remains saved while safety commands
100 percent. The dashboard exposes the override and its reason.

The percentage is a verified controller command; RPM is the measured fan
speed. The user's reported 65 C does not establish that the fault reproduced
above occurred on their board, or identify the physical cause of its temperature.

## Actual heating and thermal pause

The reported pause is consistent with the configured thermal limit. The default
Gamma limit is 65 C; `watch.cpp` pauses mining at that limit, holds the ASIC off
for at least two minutes, and permits a restart only at or below limit minus
5 C. While paused, the internal board sensor supplies the temperature and the
fan remains at 100 percent. The ASIC's independent hard stop is 75 C, confirmed
by three samples; regulator temperature at 105 C and a stopped fan also stop it.

A combined driver/policy regression now exercises 64 C (running, full fan),
65 C (reset asserted, regulator output off, full fan), an invalid 106 C external
reading while paused, the two-minute hold even on a cool board, a still-warm
61 C board after that hold, and successful restart at 60 C with a valid diode.
This uses simulated I2C and the real thermal policy and driver; it is not a
physical cooling test or a measurement of the user's board.

Frequency and core voltage remain at their saved values. The stock defaults are
525 MHz and 1150 mV. Higher profiles/voltage increase heat; the fix does not
raise a thermal limit, lower a saved fan request, or promise to prevent heating.

## Validation

- The new regression failed before the fix and passes afterward.
- Native Gamma tests include fan command failure, stopped fan, real ASIC
  overtemperature, regulator faults, initialization, cooldown and safety lease.
- The complete host suite and 121 firmware-browser regressions pass.
- The 103 tooling regressions pass.
- Portable mining/protocol tests pass with AddressSanitizer and
  UndefinedBehaviorSanitizer enabled.
- The `bitaxe-gamma-601` firmware cross-compiles successfully with the pinned
  platform. The environment's inner pioarduino Core was corrected from
  `6.2.0.post1` to `6.2.0` to resolve its platform-version compatibility error;
  no repository build configuration was changed for that workaround.

No firmware was published or flashed during the initial review. Publication of
the correction is tracked in [2.5.8 release notes](../releases/v2.5.8.md).
Physical fan speed and cooling performance require the board.
