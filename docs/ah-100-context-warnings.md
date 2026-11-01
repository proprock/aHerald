# AH-100 context warnings — 2026-11-01

## Contract and implementation plan

Keep independent optional thresholds in `[claude.warnings]` and
`[codex.warnings]`. `N`, `"N"` and `"Nk"` mean integer tokens, with `k = 1000`.
Omission disables that agent's context warning. Warn strictly above the threshold,
once per crossing; a valid reading at/below it rearms. Unknown readings do not
rearm. Key bounded, memory-only resident state by agent and session. Reuse the
existing warning engine, queue and native card; do not add a timer process.

Warning cards keep the harness name in their heading. Their main line includes
the measured context and that agent's threshold, for example
`Context: 250.001k > 250k` (`k = 1000`, up to three fractional digits, without
rounding away a crossing). Idle cards show elapsed time and the agent's configured
limit, for example `Idle: 27m 30s (limit 25m)`. Values are captured when the warning
is created and stay fixed while queued; the card's age shows delivery age. No
harness name is repeated in the warning text.

Numeric-label verification (2026-11-01): regression tests first observed generic
labels, then passed for both agents' independent thresholds, exact fractional
kilotokens, delayed/paused idle delivery, queue snapshots and event serialization.
Carousel tests passed for configured Codex/Claude values. Formatting and Clippy
passed. The full serial all-target run passed all targets except `native_assets`
(notification startup deadline) and `perf_baseline` (`artwork.prepare` 39.82 ms
against 30 ms). With the interactive carousel finished, isolated `native_assets`
passed; no deadline or performance budget was changed. The opt-in native preview
passed, and Computer Use visually confirmed that the numeric idle label fits with
the harness appearing only in the heading. No new live hook integration was added.

Work order: verify sources and live dispatch; test adapter/configuration/state;
implement Codex observation and explicit hook setup; test IPC and hook lifetime;
build Windows release; verify real-session delivery; document limits. No new
dependency, credential access, transcript parsing or global configuration edit.

## Versioned evidence

Windows 11; Codex CLI `0.166.0-alpha.5.1`; Claude Code `2.1.311`.

Claude's documented `context_window.current_usage` reports the most recent API
input, including cache creation and cache reads. A null report before a response
or following compaction is unknown. aHerald sums the three input counters, excludes
output and retains no other context fields.
[Claude statusLine](https://code.claude.com/docs/en/statusline#context-window-fields).

The current [Codex hooks guide](https://learn.chatgpt.com/docs/hooks) does not
describe `PostModelRequest`. The matching release does publish its
[input schema](https://github.com/openai/codex/blob/rust-v0.166.0-alpha.5.1/codex-rs/hooks/schema/generated/model-request.command.input.schema.json)
and [observer handler](https://github.com/openai/codex/blob/rust-v0.166.0-alpha.5.1/codex-rs/hooks/src/events/model_request.rs).
Observer output is diagnostic and cannot decide approval or continuation. Support
is versioned to the tested CLI, not a claim of a generally documented stable hook.

The matching [request loop](https://github.com/openai/codex/blob/rust-v0.166.0-alpha.5.1/codex-rs/core/src/session/turn.rs)
copies provider usage for the individual completed request into the post hook,
not cumulative session spending. aHerald uses only nonnegative `input_tokens`
after `status = completed` and rejects subagent evidence. Cached input is already
part of input usage, so it is not added again. Failed/cancelled/null reports do
not reset warning state. This measures the last reported request input, not a
prediction of the next request or an exact live context percentage.

The source-only live probe returned `OK`, observed both request hooks and a
completed main-session report with 15,710 input tokens and 5 output tokens.
Evidence: `%TEMP%\aherald-codex-context-a010c354d2ed49fa8670b59fcc79933d`.
The first sandbox run timed out without hook evidence; the successful run used
the normal account outside the sandbox and the corrected, separately checked
PowerShell recorder. This source-only probe does not establish notification delivery;
the separate delivery checks below do.

## Real-session delivery

Both checks used the release `aherald.exe`, a private resident namespace, thresholds
of one token, an initially empty queue, visuals enabled, sounds/usage polling off,
and the bridge invoking production parsing and IPC. Each ended with exactly one
queued warning and cleaned up its owned processes. Global hook configuration was
unchanged.

- Codex CLI `0.166.0-alpha.5.1`: real completed model request delivered the warning.
  Evidence: `%TEMP%\aherald-context-live-155a7dd2b07b46fdbeac56993a921883`.
- Claude Code `2.1.311`: interactive response `OK`; statusLine first reported null,
  then 3,208 input tokens; the resident queued one warning.
  Evidence: `%TEMP%\aherald-context-live-526a0a464b0f49249223b876cc998683`.
  The prompt was entered manually after launch, then `/exit` completed the probe.
  The script now separates the positional prompt from variadic options with `--`;
  that launch adjustment was syntax-checked, not repeated against the live account.

The initial Claude launch used a print-only persistence flag and failed before
observation; removing that flag allowed the interactive statusLine check. Claude
itself persists its ordinary interactive session. aHerald does not read it.
Computer Use did not expose an aHerald window, so card appearance and focus
behavior were not visually verified. Queue delivery is the observed result.

## Automated verification

Adapter/configuration/state tests cover notation, invalid input, strict threshold
boundaries, rearming, unknown data, cached-token accounting, independent settings,
and agent/session isolation. IPC round-trip and hook process checks cover delivery
and prompt, silent completion. The new adapter tests had an observed failing phase
before implementation, then passed.

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo test --all-targets`: stopped at `perf_baseline`:
  `hot_paths_stay_inside_their_baseline_budgets`, artwork preparation averaged
  44.34 ms against a 30 ms budget. No threshold was changed.
- Remaining integration targets were run separately. `tray_about` failed in
  `EnumWindows` at `tests/tray_about.rs:33`; no root cause was established.
  All other executed targets passed, including the later integration targets and
  `cargo test --examples`. Platform/manual tests retain their explicit ignores.
- `cargo build --release --target x86_64-pc-windows-msvc --bin aherald --example session_warning_bridge`:
  passed; both live checks used the native release resident.

The full suite is therefore not green. Neither failing check was modified or
repeated without a changed condition; these failures remain outside this slice.

## Coverage limits

Codex sessions must run locally with the explicit `PostModelRequest` binding;
rerun reviewed native setup to add it to an existing installation. No automatic
hook edit occurs. Untested versions, desktop dispatch, remote/cloud sessions,
compaction dispatch and subagent delivery are not claimed. Hooks return promptly;
context observation never starts or waits for a resident's UI lifetime. Both
agents require an already running resident for context warnings.

The private live bridge exercises production parsing and IPC against the native
release resident. It isolates other sessions but is not the installed hook
command. Queue evidence and visual/focus evidence must be reported separately.
No real payload is saved: evidence retains identity and numerical counters only.
