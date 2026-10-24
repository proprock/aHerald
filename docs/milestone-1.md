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
