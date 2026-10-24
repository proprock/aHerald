# Codex hook adapter compatibility

Checked 2026-10-24. Installed CLI: `codex-cli 0.164.0-alpha.11.1`.
Exact release tag `rust-v0.164.0-alpha.11.1` resolves to
`6905ecede7816e91373e9eabda3f6b0368fc747b`.

Sources:
- [Official lifecycle and output behavior](https://learn.chatgpt.com/docs/hooks).
- [Matching generated schemas](https://github.com/openai/codex/tree/6905ecede7816e91373e9eabda3f6b0368fc747b/codex-rs/hooks/schema/generated).
  Inspected both input and output for stop, permission-request, and interrupt.

The adapter maps Stop to completion, PermissionRequest to a pending permission
notification, and Interrupt to interruption. It retains only optional session/turn
identifiers; these schemas expose no request identity. It ignores other fields,
including messages, transcript paths, and tool arguments. It tolerates missing
identities for compatibility and unknown fields for forward compatibility.

For valid Stop input it writes `{}`. PermissionRequest and Interrupt write nothing.
All hook failures exit successfully, with operational diagnostics only on stderr.
No response changes permissions, input, or continuation. Input is limited to 64 KiB
and 250 ms; malformed, oversized, and unknown events do not produce notifications.

`tests/fixtures/stop.json` is synthetic and sanitized, based on the matching schema.
Tests invoke the hook process and inspect its response and lifetime. No real account
or Codex invocation is required. Real hook execution/trust and stable-release coverage
remain Milestone 3 work. No transcript or unrelated-session observation is implemented.
