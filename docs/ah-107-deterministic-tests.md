# AH-107 asynchronous test reliability

Windows verification date: 2026-11-02, OS build 10.0.22621.0. This slice changes
integration-test infrastructure only; no production behavior or dependencies change.
AH-092 is completed separately in f1312d2 and reused here.

## Inventory and resulting ownership

| Boundary | Treatment |
| --- | --- |
| CLI/config, Codex monitoring, unsupported-platform, Unix hook/IPC | Existing bounded process helpers from the merged Linux work retained. Unsupported-platform status uses a private runtime directory. |
| Codex/Claude hooks, hook logging, portable config, config notice, PowerShell quoted paths | Bounded completion, concurrently drained stdout/stderr and asynchronous stdin writes. Existing observable stdout/exit/privacy assertions remain. |
| Setup and generated wrappers | Bounded capture of already spawned children preserves the specialized Linux ETXTBSY launch retry and existing five-second setup deadline. |
| Native overlay status and macOS resident CLI | Replaced unbounded output collection. macOS changes require platform CI; no macOS execution is claimed. |
| Unix statusline/plugin scripts | Shared bounded input/output capture. Their configuration and payload assertions remain; no Unix execution is claimed in this Windows pass. |
| Windows resident launcher, cold-start races, About and tray restart | Private IPC scopes retained. Native process handles retain parentage evidence; replacements must descend from an owned process. Graceful Quit is followed by observed exit, with bounded termination of exact owned handles. Directory removal follows successful cleanup. Cold-start race tests still exercise the actual launcher. |
| Hook/overlay lifetime | Bounded hook capture; retained hook parentage supports cleanup of detached children even if a later assertion fails. Existing native overlay lifetime assertion remains. |
| Linux tray fixture | Existing pidfd ownership, bounded fallback, panic/startup-failure checks retained from the merged work. D-Bus commands already carry a two-second timeout. X11/D-Bus UI checks remain explicitly ignored. |
| IPC stop transitions and session-name worker | Replaced synchronization sleeps with channel acknowledgement or worker shutdown/join. |

`run_bounded` now applies one deadline to both child exit and output EOF. The old
reader-thread join could block after a child exited when a descendant inherited its
pipes. The executable regression failed against that behavior (success after about
1.72 seconds despite a 300 ms deadline). Capture now reports the still-open output
without printing its contents or killing an unowned pipe holder. Reader threads can
remain blocked until an external holder closes its handle; the helper does not join
them past the deadline. The finite regression holder is explicitly observed exiting.

The regression first included process startup in its short pipe deadline. A later
focused run exposed that preparation race. The final test confirms that the parent
has exited and the pipe holder exists before measuring the unchanged pipe deadline.
The hung-input fixture similarly confirms readiness before exercising capture.

## Deliberate timing and desktop resources

Retained delays are scenario inputs or bounded observation intervals, not a claim
that an asynchronous operation finished: performance event pacing, injected delayed
window hiding/output, idle-listener duration, negative transient-name/render-fault
observations, and the invalid-Apply stability interval. Invalid Apply now continuously
checks that the original resident serves throughout that interval. Short sleeps in
bounded polling loops remain. Ignored Linux visual tests retain their platform-specific
preview observations; this pass does not claim their execution or change their scope.

Native overlay tests already serialize shared placement with their process-local
mutex. Other native UI/tray tests use unique scopes and PID-filtered window lookup.
The full acceptance run is sequential; it does not establish safety for parallel
execution of separate test processes. AH-109 must schedule desktop/placement, tray,
audio and performance resources before adopting a parallel runner. Separate logger
and timezone test binaries remain separate; no support module was added to the
single-test timezone process. Performance budgets and ignored live/manual tests are
unchanged.

## Executed behavior

Windows executable fixtures cover delayed success, output larger than pipe capacity
on both streams, missing effects, inherited pipes after parent exit, unread large
stdin, hung-child timeout, panic cleanup, refusal of unverified parentage, native
process exit and independent-resident survival. Resident fixture tests also observe
released IPC/mutex resources and removed owned directories. Restart tests explicitly
observe final process exit and absent IPC after replacement cleanup.

Final verification results are recorded in DONE.md. No new live Codex/account,
manual visual, Linux or macOS acceptance is claimed. Previously documented Linux
setup baseline instability remains platform evidence rather than a hidden retry.

Final commands (all successful):

```text
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets --no-fail-fast -- --test-threads=1
cargo build --release --target x86_64-pc-windows-msvc
```

Full Windows run: **689 passed, 0 failed, 17 ignored**. The AH-092 main native
test also passed 20 consecutive exact-filtered runs after the final cleanup changes;
the forced-visible negative test passed in the full run. No automatic retries.
