# Portable configuration

Without `--config`, aHerald uses exactly `config.toml` in the directory containing
the running `aherald.exe`. It never searches the working directory or APPDATA.
Normal event, hook, resident and demo startup creates an absent standard file
with the current defaults, including documented options, from
`config.example.toml`. Help/status/quit do not create configuration files.
Existing standard files are never replaced, merged or upgraded automatically.

The file is written completely to a private file in the same directory and
published with an exclusive rename. Concurrent initial launches observe a complete
file; a file created meanwhile wins. A write failure reports the target path and
OS reason. The installation directory must be writable to create the first file;
otherwise prepare the file before installation or select a writable custom file.
In hook mode errors still honor the successful, notification-only hook contract
and report the operational problem on stderr.

`--config PATH` retains its previous meaning: use that file, fail if it is missing
or invalid, and never create it. Relative explicit paths remain relative to the
launch working directory. Relative resource paths (pet manifests, animations and
sounds) are resolved from the chosen configuration file's directory, for both
default and custom files. No environment-variable or tilde expansion is performed.

Default hook invocation is `aherald.exe --hook --resident`. The PowerShell wrapper
in `examples/hooks.windows.json` retains `| Out-String` for GUI executable response
capture. Add `--config` only when using a custom file; no automatic hook edits occur.

## Manual migration

### Active originating session (AH-098)

`[notifications] suppress_when_focused = true` and
`[sounds] suppress_when_focused = true` are independent and default on, including
in existing files that omit them. Set either to `false` to retain that output.
Suppression requires positive window **and** active-session identification;
unknown identity delivers both outputs. A suppressed card never appears later
after a focus change. Lifecycle handling and deduplication continue.

Current positive identification is limited to Claude hooks in a classic Windows
console, and on macOS (opt-in) to Claude hooks in the selected tab of Terminal.app or
iTerm2. Desktop apps, Codex and tabbed terminals retain notifications. See the
[coverage matrix and manual checks](active-session.md) before relying on suppression.

### Session warnings (AH-100 / AH-101)

Idle reminders use 55 minutes for Claude and 25 minutes for Codex once enabled. Both warning
types are off until enabled: `idle_enabled` defaults to `false` (also when the key is omitted)
and a context warning needs a threshold. Setup and the installer turn them on per agent.
Context warnings remain off until a threshold is configured. For example:

```toml
[claude.warnings]
context_enabled = true
idle_enabled = true
context_tokens = "400k"
idle_minutes = 55

[codex.warnings]
context_enabled = true
idle_enabled = true
context_tokens = "250k"
idle_minutes = 25
```

