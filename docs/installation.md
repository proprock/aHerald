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
source.

Four further checkboxes control context and idle warnings independently for each
agent. On a new installation, context is off and idle is on. When first enabling
context without a saved threshold, setup uses 250k tokens for Codex or 400k for
Claude. Idle defaults are 25 minutes for Codex and 55 for Claude. Existing custom
thresholds are retained; disabling a warning saves its enabled flag without
erasing its threshold. Enabling an idle warning previously disabled with
`idle_minutes = 0` restores that agent's default. Warnings require a running
resident and the corresponding agent integration described in configuration.md.
Selecting Claude Code together with Claude context warnings automatically installs
the statusLine wrapper. It passes the same input to an existing command and keeps
its output and options. Repeated setup does not nest wrappers. Disabling Claude
context, deselecting Claude Code, or uninstalling restores the original statusLine
only if the installed wrapper has not been edited. Backups and rollback cover this
change together with hooks and config.toml. This follows Claude's documented
[statusLine command contract](https://code.claude.com/docs/en/statusline).

On Windows the wrapper is a readable command that Git Bash and PowerShell read alike, because
Claude Code runs a `statusLine` through Git Bash if it is installed and PowerShell otherwise:
`C:/…/aherald-cli.exe --statusline`, when the installation path has no character special to
either shell (no space, quote, `&`, `$` or backtick). A path with spaces, such as
`C:/Program Files/…`, gets `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File
"C:/…/statusline.ps1"` and an aHerald-owned `statusline.ps1` beside the executable, recorded in
the receipt and removed with the wrapper. Your own command is not on the line: aherald reads
it from the receipt `aherald-setup.json`, so deleting the receipt leaves a plain status line.
A path with `"`, `$` or a backtick keeps the encoded one-line command of earlier versions.
Running setup again replaces an earlier encoded command and keeps your original exactly.

Only selected usage/warning flags and missing warning thresholds are changed. Other settings, comments and
formatting are preserved, with a backup before changes. Configuration and hook
changes share the setup transaction. Removal without setting options retains the
configuration. Restart the resident to load changed settings.

The installer invokes `aherald.exe --setup --harness codex --harness claude --apply`
with only the selected harnesses. Selecting neither removes bindings previously
managed by this installation. aHerald validates every affected JSON file before
writing, preserves unrelated settings and hooks, and backs up changed existing
files as `*.aherald-backup`. A failed write triggers restoration of earlier
writes; any restoration failure is reported. Backups contain the previous file,
so treat them with the same care as the original configuration.
Setup preserves hooks belonging to aHerald executables at other paths. It upgrades
legacy shell bindings only for its own executable path; after a move, its saved
receipt identifies the old bindings to replace. Remove any unwanted second
installation through that installation's own `--unsetup --apply` to avoid duplicate
notifications.

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

Installers can inspect usage and warning settings without starting the resident or creating
configuration by running aherald --setup-settings. Native setup and removal accept
`--codex-usage true|false`, `--claude-usage true|false`, `--codex-context true|false`,
`--claude-context true|false`, `--codex-idle true|false` and `--claude-idle true|false`; omitted options preserve
existing values. The installer uses --settings-file with an absolute target path
so its temporary executable reads and patches the installed configuration.

For silent installation, /TASKS="codex,claude,usagecodex,usageclaude" selects both
harnesses and usage displays. Add `contextcodex,contextclaude,idlecodex,idleclaude`
to select all warnings too. An explicit /TASKS is the complete desired selection;
omitted tasks are disabled, including idle reminders. Without /TASKS, silent
upgrades read existing usage and warning settings.
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

Windows uses Inno Setup. macOS uses a disk image and the app's own setup window; a
Homebrew Cask (stable releases; Apple Silicon):

```bash
brew tap proprock/tap
brew install --cask aherald
```

It installs the same disk image's app, does not remove the Gatekeeper quarantine (allow it once
with Open Anyway in System Settings, Privacy & Security, or run
`xattr -dr com.apple.quarantine /Applications/aHerald.app`), and `brew uninstall --cask aherald` first runs `aherald --unsetup --apply` as your user
to remove aHerald's hook entries. See the tap's README for upgrade and repair.

## macOS disk image and setup window

Download `aherald-<tag>-macos-arm64.dmg` (Apple Silicon only) and its `.sha256`, open the image
and drag `aHerald.app` onto `Applications`. The image is ad-hoc signed and not notarized, so the
first start needs Control-click, Open (or `xattr -dr com.apple.quarantine`; see
`docs/portable-macos.md`). No administrator rights, Homebrew or developer tools are needed.

Opening `aHerald.app` from Finder starts the **aHerald Setup** window; the same window opens
from a terminal with `aherald --setup-window`. It runs as the user and lists, for Codex and for
Claude Code, the hooks, usage monitoring and context and idle warnings, plus a launch option.

- First setup starts with nothing selected. Later runs read the saved receipt and
  `config.toml`, so the selection is retained; usage stays off until you choose it.
  Claude usage `auto` tries the login token, then the last statusLine report.
