# Notifications from containers, VMs and sandboxes

aHerald runs on your machine (the host). When the agent runs on the host, even inside the native
sandbox of Claude Code or Codex, hooks already run on the host and nothing here is needed. When the
agent itself runs inside a container or a virtual machine, its hooks cannot reach the resident's
same-user pipe, so the guest sends the hook payload over HTTP to an opt-in listener.

```
agent (guest) -> aherald-ping / aherald-ping.sh --HTTP POST--> host:47821 -> resident -> card
```

## 1. Host: turn the listener on

In `config.toml`:

```toml
[remote]
enabled = true
bind = "127.0.0.1:47821"   # the address the guest can reach; see the recipes
```

Run the resident (`aherald --resident`, or the tray) and print the settings for the guest:

```
$ aherald --remote-info
AHERALD_ENDPOINT=http://127.0.0.1:47821
AHERALD_TOKEN=...
```

Replace the host in the endpoint with the address the guest uses to reach the host (recipes below).
There is no autostart: a resident must be running. The token is a file (`remote-token` beside
`config.toml`, readable by you only); treat it like a password, and never put it in an image.

## 2. Guest: pick a sender

| | `aherald-ping` | `aherald-ping.sh` |
|---|---|---|
| Needs | nothing (static binary) | `sh`, `curl`, `head` |
| Drops tool input, prompts, transcript path, full cwd | yes | no: a `PermissionRequest` carries the command awaiting approval, in plain text |
| Drops unwanted events itself | yes | register only the five events (example files) |
| Where | release asset `aherald-ping-<version>-linux-<arch>-musl` | `examples/sandbox/aherald-ping.sh` |

Prefer `aherald-ping` wherever you can copy a binary in. Set `AHERALD_ENDPOINT` and `AHERALD_TOKEN`
in the guest's environment, check with `aherald-ping --self-test` (prints `ok:` and makes no card),
and register the hooks:

- Claude Code: merge `examples/sandbox/claude-settings.json` into `~/.claude/settings.json`.
- Codex: merge `examples/sandbox/codex-hooks.json` into `$CODEX_HOME/hooks.json`.

Both files call `/usr/local/bin/aherald-ping.sh`; to use the binary instead, change the command to
`/usr/local/bin/aherald-ping --hook --agent claude` (or `codex`). Or paste
[the setup instructions](sandbox-setup-for-agents.md) to the agent inside the guest.

A hook never fails or delays the agent for more than about a second: a missing resident, a wrong
token or an unreachable host all end in a silent exit 0.

## 3. Recipes

Status: **verified** means it was run for this document; **upstream docs** means the address or
policy comes from the vendor's documentation and was not run here.

| Environment | Address the guest uses | Status |
|---|---|---|
| Docker Engine on Linux | `http://172.17.0.1:47821`; `bind = "172.17.0.1:47821"` (the `docker0` bridge) | **verified** with an Alpine container |
| bubblewrap sharing the network | `http://127.0.0.1:47821` | **verified**: `ok:` from inside `bwrap` |
| bubblewrap with `--unshare-net` | none: the guest has no route to the host | **verified** it cannot connect; use a proxy or leave the network shared |
| Docker Desktop (macOS, Windows) | `http://host.docker.internal:47821`; `bind = "127.0.0.1:47821"` | not run here |
| Podman, rootless | `--network=pasta:--map-host-loopback,<addr>` and use `<addr>`; behaviour depends on the Podman version | upstream docs |
| Docker Sandboxes (microVM) | cannot reach host localhost directly; needs a sandbox policy rule for the port, and the sender must go through the sandbox proxy, which `aherald-ping` does not support | upstream docs; not supported |
| Lima | `http://host.lima.internal:47821` | not run here |
| coop (Trail of Bits), Firecracker | `http://172.16.0.1:47821`; the bridge exists only while a VM is up, so the listener retries its bind every 30 s | upstream docs |
| coop, isolated mode | no network at all; only an `ssh -R` forward can work | not supported |
| Anthropic `srt`, nono | depends on the sandbox's network and socket policy; allow the port | not run here |

Docker on Linux, end to end:

```sh
# host: [remote] bind = "172.17.0.1:47821"; resident running
docker run --rm -it \
  -v /path/to/aherald-ping:/usr/local/bin/aherald-ping:ro \
  -e AHERALD_ENDPOINT=http://172.17.0.1:47821 -e AHERALD_TOKEN="$TOKEN" \
  my-image aherald-ping --self-test
```

On macOS and Windows hosts a bind that is not loopback may raise an operating system firewall
prompt. Whatever the bind, limit who can reach the port with a firewall rule.

## 4. What crosses and what does not

- Taken: `Stop`, `StopFailure`, `PermissionRequest`, `Notification` (permission and input prompts),
  `SessionEnd`; for Codex `Stop`, `PermissionRequest`, `Interrupt`, `SessionEnd`. Tool and prompt
  activity stay in the guest, so context and idle warnings do not work for guest sessions.
- The host parses the payload itself: a guest cannot name a window or terminal of your machine, and
  a card from a guest is never hidden because you are "watching" a session.
- No TLS and no per-guest tokens. A guest holding the token can raise cards for any session id.
  Duplicates within 1.5 s collapse, and the listener caps requests at 64 KiB, one second and eight
  connections.