`context_tokens` accepts positive integer tokens, an integer string or an integer
string ending in lowercase `k` (`k = 1000`). Fractions, signs, zero and overflow
are rejected. Each agent's threshold is independent and omitted by default (off).
`context_enabled` defaults to `true` (it only matters once a threshold exists) and
`idle_enabled` to `false`; set either to `false` to disable that warning while retaining its
threshold. The Windows installer uses
these flags. When first enabled there without a context threshold, it supplies
250k tokens for Codex or 400k for Claude; existing thresholds are preserved.
Codex requires the `PostModelRequest` binding, verified with CLI
`0.166.0-alpha.5.1`; rerun explicit hook setup to add it to an older installation.
Other versions and desktop dispatch need separate verification. Codex uses the
successful main request's `input_tokens`, which already includes cached input.
On Linux, the explicit opt-in probe verified Codex CLI 0.166.0's real model
request through a private bridge and resident; installed hook dispatch remains
to be checked. See [Linux verification](portable-linux.md#verification-matrix).
Claude input context includes fresh input, cache creation and cache reads, each counted once. It is the
last reported API input, not a prediction of the next request. Missing readings
do not count as zero. A crossing strictly above the threshold queues one
`context_warning`; a known reading at/below the threshold rearms it.
The card shows the observed value and threshold, for example
`Context: 250.001k > 250k`. Kilotokens use `k = 1000`, with enough fractional
digits to preserve the exact token count. The harness is already in the heading.

`idle_minutes` accepts 1..525600 minutes, or `0` to disable only that agent's idle
reminder. Omission keeps the agent's default, including in a partial warnings
table. The timer starts at a main-session Stop
boundary (end of an agent response). A new user submission or tool activity
cancels it; the next completion starts a new episode. Error, interruption and
session end cancel it too. Each episode queues at most one `idle_warning`,
strictly after the threshold, on the next presentation-loop tick (about one
second while idle on Windows/macOS). Agent and session identities are isolated;
subagent evidence does not reset or start the main session's timer. This is an
elapsed-time reminder, not proof that the provider's cache expired. Another Stop
hook can continue work: observable tool activity cancels the provisional timer,
but pure model continuation and delayed hook delivery cannot be perfectly
distinguished by these lifecycle hooks.
The card shows elapsed time and that agent's limit, for example
`Idle: 27m 30s (limit 25m)`. Both warning types keep the values measured when the
warning was created; they do not advance while waiting in the notification queue.

### Messenger forwarding (AH-213)

A *sink* sends each card the queue accepted as new to a messenger, in addition to the card. The
one sink type is `webhook`, an HTTP POST that serves Telegram, ntfy, Slack, Discord and anything
else that takes a request. Nothing is sent unless you configure a sink.

```toml
[forward]
enabled = true            # initial state of the tray "Messenger" switch

[[forward.sinks]]
type = "webhook"
kinds = ["turn_complete", "permission_required", "input_required", "error"]  # the default
include_folder = false    # the default; see below
url = "https://ntfy.sh/${AHERALD_NTFY_TOPIC}"
```

- **What is sent:** a line such as `Claude: input required`, with the folder name in brackets only
  when `include_folder = true`. Never a prompt, an assistant message, a path or a session name. The
  folder name goes to a third party, which is why it is off by default.
- **Secrets are never in `config.toml`.** Write `${NAME}` where a secret belongs; aHerald replaces
  it with the environment variable `NAME` when it starts. A literal `token` key is refused at load
  without being echoed. A missing or empty variable shows a card naming the variable, not its value.
  A resident started from the Windows Run key or the installer sees only your *user* environment
  variables, so set them with `setx` or in System Properties and restart aHerald.
- **Filter:** `kinds` is independent of `notifications.suppress_when_focused`, which concerns the
  screen in front of you. aHerald does not detect that you are away from the computer.
- **Tray switch:** when a sink is configured, the tray menu has a **Messenger** item beside Sound and
  Pop-up. It switches all sinks off and on until you exit; the next start uses `forward.enabled`.
  (Windows only so far; AH-219 covers the macOS and Linux menus.)
- **Delivery:** each sink has its own worker, a queue of 32 messages (the oldest is dropped when
  full) and three tries. A slow or unreachable messenger never delays cards, the tray or hooks. A
  failure is shown as a card once for each distinct error, never with the token.

#### Webhook sink

`type = "webhook"` POSTs a body template to a URL.

**Telegram.** Create a bot with @BotFather and copy its token. Send the bot any message, then open
`https://api.telegram.org/bot<token>/getUpdates` and read `message.chat.id` (a number; a group id
is negative). Set the token in the user environment, then:

```toml
[[forward.sinks]]
type = "webhook"
kinds = ["permission_required", "input_required"]
url = "https://api.telegram.org/bot${AHERALD_TELEGRAM_TOKEN}/sendMessage"
content_type = "application/json"
body = '{"chat_id": "123456789", "text": "{text}"}'
```

**ntfy** needs no body or content type (the default is the plain message text):

```toml
[[forward.sinks]]
type = "webhook"
url = "https://ntfy.sh/${AHERALD_NTFY_TOPIC}"
```

- **`${NAME}`** (in `url` and `body`) is replaced when aHerald starts by the environment variable
  `NAME`. Use it for every secret: the token, a secret topic, a webhook address. The value is
  scrubbed from every error and card. A missing or empty variable shows a card naming the
  variable. Put no secret literally in `url` or `body`: aHerald does not check for it.
- **`{text}`, `{kind}`, `{harness}`, `{folder}`** (in `body` only) are replaced at each send.
  `{text}` is the message line, for example `Claude: input required`; `{folder}` is empty unless
  `include_folder = true`. They are filled in one pass: a value that contains `{kind}` is not
  expanded again, and other braces (JSON) stay as written.
- **`content_type`** defaults to `text/plain; charset=utf-8` and also chooses how the values are
  made safe, because a folder name can hold any character:
  a JSON type (`application/json`, `…+json`) escapes them to sit inside a JSON string, so write your
  own quotes around the placeholder (`"text": "{text}"`);
  `application/x-www-form-urlencoded` percent-encodes them; any other type inserts them as written,
  so do not use a JSON or form body with such a type.
- `url` is `https://` (plain `http://` works but sends everything in the clear). A 2xx answer is a
  success; anything else is an error card with the HTTP status.

### Claude status line text

`[claude.statusline] format` is the text `aherald --statusline` prints when you have no status
line command of your own (empty by default: nothing is printed). If you have one, native setup
wraps it as before and the format is ignored. Example, with the fields Claude Code documents at
<https://code.claude.com/docs/en/statusline>:

```toml
[claude.statusline]
format = "{context_window} · {rate_limits.five_hour} · {rate_limits.seven_day} · {model.display_name}{[ {effort.level}]} · {workspace.repo.name}{[ · {worktree.branch}]}{[ · #{pr.number}]} · +{cost.total_lines_added} -{cost.total_lines_removed} · {cost.total_cost_usd}"
```

- `{a.b.c}` inserts the JSON field at that path. Strings, numbers and booleans are inserted
  as they are; a missing, null, object or array value is empty. `workspace.repo` is an object (`host`, `owner`, `name`), so write `{workspace.repo.name}`.
  A field ending in `_usd` is shown as dollars (`$1.23`) and one ending in `_duration_ms` as a
  span in whole minutes (`1:12`, `12 min`, `2d 3:05`).
  A field whose name ends in `_percentage` is rounded and gets its sign (`24%`; write no `%`).
  Whole objects `{context_window}`, `{rate_limits.five_hour}`, `{rate_limits.seven_day}` and
  `{rate_limits.spend_limit}` render as the tray menu line does (`ctx 9% used`, `5h 79% / 3:29`,
  `7d 96% / 4d 19:00`): the context follows `[claude.statusline] display_ctx` (`used` by default;
  it names its choice, `ctx 91% remain`) and the quotas follow `display_usage` (`remain` by
  default, as in the menu; their time says what is left). With
  `[claude.statusline] color = true` (default), the label, number and time or word are coloured; the
  number uses the tray icon's thresholds on remaining capacity (green above 60%, amber above 20%,
  red below). A segment with missing data is empty, so put it in `{[ ... ]}`.
  A field named `resets_at` is shown as the time left in the usage card's form: `3d 5:10`, `4:05`, `15 min`.
  Token counts (names ending in `tokens`, and `context_window_size`) are shortened to `18k`,
  `18.6M`; other numbers are inserted unchanged.
  `config.example.toml` lists every field with its availability.
