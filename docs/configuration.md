# Portable configuration

Without `--config`, aHerald uses exactly `config.toml` in the directory containing
the running `aherald.exe`. It never searches the working directory or APPDATA.
Normal event, hook, resident and demo startup creates an absent standard file
with the current defaults, including documented options, from
`aHerald.example.toml`. Help/status/quit do not create configuration files.
Existing standard files are never replaced, merged or upgraded automatically.

The file is written completely to a private file in the same directory and
published with an exclusive rename. Concurrent initial launches observe a complete
file; a file created meanwhile wins. A write failure reports the target path and
OS reason. The installation directory must be writable to create the first file;
otherwise prepare the file before installation or select a writable custom file.
In hook mode errors still honor the successful, notification-only hook contract
and report the operational problem on stderr.

`--config PATH` retains its previous meaning: use that file, fail if it is missing
or invalid, and never create it. Relative explicit paths remain relative to the
launch working directory. Relative resource paths (pet manifests, animations and
sounds) are resolved from the chosen configuration file's directory, for both
default and custom files. No environment-variable or tilde expansion is performed.

Default hook invocation is `aherald.exe --hook --resident`. The PowerShell wrapper
in `examples/hooks.windows.json` retains `| Out-String` for GUI executable response
capture. Add `--config` only when using a custom file; no automatic hook edits occur.

## Manual migration

### Session warnings (AH-100 / AH-101)

Idle reminders default to 55 minutes for Claude and 25 minutes for Codex.
Context warnings remain off until a threshold is configured. For example:

```toml
[claude.warnings]
context_enabled = true
idle_enabled = true
context_tokens = "400k"
idle_minutes = 55

[codex.warnings]
context_enabled = true
idle_enabled = true
context_tokens = "250k"
idle_minutes = 25
```

`context_tokens` accepts positive integer tokens, an integer string or an integer
string ending in lowercase `k` (`k = 1000`). Fractions, signs, zero and overflow
are rejected. Each agent's threshold is independent and omitted by default (off).
`context_enabled` and `idle_enabled` default to `true`; set either to `false` to
disable that warning while retaining its threshold. The Windows installer uses
these flags. When first enabled there without a context threshold, it supplies
250k tokens for Codex or 400k for Claude; existing thresholds are preserved.
Codex requires the `PostModelRequest` binding, verified with CLI
`0.166.0-alpha.5.1`; rerun explicit hook setup to add it to an older installation.
Other versions and desktop dispatch need separate verification. Codex uses the
successful main request's `input_tokens`, which already includes cached input.
Claude input context includes fresh input, cache creation and cache reads, each counted once. It is the
last reported API input, not a prediction of the next request. Missing readings
do not count as zero. A crossing strictly above the threshold queues one
`context_warning`; a known reading at/below the threshold rearms it.

`idle_minutes` accepts 1..525600 minutes, or `0` to disable only that agent's idle
reminder. Omission keeps the agent's default, including in a partial warnings
table. The timer starts at a main-session Stop
boundary (end of an agent response). A new user submission or tool activity
cancels it; the next completion starts a new episode. Error, interruption and
session end cancel it too. Each episode queues at most one `idle_warning`,
strictly after the threshold, on the next presentation-loop tick (about one
second while idle on Windows/macOS). Agent and session identities are isolated;
subagent evidence does not reset or start the main session's timer. This is an
elapsed-time reminder, not proof that the provider's cache expired. Another Stop
hook can continue work: observable tool activity cancels the provisional timer,
but pure model continuation and delayed hook delivery cannot be perfectly
distinguished by these lifecycle hooks.

Use resident mode. The Claude context warning additionally needs the `--statusline`
wrapper, installed automatically by native setup when Claude Code and Claude
context warnings are selected; no OAuth access is needed. Both idle integrations
need UserPromptSubmit, PreToolUse, PostToolUse, Stop and SessionEnd; retain the
existing PermissionRequest and agent-specific error/interruption hooks. Preview
the updated installer with `aherald --install-hooks claude` or `codex`, inspect
the diff, then explicitly run with `--apply` if desired. Existing files are never
upgraded automatically; review/trust changed hooks in the agent normally.

State is memory-only and bounded to 64 recently observed sessions. Restart waits
for fresh evidence. A paused resident postpones idle delivery until resumed.
An overdue deadline is checked after the machine resumes if the OS monotonic
clock includes the suspended interval; no wake-from-sleep or cross-platform
suspend-time guarantee is provided. When the notification queue is full, a bounded
pending warning is retried on native-loop ticks; `queue_rejected` is logged once
per blocked warning. Activity/session end cancels stale pending idle warnings too.

Warning cards keep the originating agent/session. Their settings are
`[events.context_warning]` and `[events.idle_warning]`, using the ordinary event
settings (default duration 3000 ms, amber accent, no configured sound). Builtin
artwork reuses the permission/input pictures; normal pet/custom overrides work.
Preview with `--event context_warning` or `--event idle_warning`; the standard
`--demo` keeps its original five-event sequence.

**Upgrade:** this build uses IPC frame version 2 so new evidence-only activity
events cannot become false completion cards in older residents. Quit the old
resident with its old executable before replacing it, then start the new build.
Mixed versions reject frames safely; a new executable cannot send `--quit` to a
version-1 resident. The tray Quit action remains available.

### Portable path migration

Quit the resident. Copy your old `%APPDATA%\aHerald\config.toml` to `config.toml`
beside the installed executable, preserving any existing destination you want to
keep. Copy relative resources with their directory layout, or adjust their paths.
Restart aHerald, remove `--config` from hooks that should use the standard file,
and review/trust changed hook commands through Codex's normal workflow. Alternatively
keep the old file and pass its explicit path. aHerald neither reads nor moves it
automatically; there is no legacy-location fallback.

## Verification (2026-10-25)

Tests execute a copied Windows binary from another working directory, under paths
with spaces and Unicode. Invalid working-directory and APPDATA configurations are
ignored. Creation, preservation, eight concurrent first loads, explicit missing
files, write failures with path/reason and relative resources are covered. Hook
responses and bounded completion remain covered by the existing process suite.
Formatting, Clippy, all-target tests and a Windows x64 release build are required.
The earlier real-card, queue-navigation, restart and chat-transition acceptance
is recorded in tray-usage.md and notification-click.md. No user hook file or old
configuration was migrated as part of this implementation.

## Errors and diagnostics

An invalid file is reported as `invalid configuration: <table.key> <reason>` (or
`invalid TOML syntax at line N`); the message never contains the offending value. The
tray **Restart** command validates the file first. If it is invalid, the running
resident keeps its current configuration and shows the error on the details card.

`aherald.log` (beside the configuration, 64 KiB with one backup) records fixed codes
only, never payloads: `resident_started`, `resident_stopped`, `resident_startup_failed`,
`invalid_configuration`, `render_failed`, `render_fallback`, `usage_failed` (once per
failing episode), `ipc_failure`, `queue_rejected`, `agent_activation_failed`, `hook.delivery_failed` (a hook could not hand its
event to the resident) and `artwork.reduced` (artwork over the memory budget at the monitor's
scale is drawn at the configured scale; once per scale).
Invalid-configuration and startup codes are written at `warn` level regardless of
`general.logging`, because that setting cannot be read from an invalid file.
