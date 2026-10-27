# Milestone 5 — Portable release evidence

## AH-020 — Placement and display changes

Both transient and resident cards share physical-pixel placement. `position`
accepts `center`, `top_center`, `bottom_right`, and `near_cursor`. `monitor` is
`primary` or a zero-based numeric string: primary first, remaining monitors sorted
by Windows device name. Numeric selection takes precedence over cursor location;
an absent index falls back to primary. With `primary` and `near_cursor`, select
the monitor nearest the cursor. A card does not chase subsequent cursor movement.

Effective monitor DPI scales the card and margins, in addition to the configured
scale. Fit the entire card into the monitor work area, retaining its aspect ratio.
Display/work-area/DPI messages recalculate placement without activation or a
screen-sized intermediate window. Resident hit testing maps physical coordinates
back to the card. Raster resize surfaces are cached until their dimensions change.

AH-048 changed two earlier choices (agreed with the user, option A):

- The card is drawn at the effective DPI of the target monitor (configured scale ×
  DPI scale) instead of at 96 DPI and stretched, so text is sharp at 125–200%.
  Artwork is prepared at the same scale with the configured interpolation, so pixel
  art keeps nearest-neighbor sampling and nothing else is resampled. A DPI, display or
  monitor change redraws the card (`WM_DPICHANGED`, `WM_DISPLAYCHANGE`,
  `WM_SETTINGCHANGE`); the resident worker prepares artwork for one render scale at a
  time. Artwork too large to prepare at the DPI scale is prepared at the configured
  scale instead of being dropped. Nearest-neighbor stretching remains only for a card
  larger than the work area (shrink to fit).
- A card keeps one base surface, one working surface and the shared artwork frames,
  not one full-card surface per frame. Changing frame, countdown second or session
  name redraws only what changed. The 32 MiB budget applies to the prepared artwork
  (checked when it is prepared) and to the two card surfaces, so long animations are no
  longer replaced by the text card.

Tests execute placement for negative desktop coordinates, all four positions,
scaled margins, tiny work areas, oversized cards and cursor arithmetic boundaries.
The native overlay test sends display/DPI/work-area messages, observes unchanged
geometry on the same display, verifies nonactivation and dismisses the child.
`preview_placement` is an opt-in visual preview with a test-only taskbar entry.
On 2026-10-25 the user observed this preview near the tray, and the preview test
completed successfully after dismissal. The desktop inspector did not return the
window before it closed; no screenshot inspection is claimed.

Physical monitor removal, mixed-DPI migration and Windows 10 still require manual
acceptance. AH-020 remains unchecked until those required checks are observed.

AH-020 automated verification on Windows 11 build 22621, Rust 1.98.1 MSVC x64:
cargo fmt --all -- --check; cargo clippy --all-targets -- -D warnings;
cargo test --all-targets; cargo build --release --target x86_64-pc-windows-msvc
all passed outside the sandbox. Six interactive/live tests remain ignored.
The sandbox run encountered a pre-existing stdio mock startup timeout; the
unsandboxed run passed without changing its assertions or timeouts.

## AH-021 — Bounded resources and recovery

The resident asset worker owns a 16-entry, 32 MiB LRU of prepared artwork, keyed
only by configured agent and signal. Active cards share immutable frames using
`Arc`; cache eviction never invalidates a displayed card. Missing/corrupt assets
are cached as text fallback. Restart the resident after replacing assets or
configuration. Loading the next resource can temporarily retain the old active
art and a new decode; individual decoded/prepared animations remain bounded to
32 MiB. The selected card's GDI surfaces and resized GDI surfaces each have a
32 MiB budget. Cache, active resources and decode scratch are separate budgets,
not a claim that the entire process uses only 32 MiB. PNG input is bounded to
16 MiB; animation metadata to 16 KiB; pet resources and WAV have their documented
independent limits. No new dependency or GUI runtime is introduced.

The resident IPC worker writes optional `aherald.log` beside its configuration.
`general.logging` controls filtering (`off` disables files). Only Unix seconds,
severity and fixed operational codes are permitted. No API accepts event data,
paths, remote error strings, account data or credentials. Rotate at 64 KiB into
one `aherald.previous.log`; at most 128 KiB is retained. Unwritable destinations
degrade silently without changing hook responses. File operations run on the
listener, outside the GUI thread and queue lock. Idle accept polls are normal
success, so an idle resident does not fill the error log. Queue rejection and
broken/malformed IPC use fixed warning codes; lifecycle start/stop use info.

Verification includes:

- Cache reuse by pointer identity, negative caching, source separation, byte and
  entry eviction, oversized values, and live references across eviction.
- Log filtering, real on-disk rotation through 6,000 records, both file limits,
  fixed record format, unwritable directory failure and subsequent recovery.
- 10,000 queue-flood/duplicate iterations with capacity and individual-session
  checks, followed by recovery after dismissal.
- An isolated native process renders 200 cards with repeated resize, checks GDI
  and process handle counts and thread count return to baseline, and bounds
  private-commit growth to 8 MiB after warmup. This is a bounded stress check,
  not an overnight memory profile.
- Actual resident termination, ownership release, restart and new IPC event
  delivery; existing tests cover broken/stalled/malformed IPC, corrupt/missing
  artwork with native text recovery, hook lifetime isolation and unavailable
  Codex transport without blocking notifications.

Red phases were observed for the absent cache/log modules and idle-accept
behavior. The cache/log focused tests and idle/IPC/crash tests then passed.
The resource-cycle and storm tests add safety evidence for existing behavior;
no failing pre-fix phase is claimed for those checks.

AH-021 verification on Windows 11 build 22621 with Rust 1.98.1 MSVC x64 passed:
`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all-targets`, and
`cargo build --release --target x86_64-pc-windows-msvc`. The full native suite ran
outside the sandbox. The final added LRU-recency/zero-capacity test also passed
in `cargo test --test resource_cache` (three tests), followed by fmt/clippy again.
Six manual/live tests remain ignored in the ordinary suite. No Codex protocol
or hook contract changed, so the existing tested alpha-version limitations apply.

The user confirmed on 2026-10-25 that a second mixed-DPI monitor and Windows 10
are unavailable. Those checks remain unverified. Proceeding with AH-021 does not
close AH-020's outstanding physical display acceptance. Work pauses before
AH-022 at the user's request; no portable ZIP has been produced in this slice.
