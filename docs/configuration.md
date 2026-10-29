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
