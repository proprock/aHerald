# Session warnings feasibility — AH-100 / AH-101

Preliminary research, 2026-10-30; implementation notes below.
AH-100 follow-up (2026-10-31): real Codex `PostModelRequest` dispatch and per-request
input usage are now verified for `0.166.0-alpha.5.1`. The original deferred-source
conclusion below is historical. See [current contract, evidence and limits](ah-100-context-warnings.md).

## Evidence and limits

Installed versions: `codex-cli 0.166.0-alpha.5.1`; Claude Code `2.1.308`.
`codex app-server generate-json-schema --out <temporary-directory>` succeeded
without `--experimental`. Its `v2/ThreadTokenUsageUpdatedNotification.json` has
`threadId`, `turnId`, and `tokenUsage`: `last`, `total`, and optional
`modelContextWindow`. The breakdown includes input, cached input, cache-write,
output, reasoning output and total tokens. `ThreadReadResponse` exposes no
equivalent token-usage snapshot. Schema availability does not prove passive
access to another client's running session.

The repository's existing hook verification targets an older Codex alpha
(`0.164.0-alpha.11.1`, see [compatibility](codex-compatibility.md)). New hooks
still require matching-release schema and runtime verification before support
can be claimed. No prompts, transcripts, credentials or real session payloads
were read during this investigation. No new captured fixtures exist.

## Implementation update (2026-10-30)

