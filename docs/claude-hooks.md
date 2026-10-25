# Claude Code hook adapter

Source: <https://code.claude.com/docs/en/hooks> (read 2026-10-25). Payloads below were taken from the
documentation, not yet captured from a live Claude Code session; verify against your installed version.

## Setup (manual)

aHerald never edits Claude Code settings. Merge the `hooks` object from
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
| `Notification` | `idle_prompt`, `elicitation_dialog`, `agent_needs_input` | `input_required` |
| other hooks / notification types | | ignored |

Only `hook_event_name`, `session_id` and `notification_type` are read. Prompts, `message`,
`tool_input` and `last_assistant_message` are never stored or logged.

## Contract

The hook prints nothing to stdout and exits 0, so Claude Code's permission flow is unchanged;
aHerald never returns a `PermissionRequest` decision. Invalid or oversized input, a disabled
agent, or an unavailable resident process degrade to silent success (details on stderr).
Select the agent with `--hook --agent claude`; omitting `--agent` keeps the Codex behavior.

## Limitations

- Claude events carry no turn id, so Codex-style duplicate/terminal correlation does not apply.
- Click-to-activate and session names are Codex-only; Claude cards show "Claude".
- Subagent events (`SubagentStop`) and `SessionEnd` are not covered yet.

## Quota (opt-in)

Hooks carry no quota. With `[claude_usage] enabled = true`, the resident polls
`GET https://api.anthropic.com/api/oauth/usage` (WinHTTP, header `anthropic-beta: oauth-2025-04-20`)
using the access token from `~/.claude/.credentials.json` (or `CLAUDE_CONFIG_DIR`), every
300 s or more. `five_hour` and `seven_day` `utilization` (percent) and `resets_at` (ISO 8601)
feed the same tray tooltip, menu line and usage card as Codex; the tray icon bars follow Codex
unless Claude is the only enabled agent. An expired token shows "open Claude Code to refresh"
because aHerald never refreshes or writes tokens. This endpoint and its response are undocumented
and unverified against a live session; other tools (for example rate-tray) use the same source.
An alternative without tokens is the statusLine `rate_limits` field (tracked in AH-038).
