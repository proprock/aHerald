# Input-wait coverage — AH-015

Investigated 2026-10-24 using the [official app-server documentation](https://learn.chatgpt.com/docs/app-server),
[hook documentation](https://learn.chatgpt.com/docs/hooks), and the installed
0.164.0-alpha.11.1 schema export without `--experimental` (see
[version evidence](codex-compatibility.md)).

The documented app-server can distinguish active turns, server requests requiring
user input, and request resolution for clients legitimately connected to those
threads. `item/tool/requestUserInput` supplies request identity;
`serverRequest/resolved` supplies thread/request identity when answered or cleared.
Turn completion/interruption can also clear a pending request. An executing
tool's failure is neither user-input entry nor proof of agent failure.

This does not provide passive visibility into arbitrary CLI or desktop sessions.
aHerald neither owns their transport nor has an authorized subscription. Starting
its own stdio child for usage in Milestone 4 will not make those requests visible.
The docs also describe user-input APIs as experimental in some sections; a method
appearing in a schema export does not resolve that stability/coverage limitation.
No stable hook giving arbitrary input-wait entry and response resolution was
found for the target version. PermissionRequest describes a permission boundary,
not a general question and not proof of a still-open human prompt.

Consequently all current CLI/app sessions have **unsupported input monitoring**.
There is no executing/waiting/resolved state inference for those sessions.
`codex.input_monitoring = true` emits an explicit unsupported diagnostic before
resident startup or event delivery; it never starts a poller or opens transcripts.
Lifecycle hooks and manual `--event input_required` demonstrations still work.
An indefinite card stays dismissible when resolution is unobservable. Independent
transient windows have no shared pending-state correlation.

Future support requires a documented stable, legitimately observable source and
versioned request/resolution fixtures. It must not resume unrelated threads,
create a proxy client, read authentication tokens, or infer waits from idle time,
tool names, transcript files or window text. This investigation satisfies AH-015's
explicit allowance for documenting an unavailable stable source.
