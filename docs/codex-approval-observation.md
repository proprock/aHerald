# Codex human-approval observation — AH-099

Investigated 2026-11-01 on Windows 11 build 22621 with installed
`codex-cli 0.166.0-alpha.5.1`. **Reliable passive discrimination is unavailable
through the verified hooks. AH-099 remains open; no suppression is implemented.**

## Versioned evidence

- [Official hook contract](https://learn.chatgpt.com/docs/hooks), especially
  PermissionRequest, common input fields, and the release-schema warning.
- [Official auto-review flow](https://learn.chatgpt.com/docs/sandboxing/auto-review).
- Release tag `rust-v0.166.0-alpha.5.1`: annotated tag
  `b9d5adaac47e7bf474ebdafa95c7ee96724b3eeb`, commit
  `f79b26ccbb3116b052544666a181012fb45dc49b`.
- [Exact PermissionRequest input schema](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/hooks/schema/generated/permission-request.command.input.schema.json).
- [Approval routing](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/core/src/tools/approvals.rs#L522):
  matching hooks run first; a hook allow/deny decides, otherwise reviewer
  approval runs afterward. Another hook can therefore also prevent a human prompt.
- [Hook input construction](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/hooks/src/events/permission_request.rs):
  no reviewer, verdict, request/tool-use identity, or human-prompt visibility field.

The required fields are `cwd`, `hook_event_name`, `model`, `permission_mode`,
`session_id`, `tool_input`, `tool_name`, `transcript_path`, and `turn_id`;
`agent_id` and `agent_type` are optional. `permission_mode` has the values
`default`, `acceptEdits`, `plan`, `dontAsk`, and `bypassPermissions`. These describe
permission policy, not who will review an individual request or its eventual result.

## Outcomes and notification policy

| Actual situation | What this hook proves | Current safe behavior |
| --- | --- | --- |
| Auto-review later approves | An approval boundary was reached before the verdict | Keep the notification; false positives remain possible |
| A real human approval is pending | The same boundary, without proof of prompt visibility | Keep the notification so a genuine wait is not lost |
| Auto-review rejects or processing later falls back to a person | No verdict or later fallback is reported to this invocation | Keep the notification; do not infer approval, denial, or resolution |
| Reviewer/outcome unknown | No reliable discriminator | Keep the notification |

The documented rejection flow tells the main agent to find a safer path or ask
the user. It does not guarantee a second PermissionRequest or a human approval
dialog. Reviewer configuration alone would miss these distinctions; global config
also does not establish another session's effective settings. Timing delays and
tool-result inference do not supply the missing observation; since AH-304 the resident
uses them as a heuristic that drops a request answered within a short delay (see
[Permission delay](#permission-delay-ah-304)), not as proof. A hook cannot wait for its own
downstream reviewer without blocking that approval path.

App-server approval requests are useful to clients that own the corresponding
transport, but aHerald's separate usage connection does not subscribe to arbitrary
CLI/Desktop session approvals. See [input-wait coverage](codex-input-waits.md).
No transcript parsing, proxy, unrelated-thread resume, configuration guessing,
permission response, or new monitoring connection is introduced.

## Verification and remaining acceptance

`cargo test --test codex_approval_observation --test hooks --test codex_lifecycle`
passed: 15 tests. The added characterization test executes parsing and queue
admission for every documented mode, a missing mode, and a future unknown mode;
permission events retain their session/turn and remain queued with no hook reply.
Existing process tests verify silent successful PermissionRequest responses and
bounded handler lifetime. No production behavior changed, so there is no claimed
Red/Green bug fix.

These tests deliberately do **not** claim to distinguish the four outcomes above:
the input schema provides no such observation. Outcome-classification acceptance
tests and safe suppression require a supported per-request human-wait/resolution
signal first. AH-099 must not be checked off on the strength of this investigation.

The opt-in [real-dispatch probe](../tests/live/codex_approval_observation.ps1)
starts fresh ephemeral Codex sessions configured for `user` and `auto_review`,
with an invocation-local, silent recorder and one harmless escalated
`Write-Output AH099` request. It reads no credentials directly and changes no
user hooks/configuration. It retains only known field presence, a validated mode,
configured reviewer, exit status, and hook count in a unique temporary evidence
directory. Raw hook input, model responses, account diagnostics, tool arguments,
and paths are not retained. Owned processes have bounded lifetime and cleanup.
The isolated probe bypasses hook trust only for its reviewed recorder; this is
not installation guidance. It uses the installed CLI's normal authentication.

Run from PowerShell 7 with an explicit native binary:

```powershell
./tests/live/codex_approval_observation.ps1 -CodexPath 'C:\path\to\codex.exe'
```

Noninteractive execution, model behavior, or managed requirements may prevent a
requested path from running. A hook capture proves dispatch and observed fields;
neither a successful process exit nor a configured reviewer proves the verdict
or a still-open human prompt. This probe cannot certify all four outcome paths.

Observed run on 2026-11-01 (same installed version, actual model session):

| Configured reviewer | Process | PermissionRequest captures | Validated mode |
| --- | --- | --- | --- |
| `user` | Completed, exit 0 | 0 | Unobserved |
| `auto_review` | Completed, exit 0 | 1 | `default` |

The automatic-review capture contained the eight required identity/context/tool
fields apart from the event name, and no optional subagent fields. This confirms
real hook dispatch on the automatic-review path with an ordinary `default` mode;
it does not certify approval versus rejection. The noninteractive `user` run
provides no evidence of a real pending human approval and is not counted as one.
Human-prompt, rejection, and fallback acceptance remain unverified.
Sanitized local evidence was written to
`%TEMP%/aherald-ah099-c58bf45bdec042698ed05afbf25b9cf0/` (version, summaries,
and one field-presence capture); this disposable directory is not a release artifact.

`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and
`cargo build --release --target x86_64-pc-windows-msvc` passed. The initial full
`cargo test --all-targets` run stopped at the existing
`native_overlay::simultaneous_transient_cards_do_not_overlap` integration test
(`tests/native_overlay.rs:376`): two cards had the same rectangle
`(311, 24, 713, 174)`. No production or placement code changed in this task;
the cause of that failure was not diagnosed and later test binaries were not run.
The subsequent full Windows run after AH-090/native-assets verification passed:
517 tests passed, 17 ignored, including this characterization test and native overlay
checks. See [the verification audit](notes/diagnostics-audit.md). This does not supply the
missing live human-prompt, rejection or fallback evidence for AH-099.

Future implementation needs a documented, available signal for the originating
session and individual request: human-wait entry, automatic resolution, rejection,
and any fallback. Then add failing behavioral tests for those paths and unknown
data, preserve silent hook responses and prompt independence, and verify real
human waits and automatic review in a controlled Codex session before closing AH-099.

## Hook order around automatic review (AH-302)

Observed 2026-11-16 on Windows 11 build 22621 with `codex-cli 0.169.0` (npm
`@openai/codex@0.169.0`), confirming a source reading of tag `rust-v0.169.0`
(`codex-rs/core/src/tools/approvals.rs`, `tools/registry.rs`). Run:

```powershell
./tests/live/codex_approval_observation.ps1 -HookOrder -CodexPath 'C:\path\to\codex.exe'
```

Three fresh ephemeral `codex exec` sessions with `approval_policy="on-request"`,
`approvals_reviewer="auto_review"`, a `read-only` sandbox and `windows.sandbox="elevated"`. The
probe passes `--ignore-user-config`, which also drops the Windows sandbox setting; without it
every command asked for approval, including the unescalated sibling. One silent recorder is
attached to `PreToolUse`, `PermissionRequest`, `PostToolUse`, `PostModelRequest` and `Stop`. Per
call it keeps only the event name, the first 8 hex characters of a salted SHA-256 of `turn_id`
and `tool_use_id`, whether `agent_id` is set, and a sequence number taken under a named mutex.
No tool input, prompt, path or raw payload is kept.

- **approve**: one escalated `Write-Output`.
- **deny**: one escalated upload of a placeholder file to an `.invalid` host (nothing real to
  send and no resolvable host even if approved). A separate `codex exec --json` run of the
  same prompt reported the command item as `"status":"declined"`, so the missing
  `PostToolUse` is the reviewer's denial, not a failed command. Only event types and
  statuses were read from that output; it was deleted.
- **siblings**: one model response with two parallel shell calls, one plain (A) and one
  escalated (B).

`A`, `B` stand for the hashed `tool_use_id`; every call had the same turn, and none carried
`agent_id`.

| Case | Observed order |
| --- | --- |
| approve | `PostModelRequest`, `PreToolUse`(A), `PermissionRequest`, `PostToolUse`(A), `PostModelRequest`, `Stop` |
| deny | `PostModelRequest`, `PreToolUse`(A), `PermissionRequest`, `PostModelRequest`, `Stop` |
| siblings | `PostModelRequest`, `PreToolUse`(A), `PreToolUse`(B), `PermissionRequest`, `PostToolUse`(A), `PostToolUse`(B), `PostModelRequest`, `Stop` |

Findings:

- The order matches the source reading: `PreToolUse`, `PermissionRequest`, the reviewer, the
  tool, `PostToolUse` only when the call ran, then the next `PostModelRequest`. A denied call
  has no `PostToolUse`; the next completed `PostModelRequest` of the turn follows the denial.
  The first `PostModelRequest` is the model request that produced the tool calls.
- **A sibling's `PostToolUse` can arrive between a `PermissionRequest` and its resolution.** In
  the siblings case the plain call's `PostToolUse` came after the request and before the
  escalated call's `PostToolUse`. A single `PostToolUse` is therefore evidence of resolution
  only for a `tool_use_id` that was open when the request arrived, and with several open ids
  only once all of them completed.
- Both sibling `PreToolUse` calls arrived before the `PermissionRequest` here. One run does not
  show that a sibling `PreToolUse` can never follow it, and a `PreToolUse` says nothing about
  the verdict: it is not evidence of resolution.
- `PermissionRequest` has no `tool_use_id` in 0.169.0 (input schema at the tag), so a request is
  matched to a call only by order within its session and turn.
- The first live attempt, without the Windows sandbox setting, produced two `PermissionRequest`
  calls in the siblings case, both before either `PostToolUse`; the order was otherwise the
  same.

Not covered: a person answering the prompt, subagents (`agent_id`), Linux and macOS. The
sanitized evidence stayed in a disposable `%TEMP%/aherald-ah302-*` directory.

## Permission delay (AH-304)

The resident holds a Codex `PermissionRequest` that has a turn for `[codex]
permission_delay_sec` (default 3 seconds, range 0..=120, `0` turns the delay off). While it is
held there is no card, no sound and no messenger forward. It is dropped (debug log line
`event.dropped auto_resolved`) when one of these arrives for the same `session_id`, `turn_id`
and `agent_id`:

- the `PostToolUse` of a `tool_use_id` that was open (a `PreToolUse` seen, no `PostToolUse` yet)
  when the request arrived; with several open ids, once all of them finished;
- a completed `PostModelRequest` (the next model step after a denial);
- `UserPromptSubmit`, `Stop` or `Interrupt` of that turn, or `SessionEnd` of the session.

`PreToolUse` alone is never evidence. A prompt or completed model request also forgets the turn's
open tools: a call still open then was denied and never sends `PostToolUse`, and it must not keep
a later request of the turn waiting. Tool activity from a hook older than AH-304 carries no
start/finish marker and is ignored. Otherwise the request becomes an ordinary card when the
delay ends, with its sound and forward once. A request without a turn, a Claude request and
every other harness are not held. The resident remembers open tools for 64 turns of 32 ids each
and holds at most 64 requests; a further request is shown at once.

Cost and limits:

- Every request a person must answer appears `permission_delay_sec` later.
- **A long-running approved command still shows a card.** Approval is observed only through the
  call's `PostToolUse`, which arrives when the command has finished. A command the reviewer
  approves but that runs longer than the delay (a build, a test suite) produces a card, which
  stays until the turn ends.
- A tool that starts after the request is not counted, so its finish never drops the request;
  the rule errs towards showing a card.
- Transient mode has no state between hooks, so it shows every request at once.

The default comes from the owner's choice on 2026-11-16, not from measured data: the Watchdog
sample held no `PermissionRequest` rows (see `docs/notes/ah-304-permission-delay.md`).
