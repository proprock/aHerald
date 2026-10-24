# Milestone 4 — Usage and quotas

## AH-017 — Official stdio connection

Protocol checked on 2026-10-24 against `codex-cli 0.164.0-alpha.11.1`.
Sources: [official app-server documentation](https://learn.chatgpt.com/docs/app-server)
and the installed binary's `codex app-server generate-json-schema --out <temp>`
export without experimental features. This remains an alpha-version integration,
not a claim of stable-release compatibility. The matching release commit is
`6905ecede7816e91373e9eabda3f6b0368fc747b` (see codex-compatibility.md).

The adapter owns a single `codex.exe app-server --listen stdio://` child, performs
`initialize`/`initialized`, then permits only `account/rateLimits/read`. Messages
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