The maintainer approved the first slice on a separate branch and selected the end
of the agent response as the idle timer origin. Branch:
`feature/ah-100-101-session-warnings`. Claude context only; separate Claude/Codex
idle thresholds; actual cache expiry and Codex context remain outside this slice.
The corresponding AH-059 input-context counter extension is recorded in AGENTS.md.
Updated defaults agreed on 2026-10-30: Claude idle 55 minutes, Codex idle 25 minutes;
`idle_minutes = 0` disables the respective timer. Omitted values and partial warning
tables retain the agent's default. Context warnings remain opt-in.
See [configuration and upgrade instructions](configuration.md#session-warnings-ah-100--ah-101).

The implementation keeps up to 64 sessions, with at most one pending warning of
each kind per retained session. Full-queue warnings retry without repeating the
rejection log. A later Stop rebases a still-pending deadline/warning, including
pure-model continuation without tool calls. Already admitted idle warnings do
not repeat until fresh activity establishes a new episode. Unknown context does
not rearm the threshold; low valid context does. IPC v2 prevents activity-only
evidence from becoming completion cards in an older resident.

The exact installed Codex tag's generated input schemas were read for
UserPromptSubmit, PreToolUse, PostToolUse, Stop and SessionEnd:
[rust-v0.166.0-alpha.5.1 schemas](https://github.com/openai/codex/tree/rust-v0.166.0-alpha.5.1/codex-rs/hooks/schema/generated).
New `codex-user-prompt-submit.json` and `codex-session-end.json` fixtures under
`tests/fixtures` are synthetic full-schema examples, not captured sessions.
Process tests verify that new activity/end hooks return successfully and never
write an approval or continuation decision. New tool hooks incur one process
invocation each per tool boundary, even when idle warnings are disabled; disabled
activity forwarding returns without contacting a resident.

**Research correction:** the newer alpha's `model-request.command.input.schema.json`
also contains PostModelRequest with token_usage and session/turn/agent identity.
This is a promising Codex context source that the earlier app-server-only
investigation missed. It is not established here as a documented stable hook and
was not exercised against a model provider. A separate versioned compatibility
investigation should test its dispatch, semantics and release support before
revisiting the deferred Codex context scope. No transcript parsing or telemetry
collector is necessary if that hook becomes a verified supported interface.

### Manual acceptance still required

On each claimed agent integration, quit the older resident before installing the
new binary (its tray Quit still works), preview/review the updated hooks and
statusLine setup, and use a separate test configuration. Set Claude context to a
small threshold and idle to one minute; set Codex idle to two minutes. Verify:

1. A real Claude response crosses the token threshold and produces one named
   context card; repeated high reports do not repeat it. Compaction plus a new
   low reading rearms it; absent data does not.
2. Complete turns in two sessions, then send a new message in only one. Only the
   other idle session warns, at its own agent's deadline; a later completed turn
   starts a fresh episode. No card steals focus or clips its text.
3. Close a session before its deadline, pause/resume the resident, and restart it.
   Closed sessions do not warn, paused warnings wait, restart needs fresh evidence.
4. Check ordinary permission/completion cards and hook stdout remain unchanged.
   Include a continued Stop when another hook is installed. Pure-model continuation
   before the next observable boundary remains a documented timing limitation.

No user hooks, statusLine settings, credentials or installed application were
modified. A transient macOS preview was attempted but that existing path returns
`Windows is required`; readiness alone was not counted as visual acceptance.
Native card appearance and real Claude/Codex hook dispatch remain unverified.

### Verification evidence

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --offline --all-targets -- -D warnings`: passed.
- `cargo check --offline --all-targets --target x86_64-pc-windows-msvc`: passed.
- `cargo build --offline --release --target x86_64-pc-windows-msvc`: reached
  linking, failed because this macOS host has no MSVC `link.exe`; no Windows
  release executable was produced.
- Focused configuration, adapter, hook-process, queue, IPC and demo tests passed.
  Thirteen controlled-time engine/resident tests cover isolation, strict boundaries,
  pause, unknown context, cleanup, bounded state, continuation and full-queue recovery.
- Full-suite sandbox execution hit existing macOS process/Keychain restrictions.
  Outside sandbox, a parallel run hit the existing inheritable-descriptor test
  (descriptor-number reuse); the subsequent serial full suite passed. Final
  command: `cargo test --offline --all-targets -- --test-threads=1`.
- Isolated native resident check on this MacBook, using two synthetic sessions
  and no account access, sound or visible windows: the queue started empty;
  both one-minute idle reminders were queued by the actual native loop;
  activity in one session and SessionEnd in the other independently removed
  their reminders. The owned resident was shut down. Evidence directory:
  `/private/tmp/ahwarn-g3me46gh` (temporary, not a packaged artifact).

Windows runtime/UI and Linux runtime were not exercised. No installed hooks,
global settings, autostart entries or existing resident were modified. No commit,
push, tag or release was created.

## Feasibility matrix

| Feature | Claude Code | Codex CLI / desktop sessions |
| --- | --- | --- |
| Context exceeds N tokens | Feasible through statusLine; live verification pending | No usable passive source established within the current contract |
| More than N minutes since user submission | Feasible with UserPromptSubmit | Feasible where the documented UserPromptSubmit hook is supported and trusted |
| More than N minutes after turn stop | Feasible with Stop, with error/interruption handling | Feasible with Stop/Interrupt; Stop can be followed by continuation |
| Exact time since every message | Needs additional activity coverage | Not established by lifecycle hooks alone |
| Provider cache expiry | Claude-reported cache deadline is a candidate | No per-session cache-expiry source established |

Coverage follows the actual integration, not the OS alone. Claude Desktop's Code
sessions need separate validation of statusLine delivery; regular Claude chats
are not covered. Remote/cloud sessions cannot be assumed to invoke local hooks.

## AH-100: context size

Claude's documented statusLine includes `session_id` and
`context_window.current_usage`. Sum `input_tokens`, `cache_creation_input_tokens`
and `cache_read_input_tokens` for the input-context measure. Values describe the
last API response, not an immediate estimate of a newly submitted prompt.
`current_usage` may be null before the first response and after compaction.
StatusLine also runs for non-message changes, so invocation is not message
activity. Since 2.1.251, `prompt_cache` reports `ttl`, `expires_at`, `warm` and
`caching_observed` for the main conversation. These are client-reported cache
statistics; live delivery remains unverified. [Claude statusLine](https://code.claude.com/docs/en/statusline)

aHerald already forwards a restricted statusLine report through IPC. Extending
`src/statusline.rs` and `Request::StatusLine` is a small adapter change, but the
AH-059 privacy allowlist in AGENTS.md currently permits only session identity,
name and rate limits. Reading context/cache fields requires an explicit agreed
extension; this report does not change that contract.

Codex documents `thread/tokenUsage/updated` for subscribed clients. aHerald's
private stdio app-server, used for metadata/quota, does not observe unrelated
CLI/app transports. Resuming their threads or introducing a proxy is outside
the agreed scope. Therefore this notification is not currently an integration
path for universal context warnings. [Codex app-server](https://learn.chatgpt.com/docs/app-server)

An alternative worth a separate investigation is opt-in OTel log export:
documentation describes conversation identity and response token counts.
Per-turn usage metrics are not themselves current context size. A collector
would add transport, payload filtering and privacy work; documented HTTP/gRPC
receivers also conflict with the no-TCP-listener contract. Do not add one as
part of these tasks. [Codex telemetry](https://learn.chatgpt.com/docs/config-file/config-advanced#observability-and-telemetry)

Recommendation: implement Claude first, explicitly report Codex context warnings
as unsupported until a permitted stable source is verified. Proposed notation:
`120000` or `"120k"`, with `k = 1000`; reject negatives, fractions and overflow.
Warn once on crossing strictly above the threshold, rearm after a valid reading
at or below it. Missing data is unknown, not zero. These are proposals, not yet
agreed product decisions.

## AH-101: elapsed time and cache TTL

Both agents document UserPromptSubmit, Stop and SessionEnd. These permit an
identity-only activity adapter without reading message contents. User submission
is observable even though another hook may subsequently reject it. A Stop hook
is a boundary, not proof that the agent cannot continue working.
[Codex hooks](https://learn.chatgpt.com/docs/hooks),
[Claude hooks](https://code.claude.com/docs/en/hooks)

Two different meanings must remain explicit:

- **Requested reminder:** N minutes after an agreed message/activity boundary.
  User submission is the simplest precise definition. A reminder after turn stop
  is more useful for idle conversations, but needs busy/continuation handling.
- **Cache warning:** use a provider/client cache signal when available; otherwise
  label the notification as elapsed time, not confirmed cache expiration.

Claude API cache reads refresh the TTL; supported lifetimes include five minutes
and one hour. Tool-loop requests can refresh the cache without a new user
message. [Claude prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)
OpenAI retention varies by caching mode/model; API documentation is not evidence
of the policy selected by a particular Codex session. A universal fixed cache
expiry must not be inferred from its last chat message.
[OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching)

Recommendation: ship a configurable elapsed-time reminder for both supported
hook integrations. Treat Claude's reported cache deadline as an optional later
mode, pending privacy agreement and a live fixture. Exact last-message coverage
and actual Codex cache expiry remain outside the initial slice.

## Shared implementation scope

Use separate per-agent enablement and thresholds, with state keyed by
`(agent, session_id)`. Reject missing identity for session warnings. Keep the
main session separate from subagent activity: Codex documents that subagent
hooks share the parent's session id, so session id alone does not establish
main-conversation activity. Verify the matching release's agent discriminator
before letting those hooks reset or end a timer. Keep the
timer in the resident; hooks return promptly and never wait for notification
lifetime. Transient-only execution cannot deliver a future warning once all
processes exit, so initially require resident mode rather than adding timer
processes.

A bounded session-state map and shared deadline scheduler can feed the existing
notification queue on Windows, macOS and Linux. No new direct dependency appears
necessary. Existing platform presentation loops need deadline wakeups even when
the queue is empty. New warning kinds must retain agent/session identity and
avoid being deduplicated as ordinary completion cards.

Proposed first slice: memory-only state, one warning per inactivity episode,
reset on the selected activity, cleanup on SessionEnd plus bounded stale-state
eviction. After resident restart, wait for fresh evidence; do not reconstruct
history from transcripts. Define sleep/resume, pause and delayed IPC behavior
before implementation.

Before coding, agree on: message definition, defaults, kilotoken multiplier,
repeat/rearm policy, resident requirement, sleep/restart behavior, and the
AH-059 field allowlist extension. Validate with controlled-time session-isolation
tests, invalid/missing token readings, compaction, continuation, cleanup and
real sessions on each claimed integration. Native UI acceptance remains
separate. Research-only checks: documentation completeness and `git diff --check`;
no Cargo builds/tests are needed for this report.
