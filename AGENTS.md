# aHerald — instructions for coding agents

These rules apply to every task in this repository. Product decisions and acceptance
criteria live in `TODO.md`; finished work in `DONE.md`; the release procedure in
`RELEASE.md`; what the program reads and sends in `docs/privacy.md`.

## Product contract

- aHerald is a portable, native notification companion for AI coding agents: Codex and
  Claude Code through their hooks, OpenCode, Pi, Goose and Crush through `aherald --notify`
  and bridge files, and agents in containers or VMs through the opt-in `[remote]` listener
  and the `aherald-ping` sender (`crates/aherald-ping`).
- Platforms: Windows 10/11 x64 (Win32), macOS on Apple Silicon (AppKit via `objc2`), Linux
  X11/XWayland (`x11rb`, StatusNotifierItem tray over `zbus`). The behaviour is the same on all
  three unless a documented limitation says otherwise (README "Limits").
- Hooks finish fast and independently of cards, animation, sound, usage polling and
  forwarding. A failure in any of those never fails or delays the agent.
- Everything beyond showing a card is opt-in and off by default.
- Ask about unresolved product decisions instead of silently changing agreed scope. Where
  this file and `TODO.md` "Agreed decisions" differ, the newer agreed decision wins; fix the
  older text in the same change.
- Use English for code, comments, identifiers, documentation and task records.

## Boundaries and principles

- **Native and portable (firm).** The shipped program is native Rust with per-OS bindings:
  no Electron, web view, embedded browser, GUI framework or runtime the user must install.
  One executable per OS plus replaceable assets. Changing this is a product decision for
  the owner, not an implementation choice.
- **Other languages only where they are needed.** TypeScript bridges run inside a
  harness's own runtime; Python, Node, PowerShell and shell are fine for tests, packaging
  and installers. Do not move product logic into them.
- **Dependencies.** Prefer a maintained, suitable crate over hand-written infrastructure
  when it reduces code or correctness risk. Justify each new direct dependency in the pull
  request (problem, alternatives, license, MSRV, binary size). Ask the owner first only for
  heavy ones: an async runtime, a GUI or web engine, native C libraries, an unusual license,
  or a noticeable growth of the binary. Enable only the features you need; commit
  `Cargo.lock`.
- **Network.** By default aHerald listens on nothing and sends nothing. Any feature that
  listens or sends must be opt-in, off by default, authenticated where it listens, bounded
  in size and time, and documented in `docs/` and `docs/privacy.md`.
- **User configuration.** Never edit agent hooks, global settings or autostart entries
  unless the user runs an explicit command (`--setup`/`--unsetup` with `--apply`, or an
  installer option they selected). Such edits go through the setup backend (dry run,
  backups, receipt, rollback) and never touch entries aHerald does not own.

## Privacy

- Read, keep, log and send the minimum. Hook and IPC payloads are untrusted input. Never log
  full payloads, prompts, assistant messages, tool arguments, paths, credentials or tokens.
- Any new access to credentials, to another application's files, to window or session data,
  or any new transmission to a third party must be opt-in and off by default, must be agreed
  with the owner before implementation, and must be recorded in `docs/privacy.md` (what, when,
  where it goes, how to turn it off) in the same change.
- Never answer an agent's permission request, alter prompts or tool arguments, run commands
  taken from incoming JSON, or use hook responses to steer agent work. Dismissing a card
  never answers the agent.
- Secrets come from environment variables or OS stores, are held in zeroing buffers, go only
  to their intended endpoint, and never appear in an error, log or card.

## Architecture

- Keep domain decisions (queue, presenter, warnings, usage parsing, configuration) free of OS
  and I/O so they are testable on any platform. OS code renders, plays sound, runs the event
  loop and talks to the system.
- Introduce traits, layers and workers when a real boundary, test seam or measured need
  justifies them; no speculative plugin systems or empty future modules.
- Keep blocking I/O, decoding, audio and network work off the UI thread; use bounded queues
  and deadlines. An idle resident does not wake up periodically.
- Wrap unsafe OS calls in safe interfaces with RAII ownership (handles, GDI objects, CF/ObjC
  objects, X resources, child processes) and a `// SAFETY:` comment for each `unsafe` block.
- Avoid production `unwrap`/`expect` on external input or fallible OS calls; keep useful
  error context without sensitive data.

## Parity across the three platforms

- A change to resident, tray, card, sound or usage behaviour lands on Windows, macOS and Linux
  in the same pull request, or the pull request opens a `TODO.md` ticket for each platform it
  leaves out.
- Logic the residents share goes into a platform-independent module with pure tests first;
  do not copy it between `src/resident_overlay.rs`, `src/mac/` and `src/linux/`. When you touch
  one resident, check the same path in the other two for drift.

## Reuse before writing

- Before writing a helper (shell or PowerShell quoting, atomic file writes, backups, dates
  and times, templates and placeholders, URL parsing, HTTP wrappers), search for an existing
  one. A second copy means extracting a shared module instead.

## Harness integrations

Applies to `src/agent.rs`, `src/events.rs`, `src/hook_setup.rs`, `src/setup*.rs`,
`src/bridge_setup.rs`, `src/statusline*.rs`, `src/codex_*.rs`, `src/app_server.rs`,
`src/remote.rs`, `examples/`, `plugins/` and `crates/aherald-ping`.

- Before integrating with a harness, read its current official documentation and the
  released schema or source of the target version. Record versions, sources, sanitized
  fixtures and limitations (`docs/harness-compatibility.md`). Never invent event names,
  methods or fields.
