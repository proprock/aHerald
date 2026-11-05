# Setting up aHerald notifications inside a container or VM (instructions for an agent)

Paste this to the coding agent running **inside** the guest, after replacing the two values, or
follow it by hand. Give the token through an environment variable rather than the chat if you can.

---

You are inside a container or VM. Make your hook events reach aHerald on the host.

1. Check what is available: `uname -m` (x86_64 or aarch64), `command -v curl`.
2. Environment, in this shell profile and in the environment that starts the agent:
   `AHERALD_ENDPOINT=<http://host:port from the user>` and `AHERALD_TOKEN=<token from the user>`.
3. Sender:
   - If the user gave you `aherald-ping` for this architecture, install it as
     `~/.local/bin/aherald-ping` (`chmod +x`), and verify its SHA-256 against the checksum the user
     gave you. Use `aherald-ping --hook --agent <claude|codex>` as the hook command below.
   - Otherwise install `examples/sandbox/aherald-ping.sh` as `~/.local/bin/aherald-ping.sh` and use
     `~/.local/bin/aherald-ping.sh <claude|codex>` as the hook command. It needs `curl`.
4. Register hooks, merging into existing settings and keeping the user's other hooks:
   - Claude Code, `~/.claude/settings.json`: events `Notification`, `PermissionRequest`, `Stop`,
     `StopFailure`, `SessionEnd`, each `{"hooks":[{"type":"command","command":"<hook command>","timeout":3}]}`.
   - Codex, `$CODEX_HOME/hooks.json` (default `~/.codex/hooks.json`): events `Stop`,
     `PermissionRequest`, `Interrupt`, `SessionEnd`, same shape.
   Do not register `PreToolUse`, `PostToolUse` or `UserPromptSubmit`.
5. Verify: with `aherald-ping`, run `aherald-ping --self-test`; it must print `ok:`. With the script,
   `curl -s -o /dev/null -w '%{http_code}\n' -X POST -H "Authorization: Bearer $AHERALD_TOKEN" "$AHERALD_ENDPOINT/v1/ping"` must print 200.
   If it fails: refused or timed out means the host address, bind or firewall is wrong or the
   resident is not running; "refused the token" means a wrong token.
6. Report what you changed. Do not print the token back.
