# Milestone 4 — Usage and quotas

## Codex discovery correction — 2026-10-25

The ordinary Windows PATH exposed npm's `codex.cmd`, while `codex.exe` was only
available in the desktop app's inherited environment. The existing live usage
test reproduced `Unavailable` with the ordinary machine + user PATH.

Discovery now prefers `codex.exe` across absolute PATH directories, then checks
prefixes containing `codex.cmd` for the npm package's native x64 binary. The npm
launcher in the installed `@openai/codex` 0.160.0 resolves the platform package
`@openai/codex-win32-x64`, using its `vendor/x86_64-pc-windows-msvc/bin/codex.exe`,
with a bundled `@openai/codex/vendor` fallback. Nested and hoisted platform package
locations are supported. Relative/empty PATH entries and the working directory
are not searched. No package-manager commands, Node processes, binary copies, or
global PATH changes are involved; discovery runs again on reconnection.

Six filesystem-based tests cover native precedence/order, npm layouts, spaces,
incomplete installations, and missing/relative paths. Red: five discovery tests
failed with no resolver; Green: all six passed after implementation. On Windows
11 Pro 10.0.22621, the rebuilt `usage_live` test passed with the ordinary Windows
PATH and npm Codex 0.160.0, receiving one usage bucket. The same test previously
passed with desktop Codex 0.164.0-alpha.11.1. No protocol behavior changed:
the existing adapter still uses the documented
[`account/rateLimits/read`](https://learn.chatgpt.com/docs/app-server#6-rate-limits-chatgpt).
Both live discovery scenarios passed after the fix. Formatting, Clippy with
warnings denied, all-target tests, and the x64 MSVC release build also passed.
The live resident was updated with an executable backup and restarted with the
ordinary Windows PATH; its configuration hash remained unchanged.
Only the installed npm layout was inspected; future package layouts and package
managers other than npm are outside this verification.

## AH-017 — Official stdio connection

Protocol checked on 2026-10-24 against `codex-cli 0.164.0-alpha.11.1`.
Sources: [official app-server documentation](https://learn.chatgpt.com/docs/app-server)
and the installed binary's `codex app-server generate-json-schema --out <temp>`
export without experimental features. This remains an alpha-version integration,
not a claim of stable-release compatibility. The matching release commit is
`6905ecede7816e91373e9eabda3f6b0368fc747b` (see codex-compatibility.md).

The adapter owns a single `codex.exe app-server --listen stdio://` child, performs
`initialize`/`initialized`, then reads `account/rateLimits/read`. The resident's
[session names](session-source.md) use metadata-only `thread/read` over this same
connection (a separate child only when usage polling is off). Messages
are bounded to 256 KiB, incoming queues to 32 messages, requests to five seconds,
and reconnect delays to 1–60 seconds. Request IDs increase per connection. An
unexpected server request closes the connection without responding; aHerald never
answers approvals or supplies externally managed auth tokens. Remote error text
and child stderr are discarded to prevent sensitive diagnostics. Missing Codex,
authentication/service errors and unsupported methods have safe, fixed messages.
Shutdown kills and reaps the owned child and joins its pipe workers. No threads,
turns or unrelated sessions are opened. Credentials remain managed by Codex;
aHerald never reads credential files or installs hooks/configuration.

Deterministic mock tests exercise handshake ordering, request correlation,
timeouts, disconnects, unsupported/authentication errors, unsolicited requests,
and reconnect/backoff. A Windows process test exchanges JSON over real stdio,
rejects malformed/oversized messages, observes timeout/EOF and verifies the owned
process is terminated after drop. This test uses Windows PowerShell as a local
mock peer; ordinary tests do not need Codex or a live account.

The transport alone starts no background polling; the resident usage service
consumes it in AH-019.

AH-017 verification on Windows 11 Pro 10.0.22621 x64: `cargo test --test
app_server` passed (5 tests); `cargo fmt --all -- --check`, `cargo clippy
--all-targets -- -D warnings`, `cargo test --all-targets`, and `cargo build
--release --target x86_64-pc-windows-msvc` passed. Existing manual device/tray
tests remain ignored. The initial red run failed because the adapter module did
not exist; native-test compilation was corrected to use `PROCESS_SYNCHRONIZE`.

## AH-018 — Rate-limit normalization

The same installed schema defines `GetAccountRateLimitsResponse`,
`RateLimitSnapshot`, `RateLimitWindow`, `CreditsSnapshot`,
`RateLimitResetCreditsSummary`, and `AccountRateLimitsUpdatedNotification`.
Synthetic JSON fixtures in `tests/usage.rs` exercise those shapes without account
identifiers or captured payloads. A present multi-bucket map (even an empty one)
takes precedence over legacy data. Missing/null windows and fields remain
unavailable, while zero stays zero. Usage over 100% is preserved; computed
remaining usage is clamped to 0–100%. Invalid negative percentages or durations
are rejected. Unknown fields are ignored.

Preserve both windows, bucket labels and plans, Unix reset timestamps, workspace
credit flags/balance, and reset-credit counts/details. Counts are not inferred
from the potentially truncated detail list. There is no credit consumption,
account mutation, login or token access. Sparse update fields merge only when
non-null and never erase other windows/buckets or known nullable metadata. An
ambiguous update without bucket identity requires a complete refetch.

Local reset formatting uses Win32 `SystemTimeToTzSpecificLocalTimeEx`, including
historical timezone rules at the reset instant and a numeric UTC offset to
disambiguate repeated autumn hours. The existing windows crate enables its
native `Win32_System_Time` bindings; no new dependency or runtime is introduced.
The fixed Warsaw-zone test checks both sides of the 2026 spring/autumn changes
without altering the system timezone. Out-of-range timestamps remain unavailable.

Red: all four initial behavior tests failed against unavailable placeholder
implementations. Green: all five focused tests pass, including optional credit
details. The DST fixture was corrected to supply the Win32 timezone key required
by the Ex API; its expected times were preserved.

AH-018: all four standard Cargo checks and the MSVC x64 release build passed on
Windows 11 Pro 22621; five focused usage tests passed. Windows 10 is untested.

## AH-019 — Independent background usage and native presentation

Historical evidence below describes AH-019 as originally implemented. AH-031
later moved details into the shared card, and AH-034 removed age-based stale
states everywhere: last received values remain current, with age and errors in
details. See [current tray behavior](tray-usage.md).

Usage stays opt-in (`[codex.usage] enabled = true`) and resident-only. The refresh
interval defaults to 300 seconds. A dedicated worker owns the connection and
never takes the notification queue lock or blocks the GUI on Codex I/O. Manual
refresh has a bounded coalescing mailbox and a five-second minimum request gap;
failures defer further requests by 10–300 seconds, including manual requests.
Healthy polling reuses one initialized process. Shutdown cancels pending reads,
reaps the owned server and joins workers. Disabled usage starts no server/worker.

The tray tooltip/menu summary shows both main windows and explicit
loading/disabled/unavailable/stale/current state. Its standard tooltip is explicitly
enabled for tray protocol v4 as required by [Microsoft's NOTIFYICONDATA contract](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-notifyicondataw).
The modeless, resizable native details window lists all buckets and both windows,
local reset times with UTC offsets, workspace credits, and available reset credits
without actions to consume them. A read-only scrolling edit control keeps long
results accessible. It updates while open. Missing values say unavailable;
expired or failed snapshots are stale, retain their data and show the last update
age. A verified account-change notification clears previous account data before
refetching; sparse rate-limit notifications update only their identified bucket.

Verification on 2026-10-25, Windows 11 Pro 22621:

- Controlled-clock tests cover the 300-second cadence, persistent connection reuse,
  coalesced/manual request limits, exponential failure backoff, stale state,
  recovery, sparse updates and account-change invalidation. All three original
  behavior tests failed against the initial placeholder, then passed; a fourth
  verifies automatic scheduling and disabled operation.
- A native resident test injects unavailable Codex transport, opens Usage details
  while the notification overlay is hidden, requests refresh, exchanges an event
  and status over IPC, and quits. Delivery and shutdown remain responsive; the
  owned details window is destroyed. No installed Codex/account is needed.
- Native window tests read displayed bucket text, update it to stale, verify
  read-only behavior, and scroll a 150-row result. A regression test reproduced
  background refresh resetting the viewport; the fix preserves its scroll position
  and the same test now passes. The prior resident UI test now
  checks disabled usage and nonmodal refresh rather than the Milestone 2 stub.
- `cargo test --test usage_live -- --ignored --nocapture`: installed Codex
  handshake passed in the sandbox with safe usage unavailability. The same
  read-only test outside the sandbox in the user's profile returned one bucket.
  It prints only a bucket count or sanitized error; no credentials/account data.
- `cargo test --test usage_ui preview_usage_details -- --ignored --nocapture`:
  visually inspected synthetic two-bucket usage, primary/secondary percentages,
  credit values, unknown fields and the repeated DST hour's two different UTC
  offsets. Final inspection used the system GUI font; no clipped text was seen.
  The preview alone gets a taskbar style so the desktop inspector can find it.
  It was closed normally after inspection.

The live test and interactive preview are ignored in ordinary automated runs.
Final AH-019 checks passed: `cargo fmt --all -- --check`, `cargo clippy
--all-targets -- -D warnings`, `cargo test --all-targets` (85 passed, five manual
tests ignored), and `cargo build --release --target x86_64-pc-windows-msvc`.
Live verification covers this installed alpha only. Windows 10 and a stable
Codex release remain untested; authentication/network outages are explicit
unavailability rather than a promise that every account exposes limits. No
transcripts, unrelated sessions, login flows or account mutations are used.
