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
- Not verified: sub-agent (child) sessions (the `task` tool name differs from v1: a call to
  `task` failed as unknown, and `session.created` carries no parent field), the TUI permission
  flow, `session.form.*`, compaction, and what a throwing plugin does to the agent.
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
- Not verified: compaction, queued follow-ups, sub-agents.

## Repeating the probes

```sh
sh tests/live/harness_probes/run.sh pi [ok|error|abort|ui]
sh tests/live/harness_probes/run.sh opencode [ok|error|permission|interrupt|subagent]
```

Pi and OpenCode (except `opencode ok`, which sends one trivial prompt to OpenCode's free
hosted model) talk to a local fake OpenAI endpoint: no account and no billing. The non-`ok`
OpenCode scenarios let OpenCode install `@ai-sdk/openai-compatible` from npm on first use.
`opencode error` takes about 100 s. They use a private `HOME`, bound their run time and remove their files. They are opt-in
and are not part of `cargo test` or CI.
