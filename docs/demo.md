# Configuration demo verification

Update 2026-11-01: Windows **Apply + Test**, after Edit config, validates the
saved file, restarts the resident and shows all seven samples in its existing
window. **Apply** restarts without samples. Invalid configuration leaves the old
resident running. Both actions discard its previous queue.

Samples stay in a separate fixed list, marked [Test]; disabled event types are
labelled. Arrows and body clicks navigate without removing cards. Timeouts advance
without an empty phase or inter-card fades, using five seconds for persistent
types. Dismiss/Escape/window close or a real notification ends the test; the new
configuration remains active. See [tray controls](tray-usage.md).

Controlled-time carousel tests cover disabled types, wrapping, configured timing,
manual timer resets and pause. Native process tests observe a changed PID,
singleton ownership, the seven-card counter, backwards navigation and body clicks,
closing without terminating the resident, real notification interruption, a later
ordinary Apply without samples, and invalid-file recovery for both menu actions.
The explicit standalone --demo command retains the legacy scenarios below; it is
no longer launched by the tray test action.
Warning samples use the same numeric labels as real warnings, with the harness
only in the heading. Prefer the Codex threshold, or Claude when no Codex threshold
is configured. With neither context threshold configured, the synthetic sample
uses 250k; it does not enable a live warning. Idle samples likewise prefer Codex's
configured minutes, then Claude's (55 minutes if both are disabled).

Apply/carousel verification, 2026-11-01 on Windows 11: formatting, Clippy and the
Windows x64 release build passed. The serial all-target test run passed every
target except `perf_baseline`: `artwork.prepare` averaged 43.62 ms against its
30 ms budget; the budget was not changed. The five restart integration tests and
two controlled-time carousel tests passed. An opt-in native preview test exposed
only its owned fixture window to Computer Use: visual inspection confirmed the
[Test] heading, seven-card counter, arrows, countdown, context/idle samples and
Dismiss. Arrow/body navigation and dismissal were exercised; after dismissal,
the fixture confirmed the resident remained alive and ordinary Apply restarted
without repeating the test. The opt-in test passed. Mixed DPI, additional monitors
and Windows 10 were not manually checked.
Portable clean-extraction verification and the isolated installer lifecycle check
also passed (`aherald-installer-test-07c2772b08f94af0b7fd17bb14f56f01`).

AH-026, verified 2026-10-25 on Windows 11 build 22621, Rust 1.98.1,
MSVC x64. No new Codex integration was added; no live Codex session or credentials
are needed. Windows 10 and mixed-DPI/multiple monitors were not tested here.

## Behavior

`aherald.exe --demo [SECONDS] [--config PATH]` repeats indefinitely until Exit in
its own tray. The optional positive whole number controls the quiet interval
between scenarios, default 3 seconds (maximum 86400). The sequence contains every
enabled event, then a group of those events for navigation/count/dismissal. At least
one event must be enabled. The configured mode does not route demo events to the
normal resident; explicit mode flags and other operations conflict with `--demo`.

Each scenario has a minimum three-second presentation budget, extended to the
configured event duration plus fade-out if longer. Individual finite events still
expire normally. Individual persistent events clear when their scenario ends.
The grouped scenario gives each card its own minimum three-second preview budget
(or its configured duration if longer), including normally persistent waits.
Only the selected card's timer advances: three cards become `[1/3]`, `[1/2]`,
then a final card without a counter, each with its own countdown. The window hides
after the last card expires; only then does the quiet interval start. Manually
dismissing a grouped card removes only that card. Tray pause freezes
the cycle; dismissing a card continues the cycle. Presentation/audio toggles and
configuration values remain effective; a completely hidden/muted configuration
does not get forcibly enabled. Runtime changes are not written to configuration.

This uses the resident notification engine, its bounded asset cache and tray,
without a resident mutex or pipe listener. It can coexist with the ordinary resident;
`--status` and `--quit` continue to address that resident. Demo events retain the
explicit `aHerald demo` source. Artwork selects the demo agent configuration or,
when absent, the configured Codex artwork, with normal per-event/shared precedence.
Usage is real, optional, and follows the existing configured usage service.

