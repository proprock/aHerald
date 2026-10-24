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