- `{{` and `}}` are literal braces. `{[ ... ]}` is an optional group, dropped whole when a field
  written directly inside it is empty, so separators disappear with their value (`worktree`,
  `workspace.repo`, `effort` and `rate_limits` are absent in many sessions).
- The template may contain ANSI colour escapes; control characters in inserted values become
  spaces, a value is cut at 256 characters and the line at 4096 bytes.
- A mistake in the template fails configuration loading with the reason; the wrapper itself
  never fails and prints nothing if the configuration cannot be read. It never creates it.
- Native setup installs the wrapper when Claude Code is selected and either a Claude context
  threshold or a format is set. Run setup again after first adding a format.

Use resident mode. The Claude context warning additionally needs the `--statusline`
wrapper, installed automatically by native setup when Claude Code and Claude
context warnings are selected; no OAuth access is needed. Both idle integrations
need UserPromptSubmit, PreToolUse, PostToolUse, Stop and SessionEnd; retain the
existing PermissionRequest and agent-specific error/interruption hooks. Preview
the updated installer with `aherald --install-hooks claude` or `codex`, inspect
the diff, then explicitly run with `--apply` if desired. Existing files are never
upgraded automatically; review/trust changed hooks in the agent normally.

State is memory-only and bounded to 64 recently observed sessions. Restart waits
for fresh evidence. A paused resident postpones idle delivery until resumed.
An overdue deadline is checked after the machine resumes if the OS monotonic
clock includes the suspended interval; no wake-from-sleep or cross-platform
suspend-time guarantee is provided. When the notification queue is full, a bounded
pending warning is retried on native-loop ticks; `queue_rejected` is logged once
per blocked warning. Activity/session end cancels stale pending idle warnings too.

