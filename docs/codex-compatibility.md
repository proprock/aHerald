# Codex compatibility — AH-013

Verified on 2026-10-24 against installed `codex-cli 0.164.0-alpha.11.1`.
This is an alpha, not a stable-release support claim. No stable CLI release or
other desktop version has been tested. Schema compatibility is not evidence that
a hook actually ran; runtime evidence is recorded separately.

## Sources and reproduction

- [Released hook documentation](https://learn.chatgpt.com/docs/hooks), including
  event input/output, hook trust, and the warning about unreleased main schemas.
- [App-server documentation](https://learn.chatgpt.com/docs/app-server), especially
  initialization, event subscriptions, user input and request resolution.
- [Exact installed release sources](https://github.com/openai/codex/tree/6905ecede7816e91373e9eabda3f6b0368fc747b/codex-rs/hooks/schema/generated),
  tag `rust-v0.164.0-alpha.11.1`. Read `stop.command`,
  `permission-request.command`, and `interrupt.command` input/output schemas.

Run `codex --version` and `codex app-server generate-json-schema --out <temp-dir>`.
The latter was run without `--experimental`; the installed binary exports
`item/tool/requestUserInput`, `serverRequest/resolved` and `thread/status/changed`.
Exportability alone does not establish stable API availability or authorization
to observe another client's session.

## Compatibility matrix

| Source | Evidence and supported behavior | Limits |
| --- | --- | --- |
| `Stop` | Exact schema: session/turn identity; adapter maps to completion and returns `{}` | Another hook may continue the turn; this is a stop boundary, not proof that work can never resume |
| `PermissionRequest` | Exact schema: session/turn identity; maps to permission notification; silent success preserves approval flow | No request/tool-use ID in this schema; agent fields may identify a subagent; hook entry does not prove a human prompt remains open |
| `Interrupt` | Exact schema: session/turn identity; maps to interruption; silent success | Main-thread active turns only; documented 1–3 second command timeout |
| Input wait | No stable lifecycle hook for arbitrary user-input entry/resolution found | CLI and unrelated desktop sessions unsupported; no transcript heuristics |
| App-server requests/resolution | Documented for legitimately connected/subscribed clients | A private stdio server cannot observe unrelated CLI/app sessions; no proxy, resume, or subscription takeover |
| Tool failure | `PostToolUse` may describe failed commands | Not an agent execution error; intentionally ignored |

Only allowlisted normalized identity and event kinds leave the hook adapter.
Transcript paths, assistant text, arguments and prompts are neither forwarded nor
logged. Missing identities are tolerated for display but cannot establish a
matching lifecycle resolution. Unknown fields do not expand capabilities.

`tests/fixtures/stop.json` and the other hook fixtures are synthetic, sanitized
wire examples, not captured conversations. Required schema fields are retained
in full examples; tests also exercise intentionally incomplete input. No live
account, credentials, or installed Codex is needed by ordinary tests.

Pending permission cards cannot track an individual approval response with these
hooks. Matching terminal evidence can clear turn-scoped waits; otherwise cards
remain dismissible. In transient mode independent windows do not share lifecycle
state. Version-dependent hook availability and unsupported input monitoring must
remain visible in setup instructions and runtime diagnostics.
