# aHerald on Linux

The archive contains a GNU libc x86-64 executable, artwork, examples and documentation.
It uses the GNU dynamic loader and system libraries: Cairo, Pango, PangoCairo, GObject
and libpulse-simple. The release is built on Ubuntu 22.04; older systems can require a
newer glibc than they provide. This is a glibc build, not a musl/static binary.
A composited X11 desktop or XWayland is required. GNOME needs an AppIndicator extension
or another StatusNotifierItem host for the tray. Native Wayland is not implemented.

Extract the archive into a directory you can write, keeping `assets/` beside `aherald`.
Run `./aherald --version`, then `./aherald --resident`. From another terminal run
`./aherald --event input_required --resident`; dismiss the card by clicking it. The tray
menu can pause notifications, toggle sound and pop-ups, edit configuration, restart and
exit. `./aherald --status` and `./aherald --quit` also control the resident.

The tray shows usage for the enabled agents and uses two quota bars for its icon. Click
a usage line for details; scroll the card to change pages when it has more than one.
Enable `[codex.usage]` and/or `[claude.usage]` in the configuration to poll quotas;
both are disabled by default. The `tray.graph` and `codex.usage.display` settings select
the icon's data and whether the menu displays remaining or used percentages. Claude's
`source = "statusline"` uses local statusLine reports; token polling is opt-in.

To install hooks, run `./aherald --install-hooks claude` or `codex` for a dry run,
then repeat with `--apply` if the changes are correct. Codex also requires trusting the
commands in `/hooks`. For manual setup, `examples/aherald-*-hook-linux.sh` use
`AHERALD_PATH` if set, otherwise `~/.local/share/aHerald/aherald`; copy the archive there
or edit your hook configuration to point at the chosen installation. Hook setup is never
automatic. Remove hooks with `./aherald --uninstall-hooks claude --apply` or `codex`.

Session warnings require an already running resident: neither context reports nor
activity-only hooks start one. Configure each agent's context and idle thresholds
as described in [configuration.md](configuration.md#session-warnings-ah-100--ah-101).
Codex context reports require the explicit `PostModelRequest` hook. Its dispatch
and input schema were verified with Codex CLI `0.166.0-alpha.5.1` on Windows; the
hook is not documented as a stable general protocol. On Ubuntu 22.04.5 with Xvfb,
Codex CLI 0.166.0 dispatched a real context report through a private test bridge;
this did not verify the installed binding. Other versions need separate checks.
Claude context reports require the
`--statusline` wrapper; Claude Code `2.1.311` was verified on Windows, not in a
Linux real session. Preview `./aherald --install-statusline` before applying it.
These observations are not evidence of installed Linux hook delivery; that live
acceptance is tracked in AH-115. The local IPC protocol is versioned: use matching
hook and resident builds, and restart the resident after updating the executable.
Warning state is in memory and starts fresh after a resident restart.

The offline Linux integration test uses a private D-Bus and an isolated Xvfb display:

```sh
xvfb-run -a -s '-screen 0 1280x1024x24' dbus-run-session -- \
  env AH113_ISOLATED=1 cargo test --test linux_apply -- --ignored --test-threads=1
```

On 2026-11-02 this passed on Ubuntu 22.04.5, Xvfb 21.1.4 and Rust 1.99.1.
It sends synthetic Codex context and Claude statusLine reports over private IPC,
exercises the tray context switch, and waits one minute for idle cards before
clearing them with synthetic activity. It does not start either harness, verify
installed hook dispatch or use an audio device. Those live checks remain AH-115.
The test uses Linux pidfds for owned-process cleanup and requires a kernel that
supports `pidfd_open` and `pidfd_send_signal`.

Alternatively, Claude Code can install the repository's plugin marketplace:

```sh
claude plugin marketplace add proprock/aHerald
claude plugin install aherald@proprock
```

The plugin looks for `$AHERALD_PATH`, then `~/.local/share/aHerald/aherald`, then `PATH`.
Set `AHERALD_PATH` in the environment that starts Claude Code when using another location.
An explicit invalid `AHERALD_PATH` leaves the plugin inactive; it does not select another
executable. Use either the plugin or `--install-hooks claude`, not both. Removing the plugin
is separate from removing manually installed hooks. The marketplace install and a synthetic
event from its installed cache were checked locally; live agent dispatch is still AH-096.

The default `config.toml` and log live beside the executable. The example TOML documents
sound paths and settings. Stop the resident and remove the extracted directory to uninstall;
remove any hooks and startup entry you installed separately.

## Verification matrix

- Ubuntu 22.04.5 GNOME, Wayland/XWayland: overlay and tray checked, audible PulseAudio-compatible
  sound confirmed by the user. Xorg, other desktop environments and multi-monitor behavior
  remain unverified (AH-093); native layer-shell on KDE/wlroots is deferred (AH-095).
- Click-to-activate requires a matching X11 process ancestor; terminal launchers that do not
  expose one are unsupported. A synthetic Claude hook returned focus to the Warp window;
  it did not restore a previously selected tab or pane. `codex-cli 0.166.0` was present
  during these checks; real-agent hooks were not tested.
- Ubuntu 22.04.5, Xvfb 21.1.4 and Codex CLI 0.166.0: one real model request
  reached a private resident through the context-test bridge on 2026-11-02. This
  verifies adapter/IPC queue delivery, not the installed binding or card appearance.
- Ubuntu 22.04.5, Xvfb 21.1.4 and Claude Code 2.1.316: one real print-mode
  response on 2026-11-02 exercised prompt, Stop and SessionEnd through the
  private bridge, produced an idle warning and cleared it at session end. This
  does not verify installed hooks, the statusLine wrapper, appearance or sound.
- Ubuntu 22.04.5 GNOME Wayland/XWayland, 2026-11-02: the user confirmed Apply,
  Apply + Test, tray warning switches, synthetic Codex and Claude context cards,
  configured sound and dismissal, and Claude idle delivery, activity clearing
  and suppression while the idle switch was off. Agent-installed hook dispatch
  and the Claude statusLine wrapper were not part of this check.