Warning cards keep the originating agent/session. Their settings are
`[events.context_warning]` and `[events.idle_warning]`, using the ordinary event
settings (default duration 3000 ms, amber accent, no configured sound). Builtin
artwork reuses the permission/input pictures; normal pet/custom overrides work.
Preview with `--event context_warning` or `--event idle_warning`; the standard
`--demo` keeps its original five-event sequence.

**Upgrade:** this build uses IPC frame version 2 so new evidence-only activity
events cannot become false completion cards in older residents. Quit the old
resident with its old executable before replacing it, then start the new build.
Mixed versions reject frames safely; a new executable cannot send `--quit` to a
version-1 resident. The tray Quit action remains available.

### Portable path migration

Quit the resident. Copy your old `%APPDATA%\aHerald\config.toml` to `config.toml`
beside the installed executable, preserving any existing destination you want to
keep. Copy relative resources with their directory layout, or adjust their paths.
Restart aHerald, remove `--config` from hooks that should use the standard file,
and review/trust changed hook commands through Codex's normal workflow. Alternatively
keep the old file and pass its explicit path. aHerald neither reads nor moves it
automatically; there is no legacy-location fallback.

## Verification (2026-10-25)

Tests execute a copied Windows binary from another working directory, under paths
with spaces and Unicode. Invalid working-directory and APPDATA configurations are
ignored. Creation, preservation, eight concurrent first loads, explicit missing
files, write failures with path/reason and relative resources are covered. Hook
responses and bounded completion remain covered by the existing process suite.
Formatting, Clippy, all-target tests and a Windows x64 release build are required.
The earlier real-card, queue-navigation, restart and chat-transition acceptance
is recorded in tray-usage.md and notification-click.md. No user hook file or old
configuration was migrated as part of this implementation.

## Errors and diagnostics

An invalid file is reported as `invalid configuration: <table.key> <reason>` (or
`invalid TOML syntax at line N`); the message never contains the offending value. The
tray **Apply** command (**Restart** on older macOS builds) validates the file first. If it is invalid, the running
resident keeps its current configuration and shows the error on the details card.

`aherald.log` (beside the configuration, 64 KiB with one backup) records fixed codes
only, never payloads: `resident_started`, `resident_stopped`, `resident_startup_failed`,
`invalid_configuration`, `render_failed`, `render_fallback`, `usage_failed` (once per
failing episode), `ipc_failure`, `queue_rejected`, `agent_activation_failed`, `hook.delivery_failed` (a hook could not hand its
event to the resident) and `artwork.reduced` (artwork over the memory budget at the monitor's
scale is drawn at the configured scale; once per scale).
Invalid-configuration and startup codes are written at `warn` level regardless of
`general.logging`, because that setting cannot be read from an invalid file.
