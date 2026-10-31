# AH-097 verification record

Checked 2026-10-31 on Windows 11 Pro 10.0.22621 x64. This development machine has
WSL and Bash, so it is not evidence of clean-machine acceptance. No commit or
release was created.

## Automated behavior

- `cargo test --test setup`: 10 passed. Executed native setup/removal, harness
  selection changes, dry run, idempotence, malformed-file preflight, preservation
  of foreign settings/hooks and another installation's bindings, backups,
  rollback when the receipt cannot be replaced, and app-bundle receipt ownership.
  The bundle-path tests execute on Windows; they do not verify macOS support.
- Generated Claude exec-form hook executed the copied native executable with a
  path containing spaces, Unicode, `$HOME`, a backtick and an apostrophe. The
  generated Codex Windows command executed through both Windows PowerShell and
  cmd, returning the expected Stop response. Hook handling was disabled in the
  isolated configuration so these checks did not create a resident or a card.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo test --all-targets`: failed in the previously observed
  `perf_baseline::hot_paths_stay_inside_their_baseline_budgets` check:
  `artwork.prepare` averaged 52055 us against a 30000 us budget. No budget or test
  assertion was changed. Full output: `%TEMP%/aherald-ah097-native-tests.log`.
- `cargo test --all-targets -- --skip hot_paths_stay_inside_their_baseline_budgets`:
  passed. Output: `%TEMP%/aherald-ah097-native-tests-rest.log`.
- `cargo build --release --target x86_64-pc-windows-msvc`: passed.
- `package-release.ps1 -Tag v0.4.0`: passed, including version output after a fresh
  extraction under a non-administrator account.
- Inno Setup 7.1.1: production and separate test-AppId installers compiled.
- `tests/installer_windows.ps1 -Installer dist/aherald-v0.4.0-windows-x64-setup-test.exe`:
  passed. Actual install, reconfiguration/update and uninstall executed against
  isolated configuration directories. Malformed JSON prevented application
  extraction; foreign configuration and backups survived; deselected Codex hooks
  were removed; a malformed receipt blocked uninstall before binary deletion;
  cleanup succeeded after repair and after changing the harness environment.
  Logs retained under `%TEMP%/aherald-installer-test-e923e176d33a4ce9b454948772782a50`.

The release workflow now builds both portable ZIP and setup EXE, and runs the
isolated installer lifecycle check. The changed GitHub workflow itself has not
been run remotely. The installer is unsigned. No new Rust dependency was added.

## Remaining manual acceptance

- The native installer wizard launched, but Computer Use returned `app approval
  timed out` before its window could be inspected. The owned test wizard was
  closed without installing. Checkbox layout, keyboard navigation and visual
  behavior remain unverified.
- Run installation, hook delivery/dismissal, upgrade and uninstall in a clean
  Windows 10/11 x64 VM with supported harnesses and without WSL, Bash or developer
  runtimes. Preserve each harness's normal hook-trust flow.
- Earlier in this session the user observed and dismissed a card during the
  abandoned plugin-launcher acceptance. That confirms the earlier delivery check,
  not acceptance of this installer/native-setup flow. The original installed
  resident was restored then; installer lifecycle checks did not replace it.
- macOS and Linux native installer flows are tracked as AH-102 and AH-103. No
  platform acceptance is claimed by this Windows run.

AH-097 remains unchecked until the remaining acceptance criteria pass.

## Codex separate-args compatibility probe

- `tests/live/codex_hook_args.ps1 -CodexPath <absolute-path-to-codex.exe>`: passed on
  Windows with `codex-cli 0.166.0-alpha.5.1` (2026-10-31).
- This invokes the real Codex CLI and its `SessionStart` hook dispatcher. The
  control handler receives `inline-sentinel` embedded in `command`; the second
  handler executes but receives no arguments from `args: ["separate-sentinel",
  "two words"]`. The separate field is silently ignored in this version.
- The opt-in probe uses a new temporary `CODEX_HOME` and workspace, generated
  recording hooks, disabled plugin synchronization, and a provider pointed only
  at a closed loopback port. No account, model response, or installed aHerald is
  needed. It terminates its owned Codex process tree after both recordings, with
  bounded waits, and retains isolated logs. It bypasses hook trust only for its
  generated test hooks; it does not modify normal user hooks or trust.
- The first exploratory attempt was inconclusive in the sandbox. An initial
  unsandboxed attempt exposed startup plugin synchronization and contention in a
  shared recording file; the final probe disables synchronization and records
  each hook separately. These setup failures were not treated as args evidence.
- This is a version-sensitive external compatibility check, not an ordinary Rust
  unit test or CI requirement. A future version forwarding `args` fails the check
  intentionally, prompting review of the native hook setup architecture.

## GUI installation and launch follow-up (2026-10-31)

The maintainer completed the production 0.5.0 installer interactively with both
Codex and Claude selected. Inspection confirmed the installed version, both
receipt bindings, a generated default configuration and an active resident.
The previous live config and pets were restored with hash verification; the
new default config was archived separately. Delivery and visual acceptance are
still pending. The old installation remains archived for rollback.

Before implementing the requested finish-page launch checkbox, the manual
acceptance is: a successful interactive install offers a checked "Launch aHerald"
checkbox; finishing with it checked starts the resident, unchecking it does not;
silent installation and failed hook setup do not launch a resident. This change
adds no Windows autostart entry. Compilation and isolated silent lifecycle checks
are separate from interactive acceptance.


## Installer usage selection (2026-10-31)

Added independent Codex and Claude usage tasks. On new configurations both are
off; Claude keeps its default auto source (local login, then statusLine). Existing
configuration determines upgrade checkbox values and its source is retained.
The approved direct toml_edit dependency changes only selected enabled flags,
preserving comments and other settings. Native setup preflights configuration
and rolls it back if a later hook/receipt write fails.

The five new setup acceptance tests were first run against the missing behavior:
four failed on unknown options; the invalid-input case initially passed because
options were rejected, and was strengthened to require the configuration error.
After implementation all five passed. All 15 setup tests passed sequentially,
including configuration rollback on receipt-write failure. A concurrent run had
one existing PowerShell launch test exceed its five-second deadline; that deadline
was retained. Clippy and the x64 release build passed.

Both production and test Inno installers compiled. The extended black-box lifecycle
check first failed against the previous installer (usage selection absent), then
passed with the new installer. Evidence directory:
C:/Users/tonym/AppData/Local/Temp/aherald-installer-test-73317a1d88ba450291bf1dcd94a4b5bb.
It verifies selected usage values, comments, other settings and Claude source,
backup, explicit deselection, silent upgrade preservation, and hook cleanup.
Interactive checkbox layout, loaded values and finish-page launch remain manual
acceptance items; the live resident has not been replaced by these isolated tests.


Final usage follow-up checks: cargo fmt --all -- --check, clippy, release build
and PE dependency inspection passed. Full cargo test --all-targets with sequential
tests stopped at the existing perf_baseline failure: artwork.prepare averaged
53,910 us against 30,000 us. The budget was not changed. Repeating the suite with
only hot_paths_stay_inside_their_baseline_budgets skipped passed all remaining
tests. Logs: %TEMP%/aherald-usage-all-tests.log and
%TEMP%/aherald-usage-all-tests-rest.log. This performance failure and interactive
acceptance remain open before commit/release.


## Direct Codex Windows hook launch (2026-10-31)

Tested codex-cli 0.166.0-alpha.5.1 and aHerald 0.5.0. The exploratory probe
confirmed that a double-quoted executable string does not invoke it in the actual
PowerShell executor; the call operator and a single-quoted literal path work.
Spaces, Cyrillic, dollar signs, a backtick, apostrophe, ampersand, parentheses,
exclamation mark and percent-delimited environment-variable text were preserved.
This distinguishes the separate args field limitation from command-line arguments.

Added tests/live/codex_hook_launch.ps1 with a native recorder fixture. It generates
the actual setup binding, reattaches it to SessionStart in isolated hooks, and
substitutes only the target executable. It verifies exact arguments, nonempty
stdin and process ancestry. Against the previous release executable it failed for
the intended added-launcher condition:
aherald.exe -> powershell.exe -> pwsh.exe -> codex.exe.
After removing the inner PowerShell it passed:
aherald.exe -> pwsh.exe -> codex.exe.
The probe leaves ordinary hooks/trust and the installed live app untouched.
The first exploratory attempt found an npm shim in the elevated PATH; subsequent
runs supplied the absolute native codex.exe path. The first regression setup
attempt used a nonexistent generated SessionStart entry; the final probe uses
the generated Stop handler and changes only its probe event registration.

The generated Windows command calls the fixed path using the existing PowerShell
executor and Out-String, without nested powershell.exe. Matching Codex source
selects pwsh then inbox PowerShell by default. Custom non-PowerShell executors are
outside this verified Windows contract; the previous synthetic cmd test was
replaced with the actual default shell contract. Native setup does not change
Codex shell settings. Unix and legacy manual registration commands are unchanged.

The probe restores the real GUI executable with isolated event delivery disabled
and verifies response {} in PowerShell 7 and Windows PowerShell 5.1.
Timing method: outer shell -NoProfile, two warmups, nine measured samples per
variant, alternating old/new order. These are command-process timings on this
machine, not full Codex dispatch timings or universal performance budgets.

| Executor | Old nested PowerShell median | Direct call median |
| --- | ---: | ---: |
| PowerShell 7 | 706.8 ms | 471.0 ms |
| Windows PowerShell 5.1 | 502.4 ms | 283.1 ms |
| No shell, native EXE baseline | n/a | 25.3 ms |

Mean old/new differences: 229.8 ms and 227.8 ms respectively. Removing our launcher
does not remove the shell Codex starts. Green evidence:
C:/Users/tonym/AppData/Local/Temp/aherald-hook-launch-b2579f578c7f461ca120ee001399f846.
Red ancestry evidence:
C:/Users/tonym/AppData/Local/Temp/aherald-hook-launch-a00cc95dddc942b88256692c9556fa05.
All 15 setup tests, fmt, clippy and release build passed after this change.

The complete remaining all-targets suite passed sequentially with only the
previously recorded hot_paths_stay_inside_their_baseline_budgets failure skipped.
Log: %TEMP%/aherald-direct-all-tests.log. PE inspection passed; portable ZIP and
both Inno installers were rebuilt. The new installer lifecycle check passed:
C:/Users/tonym/AppData/Local/Temp/aherald-installer-test-c1f59c898a7c48799d375836631319e5.
Interactive upgrade/launch acceptance remains pending. No live bindings were
changed by these tests, and no commit or release was created.

## Graceful installer shutdown (2026-10-31)

The installer now preflights configuration before requesting any shutdown. It
reads the installed program's status, opens the reported process with query and
synchronize rights, verifies its image path against the selected installation,
requests --quit through that installed program, and waits on the retained process
handle for up to ten seconds. It never requests termination rights. Another
installation's resident is left running. Failure aborts before file replacement.
Uninstall preflights cleanup, uses the same shutdown, then removes saved hooks.

A new native old-application boundary fixture implements status/quit and keeps its
EXE image locked until graceful exit. It does not use real resident IPC, accounts,
notifications or live settings. Against the previous installer, the added running
upgrade test failed: Upgrade did not stop the running installed program.
After implementation, isolated lifecycle checks passed, including:
- Preflight rejection leaves the process running without sending quit.
- Acknowledged quit without process exit times out and leaves the EXE unchanged.
- Successful quit waits for actual process exit before replacing the EXE.
- A PID from another directory is not stopped or sent quit.
- Uninstall with an active fixture and no selected hooks waits before deletion.

Final evidence directory:
C:/Users/tonym/AppData/Local/Temp/aherald-installer-test-672056008cd344638b38b4d339105a70.
Both Inno compilation and portable packaging passed. Application Rust sources
were unchanged in this slice; existing application checks were not repeated.
The actual live aHerald remains untouched. Interactive upgrade of that resident
is the remaining manual acceptance. No commit or release was created.
