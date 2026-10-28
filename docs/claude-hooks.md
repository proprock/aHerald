# Claude Code hook adapter

Source: <https://code.claude.com/docs/en/hooks> (read 2026-10-25). Live captures on 2026-10-26
(interactive Claude Code): `PermissionRequest`, `Notification` with `notification_type =
permission_prompt` and `Stop` carried `hook_event_name` and `session_id` and no turn id, as
documented. `idle_prompt` arrived about a minute after `Stop` and showed a bogus input-required card
for an idle session (AH-074), so it is ignored; `StopFailure` is still unobserved; verify against your installed version.
`PermissionRequest` and the `permission_prompt` notification arrived 6 s apart for one prompt.

## Setup with a command

`aherald --install-hooks claude` shows what it would add to `~/.claude/settings.json` (a dry run);
`--apply` writes it with a `settings.json.aherald-backup` copy and leaves every other setting and hook
alone. `--uninstall-hooks claude --apply` removes only aHerald's entries; `--hooks-file PATH` picks
another settings file. The commands refuse a file that is not valid JSON or has an unexpected shape.

## Setup (manual)

aHerald edits Claude Code settings only when you run the command above. To do it by hand, merge the `hooks` object from
`examples/claude-settings.json` into `~/.claude/settings.json` (or a project `.claude/settings.json`)
after replacing the executable path. Remove the entries to uninstall. Disable processing without
touching settings with `[claude] hooks_enabled = false` in `config.toml`.

## Event mapping

| Claude hook | Condition | aHerald event |
| --- | --- | --- |
| `Stop` | always | `turn_complete` |
| `StopFailure` | any `error` (for example `rate_limit`) | `error` |
| `PermissionRequest` | always | `permission_required` |
| `Notification` | `notification_type = permission_prompt` | `permission_required` |
| `Notification` | `elicitation_dialog`, `agent_needs_input` | `input_required` |
| `Notification` | `idle_prompt` | none: the `Stop` card already announced the finished turn |
| `SessionEnd` | any `reason` (`clear`, `logout`, `prompt_input_exit`, ...) | none: clears that session's pending cards |
| `SubagentStop` and other hooks / notification types | | ignored |

Only `hook_event_name`, `session_id` and `notification_type` are read. Prompts, `message`,
`tool_input` and `last_assistant_message` are never stored or logged.

## Contract

The hook prints nothing to stdout and exits 0, so Claude Code's permission flow is unchanged;
aHerald never returns a `PermissionRequest` decision. Invalid or oversized input, a disabled
agent, or an unavailable resident process degrade to silent success (details on stderr).
Select the agent with `--hook --agent claude`; omitting `--agent` keeps the Codex behavior.

## Limitations

- Claude events carry no turn id, so correlation is per `session_id`: `Stop`/`StopFailure`
  resolves that session's pending permission/input cards (never another session's or
  Codex's, and even when those events are disabled), and a repeated pending event of one
  session joins that session's unresolved card at any delay (covers the
  `PermissionRequest` + `Notification` pair, seen 6 s apart). A second prompt in the same
  session shows a new card only after `Stop`/`StopFailure` or dismissal of the first.
- Every new event plays its sound once, even when a persistent card keeps the selection.
- Session names are Codex-only; Claude cards show "Claude" (the name is only delivered to a
  `statusLine` command, tracked in AH-038). Click-to-activate is described in
  `docs/notification-click.md` (Desktop session link or the terminal window; no tab selection).
- `SessionEnd` never shows a card or plays a sound. In resident mode it resolves the pending
  permission/input cards of that `session_id` (so quitting Claude Code at a prompt leaves no
  stale card), even when those events are disabled, and it never starts a resident that is
  not already running. Transient mode ignores it. Payload fields other than `session_id`
  (including `reason`) are not read. Add the `SessionEnd` entry from the example; Claude Code
  allows its hooks 1.5 s unless the entry sets a longer `timeout`, as the example does.
- `SubagentStop` is deliberately ignored: a subagent finishing does not end the user's turn,
  and the main `Stop` follows on its own.

## Quota (opt-in)

Hooks carry no quota. With `[claude.usage] enabled = true`, the resident polls
`GET https://api.anthropic.com/api/oauth/usage` (WinHTTP, header `anthropic-beta: oauth-2025-04-20`)
using the access token from `~/.claude/.credentials.json` (or `CLAUDE_CONFIG_DIR`), every
300 s or more. `five_hour` and `seven_day` `utilization` (percent) and `resets_at` (ISO 8601)
feed the same tray tooltip, menu line and usage card as Codex; the tray icon bars follow Codex
unless Claude is the only enabled agent. The details card mirrors the Codex layout: `Claude plan: <subscriptionType from the credentials
file>`, `5h` and `Week` lines, `Extra usage: enabled|disabled, limit: <used>/<monthly> <currency>`
from `extra_usage` (minor units scaled by `decimal_places`) and `Spend: enabled|disabled,
<used>/<limit> <currency>` from `spend`; missing values show `—`. Reset credits and
"ordinary usage allowed" have no counterpart in the endpoint and are omitted. An expired token shows "open Claude Code to refresh"
because aHerald never refreshes or writes tokens. This endpoint and its response are undocumented
and unverified against a live session; other tools (for example rate-tray) use the same source.
An alternative without tokens is the statusLine `rate_limits` field (tracked in AH-038).

Request safety: redirects are never followed (WinHTTP redirect policy "never"), so the
token cannot be forwarded to another host; any 3xx shows "Claude usage request redirected
(refused)". Proxy settings come from the system (WinHTTP automatic proxy: Windows proxy
settings, WPAD and PAC). After HTTP 429 the next request waits at least
`refresh_interval_sec`, manual refresh included, and at least the `Retry-After` delay
(seconds form only; an HTTP-date is ignored) capped at 15 minutes. A longer
`refresh_interval_sec` still wins over the cap. aHerald zeroes the buffers it owns that
hold the token (file contents, token, request headers) after the request; copies made by
the JSON parser, the standard library or WinHTTP are not covered.

Manual checks (not automated): (1) with `[claude.usage] enabled = true` behind a system
proxy or PAC (Settings > Network > Proxy), restart the resident and confirm the usage card
shows `5h`/`Week` values instead of "Claude usage request failed"; (2) to see the
redirect refusal against a real server, point a hosts entry or test proxy at a 302
response for `/api/oauth/usage` and confirm the card shows the "redirected (refused)"
message and the target receives no follow-up request. The loopback redirect behavior is
covered by the automated `https` test.
