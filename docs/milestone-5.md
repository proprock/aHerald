# Milestone 5 — Portable release evidence

## AH-020 — Placement and display changes

Both transient and resident cards share physical-pixel placement. `position`
accepts `center`, `top_center`, `bottom_right`, and `near_cursor`. `monitor` is
`primary` or a zero-based numeric string: primary first, remaining monitors sorted
by Windows device name. Numeric selection takes precedence over cursor location;
an absent index falls back to primary. With `primary` and `near_cursor`, select
the monitor nearest the cursor. A card does not chase subsequent cursor movement.

Effective monitor DPI scales the composed card and margins, in addition to the
configured scale. Fit the entire card into the monitor work area, retaining its
aspect ratio. Display/work-area/DPI messages recalculate placement without
activation or a screen-sized intermediate window. Resident hit testing maps
physical coordinates back to the card. Raster resize surfaces are cached until
their dimensions change. Each source animation and its resized surfaces have a
32 MiB budget; DPI enlargement is capped to that budget. Display scaling uses
nearest-neighbor sampling of the premultiplied composed card; the configuration's
interpolation setting still controls original artwork preparation.

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

