# Harness event compatibility (AH-201)

What an external command can learn from each coding harness, checked on 2026-11-03 on
macOS 26 (arm64). "Live" means a real harness process emitted the event to the probe in
`tests/live/harness_probes/`; "documented" means only the vendor's documentation or the
installed package names it. Documented rows are not acceptance evidence.

aHerald maps these to its existing kinds only: `turn_complete`, `permission_required`,
`input_required`, `error`, `interrupted`, plus the `activity` and `ended` lifecycle flags.

| Harness (version) | turn_complete | error | interrupted | permission_required | input_required | session id |
|---|---|---|---|---|---|---|
| OpenCode 2.2.0 | `session.execution.succeeded` (live) | `session.execution.failed` (name in binary) | `session.execution.interrupted` (name in binary) | `permission.asked`, resolved by `permission.replied` / `permission.rejected` (names in binary) | `session.form.*` (names only, meaning unverified) | `data.sessionID` (live) |
| Pi 1.5.0 | `agent_settled` with `aborted: false` (live) | not yet probed (`turn_end` carries `outcome`) | `agent_settled` with `aborted: true` (documented) | none: Pi has no permission event | `ui_prompt_start` / `ui_prompt_end`, kinds select, confirm, input, editor, custom (documented) | `ctx.sessionManager.getSessionId()` (live) |
| Crush | `TurnEnd` | `StopFailure` | `Interrupt` | `PermissionRequest`, `PermissionResult` | none | `session_id` on stdin |
| Goose | `Stop` | none | none | none | none | `session_id` on stdin |
| Aider | unsupported: `--notifications-command` runs with no arguments, stdin or session id, and fires on the next prompt of any kind | | | | | |
| Zero (Twigpine) | unsupported: hooks are `beforeTool`, `afterTool`, `sessionStart`, `sessionEnd`; its stream-JSON events (`final`, `permission_request`, `error`) exist only for headless `zero exec` | | | | | |

Crush and Goose are documented only (hook files with `sh -c` commands and JSON on stdin;
Goose hooks follow the Open Plugins spec, Crush's format is Claude-compatible). They need
hook configuration, not a bridge, once the universal input exists.

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
- Not verified: error, interruption, permission and subagent events (parent linkage of
  child sessions), compaction effects, and what a throwing plugin does to the agent.
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
- Not verified: abort (`aborted: true`), `ui_prompt_*`, compaction, queued follow-ups.

## Repeating the probes

```sh
sh tests/live/harness_probes/run.sh pi        # no account, no network (local fake endpoint)
sh tests/live/harness_probes/run.sh opencode  # one trivial prompt to OpenCode's free hosted model
```

Both use a private `HOME`, bound their run time and remove their files. They are opt-in
and are not part of `cargo test` or CI.
