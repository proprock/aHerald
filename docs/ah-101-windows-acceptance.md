# AH-101 Windows acceptance — 2026-10-31

AH-101 is accepted. The maintainer selected reminder delivery in real Claude and
Codex sessions as the completion criterion and requested no further testing.

Environment: Windows 11 build 22621, aHerald 0.5.0, Claude Code 2.1.311,
Codex CLI 0.166.0-alpha.5.1. Both harnesses returned `OK` using their normal local
accounts. Each probe used a separate native resident, private IPC namespace,
one-minute idle thresholds, disabled completion cards and disabled sound.

| Integration | Observed result |
| --- | --- |
| Claude | Real UserPromptSubmit and Stop; the native resident delivered one idle warning. Real SessionEnd removed it; final queue was empty. |
| Codex interactive | Real UserPromptSubmit and Stop; the native resident delivered one idle warning. A second submitted prompt cleared it. A second Stop and real SessionEnd succeeded; final queue was empty. |

The evidence observes delivery through the private resident queue with visuals
enabled. It does not constitute screenshot-based appearance or focus verification.
Claude hook handler timings were 8.96–217.40 ms, with empty stdout. Codex handler
timings were 0.93–34.99 ms; Stop returned `{}`, other events returned empty stdout.
These measure the native bridge, not the entire harness shell startup.

The optional [probe](../tests/live/session_idle_warning.ps1) uses the production
adapter and IPC library through a test-only native bridge. The resident, timer and
notification engine are the release executable. This isolates other open sessions
without changing the production IPC namespace or user settings. Claude print mode
uses a separate 75-second Stop keepalive; interactive Codex stays open normally.
No hook response approves, denies or continues agent work. No credentials were
read/copied by the probe. Normal hook configuration and autostart were not edited.
The temporarily stopped installed resident was restored with sound and visuals
enabled, unpaused, and an empty queue.

Local temporary evidence directories (not packaged artifacts):

- Claude: `%TEMP%\aherald-idle-live-fa2d201d068d46979f4ff8a08d6594cc`.
- Codex: `%TEMP%\aherald-idle-live-9345f4f48d764c10b5f81f1f2fd2d469`.

The earlier shared-IPC attempts were contaminated by other sessions and are not
acceptance evidence. Codex exec returned a model answer and Stop but did not
provide the full prompt/end lifecycle in that attempt; acceptance uses interactive
Codex. An interactive Codex warning reported a separate five-second hook timeout;
the private bridge's recorded events all succeeded. Its cause was not investigated.

Existing controlled-time tests cover thresholds, per-agent settings, session
isolation, reset, cleanup and missing activity. Added native coverage exercises
delivery from an empty queue without a visible window. Focused session-warning,
adapter and resident tests passed. Formatting, Clippy and the Windows release build
passed during this task; the final native bridge was subsequently built successfully.
The full suite is not green: its final serial run stopped at
`hot_paths_stay_inside_their_baseline_budgets` (`artwork.prepare` mean 60.91 ms,
budget 30 ms). Earlier attempts also hit native startup/hook timing deadlines.
No thresholds or assertions were weakened. Further checks stopped at the
maintainer's request. No commit, tag, push or release was created.

Protocol references: [Codex hooks](https://learn.chatgpt.com/docs/hooks),
[Claude hooks](https://code.claude.com/docs/en/hooks), and the earlier
[versioned schema research](session-warnings-research.md).
