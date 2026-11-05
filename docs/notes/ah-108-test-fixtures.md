# AH-108 Windows resident fixtures

## Scope (2026-11-02)

`tests/support/resident.rs` owns one Windows resident per fixture. Each fixture
owns its configuration and temporary directory and derives a private IPC scope
from that directory's unique name. It starts the executable directly with the
existing `--resident-child` entry point, retaining the exact child process handle
instead of adopting a process discovered by PID or through `resident::ensure`.
No production API or dependency changed.

Startup has a deadline and verifies that the responding PID belongs to the child.
On startup error or timeout, the local child guard kills and reaps that child.
Shutdown requests `Quit` only if status still identifies that child, waits for
exit, and kills/reaps the owned child if graceful shutdown fails. The directory
is dropped after process cleanup, including during unwinding.

Migrated `resident_render_faults`, `resident_perf_log`, and `perf_baseline`.
The tests keep their fault recovery, visible-window geometry, log privacy, and
performance contracts. Shared bounded polling replaces local polling loops;
queue-drain and process-exit observations replace logging synchronization sleeps.
The baseline's 120 ms event pacing and render-fault observation periods remain
intentional parts of those scenarios. Performance budgets are unchanged.

## Evidence

Environment: Windows NT 10.0.22621.0, native x64 MSVC target. Tests use private local
IPC and fixture payloads; no installed Codex or live account is required.

- Red: `cargo test --test resident_fixture` failed because `ResidentFixture` did
  not exist. This was an API compilation failure, not an observed runtime defect.
- Green: the same command passed 9 tests, covering unique scopes, two independent
  live residents, sibling survival, invalid configuration, startup timeout, owned
  process exit, panic cleanup, directory ownership, and bounded waits.
- `cargo test --test resident_render_faults --test resident_perf_log --test perf_baseline -- --test-threads=1`:
  20 passed, including shared helper tests compiled into each integration binary.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed after allowing the shared
  fixture re-export to be unused in integration crates that do not use it.
- Initial `cargo test --all-targets` stopped in the unchanged `tests/setup.rs`:
  `generated_codex_command_for_a_special_character_path_returns_the_hook_response`
  exceeded its 5 s PowerShell deadline. This run overlapped the release build;
  no causal diagnosis or baseline-failure claim is made. The migrated suites
  and resident lifecycle fixture tests passed in that run.
- After the release build completed, the isolated setup test passed in 1.32 s.
  `cargo build --release --target x86_64-pc-windows-msvc` passed and produced
  `target/x86_64-pc-windows-msvc/release/aherald.exe`.
- A subsequent `cargo test --all-targets -- --test-threads=1` passed setup but
  stopped in unchanged `tray_about` on `EnumWindows` returning an error with
  HRESULT 0. The isolated `cargo test --test tray_about -- --test-threads=1`
  passed outside the sandbox (0.16 s); desktop context may affect this test,
  but the cause of the first failure has not been established.
- Final full check outside the sandbox:
  `cargo test --all-targets --no-fail-fast -- --test-threads=1` passed across
  91 binaries: 576 passed, 0 failed, 17 ignored. Counts include shared support
  tests repeated in their integration binaries. The ignored interactive/live
  tests were not executed. `git diff --check` also passed.

## Completion

AH-108 is accepted before AH-092/AH-107 in the order explicitly requested by the
user. The final fixture slice additionally migrates `activation_log`, `artwork_log`,
`artwork_reduced_log`, `audio_log`, `session_lookup_log`, and `usage_failure_log`.
These retain separate integration processes for their global logger state.

The directory collision regression failed at runtime against the extracted original
allocator, which removed the existing directory. `TempDir` now uses exclusive
`create_dir` and skips occupied names; it never deletes a directory to claim it.
The focused final selection passed 46 checks (including repeated helper checks).
Final sequential Windows full run: 615 passed, 0 failed, 17 ignored.
Format, Clippy and the native release build passed again.

Selective inventory: launcher race/crash tests retain their actual launcher path;
tray replacement tests need explicit replacement ownership (AH-107); Linux pidfd
and macOS fixture ownership stay platform-specific. Installation/quoted-path tests
retain their scenario-specific layouts. Unix `TZ` tests retain their separate
process and are not given parallel support tests. No blanket fixture registry or
integration-binary consolidation was introduced. These deliberate distinctions do
not block this slice's observable acceptance criteria.

AH-092/AH-107 remain separate work. No additional manual visual acceptance or Unix
execution is claimed for these final Windows changes.
