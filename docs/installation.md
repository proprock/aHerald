# Native installation and harness setup

The installer places the program and assets at a stable path. The native aHerald
executable configures the harnesses explicitly selected by the user. No plugin or
marketplace is required for this hook integration. No application discovery runs
when a hook fires.

## Windows 10/11 x64

Run `aherald-vX.Y.Z-windows-x64-setup.exe`. Installation is per user, without
administrator privileges, under `%LOCALAPPDATA%\Programs\aHerald` by default.
Select Codex, Claude Code, both, or neither. Both checkboxes start unchecked on a
new installation; upgrades retain the previous selection. No autostart entry is
created. A successful interactive installation offers a checked "Launch aHerald"
checkbox on the finish page; clear it to leave the application stopped. Silent
installation and failed hook setup do not launch it. You can also start aHerald
from the Start menu, or let a hook start the resident.

The task page also offers independent Codex usage and Claude usage checkboxes.
Both are off on a new installation. On upgrades, they reflect the current
config.toml in the selected directory. Changing the directory reloads those values.
Codex usage needs a signed-in Codex account. Claude defaults to auto: local Claude
login first, then statusLine. An upgrade preserves an explicitly configured Claude
source; the installer does not install a statusLine command.

Only the two usage enabled flags are changed. Other settings, comments and
formatting are preserved, with a backup before changes. Configuration and hook
changes share the setup transaction. Removal without usage options retains the
configuration. Restart the resident to load changed usage settings.

The installer invokes `aherald.exe --setup --harness codex --harness claude --apply`
with only the selected harnesses. Selecting neither removes bindings previously
managed by this installation. aHerald validates every affected JSON file before
writing, preserves unrelated settings and hooks, and backs up changed existing
files as `*.aherald-backup`. A failed write triggers restoration of earlier
writes; any restoration failure is reported. Backups contain the previous file,
so treat them with the same care as the original configuration.

After configuring Codex, start a new Codex session, open `/hooks`, review and trust
the exact commands. Installation never grants hook trust automatically.
Disable any existing aHerald plugin or registration in another configuration layer
before enabling these hooks, to avoid duplicate events.

After successful preflight, the installer checks the resident PID and its actual
executable path. If it belongs to the selected installation directory, setup
requests a graceful exit through the installed aHerald --quit and waits up to
10 seconds for that process to exit before replacing files. Uninstall performs
the same shutdown before removing files. Failure stops the operation with an
error; the installer does not forcibly terminate the application.
A resident running from another directory is left alone. Shutdown dismisses
the current notification queue. Interactive setup can start the updated resident
through its finish-page checkbox; silent setup leaves it stopped.

Rerun the installer to change the selected harnesses. Keep the installation
directory stable: paths are resolved at setup time and do not follow a moved
portable folder.

Uninstall from Windows Settings or the Start menu. The uninstaller preflights cleanup, stops its resident, then invokes
`aherald.exe --unsetup --apply`, using `aherald-setup.json` beside the executable to
find the original hook files. It removes only the exact saved execution bindings,
including entries the installation created for events no longer supported.
If cleanup fails, uninstall stops and keeps the executable available for repair.
Configuration, logs, backups and the setup receipt are retained; remove those
manually if no longer wanted. Never delete the receipt before uninstalling hooks.

The client needs no WSL, Bash, Python, .NET or separately installed PowerShell 7.
Claude Code uses its documented exec form (command plus args) and starts
aherald.exe directly. Codex 0.166.0-alpha.5.1 accepts a shell command and chooses
PowerShell on Windows: installed pwsh first, otherwise inbox Windows PowerShell.
The generated Windows override calls the fixed executable path within that
existing shell, without starting an additional PowerShell process or searching
PATH for aHerald. Out-String waits for the GUI-subsystem executable and captures
its response. A custom non-PowerShell hook executor is outside this verified
Windows contract. Native setup never changes the harness's shell configuration.

## Portable setup on each OS

The same native setup entry point is available in portable distributions:

```text
aherald --setup --harness codex --harness claude
aherald --setup --harness codex --harness claude --apply
aherald --unsetup
aherald --unsetup --apply
```

Installers can inspect usage settings without starting the resident or creating
configuration by running aherald --setup-settings. Native setup and removal accept
--codex-usage true|false and --claude-usage true|false; omitted options preserve
existing values. The installer uses --settings-file with an absolute target path
so its temporary executable reads and patches the installed configuration.

For silent installation, /TASKS="codex,claude,usagecodex,usageclaude" selects both
harnesses and usage displays. An explicit /TASKS is the complete desired selection;
omit unwanted tasks. Without /TASKS, silent upgrades read existing usage settings.
Silent installation does not start the resident.

Without `--apply`, setup and removal only show the planned changes. `--setup`
means the complete desired selection for this installation: deselected bindings
from its receipt are removed. An empty selection uses `--unsetup`. The legacy
`--install-hooks AGENT` / `--uninstall-hooks AGENT` commands remain available for
manual per-file configuration and do not create a setup receipt.

Codex uses `CODEX_HOME/hooks.json`, or `~/.codex/hooks.json` by default. Claude uses
`CLAUDE_CONFIG_DIR/settings.json`, or `~/.claude/settings.json`. Overrides must be
absolute directories and are resolved only during setup. Removal uses saved paths,
so later changes in the environment do not redirect cleanup. The receipt is
beside the portable executable; for a macOS app bundle it is in
`~/Library/Application Support/aHerald` to keep the signed app unmodified. Run setup as the
harness's user, without sudo or administrator elevation. Installers and package
scripts must not configure a root account's hooks on behalf of a desktop user.
Move a portable installation first, then rerun setup from the new location.

## Platform packaging plan

Windows uses Inno Setup. macOS will provide an app-based setup flow and GUI
installation, with Homebrew Cask as the package-manager path. Linux starts with an
Ubuntu `.deb`, usable through apt or a graphical package manager, plus a native
user-context setup flow. These macOS and Linux installer flows are tracked
separately in TODO.md; current artifacts remain portable archives. Platform
installers share the native setup behavior, not a shell launcher.

## Building the Windows installer

Build and check the release executable, then run `package-release.ps1` and
`build-installer.ps1`. The latter requires Inno Setup 7 (or a compatible compiler
with `ExecAndCaptureOutput`) on the build machine only. Windows release CI installs
the pinned, signature-verified Inno Setup 7.1.1 compiler and publishes both the
portable ZIP and setup EXE, with SHA-256 files. The setup EXE is not code-signed by
this workflow. Clean Windows VM acceptance remains a separate manual check.

## Verified integration references

Checked 2026-10-31 against Claude Code 2.1.311 and Codex
0.166.0-alpha.5.1. Claude's [exec-form hook contract](https://code.claude.com/docs/en/hooks#exec-form-and-shell-form)
passes arguments directly without shell parsing. Codex's
[hook documentation](https://learn.chatgpt.com/docs/hooks) and matching release
[command runner](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/hooks/src/engine/command_runner.rs)
use a shell command string. These references describe launch behavior, not newly
verified event payloads or wider version compatibility.

The Windows Codex binding uses the documented commandWindows override and
PowerShell's call operator with a single-quoted literal executable path.
Apostrophes in that path are doubled; dollar signs, backticks, ampersands and
percent signs remain literal. Hook input is never interpolated into the command.
The matching release declares the
[JSON field](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/protocol/src/hooks.rs),
[selects it on Windows](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/hooks/src/engine/discovery.rs),
and [selects PowerShell by default](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/shell-command/src/shell_detect.rs).