- **Preview** is a dry run and shows the files and entries that would change. **Apply** calls
  the same backend as `--setup` / `--unsetup` with `--apply`: all files are checked first,
  backups are kept, a failed write is rolled back, repeating it changes nothing, and choosing
  no agent removes only the saved bindings. A failure is shown in the Details box and changes
  nothing. Foreign hooks and other installations are left alone, and each agent's own hook
  trust review still applies. No autostart entry is created.
- Hooks use the fixed path `/Applications/aHerald.app/Contents/MacOS/aherald` (or wherever
  the app was when you applied). The receipt, `config.toml`, logs and backups live in
  `~/Library/Application Support/aHerald`; the signed bundle is never modified.

**Claude status line text.** `[claude.statusline] format` in `config.toml` prints your own
status line text when you have no status line command of your own; it works the same on
Windows, Linux and macOS. Edit it from the menu-bar icon (**Edit configuration**), for example
`format = "{model.display_name}{[ · {worktree.branch}]} {context_window} {rate_limits.five_hour}"`,
then open the setup window and press Apply again: a format that goes from empty to non-empty
makes setup install the wrapper (a command of your own is kept and wins). The full syntax is in
`docs/configuration.md`. In an app bundle the file is
`~/Library/Application Support/aHerald/config.toml`.

**Updating.** Replace `aHerald.app` at the same path; hooks keep working. Installing does not
stop a running resident. Restart it (`aherald --quit`, then open the app) to use the new
version; until then an old resident can reject messages from a client with a different IPC
version.

**Removal.** Open aHerald Setup, clear both agents and press Apply, then drag the app to the
Trash. Configuration, customized assets, logs and backups are kept. If the cleanup reports an
error (for example a hook file that is not valid JSON), fix that file and run it again before
trashing the app; the executable is what the saved bindings point to.

**Repair.** If the app was moved, hooks still point at the old path. Open the app from its new
location and Apply the selection again: the saved bindings of the old path are replaced, and
other installations' bindings are untouched. If the old app is already gone, run the setup
window or `--setup` from the new location; stale entries can also be removed by hand from
`~/.codex/hooks.json` and `~/.claude/settings.json`.

Isolated lifecycle checks (first setup, repeat, deselection, status line, malformed files,
failed writes, upgrade, removal after environment changes) are in
`tests/macos_app_lifecycle.sh`; the release job also runs them against the program inside the
built image. Visual and live acceptance on the Mac is recorded in TODO.md (AH-102).

## Ubuntu system package

Install the downloaded x86-64 package from a terminal, or open it in a graphical
package manager:

```sh
sudo apt install ./aherald-vX.Y.Z-linux-x86_64-gnu.deb
aherald --console-setup
```

Run console setup as the desktop user, without sudo. It previews the selected
Codex and Claude hooks, usage settings and, for each selected agent, the context and
idle warnings, then asks before applying them. Each user configures their own hooks.
Every choice starts off on a first setup; later runs read the saved selections and
configuration, and an agent you deselect keeps its settings. Enabling the Claude context
warning wraps your existing status line (it is still shown and restored on removal); without
one, Claude shows the `[claude.statusline] format` text, empty by default. Claude usage's `auto`
source tries the login token, then the last statusLine report; existing source
settings are preserved. The optional launch starts a
resident for the current session; no autostart is installed. Start a new Codex
session and trust the hook entries in `/hooks` after setup.

The package places its executable and built-in assets under `/usr/lib/aherald`,
with `/usr/bin/aherald` as a stable entry point. User settings default to
`~/.config/aherald/config.toml`, the setup receipt to
`~/.local/share/aherald/aherald-setup.json`, and logs to
`~/.local/state/aherald/`; absolute `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, and
`XDG_STATE_HOME` override the corresponding bases. Custom resources may be kept
in the data directory or elsewhere, and relative resource paths are resolved
against the configuration file. Package updates and removal preserve user files.

Before package removal, each user should run `aherald --unsetup` to preview,
then `aherald --unsetup --apply` to remove their own saved hook bindings. The
package manager does not inspect or modify users' homes. Removal succeeds even
when a user has not cleaned up, in which case their harness may retain a hook
pointing to the missing executable. Reinstall the package, then run cleanup as
that user to repair it. Saved receipt paths remain valid when hook directory
environment overrides change. Keep `XDG_DATA_HOME` unchanged for cleanup so the
saved receipt can be found. The config, receipt, custom files, logs, and
backups are retained even on `apt purge`; remove them manually only if desired.

Updates do not stop running residents. Restart the resident after upgrading to
use the new binary; an incompatible IPC version can reject messages until it
is restarted. The package does not require an interactive terminal or configure
hooks from maintainer scripts. A downloaded `.deb` alone does not configure an
apt repository for automatic future updates. The `.tar.gz` portable archive
remains available and keeps its own existing layout.

Build a package from a previously compiled x86-64 GNU/Linux executable with
`sh package-deb.sh target/release/aherald`. It produces the `.deb` and SHA-256
file in `dist`. The isolated lifecycle check
`sh tests/linux_deb_lifecycle.sh dist/aherald-vX.Y.Z-linux-x86_64-gnu.deb`
requires Linux user, mount and PID namespaces plus `dpkg`. It runs in a private
root with dependencies forced and the host libraries mounted read-only, so it
does not verify dependency installation through apt or graphical installation.

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
