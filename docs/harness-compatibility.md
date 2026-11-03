# Harness event compatibility (AH-201)

What an external command can learn from each coding harness, checked on 2026-11-03 on
macOS 26 (arm64). "Live" means a real harness process emitted the event to the probe in
`tests/live/harness_probes/`; "documented" means only the vendor's documentation or the
installed package names it. Documented rows are not acceptance evidence.

aHerald maps these to its existing kinds only: `turn_complete`, `permission_required`,
`input_required`, `error`, `interrupted`, plus the `activity` and `ended` lifecycle flags.

| Harness (version) | turn_complete | error | interrupted | permission_required | input_required | session id |
|---|---|---|---|---|---|---|
| OpenCode 2.2.0 | `session.execution.succeeded` (live) | `session.execution.failed`, after ten `session.retry.scheduled` (live) | `session.execution.interrupted`, `reason: "user"` (live) | `permission.asked` `{id, sessionID, action, resources, save, source}`, resolved by `permission.replied` `{sessionID, requestID, reply}` (live) | `session.form.*` (names only, meaning unverified) | `data.sessionID` (live) |
| Pi 1.5.0 | `agent_settled`, `aborted: false` after an `agent_before_settle` with `outcome: "completed"` (live) | `agent_settled` after an `agent_before_settle` with `outcome: "error"`; `agent_settled` itself has no error flag (live) | `agent_settled` with `aborted: true`, no `agent_before_settle` (live, via `ctx.abort()`) | none: Pi has no permission event | `ui_prompt_start` / `ui_prompt_end` with `kind` (live, RPC mode only: `hasUI` is false in `-p` mode and no event fires) | `ctx.sessionManager.getSessionId()` (live) |
| Crush | `TurnEnd` (documented) | `StopFailure` (documented) | `Interrupt` (documented) | `PermissionRequest`, resolved by `PermissionResult` (documented) | none | `session_id` and `cwd` on stdin |
| Goose | `Stop` (documented) | none | none | none | none | `session_id` on stdin; `working_dir` only on tool events, so cards have no folder |
| Aider | unsupported: `--notifications-command` runs with no arguments, stdin or session id, and fires on the next prompt of any kind | | | | | |
| Zero (Twigpine) | unsupported: hooks are `beforeTool`, `afterTool`, `sessionStart`, `sessionEnd`; its stream-JSON events (`final`, `permission_request`, `error`) exist only for headless `zero exec` | | | | | |

Crush and Goose are documented only, from their official hook documentation read on
2026-11-03 (Crush `docs/hooks/README.md` on `main`; Goose `hooks.md`). Both run a `sh`-style
command with JSON on stdin and need hook configuration, not a bridge. Crush reads hooks from `crushrc` (`hook add EVENT --command ...`; the JSON `hooks` form is deprecated); its `TurnStart`, `TurnEnd` and `PermissionRequest` fire for the top-level agent
only. Goose follows the Open Plugins layout, `~/.agents/plugins/<name>/hooks/hooks.json`; its
`Stop` hook may block, which an empty stdout and exit 0 never do. No Crush or Goose binary was
run, so the shipped examples are untested against the real programs.

## OpenCode 2.2.0

- Install: Homebrew `opencode` (v2). The npm package `opencode-ai` is the 1.19.x line,
  whose plugin API (`export const Plugin = async (ctx) => ({ event })`, `session.idle`)
  differs; this milestone targets v2 only.
- Plugin contract (live): `~/.config/opencode/plugins/*.ts` is loaded; the module must
  `export default Plugin.define({ id, setup(ctx) })` with `Plugin` from `@opencode/plugin`.
  A v1-style named export is rejected with "Plugin must export a default definition with
  an id and an effect or setup function". `ctx.event.subscribe()` is an async iterable of
  `{ id, created, type, durable?, location, data }`.
- Observed order of one successful turn (live): `session.created`,
  `session.inbox.enqueued`, `session.execution.started`, `session.step.started`,
  `session.text.*`, `session.step.ended`, `session.usage.updated`,
  `session.execution.succeeded`, `location.shutdown`.
- Error (live): the provider answering HTTP 500 gives ten `session.retry.scheduled`
  (`{sessionID, assistantMessageID, attempt, at, error}`, about 100 s in total) and then one
  `session.execution.failed` `{sessionID, error}`. A notifier must not treat the retry events
  as failures.
- Interruption (live): `session.execution.interrupted` `{sessionID, reason}`, produced by
  `ctx.session.interrupt({ sessionID })`.
- Permission (live): with `permission.shell = "ask"` in headless `opencode run`,
  `permission.asked` is followed at once by `permission.replied` with `reply: "reject"`
  (non-interactive runs reject), the tool fails with `permission.rejected` and the turn still
  ends with `session.execution.succeeded`. In a TUI the pair is expected to be separated by
  the user's answer; that was not observed. Request ids pair the two events.
- Sub-agents (live): the built-in tool is `subagent` (v1 called it `task`; it takes `agent`
  and an optional `sessionID`). A child session announces itself with `session.created`
  carrying `parentID`, and runs its own `session.execution.started` / `succeeded` pair with
  the child's `sessionID`; the parent's `session.tool.progress` / `session.tool.success`
  surround it. A bridge must remember the ids of sessions created with a `parentID` and ignore
  their events, otherwise every sub-agent produces a completion notice. Unknown: child
  sessions resumed after a plugin restart.
