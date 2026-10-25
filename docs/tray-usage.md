# Tray usage and restart

The first menu item displays `Codex remain: 5h 25% / 2:35, 7d 10% / 6d 11h`.
`[usage] display = "remain"` is the default; `"used"` selects consumed percentages.
Unknown values use an em dash. Disabled, loading and unavailable states are
explicit. Last received values stay current regardless of age or refresh errors.
Countdowns show whole days and remaining whole hours from one day onward
(`1d 0h`, `6d 11h`), H:MM below a day, and minutes below an hour. Below a day,
positive times round up to a minute; expired reset times show `0 min`.

Clicking the summary creates/selects one persistent card in the same queue and
window as agent notifications. It opens beside the tray even with event visuals
disabled or paused. Incoming events remain queued, retaining their own visible
lifetimes. The counter and arrows navigate between cards. Body click dismisses
only usage and selects the next card. Selecting the summary again while usage is
selected also dismisses it. There is one reserved usage slot beyond the bounded
agent queue; opening usage cannot evict an agent event. This internal card cannot
be supplied through hook or IPC JSON and never opens an agent application.

The card includes the plan and all buckets, both limit windows with explicitly
labelled used percentages, short local reset dates (`YYYY-MM-DD HH:MM`), workspace
credits, ordinary-usage availability and all available reset credits with expiry.
Dates use Windows timezone/DST conversion. Last-update age, the latest refresh
error and unavailable fields remain visible. Long details wrap and page with the mouse wheel; arrow
buttons always navigate the shared notification queue. No account mutations are
available. Polling remains automatic at `refresh_interval_sec`.

Edit configuration uses the Windows TOML association, falling back to the system
Notepad only when there is no association. Paths are passed as data, not shell
commands. An open failure leaves the resident running and reports an error.

Restart starts a detached replacement and verifies that it has opened a handle
to the old process before requesting shutdown. It waits up to 30 seconds for that
process to exit, including worker and singleton cleanup, then reuses the original
mode and configuration arguments. Runtime Sound/Visual/Pause overrides and queued
cards are discarded. Repeated restart strips internal handoff arguments.

## Dynamic tray icon (AH-034)

Two vertical columns show primary/5h on the left and secondary/week on the right.
The Codex bucket is preferred, with the first available bucket as fallback.
Fill follows `[usage] display`: remaining quota by default, consumed quota for
`used`. Color always uses remaining quota: greater than 60% green, greater than
20% yellow, otherwise red. A missing, invalid or disabled value is solid gray.
A zero level keeps a one-pixel colored baseline so it remains distinguishable
from unavailable data. The unfilled track is dark gray, with transparent padding
and a transparent gap between columns.

The icon uses the taskbar's DPI and Windows small-icon metric (normally 16 pixels
at 96 DPI). Usage updates are checked with the existing one-second presentation
refresh; unchanged pixels reuse the native icon. Replaced icon and temporary GDI
resources are released. `TaskbarCreated` restores the latest icon, with retry
while Explorer is unavailable. A failed refresh preserves known levels in the
icon, tooltip, menu and card; age and error remain diagnostic details.

Verification on Windows 11 build 22621.4317: formatting, Clippy, all-target tests
and the Windows x64 MSVC release build passed. Functional tests cover threshold
boundaries, both fill modes, malformed/missing data, cached values and raster
sizes. Native tests inspect actual HICON dimensions/colors, handle reuse and
invalid-update preservation. The resident TaskbarCreated test and an isolated
delete/re-add test verify restoration of the current icon and shutdown cleanup.

A native DrawIconEx preview was visually inspected at 16, 20, 24 and 32 pixels
on light and dark backgrounds in both display modes. Reproduce with
`cargo test --lib tray::native_tests::manual_dynamic_tray_preview -- --ignored --nocapture`.
The isolated Explorer registration test is
`cargo test --lib tray::native_tests::explorer_restore_keeps_current_dynamic_image -- --ignored --nocapture`.
These checks use synthetic data; an actual Explorer process restart, moving the
taskbar between different-DPI monitors and Windows 10 were not exercised.

## Card and restart verification (2026-10-25)

Cargo formatting, Clippy, all-target tests and Windows x64 release build were run.
Coverage includes controlled-time formatting boundaries, unknown and cached data,
every credit remaining reachable, Unicode wrapping, native shared-window behavior,
queue preservation, and an isolated process restart with a spaced configuration
path, changed PID, reloaded settings and singleton ownership.

Synthetic native preview was visually inspected: the card starts with
`Codex plan: plus [1/2]`, without a separate Usage heading; short dates and both
reset credits fit. Clicking the queue arrow showed `Permission required [2/2]`
in the same window. Preview uses mocked account data and is isolated from the
real resident. Reproduce with `cargo test --lib manual_usage_card_preview --
--ignored --nocapture`; dismiss both cards to close, or wait for its bounded timeout.
Native menu tests verify labels, item count and checkmarks. Automatic tests do not
launch an external editor or change the user's Windows file associations.
