# Codex hook adapter compatibility

Checked 2026-10-24. Installed CLI: `codex-cli 0.164.0-alpha.11.1`.
Exact release tag `rust-v0.164.0-alpha.11.1` resolves to
`6905ecede7816e91373e9eabda3f6b0368fc747b`.

Sources:
- [Official lifecycle and output behavior](https://learn.chatgpt.com/docs/hooks).
- [Matching generated schemas](https://github.com/openai/codex/tree/6905ecede7816e91373e9eabda3f6b0368fc747b/codex-rs/hooks/schema/generated).
  Inspected both input and output for stop, permission-request, and interrupt.

The adapter maps Stop to completion, PermissionRequest to a pending permission
notification, and Interrupt to interruption. It retains optional session/turn
identifiers and PermissionRequest's optional subagent identity; these schemas
expose no request identity. It ignores other fields,
including messages, transcript paths, and tool arguments. It tolerates missing
identities for compatibility and unknown fields for forward compatibility.

For valid Stop input it writes `{}`. PermissionRequest and Interrupt write nothing.
All hook failures exit successfully, with operational diagnostics only on stderr.
No response changes permissions, input, or continuation. Input is limited to 64 KiB
and 250 ms; malformed, oversized, and unknown events do not produce notifications.

`tests/fixtures/stop.json` is synthetic and sanitized, based on the matching schema.
Tests invoke the hook process and inspect its response and lifetime. No real account
or Codex invocation is required. Stable-release coverage remains unverified.
See the [compatibility matrix](codex-compatibility.md). No transcript or
unrelated-session observation is implemented.

## Manual Windows setup and removal

1. Build/extract `aherald.exe`. Prepare `config.toml` beside it, or let the first
   normal startup create the defaults in a writable installation folder. See
   [configuration and manual migration](configuration.md).
2. Copy the event entries in `examples/hooks.windows.json` into your chosen
   `~/.codex/hooks.json` or a trusted project's `.codex/hooks.json`. Replace the executable
   path. The default configuration needs no `--config` argument. JSON doubles backslashes;
   PowerShell single quotes preserve
   spaces. A literal apostrophe in a path must be doubled inside the PowerShell
   string. Keep `| Out-String`: PowerShell needs an output consumer to wait for
   this GUI-subsystem executable and collect its hook response. The explicit
   `--resident` shares one queue between hook invocations.
3. Preserve existing event groups and handlers: append these groups to the
   existing arrays instead of replacing the file. All matching hooks run.
   Back up the original file and avoid registering aHerald at two config layers.
4. Start a new Codex session, open `/hooks`, inspect and trust the exact command
   definitions. Project-local hooks also require a trusted project. Changed
   commands require trust review again. Never bypass hook trust for normal setup.
5. Complete a harmless turn, trigger an approval using your normal policy, then
   interrupt an active turn. Confirm the matching cards and normal approval flow.
   Hooks neither answer permission requests nor alter prompts. Dismissal only
   removes a card. Remove `Interrupt` on versions that do not support that event.

Commands have a three-second Codex timeout; aHerald input and delivery have their
own shorter deadlines. The handler returns before the card expires, including
indefinite cards. The example needs no script execution-policy change. For a
direct process smoke check, pipe a sanitized fixture into the quoted executable
with `--hook --transient` (add `--config <path>` only for a custom file); this does not
test Codex trust or dispatch.

Session names: the resident reads them over one shared `codex app-server` connection
(2-20 ms per lookup after the first, about 140 ms for the first when usage polling is off).
A transient process shows one card, so a name would cost a new `codex app-server` child per
card (measured on Windows 11, release build, 8 runs: 130-150 ms until the name, 20-560 ms to
end the child). Transient mode therefore shows `Codex: session <last 8 characters>` unless
`[codex] transient_names = true` (AH-058, default false). Claude cards never look names up.

To remove integration, delete only the aHerald handler/groups, preserving other
hooks. Quit the resident with `aherald.exe --quit`, then remove portable files if
desired. Nothing edits global configuration, installs autostart, or changes trust
automatically. Re-check contracts and repeat the smoke test after Codex upgrades.

## Resident lifecycle correlation

Matching Stop/Interrupt boundaries clear current pending main-thread cards only
with a valid session and turn identity from Codex. Different turns, sources and
subagents remain isolated. Processing also runs with presentation disabled or
paused. Demo events never resolve Codex waits. Incomplete identities and subagent
waits without a supported terminal source remain manually dismissible.

Interrupt retains at most 128 terminated-turn identities to suppress delayed
wait delivery. Stop does not: another Stop hook can continue the same turn, so
subsequent waits must remain visible. A delayed Stop cannot be distinguished from
a new Stop boundary in a continued turn; these hooks have no ordering token.
Hook permission requests also have no request identity: separate invocations are
kept separate rather than incorrectly merged. Identified normalized duplicates
are suppressed within the configured window and only within the same agent.
No second independently observable Codex source is currently integrated.
