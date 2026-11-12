# Privacy

aHerald shows cards for events your coding agents report. It reads as little as it can,
keeps nothing it does not need, logs no content, and sends nothing over the network unless
you turn on a feature that does. This page lists every place where it reads data that is
not its own, or sends data anywhere, and how to turn each one off.

## Always on

| What | When | What is read or kept | Where it goes | Turn off |
|---|---|---|---|---|
| Hook and bridge payloads (`--hook`, `--notify`) | Every agent event | The event type, the session id and the working directory. Only the folder's last component is kept for the card: control and bidirectional characters are removed and it is capped at 40 characters (`src/events.rs`). Payloads are limited to 64 KiB. | The local resident, over IPC limited to your user (Windows: same user and logon session; macOS/Linux: a socket in a private per-user directory) | Remove the hook or bridge |
| Codex session names | Only with `[codex] session_names = true`; transient mode also requires `transient_names = true` | The thread name, asked read-only from your own `codex app-server` process (`src/session_source.rs`, `src/app_server.rs`). It never resumes a thread or answers a request. | Stays in memory | Set `session_names = false` (the default) |
| Which window and session you are looking at | Before showing a card, to suppress cards for the session in front of you and to bring its terminal forward on a click | The hook's process ancestry. Windows: console window and process snapshot. macOS: the parent chain up to a known terminal application, and whether that application is active. Linux X11: `/proc/<pid>/comm`, `_NET_ACTIVE_WINDOW` and `_NET_WM_PID`. Window contents are never read (`src/focus.rs`, `src/*/focus.rs`, `src/mac/terminal.rs`, `src/linux/activation.rs`). | Stays in memory | — (anything unproven means the card is shown) |
| Claude Desktop sessions | Clicking a card from a Claude Code session that runs inside Claude Desktop | Only the `cliSessionId` field of Desktop's `local_*.json` files (files at most 1 MiB, at most 30 days old), to build the link that opens that session (`src/activation/claude_desktop.rs`). Prompts, titles and every other field are ignored. | Stays in memory | — |
| Operational log (`aherald.log`) | Always, at `[general] logging` level (default `warn`) | Fixed record codes and, with `debug`, timings. Never payloads, prompts, paths, session names or tokens (`src/operational_log.rs`). | A local file, rotated by size | `logging = "off"` |
| Last raw answers for `--raw-output` | Resident with usage or a statusLine wrapper | The last unparsed usage answer and the last statusLine payload | Memory only, never written to disk | — |

## Opt-in (off by default)

| Feature | Switch | What is read | Where it goes |
|---|---|---|---|
| Terminal tab detection (macOS) | `[general] detect_terminal_tab = true` | The selected tab of Terminal or iTerm2, through AppleScript; macOS asks for Automation access (`src/mac/focus.rs`) | Stays in memory |
| Claude statusLine wrapper | `--install-statusline --apply` or the setup option | From Claude Code's statusLine payload: session id, session name, 5-hour and 7-day quota, input context token count (`src/statusline.rs`). Your own status line command receives the payload unchanged; `[claude.statusline] format` renders fields locally to standard output. | The local resident |
| Claude quota | With `[claude.usage] enabled = true`, in `token` mode or in `auto` mode without a usable statusLine quota | Claude Code's OAuth access token from `~/.claude/.credentials.json` (macOS: the login Keychain item `Claude Code-credentials`, behind the system's prompt), read per request into a zeroing buffer and never logged, persisted, refreshed or written back. The non-secret `subscriptionType` label supplies the plan name. `source = "statusline"` uses only local quota reports. | The token goes only to `https://api.anthropic.com/api/oauth/usage` |
| Codex quota | `[codex.usage] enabled = true` | Rate limits, read-only from your own `codex app-server` (`src/usage_service.rs`) | Stays on this machine |
| Session warnings | `[claude.warnings]` / `[codex.warnings]` thresholds | The input context token count (Claude from the statusLine report; Codex from its `PostModelRequest` hook: session id and token count only, `src/codex_context.rs`) and the time since the last turn | Stays on this machine |
| Messenger forwarding | `[[forward.sinks]]` (the tray **Messenger** switch stops it) | Per accepted card: the event kind and the harness label, plus the folder name only with `include_folder = true` and the computer name (or, if it cannot be read, its IP address) only with `include_host = true`. Never a prompt, an assistant message, a path or a session name. Secrets come only from environment variables named by `${NAME}`, are held in zeroing memory and never appear in an error, log or card (`src/forward.rs`, `src/webhook.rs`, `src/command_sink.rs`). | The webhook URL or the command you configured |
| Events from containers and VMs | `[remote] enabled = true` (resident only) | An HTTP listener on `bind` (default `127.0.0.1:47821`) takes hook payloads with a bearer token kept in the owner-only file `remote-token`; only turn ends and waits are used, and at most 64 KiB is read (`src/remote.rs`). No TLS: bind to loopback or a private bridge. Guest senders differ: `aherald-ping` strips the payload to the fields aHerald reads; `examples/sandbox/aherald-ping.sh` and the plugin launcher post it whole, which includes the command awaiting approval in a `PermissionRequest`. | This machine's resident |

## Never

- Answer an agent's permission request, change prompts or tool arguments, or run commands
  taken from a payload. Dismissing a card never answers the agent.
- Read transcripts or observe sessions that do not report through a hook, a bridge,
  `--notify` or the remote listener.
- Read tokens other than the opt-in Claude quota above.
- Send anything to aHerald's authors or any analytics service. There is no telemetry.
- Change agent hooks, settings or autostart entries unless you run an explicit setup
  command or choose the option in an installer (dry run first, backups, a receipt).

## Changing this page

A change that reads new data, touches another application's files, or sends anything to a new
destination must be opt-in and off by default. It must be agreed before implementation and
recorded here in the same pull request (see `AGENTS.md`, "Privacy").
