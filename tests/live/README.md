# Optional live tests

These checks exercise installed harnesses and external integrations. Run them
explicitly; they are not part of `cargo test --all-targets` or mandatory CI.
Requirements vary by check: a harness installation, account, network access,
and billable model requests may be needed. Each check must document its needs,
isolate test configuration, bound execution and clean up its owned processes.
Do not run the whole group automatically against a developer's live account.

## Session idle reminders (AH-101, Windows)

Requires PowerShell 7, authenticated native Claude/Codex installations, network
access and permission to spend account quota. Build both executables first:

```powershell
cargo build --release --target x86_64-pc-windows-msvc
cargo build --example session_warning_bridge
pwsh -File tests/live/session_idle_warning.ps1 -Agent claude -HarnessPath 'C:\absolute\claude.exe' -AheraldPath "$PWD\target\x86_64-pc-windows-msvc\release\aherald.exe" -BridgePath "$PWD\target\debug\examples\session_warning_bridge.exe"
pwsh -File tests/live/session_idle_warning.ps1 -Agent codex -Interactive -HarnessPath 'C:\absolute\codex.exe' -AheraldPath "$PWD\target\x86_64-pc-windows-msvc\release\aherald.exe" -BridgePath "$PWD\target\debug\examples\session_warning_bridge.exe"
```

Each probe owns a private IPC namespace and native resident. The optional bridge
executes the real hook parser and IPC client; it replaces the CLI's fixed resident
namespace, not its notification engine. It is **not** an end-to-end test of the
installed hook command. Ordinary process tests cover that command separately.
Normal hooks, global configuration, credentials and autostart are not edited.
Harnesses use their normal account environment; the probe does not read/copy tokens.
Interactive Codex can still load existing user hooks, whose results are separate
from the private bridge evidence.

Idle thresholds are one minute, completion cards and sound are disabled, and idle
cards persist until activity/end. Claude print mode adds a separate 75-second
test-only Stop hook so the session stays alive after its response. The bridge
must independently finish within the three-second hook budget. The script
requires real prompt/Stop/end records, a queued warning, and cleanup after end.
Print/exec mode has a 160-second limit; interactive mode has a 180-second limit.
All owned processes are stopped in `finally`. Codex exec currently lacks the
prompt/end dispatch observed in its interactive mode and cannot pass this check.

For interactive Codex, wait for the first card, submit a second short prompt and
check that the card disappears; wait for a fresh reminder, then exit with Ctrl+D.
The script checks lifecycle records and final queue cleanup; card appearance,
activity reset and fresh delivery require separate observation. Use `scope.txt`
in the printed evidence directory with `AH101_SCOPE` and the bridge's `--status`
to observe only this probe's queue. Do not infer appearance or focus behavior from
queue counts. Evidence retains event/session identity, timing, response and result,
but never raw hook payloads, prompts, tool arguments or credentials. CLI output
contains the fixed test prompt/response. Results: [Windows acceptance](../../docs/ah-101-windows-acceptance.md).

## Codex hook argument compatibility (Windows)

Requires PowerShell 7 and the native `codex.exe`. Run from the repository root:

```powershell
pwsh -File tests/live/codex_hook_args.ps1 -CodexPath 'C:\absolute\path\codex.exe'
```

This check invokes real Codex and its `SessionStart` hook dispatcher. It asserts
that an inline command argument arrives and that the separate `args` field is
ignored, as observed with Codex 0.166.0-alpha.5.1. A changed result fails the check
so native hook setup can be reviewed against the new harness version.

It needs no account or successful model request: the provider points to a closed
loopback port, plugin synchronization is disabled, and Codex is terminated once
both hook recordings are complete. This does not establish that other live
checks can run without requests, or that notification delivery works end to end.
It creates a temporary `CODEX_HOME` and workspace, retains diagnostic files under
`%TEMP%/aherald-codex-hook-args-*`, and bounds the hook wait to 25 seconds plus
5 seconds for process cleanup. Hook trust is bypassed only for its two generated
recorders; normal user hooks and trust are unchanged.

## Codex native hook launch and timing (Windows)

