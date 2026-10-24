# Milestone 3 verification

Environment queried 2026-10-24: Windows 11 build 22621,
`rustc 1.98.1 (48a229cea 2026-09-01)` MSVC x64;
installed `codex-cli 0.164.0-alpha.11.1`.
See [versioned compatibility](codex-compatibility.md) and
[manual hook setup](codex-hooks.md). Windows 10 and stable Codex remain untested.

## AH-014 — hook delivery

Added quoted Windows hooks.json example, synthetic permission/interrupt fixtures,
and setup/trust/removal guidance preserving existing hooks. Regression red:
`cargo test --test hooks stop_response_survives_invalid_configuration` returned
empty stdout instead of `{}`. Green: contract response now precedes configuration
loading and delivery. No permission response is emitted.

`cargo test --test hook_lifecycle --test hooks -- --nocapture` passed.
Actual handler processes returned in 140–231 ms for Stop, PermissionRequest and
Interrupt with both 30-second and indefinite native child lifetimes; each child
remained alive after its parent returned and was cleaned up by the test.

Real Codex dispatch was then verified with the installed version above using an
isolated temporary CODEX_HOME/project and a local mock Responses SSE provider.
A thread owned by this check completed a harmless synthetic response (`Stop`), then
requested escalation (`PermissionRequest`) and was interrupted (`Interrupt`).
No approval response or tool execution was supplied. Hook definitions were trusted
by their exact current hashes in the temporary profile only. No global hooks or
configuration were edited; no account credentials or unrelated threads were accessed.

All three actual hook executions completed successfully through PowerShell in
1648/907/878 ms respectively, within the three-second deadline. Their detached
30-second notification processes remained alive after hook completion. The
executable was copied to a path containing spaces; owned fixture processes were
terminated after observation. This verifies real Codex dispatch with synthetic
model responses, not a live model/account or the normal-user `/hooks` trust UI.

This check exposed a collector-handle leak: an overlay inherited unrelated
inheritable output handles and kept Codex waiting until timeout. Regression red:
`detached_child_does_not_inherit_unrelated_hook_collector_handles` did not observe
EOF while the overlay lived. Green: explicit Win32 handle allowlists inherit only
the two transfer handles; resident startup inherits none. The regression now
observes EOF while the indefinite notification remains alive. A separate
PowerShell process test covers responses and paths containing spaces/apostrophes.
The Windows example retains `| Out-String` so PowerShell waits for the GUI handler
and captures its response. AH-014 acceptance is complete with these limits.

The final native-process run also exposed a startup race: a losing resident
launch could remain in flight after `ensure` returned another process's status
and acquire ownership after that winner was quit. The existing
`resident_startup_races_status_forwarding_and_quit` failed its ownership-release
deadline. `ensure` now terminates and reaps its own losing startup process before
returning the winner's status; the focused resident/process checks then passed.

Full `cargo test --all-targets` passed (three existing interactive tests ignored).
Clippy initially reported one collapsible `if`; it was simplified and
`cargo clippy --all-targets -- -D warnings` then passed. Format check passed.
`cargo build --release --target x86_64-pc-windows-msvc` passed (35.27 seconds).

## AH-015 — input-wait investigation

No stable passive source is available for the sessions in scope; the accepted
unsupported-source path is documented in `codex-input-waits.md`. No heuristic or
private app-server monitor was introduced. Red: the process diagnostic test found
only a generic "later milestones" message. Green: `cargo test --test
codex_monitoring --test hooks --test cli` passed with explicit unsupported-session
diagnostics and successful event delivery. The diagnostic also precedes resident
startup. Clippy and diff hygiene passed. Full checks follow AH-016.

## AH-016 — pending/terminal correlation

Regression tests initially failed because completion did not clear matching waits
and two unidentified permission requests were incorrectly merged. Added exact
agent/session/turn matching, optional subagent identity, and bounded interrupted-
turn history. A second red/green cycle verified that Stop must allow subsequent
waits if another hook continues the same turn. Unsupported individual approval
resolution and ambiguous ordering remain explicit; no source is invented.

`cargo test --test codex_lifecycle --test queue` and `cargo test --test resident`
passed. Tests execute multiple sessions/requests, duplicate delivery, missing
identity, subagent isolation, interrupted-turn late arrival, continued Stop turns,
history bounds, selection preservation, manual dismissal, and failed tool input.
Native IPC also verifies resolution and late-event suppression when terminal
presentation is disabled. Existing queue coverage now treats demo and Codex as
different agents; this deliberately removes the old false duplicate across them.

Runtime hooks lack request IDs; separate permission boundaries cannot safely be
deduplicated. Pending state remains in the resident only; transient windows keep
manual/timer dismissal. The normal-user trust UI remains a manual setup step;
isolated real dispatch was verified as recorded above.

Final checks after the Stop-continuation and collector-handle regression fixes:
`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all-targets`, and `cargo build --release --target
x86_64-pc-windows-msvc` all passed. Three pre-existing interactive tests remain
ignored. The optimized build produced
`target/x86_64-pc-windows-msvc/release/aherald.exe`.
