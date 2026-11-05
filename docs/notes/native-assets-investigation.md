# Native asset test startup investigation

Investigated on 2026-11-01, Windows build 22621.4317, Rust/Cargo 1.98.1.

## Reproduction and boundary

`cargo test --test native_assets -- --nocapture` repeatedly failed in resource case 4:
the fresh executable copy without adjacent assets. The error was `notification startup
deadline exceeded`. The same failure occurred outside the sandbox.

Temporary timing instrumentation around `detached::spawn` and the readiness exchange
showed a failing launch spending 899 ms in process creation, then reaching the readiness
byte at 954 ms. The caller had already exceeded its 750 ms delivery budget. A separate
`--version` probe, which does not load configuration or artwork, measured fresh copies
at 524–529 ms in process creation versus 3–6 ms for the main executable. Earlier fresh
copy observations were slower. These are individual observations, not calibrated limits.

The delay is before the display-child configuration/artwork path. Its cause inside
Windows is not established. Defender was enabled, but a performance recording could
not start (`Access denied`, 0x80070005), even outside the sandbox. There is no evidence
here that identifies Defender as the cause. No protection settings or exclusions changed.

## Test correction

The asset test now starts the real `aherald.exe --display-child --config ...` process
directly and sends the same serialized demo event through its private standard input.
It retains the fresh copy without adjacent assets, all five resource cases, window
dimension assertions (including working-directory isolation), dismissal and successful
exit checks. Owned children are killed and reaped on failure. Its existing two-second
rendering deadline starts after process creation; dismissal also retains its two-second
deadline.

This separates resource rendering from hook delivery timing. Production delivery code
is unchanged: its 750 ms caller budget and 500 ms readiness budget still apply, including
to slow cold launches. `tests/process.rs` separately exercises delivery before finite or
indefinite card expiry, invalid inputs, missing executables and restricted handle
inheritance. A slow cold launch may still be rejected by that intentional production
budget; this test correction does not claim to accelerate Windows process creation.

The corrected asset test, all three process tests and the full `cargo test --all-targets`
suite passed outside the sandbox, with no concurrent build during the full run. The full
log is retained locally at `target/ah090-native-assets-full.txt`. Window
enumeration inside the sandbox did not observe the new process's fallback window, so
native desktop checks require the interactive desktop for this verification session.
