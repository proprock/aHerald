# AH-100 context warnings — 2026-10-31

## Contract and implementation plan

Keep independent optional thresholds in `[claude.warnings]` and
`[codex.warnings]`. `N`, `"N"` and `"Nk"` mean integer tokens, with `k = 1000`.
Omission disables that agent's context warning. Warn strictly above the threshold,
once per crossing; a valid reading at/below it rearms. Unknown readings do not
rearm. Key bounded, memory-only resident state by agent and session. Reuse the
existing warning engine, queue and native card; do not add a timer process.

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
PowerShell recorder. These runs do not establish notification delivery.

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
