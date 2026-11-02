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
tool-result inference cannot solve the missing observation or correlate individual
requests. A hook cannot wait for its own downstream reviewer without blocking
that approval path.

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
checks. See [the verification audit](diagnostics-audit.md). This does not supply the
missing live human-prompt, rejection or fallback evidence for AH-099.

Future implementation needs a documented, available signal for the originating
session and individual request: human-wait entry, automatic resolution, rejection,
and any fallback. Then add failing behavioral tests for those paths and unknown
data, preserve silent hook responses and prompt independence, and verify real
human waits and automatic review in a controlled Codex session before closing AH-099.
