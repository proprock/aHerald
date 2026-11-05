# aHerald completed tasks

Tasks moved here from TODO.md after every deliverable and acceptance criterion
passed. Open follow-ups and deferred items stay in TODO.md under new IDs.

- [x] **AH-243 — One atomic file writer.**
  `src/atomic_file.rs`: `write(path, bytes, backup)` (sibling temporary file, `sync_all`, permissions of an
  existing file kept, optional `.aherald-backup` copy, atomic replace; a symlink stays a symlink, missing
  parents are created), `publish(temporary, path, replace)` (`rename` or an exclusive hard link on Unix;
  `MoveFileExW` with `WRITE_THROUGH`, plus `REPLACE_EXISTING` when replacing, on Windows),
  `temporary_sibling` and the `Temporary` guard. `hook_setup::write_inner` (hence `setup::apply_change` and the
  rollback) uses `write`; `default_config::create` keeps its exclusive-create contract and now reuses
  `temporary_sibling`, `Temporary` and `publish(.., false)` instead of its own copies. Unit tests: new file
  with parents and nothing left behind, replace with backup content, a write without backup leaves the
  earlier backup, a failed publish (a directory in the way) leaves the target and no temporary file,
  exclusive publication never replaces an existing file, unique sibling names, and on Unix permissions and
  symlinks. All were red against `unimplemented!()` stubs. `tests/hooks.rs`, `tests/setup.rs`,
  `tests/setup_bridges.rs`, `tests/config.rs` and the `hook_setup` unit tests pass unchanged. Linux: ran
  locally. The Windows branch of `publish` (one `unsafe` call, with its `SAFETY` comment, replacing the one in
  `default_config.rs`) compiles only in the Windows CI: compiled/tested in CI only.

- [x] **AH-238 — Help text lists every public flag.**
  `help()` moved from `src/lib.rs` to `src/cli.rs` (`aherald::cli::help`) as a `concat!` grouped into hooks
  and events, resident control, setup, usage and diagnostics, and configuration; every line is at most 100
  columns. It now names `--notify --kind --activity --ended`, `--agent`, `--setup-window`,
  `--setup-executable` and `--help`, which the old text omitted (checked against the old text from git); the
  internal `--*-child` and `--test-after-apply` stay out. Tests in `cli.rs`: every public flag, with a minimal
  command line, is accepted by `Cli::parse` and appears in `help()`; `help()` names no flag outside that
  list or an internal one; no line exceeds 100 columns (red once: a 101-column line). `tests/config_portable.rs`
  uses the new path; `tests/unsupported_platform.rs` needed no change. Linux: ran locally.

- [x] **AH-234 — `--install-hooks` honours `CLAUDE_CONFIG_DIR` and `CODEX_HOME`; README recommends `--setup`.**
  `paths::agent_hook_file(_with)` is the one lookup of an agent's hook file (Claude: `CLAUDE_CONFIG_DIR` or
  `~/.claude`; Codex: `CODEX_HOME` or `~/.codex`); `setup::file_for`, `--install-hooks`/`--uninstall-hooks`
  and the statusLine commands in `main.rs` use it, and `hook_setup::default_file` (home-only) is gone; its
  unit test moved to `paths` with the variable cases and empty-variable and no-home cases. README now
  recommends `aherald --setup --harness claude --apply`; `docs/installation.md` keeps `--install-hooks` as the
  legacy path and says it finds files like `--setup`; the Claude and Codex hook guides name the variables.
  Red: four of five new `tests/hooks.rs::hook_file_location` tests failed (hooks and statusLine written under
  `HOME` while `CLAUDE_CONFIG_DIR`/`CODEX_HOME` was set); the fifth (no variables) already passed. The
  statusLine test is `cfg(not(windows))`; on Windows that command already used `setup::file_for`. Linux: ran
  locally; Windows and macOS: compiled/tested in CI only.

- [x] **AH-248 — MSRV on every OS.**
  The `msrv` job in `ci.yml` is a matrix over `needs.changes.outputs.os` (windows-latest, macos-15,
  ubuntu-22.04, as the change selects them) and installs `libpulse0` on Linux; `cargo check --locked
  --workspace` runs on the `rust-version` from `Cargo.toml`. PR #119 (run 42118227412): "MSRV build" passed
  on all three legs and "CI gate" passed with it.

- [x] **AH-251 — One skip list for noisy tests; release uses it.**
  `.github/noisy-tests.txt` lists the four noisy tests, `.github/noisy-test-args.sh` turns the list into
  `--skip` arguments (a missing list fails), `tests/ci_noisy_tests.sh` tests it (in the `changes` job), and the
  `cargo test` step of `ci.yml` and the three test steps of `release.yml` read it; the nightly full run still
  runs everything. The AH-221 note says the release skips the memory-growth check too. PR #119: the
  Linux, macOS and Windows test steps each print `Skipping: ` with the four names and pass (Windows reports
  one test filtered out, its native memory-growth test). Not yet seen: the release workflow's logs and a
  nightly run; they are AH-280.

- [x] **AH-246 — One source for harness labels and bridge lists.**
  `events::harness_label` takes the label from `Bridge::label` (plus Crush, which has no bridge file), so
  `HARNESS_LABELS` is gone. `Selection` maps a bridge to its field in one exhaustive `match`
  (`bridge`, `set_bridge`); `initial`, `bridges` and the Linux console prompts (`main.rs`, including the
  "OpenCode, Pi and Goose use ..." line) iterate `Bridge::ALL`. `tests/installer_bridges.rs` fails when
  `installer/windows.iss` lacks a task, the `--harness <key>` argument or the `<key>_bridge` report line for
  a bridge (mutation check: deleting the Goose report line fails it; reverted). The Inno Setup script and the
  macOS setup window are left as they were, by the task: `src/mac/setup_window.rs` still names the three
  fields and its own arrays, so a new `Bridge` variant fails to compile in `setup_selection.rs`, in the label
  and bridge tables, and in the installer test, but not in the macOS window. Red: `Selection::bridge` and
  `set_bridge` did not exist. `tests/console_setup_linux.py` passes with the new prompts. Linux: ran locally.
  Windows and macOS: compiled in CI only (no `cfg(windows)` or `src/mac/` code touched).

- [x] **AH-237 — Bridge contract tests cannot skip silently in CI.**
  `AHERALD_REQUIRE_NODE=1` makes `tests/bridges.rs` panic when Node 22+ is missing (checked locally with an
  empty `PATH`: panics with the variable, skips without it). The Linux leg of `quality` installs Node 22 with
  a SHA-pinned `actions/setup-node` (v7.1.0) and sets the variable on both `cargo test` steps. PR #118: the
  `ubuntu-22.04` log shows `test bridges_turn_harness_events_into_bounded_aherald_calls ... ok`; "Lint
  workflows" (actionlint 1.7.12, zizmor 1.32.0) passes. Linux: ran locally and in CI.

- [x] **AH-252 — Reproducible build stamp.**
  `.github/actions/setup-rust` exports `SOURCE_DATE_EPOCH` from `git log -1 --format=%ct` (checked to be an
  integer; the one `GITHUB_ENV` write carries a `zizmor: ignore[github-env]` with the reason), so every CI and
  release job gets it; `build.rs` explains why. PR #118: the step ran and the build steps show the same
  `SOURCE_DATE_EPOCH` in the `ubuntu-22.04` log. Locally two builds with one epoch print the same `--version`
  line (minute resolution). Not done: comparing the `--version` of two real release builds of one commit; the
  next release candidate shows it.

- [x] **AH-244 — One civil-date implementation.**
  `src/time.rs` holds `days_from_civil` and `civil_from_days`; `operational_log::format_timestamp` and
  `claude_usage::iso_to_unix` use them instead of two copies of Hinnant's algorithm. New table tests (epoch,
  1969-12-31, 2000 and 2024 leap days, 2100-02-28 and 2100-03-01, 1900-03-01, 2026-11-05), the century rule
  and a round trip over every day from 1800 to 2200. Expected day numbers were checked against Python's
  `datetime`; the existing log and usage tests pass unchanged. Linux: ran locally.

- [x] **AH-242 — Shared quoting helpers.**
  `src/quote.rs` has `posix` and `powershell`; the eight copies in `hook_setup.rs` (including its private
  `quote`), `setup.rs` and `bridge_setup.rs` use them. Tests: spaces, quotes, `$`, backtick, backslash,
  newline, empty string, and on Unix every posix word is run through `sh -c printf %s` and must come back
  unchanged. `tests/hooks.rs`, `tests/setup.rs` and `tests/setup_bridges.rs` pass unchanged. The three
  `cfg(windows)` call sites (`setup.rs`: Codex `commandWindows` and the encoded statusLine command) compile
  only on Windows: PR #118 "Build and test (windows-latest)" and macOS passed (compiled/tested in CI only).
  Linux: ran locally.

- [x] **AH-236 — Claude plugin version follows the crate.**
  `plugins/aherald/.claude-plugin/plugin.json` said 0.1.0 while the crate is 0.15.0. Set to the crate version;
  `tests/workspace.rs::the_claude_plugin_has_the_version_of_aherald` asserts equality with
  `CARGO_PKG_VERSION`. Red: left `0.1.0`, right `0.15.0`; green after the edit. `RELEASE.md` step 3 already
  lists the file, so a release bump keeps it in step; the one-time edit outside `RELEASE.md` was approved by
  the owner (2026-11-05). Branch `chore/AH-236-contract-tests`. Linux: ran locally.

- [x] **AH-239 — The two shell guest senders stay identical.**
  README.md and `docs/privacy.md` state what each sender posts (earlier, `docs/agents-release-privacy`).
  Now `# BEGIN remote` / `# END remote` mark the curl post and the Codex acknowledgement in
  `examples/sandbox/aherald-ping.sh` and `plugins/aherald/hooks/aherald-hook.sh`, and
  `tests/sandbox_sender.rs::the_remote_block_is_the_same_in_both_shell_senders` compares the two blocks
  without indentation (the plugin nests its copy in a branch). Red: no marker found; green after the markers.
  The guards around the block differ by design (the launcher also runs local programs), so they stay
  outside it. Linux: ran locally.

- [x] **AH-256 — Default config parity test.**
  `tests/config.rs::the_example_configuration_equals_the_built_in_defaults` parses `config.example.toml` and
  compares the whole `Config` with the defaults through `Debug` text (no `PartialEq` added to some 25 nested
  types). The only intended difference is the card colour of five events, which the example spells out and
  the program resolves through `Config::event_color`; the test checks this against that function. Mutation
  check: changing `Remote::default().bind` fails the test (reverted). Linux: ran locally.

- [x] **AH-247 — Contract test between aherald-ping and the main hook parser.**
  `aherald_ping::wanted` is `#[doc(hidden)] pub`; `crates/aherald-ping/tests/parser_contract.rs` asserts it
  agrees with `Agent::{Claude,Codex}.parse_hook` over every event name either side knows (Claude: 14 names
  and the notification types; Codex: 9 names; the recorded `tests/fixtures/*.json` hook payloads). The host
  accepts `UserPromptSubmit`, `PreToolUse` and `PostToolUse` as activity evidence while a guest never
  forwards them; they are the listed exception and a test pins that they really parse as activity.
  `SessionEnd` is forwarded by the sender (the host uses it as evidence), unlike the task text assumed.
  Mutation check: removing `SessionEnd` from the Claude arm fails the test (reverted). Linux: ran locally.

- [x] **AH-271 — Split the remaining partly shipped tasks (follow-up of AH-241).**
  `CHANGELOG.md` cites nine IDs that are still open in TODO.md because their manual or live checks are
  outstanding: AH-093, AH-102, AH-103, AH-115, AH-117, AH-118, AH-120, AH-211, AH-222. For each, move the
  shipped part to DONE.md with its status lines, and keep or open an ID holding only the checks still
  marked "Not run" / "not yet checked" (as AH-268 to AH-270 did for AH-206/213/219/220). Do not mark a
  check done without its recorded evidence.
  Acceptance: every ID in CHANGELOG exists in DONE.md; no open task describes shipped code as pending.
  Closed 2026-11-05 (branch `docs/agents-release-privacy`). AH-102, 103, 117, 118, 120, 211 and 222 moved
  to DONE.md with a note naming the release that shipped them (checked against the CHANGELOG sections);
  their open checks are AH-272 to AH-278, placed where the originals were. AH-117: `update_tap` merged a
  tap pull request for every release from 0.9.1 to 0.15.0, so that check is done. AH-093 and AH-115 stay
  open: they are acceptance tasks with no code of their own, and CHANGELOG 0.9.0-beta cites them only as
  "See TODO.md". So every ID that a CHANGELOG entry credits is in DONE.md; AGENTS.md now allows such
  "see TODO.md" pointers to open tasks.

- [x] **AH-222 — Suppress notifications while the user watches the originating session (Linux X11).**
  Windows (AH-098) and macOS (AH-120) suppress the card and the sound on positive evidence of both
  the window and the session; on Linux `focus::observe` reports "unknown", so everything is
  delivered. Evaluate X11 (`_NET_ACTIVE_WINDOW` and the window's PID against the terminal process
  aHerald records) and Wayland-under-XWayland limits; a terminal with tabs needs a way to prove the
  active tab (its tty). Same rule: suppress only on positive evidence of both, otherwise deliver; no
  new permission prompt. Agree on the design before implementation.
  Status (branch `feature/AH-222-linux-focus`): implemented for X11 sessions (`src/linux/focus.rs`).
  Window: the captured host window equals `_NET_ACTIVE_WINDOW`. Session: the owner's `comm` is a
  one-session terminal (xterm, urxvt, rxvt, st, mlterm, aterm, eterm, alacritty) and it owns no other
  window. Wayland sessions, tabbed terminals, tmux and errors deliver. Pure tests cover the rule; not
  run against a live X server here (the session is Wayland, no Xvfb): acceptance on Xorg with xterm
  and with a tabbed terminal is open. Konsole (D-Bus `currentSession`) and Kitty (remote control) could
  later give the selected tab.
  Closed 2026-11-05 (AH-271): shipped in 0.15.0 for X11 sessions. The live acceptance on Xorg is AH-278.

- [x] **AH-211 — Windows: a console companion for command-line output and the `statusLine`
  command.** One cause, two symptoms: `aherald.exe` is a GUI-subsystem program (so a hook never
  opens a console window) and has no console of its own.
  1. Command-line output. `--version`, `--help`, `--status`, `--setup`, `--perf-report`,
  `--raw-output` and their error messages print nothing in PowerShell or cmd unless piped
  (`| Out-String`); the packaging scripts and docs work around it.
  2. The `statusLine` command. On Windows the command in `~/.claude/settings.json` is always (with
  or without `--then`) a PowerShell script spelled as `[char[]]` codes, which nobody can read or
  audit. It is that way because Claude Code runs a `statusLine` through Git Bash if installed and
  PowerShell otherwise, and the command must survive both (they expand `$`, quotes and backslashes
  differently); PowerShell neither waits for a GUI program nor collects its stdout unless piped; and
  `Out-String` would re-encode UTF-8 and ANSI colours, so the script copies bytes by hand. A
  `statusLine` has no `args` or `shell` field (only `type`, `command`, `padding`,
  `refreshInterval`, `hideVimModeIndicator`), so there is no exec form as for hooks.
  Approach: ship `aherald-cli.exe` beside `aherald.exe`, a tiny console-subsystem program that starts
  `aherald.exe` with the same arguments, gives it its own stdin, stdout and stderr, waits and
  returns its exit code. It is not for hooks (a parent without a console could flash a window); it
  is fine for the `statusLine` (Claude Code runs it in a terminal; check the IDE extension) and for
  commands typed by hand. The `statusLine` command becomes `C:/…/aherald-cli.exe --statusline`.
  Decide on Windows: (a) a path with spaces cannot be quoted the same way in Git Bash and in
  PowerShell, so such an install needs the fallback below; (b) the user's wrapped command: keep
  `--then '…'` on the command line (the same quoting problem) or read the original `statusLine`
  from the receipt `aherald-setup.json`, which already holds it, so the line stays short.
  Fallback: an aHerald-owned `statusline.ps1` beside the executable (listed in the receipt, backed
  up and removed like the bridges of AH-204) with the command
  `powershell -NoProfile -ExecutionPolicy Bypass -File C:/…/statusline.ps1`, for a path with spaces
  or a missing companion; the script copies stdin and stdout byte for byte. Not chosen:
  `AttachConsole` at start (the shell still does not wait for a GUI program).
  Acceptance, by hand on Windows 11 and Server 2019 (not by reading code) and with a CI pipe test:
  `aherald-cli --version` and an unknown option print in PowerShell, cmd and Windows Terminal
  without `| Out-String`, with the exit code kept; with output piped nothing changes; the status
  line, a wrapped user command, its exit code and non-ASCII output are unchanged;
  `--install-statusline` and `--uninstall-statusline` restore the original exactly and replace the
  old encoded command; a hook from Claude Code and Codex still delivers and opens no console window;
  `package-release.ps1` no longer needs its pipe workaround.
  Status 2026-11-04 (branch `feature/ah-211-windows-console-companion`): `aherald-cli.exe`
  (std only) exists. Inheriting the console shows nothing for a GUI child (seen in a real
  PowerShell and cmd), so it relays stdin, stdout and stderr through pipes and returns the exit
  code; pipe tests compare it with `aherald.exe` (`tests/aherald_cli.rs`), and `--version` and an
  unknown option were checked by hand in PowerShell and cmd. Decisions: (a) a bare
  `C:/…/aherald-cli.exe --statusline` when no character in the path is special to either shell;
  else `powershell -File "…/statusline.ps1"` with an aHerald-owned script (receipt, backup, removal);
  the encoded command stays only for a path with `"`, `$` or a backtick. (b) The maintainer chose
  the receipt: `--statusline` without `--then` reads the original from `aherald-setup.json`; a
  missing receipt gives a plain line, a corrupt one a plain line and a note on stderr.
  `tests/statusline_windows.rs` runs the generated command through PowerShell and Git Bash:
  bytes, non-ASCII and ANSI output and exit code 7 relayed; `--unsetup` restores the original
  exactly and removes the script; setup replaces an earlier encoded command. `package-release.ps1`,
  `build-installer.ps1` and the installer ship the companion; the script no longer pipes. About 7 ms
  per status line refresh is added by the companion (25 ms to 32 ms, release build).
  Remaining: manual checks on Server 2019 and in Windows Terminal; a live Claude Code and Codex
  hook (still `aherald.exe`, unchanged) shows no console window; the installer lifecycle with the
  companion (`tests/installer_windows.ps1` still pipes).
  `--install-statusline` / `--uninstall-statusline` now work on Windows (they share the setup
  receipt; tests in `tests/statusline_windows.rs` cover dry run, wrap, idempotence, exact restore,
  the spaced-path script, an earlier encoded command, and hooks left alone).
  Closed 2026-11-05 (AH-271): `aherald-cli.exe` and the readable `statusLine` command shipped in
  0.13.0. The remaining checks are AH-277.

