# AH-090 diagnostic decisions

Audited on 2026-11-01. Scope: notification inputs and configured notification resources,
activation, usage and their file/credential boundaries. The review included production
`let _ =`, `if let Ok`, `.ok()` and `unwrap_or_default` sites under `src`; test setup,
assertions and teardown were classified separately. Source inspection identifies candidates;
the tests below execute behavior rather than inspect implementation text.

## Failures and deliberate drops

| Boundary / decision | Diagnostic and recovery | Executable evidence |
| --- | --- | --- |
| Sound read, size, duration, encoding or malformed data | `audio.rejected` with a fixed reason; one log and notice per failing file state | `audio_log` |
| Audio device/playback failure | `audio.failed`; one log and notice until successful playback | `audio_log` |
| Custom animation or shared/agent pet cannot load | `artwork.rejected custom\|pet`; one log and notice per failing file state; built-in replacement | `artwork_log` |
| Built-in artwork missing/corrupt | `artwork.rejected builtin`; text replacement, one notice per file state; an App notice never generates another notice | `artwork_log` |
| Artwork exceeds monitor-scale memory budget | `artwork.reduced` once per scale if the configured scale still works | `artwork_reduced_log` |
| Artwork exceeds the budget even at the configured scale | `artwork.rejected custom\|pet\|builtin`; notice and built-in/text replacement | `artwork_reduced_log` |
| Invalid configuration at interactive Windows startup | `config.invalid`; a separate eight-second notice, then the existing error exit (2); no resident starts on default settings | `config_notice` |
| Invalid configuration in a hook | `config.invalid`; no card, successful hook exit and the event's existing stdout reply | `config_notice` |
| Malformed, oversized, late or unreadable hook input | `hook.rejected invalid\|too_large\|timeout\|unreadable`; no card or payload in the log | `hook_log`, event input tests |
| Hook cannot deliver/start/queue | `hook.delivery_failed`; hook response precedes runtime-scope/configuration/delivery work | `hook_log`, `config_notice`; unusable Unix runtime case is Unix-only |
| Invalid IPC event, status-line identity or context report | `ipc.input_rejected`; once until an input is accepted; status polling does not reset it | `ipc_diagnostics`, `resident_request` tests |
| Queue refuses an otherwise valid input, including an App notice | `ipc.queue_rejected`; once until an input is accepted; no recursive failure card | `ipc_diagnostics`, `resident_request` tests |
| Broken framing, disconnect, connected-client timeout | `ipc.failure`; once until a successful exchange; idle accept is not a failure | `ipc_diagnostics`, `ipc` |
| Disabled, duplicate or obsolete event | Debug `event.dropped disabled\|duplicate\|obsolete`; deliberate per-input trace, no warning card | `resident_request` tests |
| Monitor index no longer exists | `display.monitor_missing`; one log and notice per configuration/display-count episode, primary-display fallback | `monitor_notice`, `native_notice` |
| Click cannot find/open/focus its agent | `agent.activation_failed`; once until successful activation; error strings/session IDs are not logged | `activation_log`, activation routing tests |
| Activation worker mailbox full/disconnected | `agent.activation_dropped`; once until a click is enqueued | `activation_log` |
| Usage connection/poll/parse/credential/Keychain failure | `usage.failed` in the portable driver; once until a successful read/update; notifications continue | `usage_failure_log`, `usage_service`, Claude source/credential tests |
| Optional session-name connection/initialization/read failure | `session.lookup_failed`; once until successful lookup; harness heading stays usable; unnamed thread is normal success | `session_lookup_log`, `session_source`, `usage_service` |
| Native rendering failure / render fallback | Existing `render.failed` / `render.fallback`; text remains available where possible | native overlay and card-surface tests |

All operational records use fixed codes and static reasons. External error strings, full
paths, hook payloads, prompts, thread IDs and credentials are never passed to these records.
Notice text may show only a filename; `Event::notice` sanitizes and limits its text. Logging
can be disabled or filtered by the configured level.

## Sites that do not represent a dropped notification input

