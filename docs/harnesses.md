# Other coding harnesses

Besides Codex and Claude Code, aHerald shows notifications for OpenCode, Pi, Crush and Goose
through one input, `aherald --notify <harness> --kind <kind> [--activity|--ended]` (see
[the input](harness-compatibility.md#the---notify-input)). You can copy the files yourself, or let
`aherald --setup --harness opencode|pi|goose` do it (below). Crush is manual only: its
`crushrc` is your own shell script, which aHerald does not edit. Event contracts and what was checked live are in
[harness-compatibility.md](harness-compatibility.md).

`aherald` must be on the harness's `PATH`, or set `AHERALD_EXE` to its full path for the
OpenCode and Pi bridges (the Crush and Goose commands can contain a full path instead).
A missing or failing aHerald never changes what the harness does.

| Harness | Mechanism | Install | Remove | Shows | Checked |
|---|---|---|---|---|---|
| OpenCode 2.x | plugin | copy `examples/harnesses/opencode-aherald.ts` to `~/.config/opencode/plugins/aherald.ts` | delete it | complete, error, interrupted, permission | live on 2.2.0 (not the TUI permission flow) |
| Pi 1.5 | extension | copy `examples/harnesses/pi-aherald.ts` to `~/.pi/agent/extensions/aherald.ts` | delete it | complete, error, interrupted, extension prompts | live on 1.5.0 (not compaction or queued follow-ups) |
| Crush | `hook add` lines in `crushrc` | append `examples/harnesses/crushrc` yourself | delete the lines | complete, error, interrupted, permission | live on 0.99.0: complete, error (not permission, interrupt) |
| Goose | plugin hooks | copy `examples/harnesses/goose-hooks.json` to `~/.agents/plugins/aherald/hooks/hooks.json` | delete the folder | complete | live on 1.57.0 |

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
  exits 0, so it never does. Cards show the folder from `working_dir`, except at session end.

Give each harness its own pet with `[pets]` (see [pets](pets.md)).

## Installing with `--setup`

```sh
aherald --setup --harness opencode --harness pi --harness goose          # dry run
aherald --setup --harness opencode --harness pi --harness goose --apply  # write
aherald --unsetup --apply                                                # remove everything aHerald installed
```

- The files are the bridges above with the full path of this `aherald` stamped in (so
  `AHERALD_EXE` is unnecessary); they go to `~/.config/opencode/plugins/aherald.ts`
  (`$XDG_CONFIG_HOME` is honoured), `~/.pi/agent/extensions/aherald.ts` (`$PI_CODING_AGENT_DIR`
  is honoured) and `~/.agents/plugins/aherald/hooks/hooks.json`.
- A dry run, which is the default, writes nothing. A file that already exists and was not
  written by aHerald is never replaced; setup stops before writing anything.
- The receipt records the exact text written. Setup updates a file only while it still holds that
  text, and removal deletes it only then; a file you edited is left in place and reported. A
  removed or replaced file is kept next to it as `*.aherald-backup`. A failed write restores the
  files already written.
- Naming a bridge with `--harness` replaces the set of installed bridges (so naming only `pi`
  removes the OpenCode bridge); a setup that names none leaves installed bridges alone.
  `--harness codex|claude` is unchanged.
- The Windows installer, the macOS setup window and the Linux console setup list OpenCode, Pi and
  Goose beside Codex and Claude. They start unchecked on a first setup and show an installed bridge
  as checked, and what they apply is the complete selection: clearing a box removes that bridge
  (it is kept as `*.aherald-backup`), and a silent Windows install with an explicit `/TASKS=` that
  omits `opencode`, `pi` or `goose` removes the installed bridge too. Crush has no bridge file, so
  it is not listed.