Requires PowerShell 7, the Rust/MSVC build toolchain, a built aHerald executable
and native codex.exe. Run explicitly:

    pwsh -File tests/live/codex_hook_launch.ps1 -CodexPath 'C:\absolute\codex.exe' -AheraldPath 'C:\absolute\aherald.exe'

The check asks native setup to generate its actual Codex binding in isolated
configuration. It attaches that command to SessionStart solely for this probe,
substituting a small native recorder for the target executable. It observes the
exact --hook --resident arguments, nonempty stdin and process ancestry:
recorder -> Codex's PowerShell -> Codex, with no extra launcher. The executable
path contains spaces, Unicode, an apostrophe, a backtick, dollar/ampersand/percent
characters, parentheses and an exclamation mark.

Codex uses the same isolated closed-loopback provider and disabled plugins as the
argument probe. No account or successful model response is needed. Only generated
test hooks bypass trust. Owned Codex processes are terminated after the recording,
with a 25-second recording deadline and five-second cleanup deadline.
Diagnostic files remain under %TEMP%/aherald-hook-launch-*.

It then restores the real GUI aHerald executable, disables event delivery in its
isolated config, and asserts real hook responses under PowerShell 7 and inbox 5.1.
It compares the generated command to the previous extra-PowerShell form using
alternating order, two warmups and nine measured calls per case (Samples is
configurable). Timings include the outer shell startup with -NoProfile; they
measure command execution, not the full Codex hook pipeline. A direct native EXE baseline is also recorded. They are diagnostic,
not hardware-independent pass/fail performance budgets. The probe does not start
a resident, display notifications or change normal user hooks/configuration.

## Codex context source and warning (AH-100, Windows)

Explicit account probes; each asks for one `OK` model response. They disable
plugins, use an empty temporary workspace and leave user hooks/configuration
unchanged. These are research/acceptance tools, not ordinary offline tests.

```powershell
pwsh -File tests/live/codex_context_source.ps1 -CodexPath 'C:\absolute\codex.exe'
cargo build --release --target x86_64-pc-windows-msvc --example session_warning_bridge
pwsh -File tests/live/context_warning.ps1 -Agent codex -HarnessPath 'C:\absolute\codex.exe' -AheraldPath 'C:\absolute\aherald.exe' -BridgePath 'C:\absolute\session_warning_bridge.exe'
pwsh -File tests/live/context_warning.ps1 -Agent claude -HarnessPath 'C:\absolute\claude.exe' -AheraldPath 'C:\absolute\aherald.exe' -BridgePath 'C:\absolute\session_warning_bridge.exe'
```

The first probe verifies actual PreModelRequest/PostModelRequest dispatch and
successful main-session input usage. The second additionally forwards the real
report through production parsing/IPC to a private native resident, verifies an
empty initial queue followed by exactly one warning, and shuts down its owned
processes. Sound and usage polling are disabled. The installed resident can keep
running. Queue evidence does not prove appearance or focus behavior.
The Claude variant requires an interactive terminal: review trust for its newly
created test directory, let the `OK` response complete, then `/exit` before the
105-second deadline. Claude normally persists this interactive test session;
`--no-session-persistence` is supported only in print mode, which does not exercise
the interactive status line. The bridge retains only session identity and the
optional context counter under the printed evidence directory.

Only session/turn identity, status and numeric usage counters are retained under
`%TEMP%/aherald-codex-context-*`; raw payloads, cwd, transcript paths, provider
errors and harness output are discarded. Source runs have a 90-second deadline,
with bounded cleanup. The second probe stores its queue result and source probe
summary under `%TEMP%/aherald-context-live-*`. Test-only hook trust bypass applies
to the generated recorders in this isolated invocation. Account credentials are
used only by Codex itself, through normal authentication.

## Existing optional account integration check

The installed Codex app-server handshake/usage check remains an ignored Rust test:

```powershell
cargo test --test usage_live -- --ignored --nocapture
```

It uses the installed Codex and its normal account environment and may make
service requests. It reports a successful handshake separately from available
usage data; it is not a notification delivery acceptance test.
