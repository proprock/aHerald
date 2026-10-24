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

AH-018 and AH-019 will connect this adapter to rate-limit normalization and the
resident UI. The transport alone starts no background polling.

AH-017 verification on Windows 11 Pro 10.0.22621 x64: `cargo test --test
app_server` passed (5 tests); `cargo fmt --all -- --check`, `cargo clippy
--all-targets -- -D warnings`, `cargo test --all-targets`, and `cargo build
--release --target x86_64-pc-windows-msvc` passed. Existing manual device/tray
tests remain ignored. The initial red run failed because the adapter module did
not exist; native-test compilation was corrected to use `PROCESS_SYNCHRONIZE`.