- Folder (live): execution events carry no location; `session.created` has `location.directory`,
  which the bridge remembers per session (fallback: the plugin process's directory).
- Not verified: the TUI permission flow, `session.form.*`, compaction, and what a throwing
  plugin does to the agent.
- Operational: `opencode run --standalone` hangs on this version unless `--print-logs` is
  given. The managed service listens on a fixed port (49374) and keeps `service.json` in
  `~/.config/opencode`, so probes run with a private `HOME`.

## Pi 1.5.0

- Install: `npm install -g --ignore-scripts @earendil-works/pi-coding-agent`; needs Node
  >= 22.19 (Node 23.7 crashes in the bundled undici).
- Extension contract (live): `~/.pi/agent/extensions/*.ts` (or `PI_CODING_AGENT_DIR`),
  `export default function (pi) { pi.on(name, async (event, ctx) => ...) }`;
  `ctx.cwd` and `ctx.sessionManager.getSessionId()` give the folder and session.
- Live trace (`-p`, local fake OpenAI endpoint): `session_start`, `before_agent_start`,
  then four `agent_start` / `turn_start` / `turn_end` / `agent_end` rounds (three with
  `outcome: "error"`, then `completed`: Pi retried by itself), then one
  `agent_before_settle`, one `agent_settled` (`aborted: false`) and `session_shutdown`.
  So `agent_end` is not a final-idle signal; `agent_settled` is the boundary to use.
- Error (live): a provider answering HTTP 500 gives four `agent_start` / `turn_end`
  (`outcome: "error"`) / `agent_end` rounds, then `agent_before_settle` with
  `outcome: "error"`, then `agent_settled` with `aborted: false`; the process still exits 0.
  The failure is visible in `agent_before_settle.outcome` (or the last `turn_end`), not in
  `agent_settled`.
- Abort (live): `ctx.abort()` (what Esc does) gives `turn_end` (`outcome: "error"`, message
  "This operation was aborted"), `agent_end`, then `agent_settled` with `aborted: true`.
  SIGINT in `-p` mode kills the process with no further events.
- Prompts (live): in `--mode rpc` an extension's `ctx.ui.confirm()` fires `ui_prompt_start`
  `{kind: "confirm"}` and, once answered, `ui_prompt_end`. These are extension dialogs: Pi has
  no built-in permission gate, so this is the only input-wait signal, and it also covers
  third-party extensions' dialogs.
- Probe artifact: the local fake endpoint's first three requests time out and Pi retries
  (`Request timed out.`), so the success scenario shows three failed rounds before the
  completed one. The cause was not found (a plain `python3 -m http.server` is also
  unreachable from `curl` in the same sandbox); the settle boundary is unaffected.
- Sub-agents (live): Pi's core has none. The example `subagent` extension runs each as a
  child process, `pi --mode json -p --no-session`, which loads the same global extensions
  again. Such a child reports `ctx.mode === "json"`, `ctx.hasUI === false` and its own session
  id, while an interactive session reports `ctx.mode === "tui"` and an RPC client
  `ctx.mode === "rpc"` (both `hasUI` true; live). A bridge should notify only when
  `ctx.hasUI` is true: that also silences `pi -p` scripts, which is intended.
- Not verified: compaction and queued follow-ups.

## Repeating the probes

```sh
sh tests/live/harness_probes/run.sh pi [ok|error|abort|ui|child|tui]
sh tests/live/harness_probes/run.sh opencode [ok|error|permission|interrupt|subagent]
```

Pi and OpenCode (except `opencode ok`, which sends one trivial prompt to OpenCode's free
hosted model) talk to a local fake OpenAI endpoint: no account and no billing. The non-`ok`
OpenCode scenarios let OpenCode install `@ai-sdk/openai-compatible` from npm on first use.
`opencode error` takes about 100 s. They use a private `HOME`, bound their run time and remove their files. They are opt-in
and are not part of `cargo test` or CI.

## The `--notify` input

Any harness that can run a command reports through one input; no per-harness adapter exists:

```sh
aherald --notify <harness> --kind <kind> [--activity | --ended]
```

- `<harness>` is `[a-z0-9_-]{1,32}` and not `codex`, `claude`, `demo`, `usage`, `aherald`,
  `default` or `animations`. OpenCode, Pi, Goose and Crush get a readable label; others show the key.
- `<kind>` is `turn_complete`, `permission_required`, `input_required`, `error` or
  `interrupted` (the context and idle warnings stay Codex/Claude only).
- `--activity` marks progress of a session without a card: it resolves that session's pending
  waits. `--ended` marks the session as over and clears its waits.
- Stdin may carry JSON; only `session_id` and the folder (`cwd`, or Goose's `working_dir`) are
  read, bounded to 64 KiB and 250 ms. Nothing is echoed or logged.
- The command always exits 0 and prints nothing to stdout, so it never changes what the harness
  does; failures go to stderr. Waits are per session; the same session id in two harnesses stays
  independent. Colour pets per harness with `[pets]` (see [pets](pets.md)).
