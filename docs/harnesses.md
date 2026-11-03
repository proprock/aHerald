# Other coding harnesses

Besides Codex and Claude Code, aHerald shows notifications for OpenCode, Pi, Crush and Goose
through one input, `aherald --notify <harness> --kind <kind> [--activity|--ended]` (see
[the input](harness-compatibility.md#the---notify-input)). aHerald never edits these
harnesses' files: you copy the bridge or merge the hook entries yourself, and remove them the
same way. Event contracts and what was checked live are in
[harness-compatibility.md](harness-compatibility.md).

`aherald` must be on the harness's `PATH`, or set `AHERALD_EXE` to its full path for the
OpenCode and Pi bridges (the Crush and Goose commands can contain a full path instead).
A missing or failing aHerald never changes what the harness does.

| Harness | Mechanism | Install | Remove | Shows | Checked |
|---|---|---|---|---|---|
| OpenCode 2.x | plugin | copy `examples/harnesses/opencode-aherald.ts` to `~/.config/opencode/plugins/aherald.ts` | delete it | complete, error, interrupted, permission | live on 2.2.0 (not the TUI permission flow) |
| Pi 1.5 | extension | copy `examples/harnesses/pi-aherald.ts` to `~/.pi/agent/extensions/aherald.ts` | delete it | complete, error, interrupted, extension prompts | live on 1.5.0 (not compaction or queued follow-ups) |
| Crush | `hooks` in `crush.json` | merge `examples/harnesses/crush.json` | delete the entries | complete, error, interrupted, permission | documentation only |
| Goose | plugin hooks | copy `examples/harnesses/goose-hooks.json` to `~/.agents/plugins/aherald/hooks/hooks.json` | delete the folder | complete | documentation only |

Not supported: Aider (its notification command gets no event or session data) and Zero (its
hooks cover tools and sessions only). Any other harness that can run a command can call
`--notify` directly.

## What the bridges send

Only the session id and the working folder, as `{"session_id": ..., "cwd": ...}` on stdin of
`aherald --notify`; never prompts, replies, tool arguments or credentials. Calls are made one
after another, each limited to three seconds. Details:

- **OpenCode:** sub-agent (child) sessions stay silent; retries are not failures, only the final
  `session.execution.failed` is an error. A short headless `opencode run` may end before the
  last call is made.
- **Pi:** only interactive sessions (terminal UI or RPC) notify, so `pi -p` scripts and
  sub-agent processes stay silent. Pi has no permission events; a card appears when an
  extension asks the user something.
- **Goose:** its `Stop` hook may block a turn from ending; the command prints nothing and
  exits 0, so it never does. Goose sends no folder with these events, so its cards show none.

Give each harness its own pet with `[pets]` (see [pets](pets.md)).