## Automated evidence

- Red: CLI rejected `--demo` as unknown; the scenario tests then failed on the
  empty queue and missing all-disabled validation before implementation.
- `cargo test --test cli --test demo`: CLI default/custom pause, invalid values,
  operation conflicts, bounded repetition, all five signals, grouped navigation,
  disabled events, persistent/long durations, and pause were verified. An additional
  `cargo test --test demo` passed exact custom-pause boundaries and missed-tick
  behavior after the full run.
- `cargo test --all-targets -- --test-threads=1`: passed all non-ignored tests.
  The new native demo test launched the executable, observed the source/signal/count
  title and nonactivating/topmost/layered styles, verified ordinary resident status
  did not identify the demo PID, then closed the owned window and observed a
  successful bounded process exit.
- The first parallel full run failed the existing
  `audio::native::tests::canceled_audio_does_not_wait_for_the_serial_gate` 300 ms
  wall-clock bound. It passed alone and in the sequential full suite; no audio
  assertions or time limits were changed. A first native assertion expected only
  the initial window title; it was corrected to assert the rendered source, signal
  and count that the shared engine actually presents.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo build --release --target x86_64-pc-windows-msvc`: passed. Output:
  `target/x86_64-pc-windows-msvc/release/aherald.exe`.
- `git diff --check`: passed.

## Manual evidence and limits

Ran `cargo test --test native_overlay preview_demo -- --ignored --nocapture`
on the interactive desktop with `examples/resources.toml`. The helper exposes only
its owned demo window to desktop capture. Originally it used a three-minute bound;
it now exits after one complete cycle, including draining the grouped cards,
with a 45-second safety deadline;
the actual CLI demo has no time limit. Observed the native card with source/signal,
external artwork and Prev/Next/Dismiss controls. The user confirmed audible sound
and visible cycling windows/artwork.

On a cold cache the text card appears before asynchronous artwork decoding
finishes; subsequent displays use cached resources. Audio runs independently of
window visibility. The user observed an interval with sound but no card during
test-process cleanup, then confirmed visible presentation was working. This is
recorded as an observation, not proof of its cause. Long audio is not truncated at
scenario boundaries; existing serialized playback and shutdown cancellation apply.
No claim is made here about a full Explorer restart or every tray action manually
tested in this demo; existing automated tray/usage tests remain part of the suite.

AH-027 subsequently replaced the controls with conditional count/navigation and
body dismissal, and added countdowns, event colors and pet state overrides. See
[current card behavior and verification](card-controls.md).

## Grouped expiry regression

The regression test `grouped_messages_expire_one_at_a_time_with_a_fresh_timeout`
failed before the fix: the group contained zero cards after three seconds instead
of two. It now checks the exact timeout boundaries, fresh selected-card clocks,
remaining counts and the quiet interval after the final expiry with controlled
time. The repeat/navigation test also drains cards individually over three cycles.

Visual inspection then exposed a second issue: grouped persistent waits expired
but had no countdown, because rendering read the configured zero duration instead
of the queued preview lifetime. Rendering now derives countdown and fade timing
from the entry lifetime, excluding fade-out. A focused test checks `3s`, `2s`,
`1s` and fade opacity for all five grouped cards, including normally persistent
waits. On Windows 11 build 22621, the corrected permission card was observed with
`[1/4]` and `1s`; the one-cycle preview also closes after draining the group.

The ordinary full test command hit the existing three-second PowerShell invocation
deadline in `powershell_gui_invocation_collects_hook_responses_with_quoted_paths`,
also when run alone. Its timeout and assertions were left unchanged. The remaining
suite is verified separately with that known failure explicitly excluded; this is
not a claim that the unfiltered full suite passes.
After the countdown correction, formatting, clippy, focused demo/queue/native/
resident tests and the full suite with that single explicit exclusion passed.
The corrected one-cycle visual preview completed successfully in 37.47 seconds.
The final MSVC x64 release build passed; the executable and adjacent resources
are in `target/x86_64-pc-windows-msvc/release/`.
