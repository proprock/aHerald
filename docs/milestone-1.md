# Milestone 1 verification

## AH-001

- Toolchain: stable-x86_64-pc-windows-msvc, rustc 1.98.1, Cargo 1.98.1.
- Red: `cargo test --test cli` failed because the executable did not print help.
- Green: process-level help test passes.
- Passed: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all-targets`, `cargo build --release --target x86_64-pc-windows-msvc`.
- Output: `target/x86_64-pc-windows-msvc/release/aherald.exe` (GUI subsystem).
- Crates downloaded outside sandbox after Schannel SEC_E_NO_CREDENTIALS inside it.
- No visual behavior is implemented or claimed at this stage.

## AH-002

- Red: CLI conflict, explicit missing file, and hook diagnostic tests failed before implementation.
- A further regression exposed finite duration on partially configured permission events;
  the fix preserves the event-specific indefinite default.
- Green: six configuration/CLI tests and the process help test pass.
- All four standard Cargo checks pass; release rebuilt.
- Default mode is transient for this slice. Resident/status/quit and requested monitoring/usage
  report their later-milestone status explicitly. Autostart is never installed.
- Configuration input is capped at 64 KiB. Diagnostics omit TOML contents.

## AH-003

- Red: Stop process response and three event normalization/bounds tests failed for
  the missing adapter. Green: all pass after bounded parsing and event normalization.
- Verified official documentation and six schemas at the exact installed Codex commit;
  see `docs/codex-hooks.md`. Sanitized fixture is exercised by the process test.
- The hook exits while stdin remains open, with a 250 ms read deadline; process tests
  also verify empty permission/interruption responses and sanitized diagnostics.
- Passed all four standard Cargo checks (14 tests total); release rebuilt.
- Windows build observed: 10.0.22621. No live Codex hook/trust installation performed.

## AH-004 delivery implementation

- Red: actual detached-process delivery test failed before implementation.
- Green: startup/transfer completes independently of 5000 ms and indefinite child
  lifetimes; invalid identities and missing executables fail safely. Owned test children
  are killed and reaped. All four standard Cargo checks pass (16 tests).
- The private inherited stdin pipe carries at most 2048 bytes. Child stdout is a
  separate one-byte readiness acknowledgement; hook output handles are not inherited.
- Handler startup deadline: 750 ms; child readiness deadline: 500 ms. A timed-out
  launch kills its child when the launch worker resumes. No global IPC endpoint exists.
- AH-004 remains unchecked until AH-005 replaces the lifetime placeholder with the
  actual overlay and repeats the lifecycle acceptance against that window.

## AH-004/AH-005 acceptance

- Red: native enumeration found no window before implementation; controlled-time
  fade tests failed, and a later test exposed an opacity jump on early dismissal.
- Green: 22 tests pass, including an actual layered/no-activate/tool window,
  click dismissal of an indefinite wait, automatic timed exit, and smooth fades.
- Full hook-process capture initially waited for notification expiry: Windows had
  inherited the hook's original output pipe despite redirected child stdio. Clearing
  inheritance of the hook's original standard handles fixes the regression.
- Actual `--hook` timing: 395 ms (30-second duration), 236 ms (indefinite duration).
  The test observes successful hook exit AND output EOF while the child is alive,
  then terminates/reaps only its own child. No arbitrary sleeps or installed Codex.
- Acknowledgement means validated event received; graphics/resources initialize
  afterward. Native window tests independently verify successful presentation.
- Passed fmt check, Clippy with warnings denied, all-target tests, release build.
- Manual Windows 11 build 22621: user reported seeing the demo and clicking it.
  Automated checks verify styles, focus exclusion, click handling, and timer lifetime.
  Full mixed-DPI/multi-monitor and Windows 10 checks remain untested (Milestone 5).
- Placement currently uses the primary monitor (or cursor monitor for near_cursor).
  Explicit numbered-monitor selection and display-change handling remain AH-020.

## AH-006 acceptance

- Red: extraction, timing and alpha tests failed against the initial loader stubs.
- Green: row-major sprite extraction, lexical sequence ordering, variable durations,
  loop/hold timing, premultiplied alpha, both scaling filters, corrupt/missing files,
  invalid frame dimensions/counts/FPS/durations pass focused execution tests.
- Native overlay tests still pass with prepared image surfaces and text fallback.
- PNG decoding and scaling run on a worker; the GUI timer only selects cached frames.
- Passed formatting, Clippy, all-target tests and the Windows MSVC release build.
