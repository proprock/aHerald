# Tray usage and restart

The first menu item displays `Codex: 5h 25% / 2:35, week 10% / 6d`.
`[usage] display = "remain"` is the default; `"used"` selects consumed percentages.
Unknown values use an em dash. Disabled, loading, unavailable and stale data are
explicit. Countdown units are whole days, H:MM below a day, and minutes below an
hour. Positive times round up to a minute; expired reset times show `0 min`.

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
Dates use Windows timezone/DST conversion. Last-update age and stale/unavailable
states remain visible. Long details wrap and page with the mouse wheel; arrow
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

## Verification (2026-10-25)

Cargo formatting, Clippy, all-target tests and Windows x64 release build were run.
Coverage includes controlled-time formatting boundaries, unknown/stale data,
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