- Observe only through documented, stable interfaces. Do not parse transcripts, resume or
  proxy unrelated sessions. Unsupported coverage is reported, not worked around.
- Hook responses follow the verified event-specific contract and degrade safely: an error
  never becomes a permission decision.
- Keep the three guest senders (`aherald-ping`, `examples/sandbox/aherald-ping.sh`, the remote
  block of `plugins/aherald/hooks/aherald-hook.sh`) consistent with the listener.

## Test-driven development

- New behaviour and bug fixes go in small Red → Green → Refactor cycles: state an
  observable criterion, write the smallest test, see it fail for the intended reason,
  implement just enough, refactor with tests green, then run the full checks.
- A bug fix starts with a test that reproduces it. Never weaken an assertion, delete
  coverage or change an expected result just to pass; change a contract only when the task
  authorizes it, and explain the test changes.
- Documentation-only and mechanical changes need no new tests. For behaviour that cannot be
  automated, define a concrete manual check before implementing and report its evidence
  separately. Never claim a red phase or a manual result you did not observe.

## Tests

- Execute the code and assert outputs, state changes, errors or visible effects; tests must
  survive renames and internal refactors. Do not grep source files or `include_str!`
  implementation files to prove behaviour.
- Use the lowest level that proves the contract: unit tests for pure logic, integration tests
  for module and OS boundaries, a few end-to-end checks for critical flows. Cover invalid
  input, limits, failure and recovery, and cross-session isolation.
- Fake external boundaries (harness transports, clocks, audio, HTTP); do not mock away the
  behaviour under test.
- Keep tests deterministic and isolated: controlled time, unique temp paths and IPC names,
  bounded waits, cleanup on failure. No live accounts, network or installed harnesses in the
  ordinary suite; live checks stay opt-in under `tests/live/`.
- A test that skips when a tool is missing must fail instead when CI requires the tool; a
  silent skip is not a pass.

## Verification and reporting

Run after code changes (all platforms):

```text
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
cargo test --locked --workspace --doc
```

- Linux extras: `xvfb-run -a cargo test --locked --test remote_listener` and
  `xvfb-run -a dbus-run-session -- env AH113_ISOLATED=1 cargo test --locked --test linux_apply -- --ignored --test-threads=1`.
- Windows extras: `cargo build --release --target x86_64-pc-windows-msvc` and `./check-release.ps1`.
- Report the status per platform with exactly one of: **ran locally**, **compiled/tested in
  CI only**, **checked by hand on hardware**, **not checked**. `src/mac/` builds only on
  macOS and the Windows modules only on Windows; never describe code you could only compile
  in CI as verified.
- Visual and tray behaviour (topmost without focus, transparency, DPI, multiple monitors,
  tray menus) needs a person at the screen; record it as manual evidence with the OS version.
- Documentation-only changes: check completeness and links; do not run unrelated builds.

## Platform notes

- **Windows** (`src/` root modules gated `cfg(windows)`, `*.ps1`, `installer/`): MSVC with a
  static CRT; IPC is a named pipe limited to the current user and logon session, remote
  clients rejected; the installer delegates all JSON editing to `aherald.exe --setup`;
  `aherald-cli.exe` is the console companion, hooks call `aherald.exe`.
- **macOS** (`src/mac/`, `package-macos.sh`, `package-dmg.sh`, `check-release-*.sh`): AppKit on
  the main thread only; the bundle is ad-hoc signed, not notarized; writable data lives
  outside the bundle; Keychain access only as recorded in `docs/privacy.md`.
- **Linux** (`src/linux/`, `src/unix/`, `package-deb.sh`, `package-linux.sh`): X11/XWayland
  only; the tray needs a StatusNotifierItem host (AppIndicator on GNOME); IPC is a unix
  socket in a private per-user directory; the `.deb` targets glibc 2.35 (Ubuntu 22.04).
- `src/unix/` and `src/mac/input.rs` compile on both macOS and Linux.

## CI and release

- Actions are pinned by commit SHA with a version comment; workflows must pass actionlint
  and zizmor; jobs keep least-privilege `permissions`.
- Adding, moving or re-gating (`#![cfg]`) a platform-specific file updates
  `.github/platform-flags.sh` and `tests/ci_platform_flags.sh` in the same change.
- Versions change only through `RELEASE.md`, in every place at once: `Cargo.toml`,
  `crates/aherald-ping/Cargo.toml`, `Cargo.lock`, `plugins/aherald/.claude-plugin/plugin.json`
  and `CHANGELOG.md`.
- Never tag, push a tag, publish a release or merge the Homebrew cask without the owner's
  explicit go-ahead.

## Docs and task records

- `docs/` holds user guides. Investigations, spikes and per-ticket acceptance records go to
  `docs/notes/`. A user-visible change updates `README.md` or the relevant guide and
  `CHANGELOG.md` (`[Unreleased]`) in the same pull request.
- `TODO.md`: each task lists dependencies and acceptance criteria; check it only after the
  criteria pass. When a task closes, move it to `DONE.md` in the same pull request with the
  commands, results and versions; leftover manual checks become new IDs. No `[x]` items stay
  in `TODO.md`, and every ID in `CHANGELOG.md` exists in `DONE.md`.

## Commits and branches

- Feature work happens on a task-scoped branch (`feature/AH-NNN-…`, `fix/…`, `docs/…`,
  `chore/…`), never directly on `master`. Preserve unrelated local work.
- Conventional Commits: `type(scope): imperative summary (AH-NNN)`; `!` and a
  `BREAKING CHANGE:` footer for breaking changes. One coherent change per commit with its
  tests and docs; inspect the staged diff. Commit only when asked or when the agreed workflow
  includes it.
