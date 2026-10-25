# Notification click activation

`[notifications] activate_agent_on_click = true` opens the selected Codex chat
when clicking a card body in resident and transient modes. `false` disables
activation. `click_to_dismiss` independently controls whether the card closes.
Arrow and Dismiss buttons only manipulate the queue. Usage and demo cards never
activate an application, even if they carry an identity.

The adapter uses the canonical `codex://threads/<thread-id>` link documented in
[OpenAI commands reference](https://learn.chatgpt.com/docs/reference/commands).
Identities are validated (nonempty, bounded, no control characters) and UTF-8
percent-encoded as a single path component. No incoming value becomes a command.
Without a valid ID, or if Windows cannot open the protocol, the adapter tries an
existing visible Codex window. It recognizes Codex.exe and the Windows Store
OpenAI.Codex package, whose current executable is ChatGPT.exe. It does not launch
a guessed executable or match an unrelated ChatGPT window by its title.

One owned worker with one pending click keeps shell/process queries off the GUI
thread. Shutdown joins it, including after a transient card is dismissed, so a
zero fade does not lose an accepted click. Failure preserves normal card behavior
and records `warn agent_activation_failed` in aherald.log beside the selected
configuration, subject to the configured log level. Diagnostics contain no IDs,
links, event payloads or external error strings.

## Verification (2026-10-25)

Focused tests cover enabled/disabled policy, demo/usage isolation, component
encoding, missing/invalid identity, failed protocol/window activation, and body
versus controls at both dismissal settings and scales. Native resident tests
exercise queue navigation and dismissal. Cargo formatting, Clippy, all-target
tests and Windows x64 release build are required for this change.

The user confirmed that clicking the native transient test card opened the
intended real chat. Installed Codex Windows package: 26.1021.3705.0;
CLI: 0.164.0-alpha.11.1. This integration is a shell URI interface, not an
app-server method, and does not resume or send messages to the chat.
The ordinary suite uses a fake activation boundary and needs no installed Codex.
An ignored interactive test can be run with a real AHERALD_TEST_THREAD environment
variable: `cargo test --test activation manual_agent_click_preview -- --ignored
--nocapture`. It expires after three minutes. A real absent-application/fallback
failure is simulated in tests; no Codex installation or protocol registration is
removed by the test suite. Windows focus restrictions can prevent foreground
activation; this produces the same diagnostic.