- [x] **AH-120 — Suppress notifications while the user watches the originating session (macOS).**
  Windows suppresses the card and the sound when the Claude session's classic console is
  focused (AH-098, `src/focus.rs`); on other systems `focus::observe` reports "unknown" and
  nothing is suppressed. Evaluate the macOS options: the frontmost application through
  `NSWorkspace` and the terminal process aHerald already records (`src/mac/terminal.rs`) tell
  the app, but a tabbed Terminal.app or iTerm window also needs a way to prove the active tab
  or session (for example its tty). The project rule stays: suppress only on positive evidence
  of both the window and the session; otherwise deliver. Agree on the design and any new
  permission (Automation/Accessibility prompts are not acceptable by default) before
  implementation; keep both `suppress_when_focused` options and their defaults.
  Acceptance: pure policy tests with the macOS evidence, and live checks on the M1 in Terminal
  and in a second terminal app, including unknown origins that must still be delivered.
  Design agreed 2026-11-03: opt-in Automation. `[general] detect_terminal_tab` (default false)
  adds `Event::tty` (the nearest ancestor's device, one `ps` per hook) and, for Terminal.app
  and iTerm2 only, reads the selected tab's device with `osascript` (1.5 s bound) when that
  terminal is in front (`NSRunningApplication.isActive`, no NSWorkspace feature added). Equal
  devices are `Session::Same`; everything else delivers. Claude main sessions only, as on
  Windows. Status 2026-11-03 (branch `feature/ah-120-mac-focus-suppression`): implemented with
  unit tests (761 pass); NOT checked live: the Automation prompt, the real tab comparison in
  Terminal.app and iTerm2, a second tab/terminal delivering, and the `NSAppleEventsUsageDescription`
  text in the bundle.
  Closed 2026-11-05 (AH-271): shipped in 0.9.0 as opt-in `detect_terminal_tab`. The live checks are
  AH-276.

- [x] **AH-118 — Context/Idle warning switches and Apply / Apply + Test in the macOS tray.**
  The Windows tray (`src/tray.rs`) and the Linux tray (`src/linux/tray.rs`, handlers in
  `src/linux/resident.rs`) offer "Context warnings", "Idle warnings", "Apply" and
  "Apply + Test"; the macOS menu (`src/mac/menu.rs`) has only "Restart". Add the same entries
  with the same order, labels and check marks, wired to `Warnings::set_enabled` and to
  re-reading the configuration with the preview cards, as on the other platforms.
  Acceptance: pure menu tests for order, labels and check state; switches change what the
  resident admits, observed through the resident (not source text); Apply keeps usage and
  event handling running; verified by hand on the MacBook Pro M1 (cards, dismissal).
  Status 2026-11-03 (branch `feature/ah-118-mac-tray-warnings`, stacked on AH-102): the menu
  has the Windows order and separators (usage lines, Sound, Pop-up, Pause, Context warnings,
  Idle warnings, About, Edit configuration, Apply + Test, Apply, Exit; Apply was Restart). The
  switches call the shared `Warnings` gates (tested through the resident state). Apply + Test
  restarts the resident with `--test-after-apply` and shows the shared `demo::Carousel` as on
  Windows and Linux: arrows, wheel and body clicks, a "[Test]" heading with "Disabled in
  config", each sample's sound once, ended by dismissal or the first real card; the click
  decision is the pure `input::carousel_click`. Automated: fmt, Clippy and
  `cargo test --all-targets` (753 passed). Not yet checked by hand on the M1: the menu itself,
  the switches' effect on live cards, Apply, Apply + Test (cards, sounds, arrows, wheel, dismiss).
  Closed 2026-11-05 (AH-271): the macOS menu shipped with the 0.9.0 parity work. The checks by hand
  are AH-275.

- [x] **AH-117 — Homebrew Cask for aHerald (split from AH-102).**
  Publish a Cask for the Apple Silicon app in a separate tap repository, installing the
  released DMG or ZIP by URL and SHA-256 from the release artifacts. Do not claim signing or
  notarization; keep the portable archive. Removal must clean owned hooks in user context
  before the executable is removed (`zap` is not enough): define the uninstall flow, its
  visible failure and the repair path, and document that Homebrew runs as the user while
  hooks belong to that user. Updates must not block on a running resident; document the
  restart. Acceptance: `brew install --cask`, setup window, hooks, delivery and dismissal,
  upgrade and uninstall on the MacBook Pro M1.
  Decisions 2026-11-03: tap `proprock/homebrew-tap` (`brew tap proprock/tap`), shared by
  future projects; no quarantine removal (README documents right-click, Open); stable releases
  only, updated by the `update_tap` release job through a pull request using the secret
  `TAP_TOKEN`. Status: `proprock/homebrew-tap` exists with the Cask (0.9.0, checksum verified
  against the released DMG), README, `update-cask.sh` and a style-audit CI (green). `TAP_TOKEN`
  is set. Not yet checked: `brew install --cask`, upgrade and uninstall on the M1, and the first
  `update_tap` run.
  Closed 2026-11-05 (AH-271): the tap and the Cask exist, and `update_tap` opened and merged a pull
  request for every stable release from 0.9.1 to 0.15.0 (`gh pr list -R proprock/homebrew-tap`).
  The checks on the M1 are AH-274.

- [x] **AH-103 — Ubuntu package and interactive console setup.**
  Uses the common native setup backend from AH-097. Deliver an x86_64 `.deb`
  installable through apt and a graphical package manager, preserving the
  portable archive. Provide an interactive console setup run by the user after
  installation, with selection of Codex, Claude Code, both or neither; start
  unchecked on first setup, retain the selection on upgrade, and delegate changes
  to `--setup` / `--unsetup` with explicit `--apply`. Do not require an
  interactive terminal or configure user hooks from package scripts.
  Match Windows graceful shutdown before updates/removal: verify the owned app
  path, wait for process exit, and abort if graceful shutdown fails.
  Match Windows usage selection and launch-after-install behavior: read current
  values on upgrades, keep new usage off, explain Claude auto, preserve configured
  sources and unrelated formatting, and include changes in setup rollback.
  Use stable installation paths and fixed hook executable paths, without runtime
  discovery or extra developer runtimes. Preserve normal hook trust and do not
  add autostart. Reuse dry run, preflight, backups, rollback, idempotence, saved
  file paths and exact binding ownership. Preserve foreign hooks, configuration,
  customized assets, logs and backups across updates/removal.
  The system package owns its executable and built-in resources. Use XDG config,
  data and state directories for each user's config, receipt, assets and logs;
  preserve user files even on purge and keep portable layout unchanged. Package
  scripts must not modify user homes or hooks. Document that each user should run
  --unsetup --apply before removal; removal remains noninteractive and may leave
  stale hooks, recoverable by reinstalling and running cleanup as that user.
  Extend Linux release CI with the package and checksum alongside the portable
  archive; add isolated native lifecycle checks for install, repeated setup,
  selection changes, malformed configuration, failed writes, upgrade, cleanup
  failure and uninstall after environment overrides change.
  Acceptance: verify apt and graphical installation, interactive console setup, hook
  delivery/dismissal, update and removal on a clean supported Ubuntu desktop
  without developer tools. Record Ubuntu version, desktop/session type and
  harness versions; document required system packages, repair and removal.
  See docs/installation.md; warning setup controls are tracked in AH-114 and
  overlay Xorg/multi-monitor acceptance remains AH-093.
  Partial 2026-11-02: package construction, XDG paths and console setup have
  isolated tests. A private dpkg root exercises install, same-version upgrade,
  user cleanup and purge with dependency checks forced; an Xvfb/D-Bus probe
  verified new-client IPC against an older resident. On Ubuntu 22.04.5, lintian
  reported no findings and apt-get -s resolved the local package on this host
  (with a warning about an unreadable third-party sources file). Clean Ubuntu apt/graphical
  installation, real hook delivery/dismissal and update/removal remain to be
  accepted; the isolated tests do not establish those results.
  Closed 2026-11-05 (AH-271): the `.deb`, XDG paths and the console setup shipped in 0.9.0-beta with
  the isolated tests above. Acceptance on a clean Ubuntu desktop is AH-273.

- [x] **AH-102 — Native macOS installer and setup UI.**
  Uses the common native setup backend from AH-097. Deliver GUI app installation
  and Homebrew Cask packaging for Apple Silicon, preserving the portable archive.
  Provide native user-context selection of Codex, Claude Code, both or neither;
  start unchecked on first setup, retain the selection on upgrade, and delegate
  changes to `--setup` / `--unsetup` with explicit `--apply`.
  Do not block package updates/removal on resident shutdown. Check old-resident
  and new-client IPC compatibility; document when users must restart residents.
  Match Windows usage selection and launch-after-install behavior: read current
  values on upgrades, keep new usage off, explain Claude auto, preserve configured
  sources and unrelated formatting, and include changes in setup rollback.
  Use a stable installed app path and fixed hook executable paths, without
  runtime discovery or developer runtimes. Preserve the harness's normal hook
  trust flow and do not add autostart. Reuse dry run, all-file preflight, backups,
  rollback, idempotence and exact saved-binding removal. Keep the receipt and
  writable user data outside the signed app bundle; never modify its contents
  during setup. Preserve configuration, assets customized by the user, logs and
  backups on upgrade/removal, and preserve foreign hooks and other installations.
  Define a user-context removal flow for both GUI and Cask installations that
  cleans owned hooks before removing the executable, with visible failure and
  a repair path. Define behavior for an active resident and moved app bundles.
  Extend macOS release CI with installer/Cask artifacts and checksums alongside
  the portable archive; do not claim signing/notarization without implementing
  and verifying it. Add isolated native lifecycle checks for first install,
  repeated setup, selection changes, malformed configuration, failed writes,
  upgrade, cleanup failure and uninstall after environment overrides change.
  Acceptance: verify the GUI, both installation paths, native hooks, delivery and
  dismissal on the MacBook Pro M1, recording OS/harness versions and limitations;
  verify a normal user installation without Homebrew or developer tools for the
  GUI path. Document setup, repair and removal. See docs/installation.md.
  Scope change (2026-11-03): the Homebrew Cask is split out as AH-117 (separate tap
  repository); AH-102 delivers the DMG, the in-app setup window and the portable archive.
  Status 2026-11-03 (branch `feature/ah-102-macos-installer`): implemented
  `setup_selection::Selection` (shared with the Linux console setup), the AppKit setup window
  (`--setup-window`, also opened by a bare launch of an installed bundle), `package-dmg.sh`,
  `check-release-dmg.sh`, `tests/macos_app_lifecycle.sh` (also run against the mounted image),
  release/CI steps and docs. Automated: fmt, Clippy and `cargo test --all-targets` pass on macOS
  (748 passed); the lifecycle check passes and fails on a no-op executable; the DMG built and
  verified locally. Manual on the M1 (macOS Tahoe 26.6.2, run from a copy of the debug build
  with isolated CODEX_HOME/CLAUDE_CONFIG_DIR): window layout, scrolling details, Preview, Apply,
  retained selection, removal and a malformed settings file passed. Still open: installing from
  the real DMG into /Applications and the Finder double-click launch, Codex/Claude hook
  delivery and dismissal with the installed app, a normal user without developer tools,
  old-resident/new-client IPC compatibility check on macOS, and Remove/Repair buttons (removal
  works by clearing both choices; repair by applying again). Not signed or notarized.
  `[claude.statusline] format` checked on macOS (Darwin 25.6.0, debug build, commands only): the
  same text prints with the config beside the executable and with the config under
  Application Support in an app bundle, and through the wrapper that `--setup` installed for a
  non-empty format. The setup window explains it; the text itself stays in config.toml.
  Closed 2026-11-05 (AH-271): the DMG, the setup window and the portable archive shipped in 0.9.0.
  The checks by hand still open are AH-272.

- [x] **AH-254 — Docs index and engineering notes split.**
  Create `docs/README.md` with user guides grouped (install, configure, harnesses, sandboxes, troubleshooting).
  Move the engineering notes (`ah-0xx-*`, `milestone-*`, `*-investigation`, `wayland-spike`, `diagnostics-audit`,
  `session-warnings-research`, `plans/`) to `docs/notes/` with `git mv` and fix links (`grep -rn "docs/ah-"`).
  Fix README.md:174: nextest is not the test runner (the decision is in DONE.md); point to `tests/live/README.md`
  and `docs/notes/ah-109-nextest.md` as history. Acceptance: a link check (`grep -o '](docs/[^)]*)'` resolves)
  passes for README, docs and TODO.
  Closed 2026-11-05 (branch `docs/agents-release-privacy`). `docs/README.md` indexes the user guides
  (install and run, configure, harnesses, sandboxes, project; there is no troubleshooting guide yet).
  Moved with `git mv` to `docs/notes/`: `ah-097-*`, `ah-100`, `ah-101`, `ah-107` to `ah-110`,
  `milestone-1` to `5`, both `*-investigation`, `wayland-spike`, `diagnostics-audit`,
  `session-warnings-research`, `release-acceptance` (a per-release acceptance record) and `plans/`.
  Links and plain paths fixed in README, TODO, DONE, the guides, the moved notes, `tests/live/README.md`
  and two comments in `tests/perf_baseline.rs`; CHANGELOG release notes keep their old paths.
  README Development says tests run with plain `cargo test` and links the nextest note as history, the
  docs index and AGENTS.md. Check: a script resolving every relative markdown link in tracked `.md` files
  reports none broken (before and after); `cargo fmt --check` clean. The packaging scripts copy the whole
  `docs/` folder; they now remove `docs/notes/` from the Windows, macOS and Linux archives (and so
  from the Windows installer), and the index links the notes on GitHub. Checked by building the Linux
  archive: no `docs/notes` entry, 27 guides.

- [x] **AH-241 — TODO.md / DONE.md hygiene.**
  Move every `[x]` item from TODO.md to DONE.md unchanged (AH-116, 105, 090, 201–204, 223–228). For AH-206, 213,
  219 and 220 (shipped in 0.13.0/0.14.0): move the implemented part to DONE.md and open new IDs only for the
  manual checks still listed as "Not run". Remove the empty "Milestone 6" and "Milestone 7" sections or give them
  open tasks. Acceptance: `grep -c '^- \[x\]' TODO.md` is 0; every ID in CHANGELOG exists in DONE.md.
  Closed 2026-11-05 (branch `docs/agents-release-privacy`). Moved the 13 `[x]` items unchanged and closed
  AH-206/213/219/220 with notes; their manual checks are AH-268 (macOS), AH-269 (Windows 11) and AH-270
  (Linux) in a new Milestone 9 section. Removed the empty Milestones 6 and 7. `grep -c '^- \[x\]' TODO.md`
  is 0. The second criterion is met for these tasks only: nine more IDs in CHANGELOG (AH-093, 102, 103,
  115, 117, 118, 120, 211, 222) are partly shipped with open checks and were not in this task's list;
  they are AH-271.

- [x] **AH-220 — Command sink (split from AH-213).**
  The webhook sink is done under AH-213. What is left is a sink that runs a command as an argument
  vector without a shell, with the same `${NAME}` and `{text}` placeholders, for services that have
  a CLI rather than an HTTP endpoint. Reuses `forward::build` and the `Sink` contract.
  Acceptance: a test with a recording command; no shell is involved; the message never reaches a
  shell parser.
  Status (branch `feature/AH-220-command-sink`): implemented. `type = "command"` with `command`
  (argument array) and optional `stdin`; `src/command_sink.rs`; a 10 s limit kills the command; the
  error names the exit status only. Tests: a recording `sh` program (arguments, stdin, hostile folder
  name runs nothing, exit status, timeout, secret scrubbed), config validation, build. Unix only: the
  recording program is `sh`, so the Windows CI skips them; not run by hand with a real mailer.
  Closed 2026-11-05 (AH-241 hygiene): shipped in 0.14.0. A run with a real command moves to AH-270
  (Linux) and AH-269 (Windows, where the automated tests are skipped).

- [x] **AH-219 — Messenger switch in the macOS and Linux tray menus (split from AH-213).**
  Windows has a **Messenger** item beside Sound and Pop-up (`crate::forward::enabled`/`toggle`).
  Add the same item to `src/mac/menu.rs` (`Command`, `items`, the AppKit handler) and to the
  StatusNotifierItem menu in `src/linux/tray.rs`; both residents already start and stop the sinks.
  Acceptance: the pure menu tests list the item only when a sink runs, in the Windows order;
  checked by hand on macOS and a Linux desktop.
  Status (branch `feature/AH-219-tray-messenger`): implemented. `mac::menu::items` and the Linux
  dbusmenu list **Messenger** after Pop-up only when a sink runs (pure tests for order, label,
  check state and absence); both residents flip `forward::toggle` and refresh the menu. Not run:
  by hand on a macOS menu bar and a Linux tray, and `mac/resident.rs` compiles only in the macOS CI.
  Closed 2026-11-05 (AH-241 hygiene): shipped in 0.14.0. Checks by hand moved to AH-268 (macOS menu
  bar) and AH-270 (Linux tray).

- [x] **AH-213 — Sinks: send cards to a messenger, Telegram first.**
  Design agreed 2026-11-04. A *sink* lives in the resident, takes only events the queue accepted
  as new cards (`Outcome::Queued`, so deduplication, per-event `enabled`, session ends and
  obsolete waits are already settled) and never feeds anything back.
  - Contract: `struct Message { kind, harness, folder: Option<String>, text }` and
    `trait Sink: Send { fn name(&self) -> &str; fn send(&mut self, &Message) -> Result<(), String>; }`.
    No enum of kinds, no plugin loading; one method, tested with a stub.
  - Privacy by construction: the message carries the event kind, the harness and the folder name
    only, never prompts, `last_assistant_message`, paths or session names. The folder goes to a
    third party, so it is off unless `include_folder = true`.
  - Delivery: a worker thread with a bounded queue (the oldest is dropped when full), a timeout and
    capped retries, so a hung network never delays cards, IPC or the tray. Failures are shown, not
    hidden: the first and each different error posts a card through `notice::report_changed`; no
    secret ever appears in an error, a log line or `--raw-output`.
  - Per-sink filter: event kinds (a messenger usually wants `turn_complete`, `permission_required`,
    `input_required`, `error`), independent of `suppress_when_focused`, which is about the screen
    in front of the user. "Only when away from the computer" needs an idle detector that does not
    exist; leave it out of the pilot.
  - Secrets from the environment: the configuration names a variable, never holds the value
    (`token_env = "AHERALD_TELEGRAM_TOKEN"`, `chat_id_env = …`); `token_file = "…"` (owner-only
    file) as the second form. A literal `token = "…"` is rejected, so a pasted
    `config.toml` cannot leak it. A missing or empty variable is a startup notice naming the
    variable, not the value. On Windows a resident started from the Run key or the installer sees
    the user's environment only; say so in the docs and in the notice.
  - Telegram: `POST https://api.telegram.org/bot<token>/sendMessage` with `chat_id` and `text`
    (JSON body, no parse mode, so nothing needs escaping). `https.rs` (WinHTTP) and the `ureq`
    path only have GET: both need a bounded POST with a body, reusing the existing proxy/TLS
    handling and the `Wiped` secret type for the token. This is the largest part of the work.
  - Order of work: (1) POST in both HTTP layers, tested against the existing local test server;
    (2) the trait, the worker and the filter with a stub sink; (3) a generic *command or webhook*
    sink (a template with the kind and folder, run as an argument vector without a shell, or a
    POST to a URL), which already serves ntfy, Slack and Telegram; (4) Telegram as a preset on
    top of it, only if a preset is still wanted; (5) the settings window and `--setup` stay
    unchanged until the keys settle.
  Acceptance: unit tests for the filter, retry and drop-oldest, error de-duplication and secret
  redaction; a process test with a local fake Telegram server that sees exactly the allowed
  fields; a hung server leaves card delivery within its budget; by hand, one real bot on Windows 11
  and Linux.
  Status (branch `feature/AH-213`): done and tested — POST in both HTTP layers, the sink contract,
  worker, filter, retry, drop-oldest, error de-duplication, secret redaction, `[forward]`
  configuration, the generic **webhook** sink (`${NAME}` environment secrets, `{text}` body
  template, escaping by content type) tested against a fake server, and the Windows tray
  **Messenger** switch (an agreed addition). Telegram is a documented webhook example, not a
  separate sink (decided with the maintainer; `token_file` was dropped with it). Checked by hand
  on Linux (2026-11-05, Ubuntu 22.04, real Telegram bot, `include_folder` on and off, package 0.12.0
  resident). Left open: the by-hand check on Windows 11, and a compile and test run on macOS
  (AH-219 for the macOS and Linux menus).
  Not run here: `cargo check` for the macOS and Linux targets (no C cross toolchain).
  Closed 2026-11-05 (AH-241 hygiene): shipped in 0.13.0; the macOS and Linux menus followed in 0.14.0
  (AH-219), and the release workflow compiled and tested every platform. The check by hand with a real
  messenger on Windows 11 moves to AH-269.

- [x] **AH-206 — Offer the new harnesses in the installers.**
  Depends on AH-204. Windows installer, macOS setup window and Linux console setup list OpenCode,
  Pi and Goose beside Codex and Claude (default unchecked, installed ones retained) and pass the
  selection as `UsageOptions::bridges`. The Windows part can be checked by CI only on this setup.
  Status (branch `feature/AH-206-installer-harnesses`): implemented on all three. `Selection` holds
  the bridges and `options()` always names them; `--setup-settings` reports `<name>_bridge=` read
  from the receipt beside the selected configuration (the Windows installer asks from a temporary
  copy). Linux console setup asks three more questions (`tests/console_setup_linux.py` covers
  install, keep, remove and removal with hooks); the macOS window has a row of three checkboxes;
  `installer/windows.iss` has three tasks and `tests/installer_windows.ps1` a bridge lifecycle.
  Not run: the Windows installer test (CI only), the macOS window by hand, and `mac/setup_window.rs`
  compiles only in the macOS CI.
  Closed 2026-11-05 (AH-241 hygiene): shipped in 0.14.0. The macOS setup window check by hand moves
  to AH-268; the Windows installer lifecycle runs in the Windows CI (`tests/installer_windows.ps1`).

- [x] **AH-228 — Release `aherald-ping` for musl and test it in a container.**
  Depends on AH-226. x86_64 and aarch64 musl artifacts with checksums and attestation, a static
  link check, and a Linux CI job with a Docker container (non-required at first).
  Status: `package-ping.sh x86_64|aarch64` (static check: no INTERP, no NEEDED; names the file and its
  checksum), release job `package_ping` (rust-lld links both musl targets, no C cross toolchain;
  attested; part of the draft's checksum check), and CI job `ping_container` (nightly only, `continue-on-error`,
  not in the gate) running `crates/aherald-ping/tests/container.rs` (ignored by default) against a
  listener on the `docker0` bridge. Locally: both targets build and pass `package-ping.sh`; the
  container test passes with Alpine. The aarch64 binary was only built, never run (no emulator
  here); the release and CI jobs themselves have not run on GitHub.

- [x] **AH-227 — Documentation and the agent instructions for guests.**
  `docs/sandboxes.md` with a recipe per environment (each proven by AH-223),
  `docs/sandbox-setup-for-agents.md`, `docs/configuration.md` `[remote]`, `examples/sandbox/`.
  Status: written. Recipes are labelled verified (Docker Engine, bubblewrap on Linux) or upstream docs.

- [x] **AH-226 — `aherald-ping`, a static sender binary.**
  Depends on AH-224. Separate crate `crates/aherald-ping` in a new workspace (root package keeps
  its paths); std plus `serde_json`; a `[profile.ping]` with `panic = "abort"`. Accepts `--hook`,
  `--agent`, `--resident`, `--transient`, `--endpoint`, `--token-file`, `--self-test`; drops events
  outside the five; removes everything except an allowlist of fields (so `tool_input`,
  `transcript_path` and prompt text never leave the guest); prints the host's reply; exits 0.
  Acceptance: unit tests for the filter and redaction; integration tests against the listener.
  Status (branch `feature/AH-224-remote-listener`): implemented and tested on Linux (gnu). Workspace
  with `[profile.ping]`; the root package stays the default member, so `cargo build --release` and
  the packaging scripts are unchanged (checked: `cargo build --release` builds no `aherald-ping`; the
  scripts' `sed` still reads the root version first; `tests/workspace.rs` keeps the two versions
  equal). CI and release now use `--workspace` for tests, clippy and MSRV check and pick the
  package named `aherald` out of `cargo metadata` (it lists `aherald-ping` first). `--self-test`
  uses a new authenticated `GET/POST /v1/ping` on the listener. 17 unit and integration tests
  in the crate. Redaction keeps only `hook_event_name`, `session_id`, `turn_id`, `agent_id`,
  `notification_type`, the last component of `cwd`, task types and wakeup recurrence (`tool_name` is
  dropped too: the host does not read it). A `ping` profile build is 402 KB dynamic on gnu; the
  `x86_64-unknown-linux-musl` build is 517 KB, static-pie (`file`, `ldd` agree). Checked end to end
  from an `alpine` container on Docker Engine (Linux, bridge `docker0` = 172.17.0.1, listener bound
  there): `--self-test`, a Claude Stop and a Codex Stop through `aherald-ping` and a Stop through
  `aherald-ping.sh` (curl from `apk`) all reached an isolated resident; a wrong token is refused.
  **Not done:** the aarch64 musl target (AH-228); a proxy (Docker
  Sandboxes) is not supported; Windows and macOS builds of the crate were not compiled here.

- [x] **AH-225 — Shell sender for guests.**
  Depends on AH-224. A branch in `plugins/aherald/hooks/aherald-hook.sh` when `AHERALD_ENDPOINT`
  is set, plus a standalone `examples/sandbox/aherald-ping.sh`, and hook snippets for Claude and
  Codex that register only the five events. Always exits 0, bounded to about 1 s.
  Acceptance: script tests like `plugin_hook_script.rs`; behaviour unchanged when the variable is
  unset; refused and blackholed endpoints return quickly.
  Status (branch `feature/AH-224-remote-listener`): implemented. `examples/sandbox/aherald-ping.sh`,
  `claude-settings.json` and `codex-hooks.json` (five events, two for Codex without `StopFailure`
  and `Notification`), and the same branch in `plugins/aherald/hooks/aherald-hook.sh`.
  `tests/sandbox_sender.rs` runs both scripts against the real listener: delivery, Codex `{}`,
  wrong token, missing settings, refused and hung hosts under 3 s, oversize, unknown agent, and
  that an explicit endpoint never runs the local program. Needs `curl` and `head -c` in the guest;
  the token is visible in the guest's process list while curl runs.

- [x] **AH-224 — Host listener for forwarded hook events.**
  Depends on AH-223 (build facts only). `[remote]` config (`enabled = false`, `bind`), a
  generated token in a 0600 file in the data directory, `src/remote.rs` (std only): HTTP/1.1
  `POST /v1/hook?agent=claude|codex`, Bearer token compared in constant time, 64 KiB body limit,
  1 s read deadline, at most 8 connections, one thread per connection, never holding the shared
  lock while reading. Runs only in the resident. Reuses `Agent::parse_hook`, forces `window` and
  `tty` to none, applies the same kind and `hooks_enabled` gates as the local hook (extracted into
  a library function used by both paths), then `resident_request::respond`. Only the five terminal
  and wait events are accepted; other events get a quick 200. Answers with the hook reply.
  A bind failure is logged and retried with backoff. A non-loopback bind logs a warning.
  Acceptance (tests first): wrong, missing and extra tokens (401), oversize (413), malformed (400),
  unknown agent, hung client honours the deadline, connection cap, `window` in the payload is
  ignored, concurrent sessions, resident restart, listener off by default; existing hook tests
  unchanged.
  Status (branch `feature/AH-224-remote-listener`): implemented on Linux and tested. `src/remote.rs`
  (std only), `[remote]` in `config.rs`, `resident_request::remote_deliver`, `aherald --remote-info`,
  `remote-token` beside `config.toml` (0600), log records `remote.listening/bind_failed/rejected`,
  config.example.toml, docs/configuration.md, CHANGELOG. Tests: 16 unit tests (routes, token, size,
  deadline, connection cap, bind retry, activity dropped, busy Stop, window ignored, token file),
  4 integration tests with a real resident (`tests/remote_listener.rs`), config tests; clippy
  clean. The dispatch gate did not need extracting: the listener applies `hooks_enabled` and the
  busy-Stop rule itself and `respond` applies the per-kind switch. **Not verified here:** the
  macOS and Windows resident wiring and the Windows `BCryptGenRandom` token (new
  `Win32_Security_Cryptography` feature) were written but not compiled — the CI legs must build
  them; the real hook path under `xvfb` in CI; a guest on a real bridge (AH-223).

- [x] **AH-223 — Spike: verify the assumptions behind the sandbox sender.**
  Check, and write down in `docs/sandboxes.md` (or the task status): which build layouts the
  packaging scripts accept (`--workspace` use), whether the lib builds for musl, Codex's reply
  contract for an empty hook reply, and per environment (Docker Linux, Podman, Docker Sandboxes,
  Lima, coop, srt, nono) how a guest reaches the host. Unverified items stay marked as such.
  Acceptance: a table of verified, documented and unverified items with the command or URL.
  Status 2 (Linux, one-off): verified Docker Engine (Alpine, bridge 172.17.0.1), bubblewrap (shared
  network works, `--unshare-net` cannot connect), `srt` 0.0.81 (loopback fails; works with the curl
  sender through its proxy for an allowed non-loopback address; `aherald-ping` has no proxy support)
  and nono 0.81 (open network works, `--block-net` blocks; Landlock V1 here, no TCP filtering).
  Kept as ignored tests in `crates/aherald-ping/tests/{container,sandbox_runtimes}.rs`. Found on the
  way: curl rejects `--connect-timeout 0.5` under a comma-decimal locale, so the scripts now use
  whole seconds. Still unverified: Podman, Docker Sandboxes, Lima, coop, Docker Desktop.
  Status (2026-11-05, local part done): verified here — CI and release use `cargo build --locked`,
  `cargo test --locked --all-targets` and `cargo test --locked --doc` with no `--workspace`, so a
  workspace would leave `aherald-ping` untested unless `-p aherald-ping` is added. A scratch
  workspace (root package plus `crates/aherald-ping`) builds only `aherald` by default and keeps
  `target/debug/aherald`. **But** `cargo metadata --no-deps` lists `aherald-ping` first, and
  `ci.yml:100,250` and `release.yml:77,152` read `packages[0]` for the version and
  `rust_version`; those lines must select the package by name (AH-226). Codex: Stop writes `{}`,
  PermissionRequest and Interrupt write nothing (`docs/codex-hooks.md:23`), and the launcher already
  prints nothing when no executable is found, so an empty reply is the existing degraded path.
  Not done: musl build of the lib (target not installed here), per-environment reachability
  (needs the real environments), srt/nono/Codex-sandbox behaviour.

- [x] **AH-204 — Bridge file setup for harnesses (command line).**
  Former AH-205 and AH-208. `aherald --setup --harness opencode|pi|goose` installs, updates and
  removes aHerald-owned bridge files through the setup receipt (exact text recorded; dry run,
  `*.aherald-backup`, rollback; foreign or edited files are never replaced). The bridges are
  embedded in the executable, so nothing is packaged. Crush stays manual (its `crushrc` is the
  user's script). Installer pages (Windows installer, macOS setup window, Linux console setup)
  are not extended: they leave installed bridges alone; see AH-206.
  Acceptance: process-level tests for dry run, install, repeat, foreign file, deselection,
  edited-file removal, rollback and argument validation.

- [x] **AH-203 — OpenCode and Pi bridges, Goose and Crush examples.**
  Depends on AH-201 and AH-202. A minimal TS bridge for each of OpenCode and Pi runs
  inside the harness runtime and calls `aherald --notify` at a fixed path, passing only
  the permitted fields (never prompts, message bodies, tool arguments or credentials).
  Pi uses its final-idle boundary (`agent_settled`, `aborted` -> interrupted); OpenCode
  maps session idle/error and, if the probes confirm it, permission asked/replied.
  Goose and Crush need no code: ship example hook configuration (Crush `TurnEnd`,
  `PermissionRequest`/`PermissionResult`, `StopFailure`, `Interrupt`, `SessionEnd`;
  Goose `Stop`, `UserPromptSubmit`, `SessionEnd`). Bounded delivery; a missing or
  failing aHerald never affects the harness. Manual setup and removal documented in
  `docs/`, with a support matrix.
  Acceptance: the bridges run against controlled fixtures through the real handler and
  IPC path (normalization, ignored events, lifecycle clearing, bounded completion,
  safe failure with no resident or executable); real-version results are recorded
  separately from synthetic ones.
  Result (2026-11-03): bridge contract tests (`tests/bridges`, Node) use synthetic events and a
  recorder in place of aherald; the bridges also ran live on OpenCode 2.2.0 and Pi 1.5.0
  (`BRIDGE=1 sh tests/live/harness_probes/run.sh ...`). Not checked live: the OpenCode TUI
  permission flow, Pi compaction and queued follow-ups, and Crush permission and interrupt hooks.
  Crush 0.99.0 (complete, error) and Goose 1.57.0 (complete) ran live with the shipped examples.
  Calls are serialized, so a short headless OpenCode run can end
  before its last call.

- [x] **AH-202 — Universal external event input and per-harness pets.**
  Depends on AH-201. `--notify <harness> --kind <existing kind> [--activity|--ended]`
  with the session id and `cwd` (card folder) read from bounded stdin; nothing else is
  read or logged. The harness key matches `[a-z0-9_-]{1,32}`; labels come from a short
  table (OpenCode, Pi, Goose, Crush) and default to the key. `Source::External(key)`
  joins the queue identity, so the same session id in two harnesses stays independent;
  waits are per session and `--activity` / `--ended` resolve them as for Claude.
  Replace the IPC source wire format outright (no versioned migration). Replace
  `[notifications.pet]`, `[notifications.pet_animations]` and `[agents.*]` with
  `[pets]` (`default`, and one path per harness key, `demo` for demo cards) and
  `[pet_animations]` (shared) / `[pet_animations.<harness>]`; `default` and `animations`
  are not valid harness keys; update the example configuration and docs. Codex and
  Claude behavior, their hooks and warnings stay as they are.
  Acceptance: tests cover valid/invalid input and size limits, each kind, per-session
  waits, activity and end resolving waits, identical session ids in different
  harnesses, label fallback, the new pet configuration and its rejection of the old
  keys; the existing Codex/Claude suite passes unchanged except for configuration keys.

- [x] **AH-201 — Verify the harness event contracts (OpenCode, Pi, Goose, Crush).**
  Inspect official documentation and the released source/types of the installed
  versions. Record, per harness, how an external command can learn: turn end, error,
  interruption, permission request and its resolution, input wait, session identity,
  main session versus subagents, extension/hook discovery and failure behavior (a
  failing hook must not stop the agent). Distinguish final idle from intermediate
  completion (retries, compaction, queued follow-ups). Target versions: OpenCode 2.2.0
  (Homebrew `opencode`; the npm `opencode-ai` line is 1.19.x), Pi 1.5.0 (needs Node
  >= 22.19; Node 23.7 fails in its bundled undici), Goose and Crush as documented.
  Aider and Zero are recorded as unsupported: Aider's `--notifications-command` gets no
  arguments, stdin or session id and fires on the next prompt of any kind; Zero's hooks
  (`beforeTool`, `afterTool`, `sessionStart`, `sessionEnd`) have no turn-end or
  permission event and its stream-JSON events exist only for headless `zero exec`.
  Acceptance: `docs/harness-compatibility.md` with a versioned matrix of supported
  notifications, unsupported cases, sources and sanitized fixtures; candidate events
  of OpenCode and Pi verified with isolated opt-in probes (documentation alone is not
  live acceptance). No transcript parsing, proxy sessions or account-token access.
  Progress 2026-11-03: Pi and OpenCode turn end, error, interruption, permission (OpenCode)
  and prompts (Pi) are live-verified, and so are sub-agents (OpenCode child sessions carry
  `parentID`; Pi children run as `--mode json` processes, `ctx.hasUI` false). Open: the TUI
  permission flow and live probes for Crush and Goose (documentation only so far).
  Closed 2026-11-03 with those limits recorded in docs/harness-compatibility.md: the
  Crush/Goose rows, the OpenCode TUI permission flow, Pi compaction and queued follow-ups
  are not live-verified and must be re-checked in AH-203 before their mappings are shipped.

- [x] **AH-116 — Profile the macOS cost of `--statusline --then` on real hardware.**
  On a GitHub macOS runner (2026-11-02) `aherald --statusline --then true` took 8.4 and
  9.6 ms more than starting `sh` alone, while the bare wrapper costs about 1.2 ms over
  `aherald --version` (as on Linux) and a test process starting `sh -c true` costs 0.08 ms.
  Spawning a child from the aHerald process seems slower on macOS; the cause is unknown.
  Reproduce on the user's MacBook Pro M1 with `cargo test --test perf_statusline --
  --nocapture`, then profile the spawn (Instruments or `sample`, `dtruss`): suspects are the
  size of the process image, `posix_spawn` versus fork, and the piped stdin/stdout setup in
  `statusline::run_then`. Acceptance: the cause is recorded in
  docs/statusline-performance.md and either fixed, or the 12 ms macOS allowance
  (`THEN_SPAWN_ALLOWANCE_MS` in tests/perf_statusline.rs) is justified by the measurement
  or removed. Low priority: a status line refreshes at most every 300 ms.
  Done 2026-11-03 on the M1: `--then true` costs 3.3-3.5 ms over the bare wrapper, equal to
  starting `sh` (3.28 ms); the spawn adds nothing. The CI figure came from subtracting the
  `--version` median, which clamped the shell's cost to zero. The macOS allowance is removed and
  the check uses the absolute `sh -c true` median (docs/statusline-performance.md).

- [x] **AH-105 — Create an application icon, at least for Windows.** Design and
  add an aHerald application icon, with Windows as the minimum supported platform.
  Acceptance: the built Windows executable displays the icon correctly in Explorer
  at common icon sizes and display scales. Implementation and verification:
  [Windows application icon](docs/app-icon.md). Completed 2026-11-01: embedded
  multi-size resource, native release-EXE loading tests and Explorer screenshots accepted.

- [x] **AH-090 — No silent decisions: a log line for every dropped input, an error card for
  the ones the user can fix.**
  aHerald sometimes drops or degrades what was configured or sent without a trace: a sound
  longer than five seconds, larger than 2 MiB, not 16-bit PCM or missing is ignored
  (`audio::cached` and `Wave::parse` errors vanish), and other `let _ =` or `if let Ok` sites
  swallow a failure. First, audit every such site (sounds, artwork, configuration values that
  fall back to defaults, events refused by validation or the queue, clicks that find no agent,
  usage polls, Keychain and file reads) and list them here. Then, at least: one operational log
  line per case with a fixed code (never a payload, a prompt, a token or a full path), once per
  failing episode rather than per event. Second channel, for failures the user can act on (a
  refused sound or artwork file, an invalid configuration value): the standard notification card
  with the error icon, shown once per episode and never from the hook process, which must stay
  fast and silent. Acceptance: each audited case has a test that the record is written and,
  where it applies, the card is queued; hook behavior and its stdout contract are unchanged.
  Decide when the card is built: whether it is a normal `error` event from aHerald itself and how
  repeats are throttled.
  Status 2026-10-28 (branch `feature/ah-090-no-silent-decisions`): log channel started.
  Done, each with a test that the line appears once per failing file state and never carries a
  path: a sound that cannot be used (`audio.rejected unreadable|too_large|too_long|unsupported|
  malformed`, from the new `audio::SoundError`), a sound that cannot be played (`audio.failed`,
  once per run of failures), a configured custom animation or pet that falls back to the built-in
  picture (`artwork.rejected custom|pet`). Found by the audit and fixed: log timestamps on Unix
  carried a +00:00 offset whatever the zone (`log_local_offset` test).
  Status 2026-10-28 (branch `feature/ah-090-audit-rest`): audit items closed with tests: hook
  input that is unreadable, too large, late or not JSON (`hook.rejected invalid|too_large|timeout|
  unreadable`, Warn, fixed codes, no input; stdout and exit code unchanged); a valid event that
  shows nothing on purpose (`event.dropped disabled|duplicate|obsolete`, Debug, from
  `resident_request::Effects::dropped`); a `monitor` with no such display, now one portable
  `notice::MonitorWatch` used by macOS and by the Windows placement (card and log line once per
  set of connected displays). Decided not to log: a hook event name the agent does not report on
  (normal traffic, e.g. every Claude event configured), and the `kill`/`wait` cleanup `let _ =`
  in `resident_client` (failures there already return an error to the hook's stderr).
  Branch `feature/ah-090-last-silent-sites` (2026-10-29): the last two silent sites now log, with
  tests: `artwork.reduced` (Warn, once per scale) and `hook.delivery_failed` (Warn, hook cannot
  reach, start or queue to the resident; stdout and exit code unchanged). Noticed, not changed:
  with an unusable runtime directory the hook fails before it prints its reply. The Windows look of
  the notice card and the Windows `monitor` notice are not seen on a Windows machine.
  Decided 2026-10-28 with the maintainer: the second channel is an ordinary `error` card that
  belongs to neither agent (`Source::App`, headed "aHerald notice", then only the message in up to
  three lines and no kind row, the built-in
  error picture only, no pet, custom artwork, sound or per-event setting, always shown for 8 s),
  once per file like the log line, and also for configuration problems found at start. Built on
  branch `feature/ah-090-notice-card`: `events::Event::notice`, `notice` (sink installed by a
  resident, so hooks and transient cards stay silent), `resident_request::notice_sink`, a card for
  a refused sound, an unplayable sound, unusable artwork, and (macOS) a `monitor` that is not
  connected (`display.monitor_missing`). Seen live on the M1 as log lines and three queued cards;
  the cards were looked at on the M1 (red error icon, message wrapped in up to three lines). Remaining: the missing
  `monitor` notice on Windows, the Windows look of the notice card, and the rest of the audit above.
  Status 2026-11-01 (branch `feature/ah-090-completion`): audited decisions and executable
  evidence are listed in [the diagnostic audit](docs/notes/diagnostics-audit.md). Added portable
  usage/name-lookup failure episodes; invalid-input, framing and full-queue episodes; bounded
  activation-mailbox diagnostics; per-agent pet, missing built-in and preparation-budget
  diagnostics. Interactive startup configuration errors show a separate eight-second notice
  and retain exit 2; hooks stay prompt and card-free. Hook replies now precede runtime-scope
  creation. The Unix runtime regression requires Unix execution. Windows monitor fallback
  passed its native log/card test. The real resident notice was inspected on Windows build
  22621.4317: built-in red error picture, two-line message, no kind row, 8 s timer.
  Verification completed on Windows: formatting, Clippy and full `cargo test --all-targets`
  passed, including the performance baseline and native asset checks. The asset test now
  separates Windows creation of a fresh copied executable from rendering checks; production
  delivery budgets are unchanged. See [the startup investigation](docs/notes/native-assets-investigation.md)
  and the audit for observed timings and platform limits.

- [x] **AH-240 — Release procedure follow-ups.**
  `RELEASE.md` is rewritten and tracked on `docs/agents-release-privacy` (four platforms, version sync in
  four files, compare links, tap, rollback including the cask). Left: link it from the README Development
  section; mark `docs/notes/release-acceptance.md` as the v0.1.0 record at its top; add the missing compare links for
  past versions at the bottom of `CHANGELOG.md`. The `plugin.json` version is AH-236.
  Acceptance: README links `RELEASE.md`; `CHANGELOG.md` has a compare link for every version heading.
  Closed 2026-11-05 (branch `docs/agents-release-privacy`). README Development links `RELEASE.md`;
  `docs/notes/release-acceptance.md` opens with a note marking it as the v0.1.0 record; `CHANGELOG.md` has
  compare links for `[Unreleased]` and all 13 version headings (v0.1.0 links its tag), and its header
  mentions adding the link and points to `RELEASE.md`. Checks: every `## [X]` heading has a matching
  `[X]:` reference and every tag exists (`git rev-parse vX`); the release-notes `awk` extraction picks
  up the links only for the 0.1.0 section, which is never republished. Docs only; no build run.

- [x] **AH-235 — Remove stale "Windows first / Codex first" text.**
  `Cargo.toml:7` description; `TODO.md:3-5` intro (cross-platform, six harnesses, plus a portable aherald-ping);
  `docs/portable.md:4,146`; `about_card_text()` in `src/lib.rs` ("for coding agents"); the `src/unsupported.rs`
  header (drop "arrives with AH-068"). (`TODO.md:65` is already fixed on `docs/agents-release-privacy`.)
  Acceptance: `grep -rn "Windows first\|show no cards yet\|first supported agent" --exclude-dir=target .`
  matches only DONE.md/CHANGELOG history; the `tray_about`/About tests are updated.
  Closed 2026-11-05 (branch `docs/agents-release-privacy`). Updated `Cargo.toml` description, the
  `TODO.md` intro, `docs/portable.md` (now the Windows portable guide, pointing to the macOS and Linux
  guides), the About card text in `src/lib.rs`, the `src/unsupported.rs` header and the Debian package
  description in `package-deb.sh`. `TODO.md:65` had already been fixed in the same branch. Checks on
  Linux: the acceptance grep matches only the task text and the local audit file;
  `cargo test --locked --lib build_stamp` passed; `cargo fmt --all -- --check` passed. The Windows
  `tray_about` tests do not assert the changed sentence (CI only).

- [x] **AH-095 — Evaluate native layer-shell on KDE/wlroots (split from AH-073).** Closed
  2026-11-04 with the maintainer as won't work. No KDE or wlroots session is available, and the
  evaluation is low priority: these desktops are a small share of Linux users. Ubuntu's default
  GNOME session is unaffected; the current XWayland path stays.

- [x] **AH-216 — Manual system-proxy check for the Claude quota request (split from AH-045).**
  Closed 2026-11-04 with the maintainer as won't work. The check needs a machine behind a manual
  or PAC system proxy, which is not available. AH-045 keeps its automated redirect and proxy tests.

- [x] **AH-217 — Windows multi-monitor and mixed-DPI acceptance (split from AH-020).** Closed
  2026-11-04 with the maintainer as won't work. The check needs real multi-monitor and mixed-DPI
  hardware, which is not available. AH-020 keeps its automated checks.

- [x] **AH-097 — Native setup and Windows installer.** Closed 2026-11-04 with the maintainer,
  without clean-machine acceptance. Implemented: setup backend, Windows installer and CI
  packaging. Setup checks: 26 passed, one optional Git Bash test ignored; isolated installer
  lifecycle and real-Codex launch/response/ancestry/timing probe passed; formatting, Clippy,
  release build and packaging passed. Not verified: installer GUI and launch-after-install
  option, installed-harness hook delivery and dismissal, a clean Windows 10/11 machine. Full
  Cargo tests stop at `native_assets` (notification startup timeout). Remaining: AH-214, AH-215.
  Evidence: docs/notes/ah-097-windows-acceptance.md.

- [x] **AH-045 — Harden the Claude quota HTTPS request.** Closed 2026-11-04 with the maintainer.
  Implemented and tested: redirects refused and 3xx mapped to a sanitized error (loopback test,
  one request); automatic proxy; wiped credential, token and header buffers. Not verified: the
  manual check behind a system proxy (the dev machine has none). That check was closed as won't work: AH-216.

- [x] **AH-020 — Finish placement and display-change handling.** Closed 2026-11-04 with the
  maintainer. Implementation and automated checks committed; a card on a second monitor with a
  different DPI is drawn at that monitor's scale. Not verified: manual multi-monitor and
  mixed-DPI acceptance (no hardware, confirmed 2026-10-25). That check was closed as won't work: AH-217.

- [x] **AH-110 - Apply rstest selectively for named test cases** (2026-11-02).
  Owner approved `rstest` `=0.28.0` (MIT OR Apache-2.0, MSRV 1.85, no default
  features) as a dev-dependency only; `cargo tree -e normal` shows no runtime use.
  Converted 3 matrices (hook silent-success payloads, hook event names, config
  duration pairs) to named cases with all input/expected pairs preserved; a single
  case runs alone (`-- case_4_pre_tool_use`). `cargo test --all-targets`: 705 passed,
  0 failed; fmt, clippy and release build clean. Details: `docs/notes/ah-110-rstest.md`.

- [x] **AH-109 - Evaluate nextest and retain Cargo for mandatory CI** (2026-11-02).
  Compared pinned signed Windows nextest 0.9.150 against Cargo with equal package-cold
  builds and three alternating warm full runs. Matched exact names/ignored flags in
  93 binaries: 693 ordinary tests passed, 0 failed, 23 ignored with both runners in
  every round. Build wall: Cargo 158.448 s, nextest 168.737 s; execution medians:
  Cargo 117.652 s, nextest 196.335 s (66.9% higher). Retained Cargo CI and all existing
  platform jobs; preserved integration-binary isolation and performance budgets.
  Added conservative native serialization, exclusive performance scheduling, bounded
  per-test execution, zero retries and opt-in JUnit reports. Independent runtime
  fault injection reproduced both missing-group and missing-exclusivity collisions.
  Green probes verified scheduling, hung-test classification/reporting, exact owned
  test/resident termination, and released native mutex/pipe/file resources.
  Format, Clippy and Windows release build passed; separate doctest command passed
  (zero current doctests). No new visual, installed-account or Unix results claimed.
  Reproduction, resource inventory, selections and limits: docs/notes/ah-109-nextest.md.

- [x] **AH-107 - Make asynchronous checks and process cleanup deterministic** (2026-11-02).
  Inventoried process, timing, desktop and cleanup boundaries. Bounded hook/setup/UI
  process capture, including inherited output after child exit; retained concurrent
  pipe draining and owned-child timeout cleanup. Native detached guards verify
  parentage, retain exact handles across replacement, request Quit, observe exit and
  apply bounded owned-process fallback before directory removal. IPC stop and name
  worker checks now synchronize on observed completion. Preserved scenario delays,
  cold launcher races, performance limits and logger/TZ process isolation.
  Runtime regression reproduced the inherited-pipe deadline failure. Native tests
  cover large output, unread stdin, hung processes, panic cleanup, independent
  resident survival and released IPC. AH-092 passed another 20 consecutive Windows
  runs with final cleanup; its forced-visible negative case passed in the full run.
  Format, Clippy, native release build and sequential Windows full tests passed:
  689 passed, 0 failed, 17 ignored. No new manual UI or Unix execution is claimed.
  Inventory, evidence and platform/resource limits: docs/notes/ah-107-deterministic-tests.md.

- [x] **AH-092 - Wait for the paused resident window to hide** (2026-11-02).
  Preserved the native visibility assertion and added a bounded two-second wait
  with expected/last visibility diagnostics. Native tests reproduce delayed hiding
  (red before the wait) and reject a window that remains visible. A real paused
  resident forced visible is also rejected after a visible card establishes readiness.
  Both the original resident test and the negative resident test passed 20 consecutive
  Windows runs without retries. Format, Clippy, release build and full sequential
  Windows tests passed: 628 passed, 0 failed, 17 ignored. No production change.

- [x] **AH-108 - Extract small reusable test fixtures and bounded wait helpers** (2026-11-02).
  Added owned temporary directories, unique resident IPC scopes, configuration creation,
  exact child-process guards and bounded state waits. Migrated the three designated
  Windows resident/render/performance suites and six further logging suites; earlier
  Unix migrations remain intact. Lifecycle tests verify independent live residents,
  sibling survival, startup failure, timeout and panic cleanup. A regression reproduced
  deletion of an existing temporary directory; allocation now skips occupied names.
  Preserved separate logger/TZ test processes and tests that exercise launcher races.
  Per the user's requested order, accepted this focused fixture slice before the
  remaining AH-092/AH-107 reliability work. Format, Clippy, release build and full
  sequential Windows tests passed: 615 passed, 0 failed, 17 ignored.
  Evidence and retained specialized fixtures: docs/notes/ah-108-test-fixtures.md.

- [x] **AH-098 — Suppress notifications for the active originating session** (2026-11-02).
  Added independent card and sound suppression, both enabled by default, requiring
  positive originating-window and active-session evidence. Admission freezes the
  decision; hidden entries retain lifecycle/deduplication state, never reappear on
  focus changes, and do not block visible events. Automated tests cover independent
  options, known/unknown focus and session identity, queue isolation and expiration.
  After upgrading the installed resident to this branch's 0.9.0-beta build, the user
  confirmed combined and independent suppression for Claude in a focused classic
  Windows console. Real Claude and Codex completion events from another Windows
  Terminal tab produced cards and sounds without focus theft; routine terminal/Desktop
  completion and permission delivery was also user-confirmed. Positive suppression
  remains limited to Claude in a classic Windows console. Codex, Desktop, tabbed
  terminals, macOS and Linux retain delivery when active-session identity is unknown;
  no macOS/Linux execution is claimed by this acceptance. See docs/active-session.md.

- [x] **AH-113 — Add Apply and Apply + Test to the Linux tray** (2026-11-02).
  Added validated resident replacement and the shared seven-format preview, aligned
  the tray menu with Windows, and sized warning cards for their measured text.
  The isolated Linux integration test covers both actions with valid and invalid
  settings, replacement and reload, navigation, dismissal, event interruption and
  a subsequent ordinary Apply. It passed on XWayland and Xvfb and is enabled in
  Linux CI. The full suite and Clippy passed locally; the user confirmed both tray
  actions and the revised preview in the live GNOME Wayland/XWayland session.

- [x] **AH-112 — Add Linux tray warning switches** (2026-11-02).
  Added independent runtime context and idle switches using shared warning
  controls. Tests cover both agents, pending and queued warnings, pause/resume,
  restart and tray state. The full suite passed sequentially. The GNOME
  StatusNotifierWatcher registered the resident; D-Bus actions and check states
  were verified, and the user confirmed the menu items in the live instance.

- [x] **AH-111 — Verify Linux session warning delivery** (2026-11-02).
  Traced both context adapters and versioned IPC to the Linux resident and its
  shared warning and X11 card path; no Linux-specific integration gap was found.
  Linux resident tests exercise independent thresholds, crossings and rearming,
  empty-queue idle deadlines, pause/resume, full-queue retry and stale cancellation;
  shared tests cover numeric card text and the remaining agent/session cases.
  Unix IPC tests passed in an isolated runtime directory, and the existing X11
  window test passed on GNOME Wayland/XWayland. The latter checks the native window,
  not the appearance of a warning card. See `docs/portable-linux.md` for resident
  and protocol limitations. Real-agent and warning-card desktop acceptance remain
  AH-115.

## Milestone 1 — Functional notification engine

- [x] **AH-001 — Bootstrap the native Rust project.** Dependencies: none.
  Create a testable library and one GUI executable targeting
  `x86_64-pc-windows-msvc`. Use focused modules for CLI/configuration, normalized
  events, process transport, notifications, tray, and Codex adapters as needed;
  avoid empty speculative subsystems. Prefer `windows`, `serde`, `serde_json`,
  `toml`, PNG-only `image`, and `tracing`; use standard-library concurrency and
  commit Cargo.lock. Acceptance: Cargo checks pass and a release executable builds
  without an external GUI framework or runtime dependency.

- [x] **AH-002 — Implement CLI and validated TOML configuration.** Depends on AH-001.
  Support `--hook`, `--resident`, `--transient`, `--event`, `--status`, `--quit`,
  and `--config`; report unimplemented later-milestone operations explicitly.
  Explicit mode arguments override the configured mode; reject conflicting
  explicit operations. Resolve relative assets against the configuration directory.
  Cover event enablement/duration, positioning/scaling/fades, sound volume,
  deduplication, resident options, Codex monitoring, and usage refresh.
  Acceptance: loading, defaults, invalid values, missing/malformed files, path
  resolution, and mode selection are tested; configuration failures cannot crash
  or change permission behavior in hook mode.

- [x] **AH-003 — Define normalized events and bounded hook input.** Depends on AH-002.
  Model `turn_complete`, `permission_required`, `input_required`, `error`, and
  `interrupted`, with priority, source, and optional session/turn/request identity.
  Parse stdin using explicit size/time limits; tolerate unknown JSON fields and
  reject malformed or oversized inputs safely. Verify official hook contracts
  before adding Codex-specific parsing and output. Keep operational diagnostics
  off stdout and never emit approval, denial, continuation, or input rewrites.
  Acceptance: fixtures test normalization, optional identifiers, unknown events,
  malformed input, input bounds, and event-specific non-blocking responses.

- [x] **AH-004 — Implement detached transient delivery.** Depends on AH-003.
  Launch a detached GUI child and hand off only bounded normalized event data
  through a local channel, not sensitive command-line arguments. Bound startup
  and transfer time. Keep `--event` available for demonstrations without Codex.
  Acceptance: an integration test proves the hook returns before an overlay's
  lifetime ends, including `duration_ms = 0`; startup failure exits safely.

- [x] **AH-005 — Render a native layered overlay.** Depends on AH-004.
  Use `WS_POPUP`, `WS_EX_TOPMOST`, `WS_EX_LAYERED`, `WS_EX_TOOLWINDOW`, and
  `WS_EX_NOACTIVATE`, with `UpdateLayeredWindow` and correct alpha handling.
  Implement text fallback, click-to-dismiss, automatic dismissal, smooth fades,
  and message-loop timers. A zero duration persists until dismissal or a verified
  resolution event. Acceptance: no focus theft or Alt-Tab entry, responsive UI,
  transparent background, working timers and dismissal; basic DPI-correct sizing.

- [x] **AH-006 — Load and animate external PNG assets.** Depends on AH-005.
  Support TOML-described sprite sheets in row-major order and deterministic PNG
  frame sequences. Support FPS and optional per-frame duration metadata, looping,
  and nearest-neighbor/bilinear scaling. Bound decoded dimensions, frame counts,
  and memory; handle invalid dimensions, missing files, and corrupt images with
  the text fallback. Acceptance: frame extraction, timing, sequence ordering,
  bounds, alpha conversion, and asset failures have focused tests.

- [x] **AH-007 — Add native asynchronous sound and sample resources.** Depends on AH-006.
  Use a Windows audio API supporting per-playback volume without changing system
  volume. Serialize playback to prevent overlap; honor global/per-event settings
  and repeated-event suppression. Add original sample PNG/WAV resources, metadata,
  and example TOML for the event types. Acceptance: sound does not block the GUI,
  mute/volume work, missing WAV files are harmless, and asset provenance is recorded.

- [x] **AH-008 — Accept the first vertical slice.** Depends on AH-001–AH-007.
  Run the standard Cargo checks and release build. Demonstrate transient visual
  and sound notifications using external resources and text fallback. Record a
  Windows smoke test and hook-return timing under finite and indefinite duration.
  Acceptance: the engine works without Codex installed and all focused tests pass.

## Additional asset support

- [x] **AH-024 — Load local Codex pet packages.** Depends on AH-008.
  Accept `notifications.pet` pointing to local `pet.json`, resolve the sprite path
  inside the package, and decode PNG/WebP standard v1/v2 atlases. Map aHerald events
  to the documented occupied state frames and timing. Keep explicit event animations
  as overrides; resource errors use text fallback. Do not run installer commands.
  Acceptance: focused tests verify paths, frames, alpha, timing, version/size bounds,
  invalid resources and native window recovery; visually demonstrate a real pet.
  User approved extending the existing image decoder with WebP on 2026-10-24.

- [x] **AH-025 — Keep source and signal text beside optional agent artwork.** Depends on AH-024.
  Always render the source and normalized signal in one native card. Add animation
  as an optional decoration, selected by agent with a shared fallback; preserve
  explicit event animation overrides. Invalid artwork keeps the source/text card.
  Acceptance: test source isolation, asset precedence, visible text pixels, alpha
  composition, native card recovery/dismissal; visually inspect the Codex/Becky card.
  This slice does not implement Claude event ingestion, a resident queue, or replies.

## Milestone 2 — Resident architecture

- [x] **AH-009 — Enforce a single resident instance.** Depends on AH-008.
  Scope a named mutex and named pipe to the current Windows user and logon session.
  Apply appropriate security descriptors and reject remote pipe clients. Require
  no elevation. Acceptance: concurrent starts yield one resident; tests exercise
  ownership, access restrictions, and recovery after process termination.

- [x] **AH-010 — Implement bounded, versioned IPC and control operations.** Depends on AH-009.
  Add framing, validated lengths, acknowledgement, connection/read/write deadlines,
  and bounded retries. Forward hook events, start resident when absent, and implement
  `--status`/`--quit`. Acceptance: round-trip, oversized/malformed messages,
  incompatible versions, broken connections, startup races, and graceful lifecycle
  are tested; no TCP listener exists.

- [x] **AH-011 — Queue, deduplicate, and switch notification events.** Depends on AH-010.
  Use a bounded queue with High/Medium/Low priority and FIFO within each priority.
  Show one overlay with count and navigation controls. Persistent waits must not
  prevent presentation of other events; dismissal affects only the selected event.
  Correlate duplicates by available session/turn/request identity and source,
  including cross-source duplicates; avoid merging unrelated sessions.
  Acceptance: simultaneous sessions, persistent events, overflow, repeated delivery,
  switching, priority, and individual dismissal are tested.

- [x] **AH-012 — Add native tray and graceful shutdown.** Depends on AH-011.
  Use `Shell_NotifyIconW`; restore the icon after Explorer restarts. Add usage
  summary/details/refresh, sound and visual toggles, pause, configuration location,
  and exit. Pause suppresses presentation/audio while resolution processing
  continues. Acceptance: menu actions work, Explorer restart recovers, and exit
  stops workers and releases native resources; unavailable usage is explicit.

## Milestone 3 — Verified Codex integration

- [x] **AH-013 — Record version and event-source compatibility.** Depends on AH-008.
  Inspect current official documentation and matching release/protocol schemas.
  Record tested Codex versions, payload fixtures, capabilities, and coverage limits.
  Planning observed `codex-cli 0.164.0-alpha.11.1`; this is not proof of stable
  release support. Acceptance: a technical note distinguishes verified behavior,
  unsupported behavior, and experimental/version-dependent behavior.

- [x] **AH-014 — Deliver lifecycle hook integration and examples.** Depends on AH-010, AH-013.
  Verify `Stop`, `PermissionRequest`, and version-supported `Interrupt`; provide
  Windows hooks.json examples with correct path quoting and response contracts.
  Never decide permissions. Acceptance: mocked and real supported hook invocations
  forward notifications promptly; instructions explain manual installation,
  trust requirements, removal, and preservation of existing hooks.

- [x] **AH-015 — Implement supported input-wait observation.** Depends on AH-013.
  Investigate documented stable sources for actual wait entry and resolution,
  including app-server request/resolution events where legitimately observable.
  Share app-server transport with AH-017 if needed. Do not assume a private server
  observes other CLI sessions, resume their threads, create a proxy client, or
  parse unstable transcripts. Acceptance: supported sources distinguish executing,
  waiting, and resolved states; unobservable sessions report unsupported explicitly.
  Lack of a stable source is documented rather than replaced with heuristic detection.

- [x] **AH-016 — Correlate pending and terminal states.** Depends on AH-011, AH-014, AH-015.
  Resolve pending notifications only on verified matching request/turn lifecycle
  evidence. Deduplicate multiple sources and handle completion/interruption.
  A failed individual tool call is not automatically an agent execution error.
  Acceptance: fixtures cover multiple requests/sessions, late resolution,
  duplicate sources, and unsupported resolution; manual dismissal always works.

## Milestone 4 — Usage and quotas

- [x] **AH-017 — Connect to the official app-server.** Depends on AH-013.
  Manage one long-lived stdio child with handshake, bounded JSON messages,
  request IDs, timeout, reconnect/backoff, and shutdown. Handle Codex absence,
  authentication unavailability, and unsupported methods without blocking UI.
  Acceptance: mock transport tests cover normal responses, timeouts, disconnects,
  malformed messages, and reconnect; credentials are never read or logged directly.
  Codex discovery: prefer codex.exe in absolute PATH directories, then fall back
  to the existing npm installation's native x64 binary; never copy/install Codex
  or execute npm shims. Verify precedence, spaced prefixes, missing/incomplete
  installations, and a live usage read with the ordinary Windows PATH.

- [x] **AH-018 — Normalize available rate-limit data.** Depends on AH-017.
  Implement `account/rateLimits/read` and supported updates, preferring multi-bucket
  data when available. Preserve primary/secondary windows, percentages, Unix reset
  timestamps, additional buckets, and officially exposed credits. Distinguish
  missing data from zero. Do not consume reset credits or add account mutations.
  Acceptance: fixtures cover legacy/multi-bucket, optional fields, unknown fields,
  unavailable values, and local-time reset conversion including DST boundaries.

- [x] **AH-019 — Present usage independently of notifications.** Depends on AH-012, AH-018.
  Show compact tray usage and a native details window for all available buckets.
  Use local reset times, default 300-second refresh, manual refresh, and explicit
  cached values and unavailable states. Acceptance: usage failure leaves notifications working,
  polling is bounded/backed off, and no session data goes to third parties.

## Milestone 5 — Polish and portable release

- [x] **AH-100 — Per-session context-size warning.** Provide independently configurable
  context-size warnings for Claude and Codex, with separate enablement and thresholds.
  Warn when a session's context exceeds its configured threshold; accept both `N`
  tokens and `Nk` kilotokens as threshold notation. Track context and warning state
  by agent and session so one session cannot trigger or suppress another's warning.
  Verify supported context-size sources for each agent before implementation; document
  unavailable coverage and resolve the kilotoken multiplier, defaults and repeat/reset
  policy before coding. Acceptance: tests cover both notations, invalid values, the
  threshold boundary, independent agent settings, session isolation and unavailable
  context data; verify warnings in real supported agent sessions.
  Preliminary research (2026-10-30): Claude statusLine is a viable source, subject
  to an agreed AH-059 privacy allowlist extension; no permitted passive source
  for arbitrary Codex sessions established. See [feasibility report](docs/notes/session-warnings-research.md).
  First slice on `feature/ah-100-101-session-warnings` provided Claude context
  thresholds; Codex context was initially deferred. The
  maintainer authorized the corresponding AH-059 input-context allowlist extension
  on 2026-10-30. Implementation/verification evidence: [session warnings](docs/notes/session-warnings-research.md).
  Completed follow-up 2026-11-01 on `feature/ah-100-context-warning`: independent
  thresholds implemented for both agents. `k = 1000`; omitted means disabled;
  warn once strictly above the threshold, rearm at/below it, and retain state on
  unknown readings. Real-session delivery verified for Claude Code `2.1.311` and
  Codex CLI `0.166.0-alpha.5.1` through a private production-parser/IPC bridge.
  Codex support is limited to that tested version's `PostModelRequest` observer,
  not a generally documented stable hook; desktop/remote/subagent coverage is not
  claimed. Card appearance was not visually verified. Focused tests passed;
  full checks retain `perf_baseline` and `tray_about` failures.
  See [current evidence and limits](docs/notes/ah-100-context-warnings.md).

- [x] **AH-101 — Per-session cache-TTL warning.** Provide independently configurable
  cache-TTL warnings for Claude and Codex, with separate enablement and thresholds
  in minutes. Warn when more than the configured number of minutes has passed since
  the last message in that session. Track the last-message time, timer and warning
  state by agent and session; activity in another session must not reset the timer.
  Verify supported message-activity sources for each agent before implementation;
  agree on which messages reset the timer, defaults, repeat/reset behavior and session
  end/restart handling before coding. Describe this as an elapsed-time warning, without
  claiming that the provider's cache has actually expired. Acceptance: controlled-time
  tests cover the threshold boundary, timer reset, independent agent settings, session
  isolation, session cleanup and unavailable activity data; verify warnings in real
  supported agent sessions.
  Preliminary research (2026-10-30): a per-session elapsed-time reminder is feasible
  through lifecycle hooks, with resident-owned timers. Exact last-message semantics
  and actual cache expiry are separate questions; Claude additionally documents a
  reported cache deadline. See [feasibility report](docs/notes/session-warnings-research.md).
  Agreed 2026-10-30: start the timer at the end of the agent response, not at user
  submission. New user/tool activity cancels it; the next completion rearms it.
  Default idle thresholds: Claude 55 minutes, Codex 25 minutes (agreed 2026-10-30);
  `idle_minutes = 0` disables an agent's reminder. One warning per idle episode,
  resident-only, memory-only bounded state. This slice is an elapsed-time reminder,
  not a provider cache-expiry guarantee. Accepted on Windows 2026-10-31 with real
  Claude and Codex responses and one-minute reminders; the maintainer confirmed
  reminder delivery as sufficient acceptance. See [Windows evidence](docs/notes/ah-101-windows-acceptance.md).
  Agreed 2026-11-01: Windows tray has a separate Context warnings / Idle warnings
  section. Warning text includes measured context in kilotokens and its threshold,
  or elapsed minutes/seconds and the configured idle limit. Keep the harness name
  only in the heading; capture the values when creating the warning.
  The tray warning section is below notification controls. Each runtime switch gates both agents,
  preserves configured thresholds, and resets on restart. Disabling idle drops
  scheduled reminders; a new completion after re-enabling starts a fresh timer.
  Disabling context drops pending delivery but preserves crossing state. Existing
  queued cards and explicit event previews remain available. About is below the
  following separator, immediately above Edit config. Apply + Test and Apply follow
  Edit config. Both validate the saved file before a resident restart; an invalid
  file leaves the running instance active. Restart discards the previous queue.
  Apply + Test opens all seven formats in the replacement resident's existing
  window, marked [Test], including disabled formats. Keep samples separate from
  the working queue. Arrows and body clicks navigate without removal; timeouts
  advance without gaps or fades, with five seconds for persistent samples. Reset
  the timer on manual navigation and play sounds on selection. Dismiss or a real
  notification ends the test; ordinary Apply must not inherit test mode.

- [x] **AH-026 — Preview the current configuration in a looping demo.**
  Support `--demo [SECONDS]`, with a three-second default pause between scenarios.
  Cycle enabled event types and a multi-event navigation scenario through the shared
  native engine; retain configured resources, sounds, position, scale and finite
  durations. Advance persistent waits automatically. Keep demo events isolated from
  the resident and allow tray pause/exit. Acceptance: CLI validation, deterministic
  timing/repetition/disabled-event tests, native lifecycle/isolation tests, visual
  inspection and standard Cargo checks. No configuration is written.
  Verified on Windows 11 build 22621 on 2026-10-25; see `docs/demo.md` for
  commands, manual observations, cold-cache behavior and test limitations.

- [x] **AH-027 — Simplify cards and expose visual event settings.**
  Hide count/navigation for one event; show bottom-right `<`/`>` for multiple.
  Dismiss informational cards by body click; retain an explicit Dismiss button
  only when body dismissal is disabled. Show remaining seconds for finite events.
  Use configurable per-event accent colors and shared/agent pet state overrides
  covering all nine standard tracks. Acceptance: behavioral layout/input/timer,
  configuration and atlas extraction tests, native pixel/lifecycle checks, visual
  inspection and standard Cargo checks. See `docs/card-controls.md`.

- [x] **AH-028 — Make artwork sources explicit and defaults usable.**
  Support global auto/builtin/pet and per-event inherit/builtin/pet/custom.
  Enable full builtin, full pet and both hybrid directions without clearing old
  animation paths. Stage external builtin resources beside Cargo executables.
  Recover failed pet/custom assets to builtin, then text. Acceptance: executable
  source-selection/configuration/fallback tests, native default-resource checks,
  visual inspection and standard Cargo checks. See `docs/visuals.md`.

- [x] **AH-029 — Expire grouped demo cards individually.**
  Give each selected grouped card its own preview timeout; retain the remaining
  cards until they expire or are dismissed. Hide the window only after the last
  card, then begin the configured quiet interval. Use the queued lifetime for
  countdowns and fades, including demo waits. Acceptance: a controlled-time
  three-card regression, repeat/navigation checks, one visual pass and Cargo checks.
  See `docs/demo.md`.

- [x] **AH-030 — Identify the originating Codex session.**
  Show `Codex: <stored thread name>` in resident and transient cards, falling back
  to a short session ID when unavailable. Resolve only supported read-only metadata
  in the background, without delaying hooks or restarting card lifetimes. Bound
  cache/queues and isolate concurrent sessions. Acceptance: protocol, cache and
  native rendering checks, one visual inspection, and standard Cargo verification.
  See `docs/session-source.md`; automatic sidebar captions may have no stored name.

- [x] **AH-021 — Harden resources and recovery.** Depends on AH-016, AH-019, AH-020.
  Add bounded resident asset caching and operational logging with no credentials,
  full conversation content, or sensitive tool arguments. Exercise queue floods,
  duplicate storms, malformed assets, resident crashes, broken IPC, and unavailable
  Codex. Acceptance: limits and recovery behavior are documented and tested;
  repeated notifications do not leak handles, GDI objects, threads, or memory.

- [x] **AH-031 — Simplify the tray menu and compact usage summary.**
  Show `Codex: 5h 25% / 2:35, 7d 10% / 6d 11h`; configure `[usage] display`
  as `remain` (default) or `used`. Use days and whole hours, H:MM below one day, and minutes
  below one hour, with explicit missing values and retained last received values. Remove Refresh usage and
  the separate Usage details action. The summary adds one persistent usage card
  to the shared notification queue beside the tray: plan, both windows, local
  minute-precision reset dates, workspace credits and all reset credits. Queue
  arrows switch cards; mouse wheel pages long details; body click dismisses only
  the usage card. Open configuration through its Windows association, falling
  back to Notepad. Restart gracefully replaces the process and reloads settings.
  Verified with domain/native lifecycle tests, synthetic visual inspection of
  details and queue navigation, and standard Cargo checks. See `docs/tray-usage.md`.

- [x] **AH-032 — Switch to the agent application on notification click.**
  Configure `[notifications] activate_agent_on_click = true` by default.
  Body click opens `codex://threads/<thread-id>` with validated, encoded identity;
  missing identity or shell failure tries an existing Codex window. Dismissal
  remains controlled independently by `click_to_dismiss`. Arrows and Dismiss
  never activate; demo and usage stay isolated. Resident and transient share the
  activation adapter. Failures retain card behavior and record a fixed diagnostic.
  Verified with focused/native tests, standard Cargo checks, and user-confirmed
  transition from a transient test card to the correct chat. See
  `docs/notification-click.md` for versions and manual-check limitations.

- [x] **AH-033 — Default to config.toml beside the executable.**
  Without `--config`, use `config.toml` beside `aherald.exe`, independently of
  the working directory. Create an absent standard file with current defaults;
  preserve existing files. No APPDATA search or automatic migration. Explicit
  missing custom files remain errors. Resolve resources from the selected file.
  Report creation errors with target path and reason. Simplify standard hook
  examples and document manual migration. Verified with process tests under
  Unicode/spaced paths, concurrent creation, preservation and failures, and
  standard Cargo checks. See `docs/configuration.md`.

- [x] **AH-034 — Show usage levels in a dynamic tray icon.**
  Two columns show primary/5h and secondary/week. Fill follows usage.display;
  remaining quota sets green (>60%), yellow (>20%) or red, with unavailable gray.
  Last received values remain current throughout the icon, tooltip, menu and card;
  age and refresh errors remain in details. Native sizes 16/20/24/32 were visually
  checked. Tests verify updates, pixel/handle reuse, TaskbarCreated recovery and
  restoring the current icon after shell deletion. Standard Cargo checks and the
  Windows x64 release build passed. See `docs/tray-usage.md` for evidence and limits.

- [x] **AH-035 — Configure card background colors.**
  Add global `[notifications] background_color` with the existing #1C232D default.
  Optional `[events.NAME] background_color` and `[usage] background_color` override
  it by card type; omitted overrides inherit the global setting. Accept #RRGGBB
  as for accent color, preserving card opacity, rounded transparency and artwork.
  Verified: parsing/inheritance and native rendered pixels pass functional tests;
  inherited event, overridden event and usage backgrounds were visually checked.
  Standard Cargo checks and Windows x64 release build passed. See `docs/card-controls.md`.

- [x] **AH-036 — Claude Code hook notifications.**
  `Agent` enum (`src/agent.rs`), `--hook --agent claude`, `[claude] hooks_enabled`,
  mapping of `Notification`/`Stop`/`StopFailure`/`PermissionRequest` per
  `docs/claude-hooks.md`; the hook never answers Claude Code. Automated tests pass.
  Live checks (Claude Code, 2026-10-25 and 2026-10-26, interactive session with project
  hooks and `--setting-sources project`): `PermissionRequest`, `Notification`
  (`permission_prompt`, `idle_prompt`) and `Stop` payloads matched the documented
  fields; permission required and turn complete cards appeared, the permission card was
  cleared by `Stop`, and the hook exited 0 with empty stdout. `PermissionRequest` and
  `permission_prompt` arrived 6 s apart and showed two cards; fixed so an unresolved
  Claude card absorbs repeats (`claude_notification_arriving_seconds_after_permission_request_joins_the_pending_card`
  in tests/queue.rs, red before the fix). After the fix one card was observed.
  Not observed live: `StopFailure` (follow-up AH-054).

- [x] **AH-039 — Select tray graph agents.**
  Add `[tray] graph = "auto" | "both" | "claude" | "codex"`, default auto.
  Auto shows the agent with the lowest remaining percentage across 5h/week;
  ties prefer Codex, disabled/no-data agents are excluded, missing windows stay gray.
  Both splits each window's bar by width: Claude left, Codex right, independent
  full-height fills and fixed positions. Exclusive modes never fall back.
  Preserve display modes, colors, cached values, DPI sizing and icon reuse/restore.
  Acceptance: config/selection/raster/native tests, four-mode visual preview at
  16/20/24/32 px, isolated Explorer restoration and standard Cargo checks pass.
  Graph selection does not enable polling or change menu/card behavior.
  Verified on Windows 11 build 22621 with synthetic data; see `docs/tray-usage.md`.

- [x] **AH-040 — Resolve stale Claude pending cards.** Depends on AH-036.
  Done: Claude `Stop`/`StopFailure` resolve same-session pending cards; Claude pending
  events dedupe per session; every new event sounds once (agreed with the user).
  The live double-fire of `PermissionRequest` + `Notification` is still unobserved;
  dedup is safe either way. Original brief:
  Claude permission/input cards are persistent and never receive terminal evidence
  (`turn_id` is absent; `is_terminal_evidence` accepts only Codex hooks). They stay
  until dismissed, hold the High-rank selection and silence every later sound; with
  visuals off they cannot be dismissed at all. A Claude `Stop`/`StopFailure`
  resolves pending cards of the same `session_id` only. Deduplicate a
  `PermissionRequest` and `Notification` `permission_prompt` pair for one prompt if
  the AH-036 payload captures show both fire. Decide with the user how queued,
  unselected events announce themselves. Overlaps the terminal-evidence part of
  AH-038. Acceptance: queue tests for same-session clearing, cross-session and
  cross-agent isolation, the duplicate pair, and a later sound after a hidden
  persistent card; standard Cargo checks. Source: `audit-20261026.md` E1.

- [x] **AH-041 — Keep the resident alive on card render failures.** Depends on AH-021.
  Done: shared `Composition::compose` (largest frame, byte budget, text fallback) used by
  both modes; a resident draw failure hides the window, logs `render_failed` once,
  keeps the queue and retries next tick. Placement failure shows no card until placement
  succeeds (no fabricated position). Native fault tests use a debug-build-only
  `aherald.fault` file beside the configuration. Transient already cached countdown
  frames, set the title and scaled mouse input. Original brief:
  Card, artwork, placement and countdown errors propagate through `Ui::draw` and
  stop the resident, while transient mode falls back to a text card. Extract one
  card composition shared by both modes: size by the largest frame, enforce the
  animation byte budget, fall back to text, cache countdown frames, update the
  window title and scale mouse coordinates. Stop the resident only when its window
  or message loop is lost, and record a fixed diagnostic. Acceptance: an injected
  composition or placement failure leaves the resident accepting events and showing
  a text card; native rendering and leak tests and standard Cargo checks pass.
  Source: `audit-20261026.md` E2, A1.

- [x] **AH-042 — Wake the resident only for pending work.** Depends on AH-041.
  Done (part 1, agreed with the user; the rest is AH-051): `serve_until` waits on
  overlapped accept plus a stop event; IPC posts `WAKE` to the window; the 16 ms timer
  runs only while a card is visible or a timed card counts down (a 15 s housekeeping
  timer remains only while usage polling is enabled, so the tray tooltip/icon may lag
  by up to 15 s when idle); `Driver::wait` lets the usage worker sleep to its deadline
  (a live app-server connection is still polled every 500 ms for pushed updates);
  tooltip text is applied only on change. A test counts `WM_TIMER` deliveries on the
  idle GUI thread. Original brief:
  An idle resident wakes about 100 times per second: the 16 ms GUI timer, the 50 ms
  pipe accept poll with a new event per attempt, and the 50 ms usage worker loop per
  enabled agent. Post new IPC events to the window, run the timer only while a card
  is visible or animating, wait on overlapped accept plus a stop event, and sleep
  the usage worker until its next deadline. Present a static card only when frame,
  alpha, countdown second or placement changes; update the tooltip only when its
  text changes; cache usage-card layout and tray placement while the card is open.
  Move "which card, frame, alpha and sounds now" into a pure presenter driven by
  queue state and a controlled clock, leaving `Ui` a thin Win32 adapter, and stop
  the window procedure from borrowing `Ui` while `draw` holds it across reentrant
  `ShowWindow`/`SetWindowTextW` calls. Acceptance: presenter unit tests; a native
  test observes no periodic wakeups while idle and prompt display of a forwarded
  event; lifecycle, tray and leak tests and standard Cargo checks pass.
  Source: `audit-20261026.md` P1, P2, A6, E7.

- [x] **AH-043 — Report configuration errors and guard Restart.**
  Done: `invalid configuration: <table.key> <reason>` messages without values (see
  `docs/configuration.md`); tray Restart validates first and shows the error on the
  details card; fixed log codes for invalid configuration, render fallback/failure,
  resident startup failure and usage failure. Original brief:
  Parsing reports only "invalid configuration fields" and validation one generic
  message; the resident and Restart run without a console, so errors vanish and an
  invalid file makes Restart exit silently. Name the failing field and reason
  without echoing values. Validate the file before the tray Restart replaces the
  process; on failure keep the running resident and show the error on a card. Add
  fixed operational-log codes for invalid configuration, render fallback, resident
  startup failure and usage failure, without payloads. Acceptance: config tests
  assert field-level messages; a process test makes the file invalid, requests
  Restart and observes the original resident still serving; standard Cargo checks.
  Source: `audit-20261026.md` E5, A8.

- [x] **AH-044 — Separate usage sources and slow Claude retries.** Depends on AH-037.
  Done (agreed with the user; the tail is AH-052): `usage_service::UsageSource`
  (`read`, `listen`, `updates`, `is_live`) is implemented by `CodexSource` (the
  unchanged `Connection` and rolling updates) and `claude_usage::ClaudeSource` (one
  typed HTTPS read, no JSON-RPC emulation, no second backoff layer);
  `View` carries an `Agent`, and card text is chosen by it, not by label. HTTP 429 maps
  to `Error::RateLimited`, and the driver then waits at least `refresh_interval_sec`
  (manual refresh included); other failures keep the 10 s..300 s backoff. Fake-clock
  driver tests cover both; Codex tests and texts are unchanged. Original brief:
  `ClaudeTransport` emulates the Codex JSON-RPC app-server and reshapes HTTP data
  into Codex JSON, which adds pre-formatted `Bucket.details`, a second backoff layer
  and Codex-worded errors for Claude. Introduce a `UsageSource` boundary implemented
  by the Codex client and the Claude HTTPS client; select card text by `Agent`, not
  by label; remove the test-only `summary`/`details`/`brief` path and move its
  assertions to the shown card text. After a Claude HTTP 429 the next scheduled
  request waits at least `refresh_interval_sec`; manual refresh stays rate-limited.
  `Retry-After` and the 15-minute cap remain in AH-038. Acceptance: driver tests with
  a fake clock show no early Claude request after 429; Codex usage behavior and
  error text are unchanged; standard Cargo checks. Source: `audit-20261026.md` A3,
  A4, E4.

- [x] **AH-046 — Accept concurrent hook bursts.** Depends on AH-010.
  Done: `sixteen_concurrent_hooks_reach_one_resident_exactly_once` (tests/resident.rs)
  starts 16 `resident::forward` calls together against a cold resident; all succeed
  and one resident queues all 16. No event was lost in 28 runs at 16 hooks or 8 runs at
  48, so the server (single pipe instance, `ensure` before send) is unchanged; the
  test guards it, and `WaitNamedPipeW` or several instances stay the fallback if a
  slower machine loses events. Original brief: The resident pipe has one instance; busy clients retry with 10 ms sleeps, the
  status probe allows 30 ms and the event 150 ms, and each forward makes two round
  trips. Parallel subagent hooks may start extra residents or drop notifications
  while the hook still reports success. First add a test that forwards 16 hook
  events concurrently. If any is lost, serve several pipe instances or wait with
  `WaitNamedPipeW`, and send the event before probing status. Acceptance: one
  resident queues all 16 events exactly once and every hook returns within its
  existing timeout; IPC security and race tests and standard Cargo checks pass.
  Source: `audit-20261026.md` E3.

- [x] **AH-047 — Fix small resident and transient edge cases.**
  Done (decisions agreed with the user): the sound slot loss was already fixed by
  `Queue::take_unannounced(room)` (covered in tests/queue.rs), so no change;
  `[resident] tray_enabled` was removed from the config, the example and INFO.md
  (a file that still sets it is now rejected as an unknown key; no compatibility is
  kept, there are no external users); `overlay::register_class` accepts an existing
  class; simultaneous transient cards stack (`placement::stack`: downward, upward for
  `bottom_right`) by looking at visible `aHerald.Overlay` windows, with placement
  serialized across hook processes by a named mutex held until the card is shown.
  Tests: `stacking::*` in tests/placement.rs, `simultaneous_transient_cards_do_not_overlap`
  in tests/native_overlay.rs (3 real cards), `class_tests` in src/overlay.rs (no red
  phase observed: the helper is new). Not covered: the two-process race at the mutex
  timeout (500 ms). Each transient card still starts its own `codex app-server` for
  the session name. Original brief: The resident marks an event as sounded before checking for a free audio slot, so a
  fifth simultaneous sound is lost. `[resident] tray_enabled` is parsed and
  documented but has no effect; decide with the user whether to implement or
  deprecate it, keeping existing configuration files loadable. Transient window
  registration does not tolerate an existing class, and simultaneous transient cards
  overlap at one position; decide whether they stack. Acceptance: a sound waiting
  for a slot plays once a slot frees; `tray_enabled` behaves as documented; tests
  cover the chosen class and stacking behavior; standard Cargo checks.
  Source: `audit-20261026.md` E8.

- [x] **AH-055 — Claude click-to-activate (split from AH-038).** Depends on AH-036.
  Clicking a Claude card opens its own session. Desktop: the hook `session_id` is matched
  to `cliSessionId` in Claude Desktop's session files (only that field is read; user-approved
  2026-10-26) and `claude://claude.ai/epitaxy/local_<id>` is opened, with the Desktop window
  as fallback. Terminal: the hook records the visible console window of its own or an ancestor
  process (`console_window`), accepted at click time only for `conhost.exe`, `OpenConsole.exe`
  and `WindowsTerminal.exe`. Automated: policy with a fake agent, Desktop lookup in temp dirs,
  ancestor walk, event window field (`tests/activation.rs`, `tests/claude_desktop.rs`,
  `tests/click_target.rs`). Manual 2026-10-26/27 (classic console host, Claude Desktop):
  a card raised its terminal window; two Desktop sessions answered with a delay produced two
  cards, and each click switched to its own session. Not verified: Windows Terminal and two
  terminal windows (tracked in AH-056).

- [x] **AH-048 — Reduce animation memory and sharpen scaled text.** Depends on AH-020, AH-041.
  A card keeps one base surface, one working surface and the shared artwork frames; only
  the changed part is redrawn (frame, countdown second, session name). An 80-frame
  animation on a 530×244 card, which as full-card frames needed ~41 MiB and fell back to
  text, now shows its artwork (`composition_tests`); native-resource cycle test covers
  frame, countdown and heading redraws. Cards are drawn at the target monitor's effective
  DPI (user agreed, option A) and redrawn on DPI, display or monitor change; artwork too
  large for the DPI scale is prepared at the configured scale; nearest-neighbor stays
  only for fitting an oversized card and for pixel art via the interpolation setting
  (`docs/notes/milestone-5.md`). Automated on Windows 11 build 22621: 277 passed, 0 failed,
  14 ignored (three consecutive runs); `fmt`, `clippy -D warnings` and the release
  build clean. `placement_failure_keeps_the_resident…` raced the failing draw once
  composition got fast; it now sends a wake after clearing the fault, as the resident
  retries on the next wake. Visual check at 100/150/200% moved to AH-076.

- [x] **AH-077 — Structured log lines and debug performance metrics.**
  Log lines are `<local ISO-8601 time, ms, UTC offset> <LEVEL> <component.event> [key=number]`
  through one shared logger with an atomic level gate; rotation cap is 512 KiB at `debug`,
  64 KiB otherwise. `perf` spans (IPC handling, process scan, Claude Desktop session
  lookup, artwork preparation, composition build, frame render and present, placement,
  usage reads, WAV load, configuration load) are aggregated per thread and window into
  `perf <span> n= avg= sd= min= p95= max=` lines, with cache/composition counters and
  gauges; a single over-threshold sample is a throttled `WARN slow` line. Tests cover
  format, gating, rotation, the aggregation math with injected time, and a debug resident
  that logs only fixed names and numbers. Not done: a queue-wait span (needs queue
  timestamps; the event-trace option was excluded), so IPC handling is the only
  receive-side span. Adds `Win32_System_ProcessStatus` to the existing `windows` crate.
  Automated on Windows 11 build 22621: 301 passed, 0 failed, 14 ignored (two consecutive
  runs); `fmt`, `clippy -D warnings` and the release build clean. A live-log read and the
  overhead comparison moved to AH-082.

- [x] **AH-082 — Read a real debug log and set a performance baseline (split from AH-077).**
  A debug resident under an isolated scope showed 24 animated cards; the log and
  `--perf-report` are readable and the top spans are real work (table and assessment in
  `docs/notes/milestone-5.md`): a frame costs ~1 ms, nothing is slow, no frame is skipped, the
  artwork cache hits 20 of 24. `tests/perf_baseline.rs` now fails when a hot-path mean
  exceeds about three times the baseline, on cache misses, rebuilds or skipped frames.
  Finding: cards are composed ~3 times each (AH-083). The `warn` versus `debug` overhead
  comparison was not done (AH-084). The red phase of the budget
  assertions was not observed (budgets were set after measuring).

- [x] **AH-058 — Transient mode: avoid a `codex app-server` child per Codex card
  (split from AH-049).** Measured first (release, 8 runs): a transient name lookup costs
  130-150 ms and 20-560 ms to end the child; the resident's shared lookup costs 2-20 ms; Claude
  has no lookup. Decision with the user: no global switch, only transient mode is affected.
  New `[codex] transient_names` (default false): off, a transient card shows
  `Codex: session <id suffix>` and starts no child or worker thread; on, it keeps the old
  lookup. Resident unchanged. Test `tests/transient_names.rs` counts connection factory
  calls with a fake transport (0 when off, 1 when on) and checks the default; the stub
  `codex` on `PATH` process test from the ticket was replaced by this seam because the
  discovery only accepts a native `codex.exe`. Red phase: the test did not compile before
  the change. Automated: 307 passed, 0 failed, 14 ignored; `fmt` and `clippy -D warnings`
  clean; release build not run. Docs: `docs/codex-hooks.md`, `aHerald.example.toml`.

- [x] **AH-084 — Measure the cost of debug logging itself (split from AH-082).**
  Seven alternating runs of the 24-card burst at `warn` and `debug`, comparing the
  resident's CPU time: medians 172 ms (warn) and 141 ms (debug), spread about 100 ms, so
  the cost is below the method's resolution and far under 2% of a 16 ms frame; no code
  change. Figures in `docs/notes/milestone-5.md`. The measuring test was temporary and is not
  kept. Wall time from first event to idle was not used: it is set by the 120 ms event
  spacing and card duration, not by frame cost.

- [x] **AH-074 — Claude "input required" card in an idle terminal session (found in AH-056).**
  Cause: the `idle_prompt` `Notification`, which Claude Code sends about a minute after
  `Stop`, was mapped to `input_required`, so a finished, idle session got a second card.
  `idle_prompt` is now ignored (the `Stop` card already announced the turn); `elicitation_dialog`
  and `agent_needs_input` still map to `input_required`. `docs/claude-hooks.md` updated. Red
  phase observed in `claude_notification_types_map_to_event_kinds`. The payload was not
  re-captured live: the cause rests on the earlier 2026-10-26 capture (`idle_prompt` after
  `Stop`) and the two cards seen on 2026-10-27. Automated: full suite 0 failed; `fmt` and
  `clippy -D warnings` clean.

- [x] **AH-083 — Compose a resident card once instead of about three times (found in AH-082).**
  A queue-count or index change redraws only the heading unless the navigation buttons
  appear or vanish, and a newly selected card waits up to 300 ms for its artwork before
  falling back to the text card (agreed with the user after timing ten installed pets:
  artwork preparation took 14-130 ms in the release build, mean about 60 ms, on a cache
  miss only). Rebuilds for 24 cards fell from 69 to 25; `tests/perf_baseline.rs` limit
  is 30. `tests/resident_render_faults.rs` covers the first card appearing with its
  artwork and a stalled worker (debug-only `slowart` fault) showing the text card after
  the wait limit. The red phase was observed for the rebuild limit and the stalled-worker
  test; the first-card test already passed before the change.

- [x] **AH-078 — GUI-thread stall detector (from AH-077).** GUI messages are timed per
  class (`gui.timer`, `gui.wake`, `gui.input`, `gui.other`); a message over 50 ms is a
  throttled `WARN slow gui.<class>` line. Implemented as per-class spans instead of a
  gap between loop iterations, because an idle `GetMessage` wait is not a stall.

- [x] **AH-079 — Animation frame-budget report (from AH-077).** `FramePacer` counts the
  animation frames a card jumped over because a draw came late (`frame.skipped`) next to
  `frame.presented`; tested with loops, repaints of one frame and new cards. Late-versus-
  target frame rate was not added: frames are presented at their own cadence, so skipped
  frames are the faithful measure.

- [x] **AH-080 — Periodic resource snapshot at debug (from AH-077).** Once a minute the
  resident writes `resources working_set_kb= gdi= user= threads=`; the composition size is
  the `composition.bytes` gauge. The flat GDI count across repeated redraws is covered by
  the existing native-resource cycle test.

- [x] **AH-081 — `aherald --perf-report` (from AH-077).** Ranks spans from
  `aherald.previous.log` and `aherald.log` by total time (samples, total, mean, worst-window
  p95, max, slow count) and sums counters; unparseable lines are skipped. Tested with a
  fixture log and through the binary.

- [x] **AH-056 — Verify Claude click in Windows Terminal (split from AH-055).**
  Depends on AH-055. Manual 2026-10-27 (Windows Terminal 1.24.12741.0, Claude CLI): a card
  raised the Windows Terminal window; with two sessions in two windows, each click raised
  its own window, and both restored from minimized. The first click was 6.7 s late because
  the Desktop session scan (703 files, cold cache) ran before the recorded window; the
  window is now tried first (`tests/activation.rs`), and clicks log
  `debug agent_activated ms=N` (`tests/operational_log.rs`); clicks after the fix took
  1 to 226 ms. Known limit, accepted: tabs are not selected, UI Automation tab selection
  is not built (fragile across Windows Terminal versions). Automated: 270 passed, 0 failed,
  14 ignored; `fmt` and `clippy -D warnings` clean. Follow-ups: AH-074, AH-075.

- [x] **AH-037 — Opt-in Claude 5h/7d quota.** Depends on AH-036.
  `[claude.usage] enabled` (default false, interval 300..86400 s) polls the undocumented
  `GET api.anthropic.com/api/oauth/usage` over WinHTTP with the token Claude Code stores
  in `.credentials.json`. Agreed exception to the token-reading rule (see AGENTS.md):
  token used only for that call, never logged, stored or refreshed. Parsing, credential,
  status and driver behavior are tested with fakes (no network). Live check 2026-10-25
  (signed-in Claude Code, `anthropic-beta: oauth-2025-04-20`): HTTP 200 with
  `five_hour`/`seven_day` `{utilization, resets_at}`; a stale token gave 401. Tested with
  Claude Code 2.1.310. Manual 2026-10-25/27: separate Claude menu line, details card with
  plan, 5h/Week, Extra usage and Spend, forced refresh on opening details, and tray
  tooltip/menu/card with both agents enabled.

- [x] **AH-053 — Test the transient placement mutex timeout race (split from AH-047).**
  Depends on AH-047. `tests/native_overlay.rs` holds the session-wide placement mutex from
  the test process, delivers a transient card and asserts that the card still appears
  (after the 500 ms wait) while the lock is held, then dismisses it and observes a clean
  exit. Checked by mutation: a placement wait that never times out makes the test fail
  with "card missing". Card-showing tests in that file now run one at a time because they
  share that mutex and screen area.

- [x] **AH-052 — Finish the usage-source cleanup (split from AH-044).** Depends on AH-044.
  Removed the test-only `View::summary`/`details`, `Service::summary`/`details` and
  `brief`; their assertions in `tests/usage_service.rs` and `tests/tray_usage.rs` now check
  the shown `card_text`. `Bucket.details` (pre-formatted lines) became typed
  `Bucket.extra_usage`/`spend` (`usage::Allowance`: enabled, used, limit, currency);
  the card renders the same text from them. `claude_usage::parse_response` builds the
  `Snapshot` directly (no Codex-shaped JSON, `snapshot_from_response`/`with_plan` gone).
  Codex card text is unchanged. Note: the dropped `details` view also showed reset-credit
  `granted` times and full-length reset timestamps, which the card never did.

- [x] **AH-051 — Pure card presenter and P2 redraw limits (split from AH-042).** Depends on AH-042.
  Done (part 1, agreed with the user; the reentrancy half is AH-057): `presenter::Presenter`
  turns the selected queue entry, animation clock and fade settings into `Shown`
  (frame, alpha, countdown second) and reports a card only when that differs from what is
  on screen; `Ui` presents and calls `ShowWindow` only then. The usage card measures its
  height and asks Explorer for the tray rectangle once per layout, not every 16 ms, and
  the cached placement is dropped on display, DPI, setting and taskbar changes.
  `tests/presenter.rs` covers once-only static cards, countdown seconds, animation frames,
  fade alpha and invalidation. Not covered by an automated test: the present-call count
  of the live window. Original brief: see TODO.md history; source `audit-20261026.md`
  A6, E7, P2.

- [x] **AH-057 — Reentrancy-safe resident Ui (split from AH-051).** Depends on AH-051.
  The resident window now owns `Ui` behind a `reentry::Gate`: the procedure borrows it
  only when no handler is active, so messages that `ShowWindow`/`SetWindowTextW` send
  during `draw` are kept (identical ones merged, backlog capped at 64) and handled after
  it, and the tray popup menu's nested loop runs with the gate open. The tray
  `TaskbarCreated` id lives outside `Ui`. `presenter::sound_slots`/`MAX_SOUNDS` replace the
  inline 4-sound cap. Tests: `tests/reentry.rs` (gate), `sound_slots_cap_concurrent_playback`,
  and a debug-build native test that sends a message mid-draw (`aherald.fault` = `reenter`).
  Limit: aliasing undefined behavior is not observable at run time, so the native test
  proves the deferral path stays alive, not the absence of aliasing. Source:
  `audit-20261026.md` E7, A6.

- [x] **AH-049 — Reuse app-server clients and decoded sounds.**
  Done (agreed with the user; transient mode is AH-058): with Codex usage polling on, the
  resident reads session names over the usage worker's connection
  (`usage_service::NameLookup`, `Connection::thread_name`), so one `codex app-server`
  child runs instead of two; with polling off the name worker keeps its own child.
  `audio::SoundCache` (8 entries, keyed by path, volume, size and modification time)
  replaces a disk read and parse per event. `Queue::push` returns `queue::Outcome`
  (Queued, Resolved, Duplicate, Obsolete, Full, Invalid) and the IPC listener no longer
  repeats the terminal, duplicate and obsolete checks (a repeated open Claude wait now
  answers Accepted instead of Rejected). Bilinear `Animation::prepare` premultiplies once
  and `activation::find` reuses one path buffer per enumeration. Tests:
  `tests/usage_shared.rs`, `tests/queue_outcome.rs`, `audio::cache` and a bilinear
  `prepare` test. Limit: the child count is asserted through a fake transport's connection
  factory (one factory call is one child), not by counting real processes; the
  allocation reductions have no automated test. Source: `audit-20261026.md` P5, P6.

- [x] **AH-050 — Tighten internal types and remove dead code.**
  Done: `general.mode`, `general.logging`, `notifications.position`, `monitor` and
  `interpolation` are the enums `Mode`, `LogLevel`, `Position`, `Monitor` and
  `Interpolation`; `Config::events` is `config::Events`, indexed by `events::Kind`
  (`Kind::ALL`, `Kind::from_name`; `EVENT_NAMES` is gone), so a missing key cannot panic.
  The TOML format is unchanged and bad values still fail without echoing them. In
  `pipe.rs` a private `Failure` enum replaces matching on `"local pipe deadline
  exceeded"`; public pipe functions still return the same `String` errors.
  `audio_windows` uses `local::Handle`; `overlay.rs` has `Font::segoe`, `draw_text`,
  `restore_alpha` and safe `Surface::pixels`/`pixels_mut`, so pixel loops run outside
  `unsafe`. Removed `Agent::hook_source` and the unused `now` argument of
  `View::status` and `View::compact`. Tests: `enumerated_settings_*` in
  `tests/config.rs`, `a_connected_client_that_stalls_*` in `tests/ipc.rs`; existing
  pixel and config tests guard the refactors. Limits: the `https.rs` handle closes with
  `WinHttpCloseHandle`, so it cannot share `local::Handle`; `[resident] tray_enabled`
  was already gone; test-only helpers (`pet::for_notification`, `Animation::index_at`,
  `AssetCache::retained_bytes`) stay because tests use them. Source:
  `audit-20261026.md` A2, A5, A7.

- [x] **AH-038 — Claude follow-ups.**
  Done (2026-10-27, `feature/AH-038`): Claude quota polling honors `Retry-After` (seconds
  form) capped at 15 minutes and never earlier than `refresh_interval_sec` (driver tests with
  a fake clock, loopback header test); `SessionEnd` clears that session's pending cards
  without a card or sound and reaches only a running resident (`Event.ended`; queue,
  hook-parse and resident IPC tests); `SubagentStop` stays ignored by decision; terminal
  evidence, per-session waits, session names, console window capture and activation
  selection go through `Agent`. Also fixed: an empty header slice reached
  `WinHttpSendRequest` as a dangling pointer. Not done, split to AH-059: Claude session
  names and the statusLine quota fallback. Not verified live: `SessionEnd` and a real 429
  `Retry-After` from Claude Code.

- [x] **AH-075 — Bound the Claude Desktop session scan by recency (found in AH-056).**
  Done (2026-10-27, `feature/AH-075`): on the dev machine (708 session files) a file's mtime
  tracked `lastActivityAt`, so `local_session_id` now skips files not modified in the last
  30 days (`MAX_AGE`); `MAX_FILES` still applies. Test: `tests/claude_desktop.rs`
  `sessions_untouched_for_months_are_not_scanned` (mtime set with `File::set_modified`).

- [x] **AH-022 — Package and document the portable application.**
  Done (2026-10-27, `feature/AH-075` branch): `package-release.ps1`
  builds `aherald-vX.Y.Z-windows-x64.zip` + `.sha256` (exe, example config, `assets/`,
  `examples/`, `docs/`, LICENSE, README, CHANGELOG, `START-HERE.md`, `MANIFEST.txt` with
  per-file SHA-256), then extracts it into a fresh temp folder as a non-administrator and
  runs `aherald.exe --version` (observed: `aHerald 0.1.0`, 1.29 MB zip). `release.yml` calls
  the script. `docs/portable.md` covers modes, CLI, configuration, replacing assets, manual
  hooks and autostart, uninstall, build commands and limits. `check-release.ps1` passes (no
  separate C runtime). Not verified: the workflow itself has not run on GitHub (not pushed);
  Windows 10 extraction belongs to AH-023.

- [x] **AH-023 — Complete release acceptance.**
  Done (2026-10-27): automated checks, hook latency, card/About/DPI visual checks and
  sound on Windows 11 build 22621, and the release workflow on tag `v0.1.0` are recorded
  in `docs/notes/release-acceptance.md`. Long-lived waits are covered by the live resident's
  multi-hour runs, not a timed pending card. Windows 10 is out of scope and labelled
  untested. Not verified and labelled so: a full Explorer process restart and
  multi-monitor/mixed DPI (open as AH-020).

- [x] **AH-060 — Platform seam, non-Windows build and CI.**
  Done (2026-10-27): the library and `main.rs` compile on macOS and Linux behind stub
  platform modules (`src/unsupported.rs`); `src/paths.rs` is the path helper;
  `.github/workflows/ci.yml` runs fmt, clippy and tests on windows, macos and ubuntu
  (green on all three, PR #38); `.gitattributes` added; target statements updated.
  The Windows-only `\\` and `LOCALAPPDATA` literals in `src/activation/native.rs` and
  `src/shell.rs` stay because those files are Windows-only.

- [x] **AH-087 — State "not tested on Windows 10" in the release notes.**
  Done (2026-10-27): the 0.1.0 section of `CHANGELOG.md` says the release was not tested
  on Windows 10 (`README.md` and `docs/portable.md` limits already did). The
  "Windows 10/11" platform wording was left unchanged by decision.

- [x] **AH-085 — Redesign the notification card layout.**
  Layout: wider accent stripe by event kind; artwork and countdown unchanged; rows
  `<harness>, <folder>`, session name (row always reserved), larger event kind; bottom row
  `[Dismiss] <age>` and `[<] n/m [>]` with wrap-around; usage card on the same layout;
  `border_color` (global, per event, usage). Automated tests cover folder extraction, the
  heading model, button geometry, navigation, age text, IPC carrying the folder and equal
  card height. Done 2026-10-27: manual visual check of both agents at 100% and 150%
  observed by the user (see `docs/notes/milestone-5.md`).

- [x] **AH-086 — `--version` and a tray About card.**
  `aherald --version` prints `aHerald <version>`; the tray item "About aHerald" shows the
  version on a usage-style card and a second choice closes it. Automated tests cover the
  CLI, the card text, the menu entry and a native open/close test. Done 2026-10-27: the
  card was observed at 100% and 150%, and redrew correctly when the Windows scale was
  changed while it was open.

- [x] **AH-076 — Visual check of native-DPI cards at 100/150/200% (split from AH-048).**
  Done 2026-10-27 (criterion narrowed with the user): text sharp at 100%, 150% and 200%;
  pixel art crisp and the largest local pet (WALL-E, 2.3 MB spritesheet) shows its artwork
  instead of the text fallback; the card redraws when the Windows scale is changed while
  it is open, on all three scales. The two-monitor mixed-DPI case moved to AH-020 (cards
  are not draggable). Not verified: the frame count of the WALL-E sheet. Results in
  `docs/notes/milestone-5.md`.

## Milestone 5a — macOS and Linux support

- [x] **AH-091 — Hook setup commands and plugins.** Depends on AH-067.
  Agreed 2026-10-28: relax the "never edit hooks" rule to "only on an explicit user command, with a
  dry run by default, a backup and a diff of aHerald's own entries" (AGENTS.md). (1)
  `aherald --install-hooks|--uninstall-hooks claude|codex [--apply] [--hooks-file PATH]` merges only
  aHerald's entries (idempotent, key order and other content kept, symlink and mode kept, `*.aherald-backup`);
  Codex still needs the user to trust the hooks in `/hooks`. Needs serde_json `preserve_order` (approved).
  (2) Plugin `aherald` in a marketplace in this repository (`.claude-plugin/marketplace.json`, also read by
  Codex), hooks in `plugins/aherald/hooks/hooks.json` through a launcher script that finds `aHerald.app`
  and always exits 0. macOS first; Linux launcher discovery and installation were verified separately. Acceptance: on the M1 the plugin installs from the repo with
  `claude plugin marketplace add` and delivers a card; `--install-hooks` round-trips on a real file.
  Accepted 2026-10-29 on the M1 (macOS): `--install-hooks claude` on a copy of the real
  `~/.claude/settings.json` (dry run unchanged; apply adds five entries and a backup; second apply no
  change; uninstall restores the content and key order, only a trailing newline is added); the plugin
  installed from this repository (`claude plugin marketplace add` and `install aherald@proprock`,
  isolated `CLAUDE_CONFIG_DIR`); the launcher script delivered a card through the installed app and
  exits 0 without it. Not run on the M1: Codex (`codex` not installed there).
  Linux, 2026-10-30: Claude Code 2.1.314 installed the marketplace and plugin in an isolated
  configuration; a synthetic event from the installed cache reached an isolated resident.
  Linux Claude agent dispatch was subsequently verified in AH-096; the Windows launcher
  variant remains AH-097.

- [x] **AH-096 — Verify Linux plugin in a real Claude Code session.** Depends on AH-091.
  On 2026-10-30, installed aHerald 0.4.0 and `aherald@proprock` in the user's Ubuntu
  environment. Claude Code 2.1.314 was installed; the user confirmed receiving a
  notification from a real Claude session. The installation remains in use. A synthetic
  event through the installed plugin also reached the resident; the session-specific
  version and event type were not captured.

- [x] **AH-073 — Wayland spike.** Depends on AH-068.
  Done 2026-10-30: [findings](docs/notes/wayland-spike.md) record the GNOME Wayland registry,
  XWayland window and focus checks, upstream compositor support and untested behavior.
  The user agreed to retain XWayland on GNOME and defer native KDE/wlroots work to AH-095.

- [x] **AH-094 — Linux tray usage summaries and dynamic icon.** Depends on AH-069.
  Done 2026-10-30: enabled agents' menu lines and detail cards, paged usage text, and
  quota bars using the shared graph and display settings; polling remains opt-in. Fake
  data tests cover menu, icon updates and watcher restart without external requests.
  On Ubuntu 22.04.5 GNOME/XWayland the user confirmed both Codex and Claude menu lines,
  detail cards and the split icon. Claude was initially unavailable because its token
  had expired; values appeared after the user renewed the Claude Code login.

- [x] **AH-071 — Linux click-to-activate and path discovery.** Depends on AH-068.
  Done 2026-10-30: capture a matching X11 terminal window and request activation on
  a body click; existing native Codex PATH discovery has fake-PATH tests. On Ubuntu
  22.04.5 GNOME/XWayland the user confirmed a synthetic Claude hook returns focus to
  the Warp window. Warp's previously selected tab or pane is not restored. A real
  agent hook was not exercised in this check.

- [x] **AH-072 — Linux packaging, documentation and verification matrix.**
  Depends on AH-068, AH-069 and AH-070. Done 2026-10-30: GNU/glibc x86-64
  `tar.gz`, release workflow, shell hook examples and Linux setup/removal guide. On
  Ubuntu 22.04.5 GNOME/XWayland, a release archive was extracted, its SHA-256 checked,
  and an isolated resident accepted an event and quit. `codex-cli 0.166.0` was present;
  a real Codex hook was not exercised. Xorg and multi-monitor checks remain AH-093;
  native Wayland and unsupported environments are noted in the guide.

- [x] **AH-070 — Linux audio.** Depends on AH-061.
  Done 2026-10-30: approved PulseAudio/PipeWire-pulse backend with the shared Unix
  cross-process serialization and deduplication gate. The serialization test confirms
  playback does not overlap; the user confirmed an audible notification on Ubuntu
  22.04.5. Tray sound and pause switches cancel active playback.

- [x] **AH-069 — Linux tray.** Depends on AH-068.
  Done 2026-10-30: StatusNotifierItem and dbusmenu using approved `zbus`; GNOME
  AppIndicator requirement documented. The user confirmed the icon and menu on Ubuntu
  22.04.5 GNOME/XWayland; a D-Bus menu command changed the resident's pop-up setting.
  A private D-Bus session test verified registration after the watcher restarted.

- [x] **AH-068 — Linux X11 overlay.** Depends on AH-061.
  Done 2026-10-30 in PR #66: `x11rb` ARGB notification windows, RandR monitor placement
  and DPI, non-focusing window hints, resident and transient presentation, and
  Pango/Cairo text rendering. On XWayland, IPC delivery, window properties and unchanged
  active window were checked; the user confirmed the overlay works in the current
  window manager. Formatting, Clippy, tests and build passed; CI passed on Linux,
  macOS and Windows. Xorg and multi-monitor manual acceptance are deferred to AH-093.

- [x] **AH-061 — Unix common layer (IPC, spawn, HTTPS, open).** Depends on AH-060.
  Unix-domain-socket IPC in a per-user directory (mode 0600, peer-credential check,
  no TCP, `flock` single instance); detached child spawn with fd closing; restart;
  `open`/`xdg-open`; an HTTPS client for usage polling (dependency comparison and user
  approval first). Acceptance: IPC framing/lifecycle, single-instance race and
  rejected-peer tests pass on macOS and Ubuntu; hook returns independently of the resident.
  Status 2026-10-27: implemented in `src/unix/` (`local`, `pipe`, `detached`, `restart`,
  `shell`, `https`; `libc` and `ureq` with rustls approved by the user, none of them in
  the Windows build) and `src/resident_client.rs` (`ensure`/`forward`, now shared).
  Verified on Windows (fmt, clippy, tests) and on Debian under WSL (clippy
  `-D warnings`, `cargo test --all-targets`: `tests/unix_ipc.rs`, `tests/unix_hook.rs`,
  `tests/https.rs` and the unit tests in `src/unix/`). Closed 2026-10-27: the same run passed on macOS 26 (aarch64-apple-darwin, M1: fmt,
  clippy `-D warnings`, `cargo test --all-targets`) and CI run 40589344740 on master was
  green on macos-latest, ubuntu-latest and windows-latest. Not covered yet: the
  Unix resident loop itself needs an overlay (AH-062, AH-068); proxy behaviour of
  `ureq` (environment variables only) is unchecked, like the Windows system-proxy item
  in AH-045.

- [x] **AH-063 — macOS tray.** Depends on AH-062.
  Includes the usage card: it is opened and paged from the tray's usage rows and anchored
  under the status item (as on Windows), so it has no trigger before the tray exists.
  `NSStatusItem` and menu parity with Windows (sound, visual, pause, edit configuration,
  restart, exit, usage rows). Acceptance: manual menu check on macOS.
  Status 2026-10-28 (branch `feature/ah-063-macos-tray`): `src/mac/tray.rs` (`NSStatusItem`,
  quota-bar icon, tooltip), `menu.rs` (menu model with the Windows order, separators and check
  marks, unit-tested), the About and Codex usage cards under the item (`geometry::place_below`,
  scroll wheel pages), and the Restart-refused notice. Observed on the M1 with synthetic clicks:
  the menu opens, Pause toggles and shows its check mark, About and the Codex line open their
  card, a click closes it, Exit stops the resident. Usage with live quota data was checked against Codex 0.133 from the
  desktop app (95% / 95%, plus plan, matching the app); the CLI is found in
  `/Applications/Codex.app` or `~/Applications` when it is not on `PATH`, and reset times use the
  local zone. Checked by hand 2026-10-28: Edit configuration opens the file in the default
  editor; Restart replaces the process (the old one exits, a new pid answers `--status`, the
  icon disappears and returns). The menu bar icon was also read in dark and light system
  appearance (the bar keeps the wallpaper tint, the green bars stay readable in both; a light
  wallpaper was not tried). Not verified: the status item on a second display. Claude usage rows wait for AH-066.
  Closed 2026-10-28 with the maintainer: the acceptance (manual menu check) and the extra checks
  above passed. The status item on a second display moved to AH-062 and the light-wallpaper
  icon to AH-089.

- [x] **AH-064 — macOS audio.** Depends on AH-061.
  CoreAudio/AudioQueue playback with the serialized-playback gate. Acceptance: no
  overlapping playback across processes (process test), audible check on macOS.
  Status 2026-10-28 (branch `feature/ah-064-macos-audio`): `src/unix/audio.rs`
  (`audio::serial`: `flock` gates for serial playback and repeat suppression, cancel-aware,
  reusable by Linux) and `src/mac/audio.rs` (AudioQueue through declared AudioToolbox calls, no
  new crate). Automated: gate and dedup unit tests with a fake output, `tests/unix_audio.rs`
  (a sound waits for another process's sound; a killed process frees the gate; the test fails
  when the gate is removed). The resident plays each new event once and mutes on Sound/Pause.
  Audible check: the manual smoke test plays the sample (`cargo test --lib
  mac::audio::tests::the_default_device -- --ignored`); the maintainer heard the three sounds of a resident
  in turn (turn_complete, input_required, error), with the cards, on 2026-10-28. Sounds are off
  unless an event sets `sound = '<wav>'`. Transient mode stays unsupported on macOS.
  Closed 2026-10-28: both acceptance checks passed. Playback while the output device changes
  (for example Bluetooth) moved to AH-089.

- [x] **AH-065 — macOS click-to-activate.** Depends on AH-062.
  Replace the HWND capture with PID/TTY-based terminal identification and
  `NSRunningApplication` activation; Codex and Claude Desktop URL schemes; Claude Desktop
  session lookup under `~/Library/Application Support/Claude` (still `cliSessionId` only).
  Document Accessibility/Automation prompts. Acceptance: manual check with Terminal,
  iTerm2 and Claude Desktop; policy tests stay platform-neutral.
  Acceptance narrowed 2026-10-28 with the maintainer: Warp replaces iTerm2, which is not
  installed on the test machine.
  Status 2026-10-28 (branch `feature/ah-065-macos-activate`): `src/mac/terminal.rs` (parent
  chain from `proc_pidinfo`, falling back to `ps` for root-owned `login`; first ancestor whose
  bundle id is Terminal, iTerm2, Ghostty, Warp, kitty, Alacritty or WezTerm; its pid travels in
  `Event::window`; raised through `open -b`, re-checked at click time) and `src/mac/activation.rs`
  (macOS agents and the worker; the policy is the shared `activation::click`). No Accessibility
  or Automation permission is needed because nothing is scripted; only the application comes
  forward, not a window or tab. Checked by hand on the M1: a hook run from Terminal.app, a click
  from Finder brings Terminal forward; a Claude event with a Desktop session id brings Claude
  Desktop forward (link `claude://claude.ai/epitaxy/local_*`); a Codex event brings Codex.app
  forward. With Desktop showing another session, a click on a card of this session's hook
  event switched it back to this session (title checked on a screenshot). With a second
  Desktop session of the same project, events for each session's id moved Desktop to that
  session and back (titles checked on screenshots).
  Closed 2026-10-28: Terminal.app and Warp (a hook run inside each, click from Finder brings it
  forward), Claude Desktop (two sessions of the project switched by their ids) and Codex.app
  passed. iTerm2, the other listed terminals and tab or window selection are not verified; the
  latter would need Automation.

- [x] **AH-089 — macOS audio device change and light menu bar (split from AH-064, AH-063).**
  Manual checks only: a sound while the output device changes (for example Bluetooth), and the
  status icon on a light wallpaper. Acceptance: observed on the M1; fix what fails.
  Closed 2026-10-28 with the maintainer: on a light wallpaper (set from a generated image and
  restored, settings file hash unchanged) the menu bar turns light and the green bars stay
  readable; four back-to-back sounds (about 20 s through the serial gate) were played while the
  maintainer switched the headphones' output, and everything was audible without overlap. The
  five-second sound limit hid a first attempt with a 20 s file: see AH-090.

- [x] **AH-066 — macOS Claude usage via Keychain.** Depends on AH-061.
  Optional, default off: when `[claude.usage]` is enabled, read the Claude Code OAuth
  item from the Keychain and use it only for the usage endpoint. Needs an explicit
  AGENTS.md exception (extends AH-037) agreed first. Never log, copy or write back.
  Acceptance: parsing tests with fixtures; manual check; token never appears in logs.
  Status 2026-10-28 (branch `feature/ah-066-macos-keychain-usage`): AGENTS.md exception agreed
  (kept in the local `AGENTS.md`, which git ignores). `src/mac/keychain.rs` reads the generic-
  password item `Claude Code-credentials` through the Security framework (`SecItemCopyMatching`)
  into a zeroed buffer; `claude_usage::fetch_with` takes the credentials and the HTTPS call as
  parameters. Tests: status mapping, a missing item, the whole fetch with fakes (token only in the
  `Authorization` header to `api.anthropic.com/api/oauth/usage`, expired or empty items make no
  request, no failure carries the token) and a manual create/read/delete of a test item. Live on
  the M1: the system prompt for the item appeared and was answered by the maintainer, the item
  was read and parsed, and the card showed "Claude token expired (open Claude Code to refresh)".
  Closed 2026-10-28: after the Claude Code CLI was installed (Homebrew cask, 2.1.308) and signed in
  by the maintainer, the Keychain item held a fresh token and the resident showed live quota:
  the tray icon with both bars, the menu line "Claude remain: 5h 73% / 3:54, 7d 47% / 3d 2h" and
  the details card (plan pro, both windows with local reset times, extra usage and spend).
  Restarting Claude Desktop on this Mac did not change that item, but Desktop had last been used
  on another computer, so whether Desktop uses or refreshes it is not established. The token never
  appeared in any log or output. `[claude.usage]` stays off by default.

- [x] **AH-062 — macOS overlay.** Depends on AH-061.
  Non-activating, transparent, floating `NSPanel` via `objc2`, reusing the portable BGRA
  rendering; text through CoreText. Handle multiple displays, Retina scale, Spaces and
  full-screen apps. Acceptance: manual check on the M1 MacBook: no focus theft, correct
  transparency, dismissal, event switching, display change.
  Agreed 2026-10-27: `objc2`, `objc2-app-kit`, `objc2-foundation` only under
  `cfg(target_os = "macos")`; text drawn with AppKit (`NSString`) into the bitmap, so no
  CoreText crate; scope is the overlay plus a resident without a tray (tray is AH-063).
  Step 1 done: `src/card_paint.rs` holds the card's shapes (straight BGRA) and a list of
  text runs; Windows `Surface::card` draws those runs with GDI (unchanged pixels).
  Status 2026-10-27 (branch `feature/ah-062-macos-overlay`): `src/mac/` has the AppKit
  text renderer (`text`), the in-memory `Surface`/`Composition` (`surface`), screen
  placement (`geometry`), the non-activating `NSPanel` (`panel`) and the resident loop
  (`resident`: IPC thread plus AppKit loop, fades, countdown, artwork, queue). Automated:
  unit tests for text, surface and geometry, `tests/mac_resident.rs` (status, queueing, quit,
  single instance). Observed with `--demo`: a 402x150 pt window at level 25, no change of the
  frontmost application. Mouse input (dismiss button, arrows, click-to-dismiss) works: `src/mac/input.rs` tests plus a
  live check with synthetic clicks on `--resident` (arrow 1/3 to 2/3, body click removes one
  card, one click one action). `ureq` is now behind the default `https-client` feature, so `cargo check --no-default-features
  --target aarch64-apple-darwin` needs no C toolchain (checked natively on the M1; not yet from
  Windows).
  Closed 2026-10-28 after manual checks on the M1 with an iPad as a second display (Sidecar,
  1112x834 at 2x): the card on the configured display and near the cursor on either display;
  the status item menu and the About and usage cards on the second display's menu bar; the card
  above a full-screen Terminal in its own Space on the iPad; display change. Disconnecting
  the iPad left the card at its old coordinates, outside the only display (found and fixed with
  `geometry::ScreenWatch`, which places the card again whenever the set of displays changes);
  after the fix the card fell back to the main display, moved to the iPad when it was connected
  again and came back when it was disconnected (observed by the maintainer). Not tested
  separately: a full-screen app on the main display. Code shared with Windows moved to AH-088.

- [x] **AH-067 — macOS packaging and documentation.** Depends on AH-062, AH-063, AH-064.
  `.app` bundle (`LSUIElement`), config and assets location for a bundle, `.sh` hook
  examples, setup and removal steps, release check script. Decide arm64-only versus
  universal. Signing and notarization are deferred and documented as a limitation.
  Acceptance: bundle launches on the M1 MacBook; docs list tested macOS and Codex versions.
  Done 2026-10-28: `package-macos.sh` builds `aHerald.app` (arm64 only, ad-hoc signature, `LSUIElement`)
  and `dist/aherald-vX-macos-arm64.zip`; `check-release-macos.sh` verifies it from a clean
  extraction. Inside a bundle the configuration and log live in
  `~/Library/Application Support/aHerald` and artwork in `Contents/Resources/assets`; a bare
  executable keeps the portable layout. Docs: `docs/portable-macos.md`, `examples/*.macos.json`,
  `examples/*.sh`. Checked on the M1 (macOS 26.6): the extracted bundle starts through `open`,
  creates its configuration, shows the menu-bar icon and a card. Limitations: no Developer ID
  signing or notarization, Intel unsupported. The release workflow's `package_macos` job (added after
  this ticket, unrun until the next tag) builds the archive.

- [x] **AH-088 — Share the code the macOS port duplicated from Windows.** Depends on AH-062,
  AH-065.
  The resident IPC request handler, `event_card_text`, the activation worker and the card
  surface/composition exist twice (`src/resident.rs`, `overlay.rs`, `activation/native.rs` and
  `src/mac/`) because the Windows code could not be compiled while the macOS port was written.
  Extract them into portable modules used by both. Acceptance: no behavior change, Windows CI
  and the native Windows tests green.
  Status 2026-10-28 (branch `refactor/ah-088-share-code`): the IPC request handler is
  `resident_request::respond` (unit-tested; both residents call it), `event_card_text` lives in
  `card_paint`, and `activation::Worker` is portable with a per-platform runner (unit-tested).
  The Windows side was type-checked and linted from the Mac with
  `cargo clippy --target x86_64-pc-windows-msvc --all-targets`; Windows CI is the run check.
  Remaining: the card surface and composition (`overlay.rs` `Surface`/`Composition` against
  `mac/surface.rs`), which needs the GDI surface moved onto a shared pixel buffer.
  Done 2026-10-28 (branch `refactor/ah-088-surface`): the card surface contract and the artwork
  composition are `card_surface::{CardSurface, Composition<S>}`, used by `overlay.rs` (GDI DIB
  `Surface`) and `mac/surface.rs` (memory `Surface`); each keeps only its pixel storage and countdown
  text. The composition tests run on every OS against a plain test surface. macOS: `cargo test
  --all-targets` and clippy pass. Windows: type-checked and linted from the Mac with
  `cargo clippy --target x86_64-pc-windows-msvc --all-targets`; the native Windows tests run only in
  Windows CI, which is the acceptance check for "no behavior change".

- [x] **AH-059 — Claude session names and statusLine quota fallback (split from AH-038).**
  Priority: low. Deferred to the next milestone (decided 2026-10-27); not started.
  Both items need the Claude `statusLine` command, the only place `session_name` and
  `rate_limits` (`five_hour`/`seven_day` `used_percentage`, `resets_at` in Unix seconds)
  are delivered; hooks carry neither, and transcripts are off limits (AGENTS.md). Claude
  Code allows one `statusLine` command, so aHerald must not replace the user's.
  Candidate design, to confirm with the user first: `aherald.exe --statusline
  [--then <command>]` forwards only `session_id`, `session_name` and `rate_limits` to the
  resident, then pipes stdin to the user's previous command and prints its output; manual
  setup only, with an example and removal steps. Alternative to check for Desktop only:
  whether a readable title exists in `local_*.json` (needs a new privacy exception).
  Acceptance: tests for the forwarded fields and the pass-through; the name shown on the
  Claude card; a quota card without reading the OAuth token; docs updated.

## Standard verification

```text
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release --target x86_64-pc-windows-msvc
```

Use mocked Codex streams and app-server responses for deterministic tests. Native
integration tests must clean up their own processes and IPC objects. Manual visual
checks supplement, rather than replace, logic and lifecycle tests.

## Official integration references

Planning references checked on 2026-10-24; verify again against the version being
implemented. Release behavior and installed capabilities take precedence over
unreleased schemas.
  Done 2026-10-29 (branch `feature/ah-059-statusline`), design approved by the owner: `aherald
  --statusline [--then COMMAND]` forwards only `session_id`, `session_name` and `rate_limits` to a running
  resident over IPC (`Request::StatusLine`, never starting one) and runs the user's own command with the same
  input; `--install-statusline` / `--uninstall-statusline` (dry run unless `--apply`, backup, exact restore)
  set it up; the resident shows the name on Claude cards and `[claude.usage] source = auto|token|statusline`
  chooses the quota source (token first, statusLine fallback; `statusline` never reads the login). Measured
  cost of the wrapper: about 10 ms per status line refresh, off the agent's turn. Limits: macOS/Unix shell
  only (Windows reports "not supported yet"); `rate_limits` only for Pro/Max after the first response;
  Claude Desktop does not run `statusLine` (probe 2026-10-29), so names and the statusLine quota
  reach cards only for terminal/IDE sessions.

