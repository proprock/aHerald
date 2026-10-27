# Session source labels

Codex hook events retain their existing `session_id`; no hook or IPC format change
is needed (AH-085 added the optional `folder`: the last component of the hook's `cwd`).
A card's first row is `<harness>, <folder>` (`Codex, aHerald`; only the harness when
there is no usable folder, and `aHerald demo` for demo cards). The second row is the
Codex `<thread.name>`; it stays empty, and keeps its height, until a lookup completes,
for unnamed/inaccessible threads, and with unavailable Codex. A session ID is never
shown. Claude cards have no name row content (AH-059). Long text is ellipsized within
the card. The folder is display-only: only the last path component is kept, control and
bidirectional-override characters are removed, it is cut at 40 characters, and it is
never logged.

In the resident with Codex usage polling enabled, names are read over the usage
worker's own app-server connection (`usage_service::NameLookup`): requests are
strictly serial on that one connection, so only one `codex app-server` child runs.
An unknown thread (a JSON-RPC error) leaves the connection open; a lost connection
follows the usage worker's reconnect backoff and yields the ID fallback meanwhile.
With usage polling off, and in transient mode (one card per process), the worker
below owns its own child. Documentation checked 2026-10-27: after the single
`initialize` handshake a client may send further requests, each with its own `id`
([app-server](https://learn.chatgpt.com/docs/app-server)); the page documents no
per-connection request limit for stdio.

A separate background worker lazily owns a stdio app-server process. It performs
`initialize`, `initialized`, then only `thread/read` with `includeTurns: false`.
The requested ID must match the response ID. It never resumes/subscribes to a
thread, reads transcript files, starts turns, or answers permission requests.
Metadata requests have a 750 ms deadline (initialization has its own deadline).
The cache holds at most 64 sessions, request/result channels hold 32 entries each,
and selected sessions refresh after 60 seconds. Missing/failed lookups are cached
for the same interval. Clearing a stored name restores the ID fallback after refresh.
Title updates neither restart notification expiry nor reset animation playback.
Shutdown cancels the metadata transport and reaps only the owned app-server child.

Names are not logged or written to configuration. Display normalizes whitespace,
removes directional formatting controls, and bounds titles to 120 characters.
The source uses only the dedicated name field, never prompt text or `preview`.

## Verified interface

Checked on 2026-10-25 against `codex-cli 0.164.0-alpha.11.1`, its generated protocol
schemas without experimental features, and the official
[hooks](https://learn.chatgpt.com/docs/hooks) and
[app-server](https://learn.chatgpt.com/docs/app-server) documentation.
`ThreadReadParams` supports `threadId` and `includeTurns`; the response's `name`
is nullable. Read-only verification against the current live chat returned a valid
thread with `name: null`, despite its sidebar caption. An automatic sidebar caption
therefore cannot be promised as a name. Explicitly stored names are covered by
sanitized executable fixtures, including a mismatched-session response rejection.

## Verification

- `tests/app_server.rs`: exact metadata-only request, named/unnamed results,
  response/session correlation, no resume or subscription calls.
- `tests/session_source.rs`: immediate fallback while the resolver is blocked,
  simultaneous session isolation, cache/refresh, normalization and unavailable titles.
- Native card tests: asynchronous window-title update, no activation, and pixel
  preservation outside the heading area. The ignored `preview_session_heading`
  test provides one 30-second visual check with a Russian title.
- The ignored `installed_codex_reads_name_without_resuming_thread` test reads only
  `AHERALD_SESSION_ID`; `AHERALD_EXPECTED_SESSION_NAME` specifies an expected stored
  name, otherwise the test expects the verified unnamed case. No title is printed.

Observed on Windows 11 build 22621 with Codex 0.164.0-alpha.11.1:
formatting, Clippy with warnings denied, all 119 ordinary tests, and the MSVC x64
release build passed. Nine environment/manual tests were ignored by the ordinary
suite. The read-only installed-Codex metadata test and the single Russian-heading
visual preview were run separately and passed; the preview did not steal focus.
