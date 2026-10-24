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
