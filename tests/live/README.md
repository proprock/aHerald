# Optional live tests

These checks exercise installed harnesses and external integrations. Run them
explicitly; they are not part of `cargo test --all-targets` or mandatory CI.
Requirements vary by check: a harness installation, account, network access,
and billable model requests may be needed. Each check must document its needs,
isolate test configuration, bound execution and clean up its owned processes.
Do not run the whole group automatically against a developer's live account.

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

## Existing optional account integration check

The installed Codex app-server handshake/usage check remains an ignored Rust test:

```powershell
cargo test --test usage_live -- --ignored --nocapture
```

It uses the installed Codex and its normal account environment and may make
service requests. It reports a successful handshake separately from available
usage data; it is not a notification delivery acceptance test.