* `config`, `events`, `statusline`, `usage`: defaults for omitted optional fields, unknown
  optional labels, absent reset dates and unknown quota are part of their contracts. Invalid
  configuration values return errors rather than silently using defaults. No warning is
  warranted for ordinary absence; parsing/validation tests cover these cases.
* `activation` and platform activation adapters: failed individual window/link/file probes
  can still reach another valid activation target. The worker reports the overall outcome;
  reporting each unsuccessful candidate would incorrectly describe successful activation
  as failure. Desktop metadata reads remain restricted to `cliSessionId`.
* `session_source`, `usage_service`, resource-worker mailboxes: a missing name, cache eviction,
  coalesced refresh, canceled caller, bounded busy lookup or worker shutdown is an expected
  optional-data/backpressure outcome. Typed connection/lookup failures are reported at their
  source. Dropped activation clicks and refused presentation inputs are handled separately.
* `main` status-line/context forwarding: observation is best effort and never starts a
  resident or changes a status-line's passthrough output. An absent resident is normal.
  Hook event names outside the adapter's supported lifecycle are also normal traffic.
* `local`, `pipe`, `detached`, `process`, `resident_client`, `restart`, audio/overlay/tray
  destructors: handle release, overlapped cancellation/draining, owned-process kill/wait,
  disconnected recipients and thread joins are teardown paths. Relevant operation errors
  already propagate or log at the owning boundary; teardown must not generate error cards.
* Win32 two-call buffer sizing, `ShowWindow`'s previous-visibility return, message translation,
  an already-established DPI context and advisory cursor/DPI probes are not input admission
  decisions. Explorer absence is retried while retaining the tray image. Native resource,
  placement, restart and tray tests cover the supported fallback behavior.
* `operational_log`: log-write failures are intentionally not logged recursively. Performance
  samples and optional process-statistics probes may be unavailable without affecting cards.
* Hook setup/install operations are explicit CLI transactions with their own returned errors,
  backups and receipts; they are not notification-input handling. This task does not change
  their permission-copy or rollback behavior.

## Windows evidence and platform limits

Native evidence: Windows build 22621.4317, Rust/Cargo 1.98.1. The interactive
`native_notice::preview_notice` uses a private resident scope and the real `aherald.exe`,
with its normal adjacent assets. The missing-monitor notice was selected from the two-card
queue: red built-in error picture, `aHerald notice` heading, two message lines, no kind row,
eight-second timer. The initial direct test-executable preview had no adjacent assets and
therefore showed text fallback; it is not evidence for packaged artwork.

`config_notice` checks the native startup window's no-activate style, foreground preservation,
eight-second lifetime, empty startup stdout and exit 2, followed by a fast, card-free hook
with the same invalid configuration. The hidden-usage wake test reads the tooltip applied
by the GUI thread, rather than using a worker log line as a proxy for GUI observation.

Keychain and Unix runtime-directory failure paths were source-reviewed and covered through
portable source/driver fixtures where available. They were not executed on macOS/Linux in
this Windows session. No live agent account, credential read or network request is required
by the added tests.

## Verification status (2026-11-01)

Formatting, Clippy (`--all-targets -- -D warnings`) and
`cargo build --release --target x86_64-pc-windows-msvc` passed. Added diagnostic suites
passed, including the native configuration and missing-monitor checks. The remaining
integration suites were run after each full-run stop. `tray_restart` failed at desktop
enumeration inside the sandbox, then passed all five checks outside it.

The full `cargo test --all-targets` run is **not green**. `native_assets` reproducibly
fails in its final resource case: starting a fresh executable copy with no adjacent assets
exceeds the existing 750 ms readiness deadline, both inside and outside the sandbox.
A separate startup probe observed 85 ms for the main executable and 1422 ms for a fresh
copy. The reason for that delay is not established; neither the timeout nor assertions
were changed. The test now identifies the failing resource case in its error.

One continued run also exceeded `perf_baseline`'s artwork budget (89.6 ms mean against
61 ms) while Clippy was running concurrently. An isolated run without concurrent builds
passed at 41.8 ms mean; this does not establish the cause of the earlier overrun.
AH-090 remains unchecked pending the native-assets regression check. Windows logs are
retained locally under `target/ah090-*-tests.txt` and `target/ah090-tests.txt`.
