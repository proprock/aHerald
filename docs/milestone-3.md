# Milestone 3 verification

Environment: 2026-10-24, Windows 11 (build 22621 from prior milestone evidence),
Rust MSVC x64; installed `codex-cli 0.164.0-alpha.11.1` queried again for this slice.
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

These are real aHerald process invocations with sanitized input, not observed
dispatch by Codex. The normal-user `/hooks` trust review and live Codex lifecycle
smoke check have not been performed. AH-014 remains unchecked until that manual
acceptance passes; no global hooks/configuration were edited.

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
