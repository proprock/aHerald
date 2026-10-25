# Card controls and event appearance (AH-027)

Cards are informational, including permission/input notifications. Closing one
never responds to an agent request. With the default `click_to_dismiss = true`,
any click except a navigation arrow dismisses the selected event. Dismiss is hidden.
With body dismissal disabled, an explicit Dismiss button remains available.

A single event has no count or navigation. Multiple events show `[index/count]`
beside the source and `<` / `>` at the bottom right. Navigation keeps events queued.
The resident and transient renderer share button geometry and hit testing.

Finite events display ceiling remaining seconds at the top right, without `0s`.
The badge disappears at expiry and the configured fade-out completes dismissal.
Persistent waits have no timer. Resident countdowns use selected visible time,
so switching away or pausing preserves the remaining time.

The existing `[events.NAME].duration_ms` controls every event type, including
`permission_required`; there is no separate permission timeout. A positive value
expires the card after that many milliseconds of selected visible time, followed
by the configured fade-out. Zero explicitly requests persistent display. For
example, `[events.permission_required] duration_ms = 3000` gives a three-second
notification regardless of whether Codex has already received an approval answer.

Set `color = '#RRGGBB'` under `[events.NAME]` for the accent stripe. Defaults:
completion `#41D28C`, permission `#FAB437`, input `#50A5FA`, error `#F55564`,
interruption `#B482F0`. This does not recolor external sprites.
Set `[notifications] background_color = '#RRGGBB'` for the shared card background
(default `#1C232D`). An optional `background_color` in `[events.NAME]` or `[codex.usage]`
overrides it for that card type; omitted values inherit the global setting.
The existing card opacity, transparent rounded corners, accent stripe and sprite
colors are preserved. This applies in resident, transient and demo modes, including
background title updates. Text remains its existing light color; choose backgrounds
that keep it readable. Restart a running resident after editing configuration.
See `aHerald.example.toml` and [pet state overrides](pets.md#per-event-state-overrides).

## Background color verification (AH-035, 2026-10-25)

The configuration test first rejected background_color as an unknown field.
The native pixel test then failed because the renderer still used its fixed
background. Both now pass. Tests cover global defaults, event/usage inheritance,
overrides, invalid HEX values, premultiplied native pixels, rounded-corner alpha,
unchanged accent colors and background preservation after a heading update.

The native DIB comparison preview was visually inspected on Windows 11 build
22621.4317: global green, event override burgundy and usage override blue were
distinct, with readable unchanged text and accent stripes. Run
`cargo test --lib manual_background_color_preview -- --ignored --nocapture`
on the interactive user desktop to reproduce the bounded 60-second comparison.
It displays the actual card surfaces in a comparison window; it does not exercise
queue navigation or change the running resident. Formatting, Clippy, all-target
tests and the Windows x64 MSVC release build passed. Windows 10 and mixed-DPI
multi-monitor behavior were not rechecked for this setting.

## Verification, 2026-10-25

Red: executable card tests failed against stubs for single/multiple controls,
body dismissal, explicit dismissal and countdown boundaries. Configuration and
pet extraction tests rejected the new fields; additional standard track names
also failed before support was added. Green: the tests pass with actual atlas
pixels, frame counts and durations. The native resident test clicks the new arrow,
observes selection without removal, then dismisses down to a single unnumbered card.

Native DIB tests inspect the rendered accent color, hidden/visible footer regions,
different `3s` / `2s` / `1s` text pixels, unchanged base cards and premultiplied alpha.
Repeated countdown rendering and DPI resizing release owned GDI resources.

Passed on Windows 11 build 22621, Rust 1.98.1:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --all-targets -- --test-threads=1`: 108 passed, 7 opt-in checks ignored.
- `cargo test --test config`: passed after adding explicit parsed/default color assertions.
- `cargo build --release --target x86_64-pc-windows-msvc`

Visual check used `cargo test --test native_overlay preview_demo -- --ignored --nocapture`
on the interactive desktop. Inspected the actual card: a single completion has
no counter/footer and a top-right `2s`; the grouped scenario has `[1/5]`, a `3s`
badge and bottom-right `<` / `>`. The green stripe matches completion artwork;
a separate error card shows a red stripe matching its artwork and a `2s` badge.
The preview exposes only its owned window for capture and closes after three
minutes. Normal demo runs indefinitely. No new claims about Windows 10,
multiple monitors, mixed DPI, Explorer restart or every tray action are made here.
