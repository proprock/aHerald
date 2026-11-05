# aHerald documentation

User guides, grouped by task. Start with the guide for your platform; the
[README](../README.md) gives the overview and [privacy](privacy.md) what aHerald reads and sends.

## Install and run

- [Windows portable guide](portable.md), [macOS](portable-macos.md), [Linux](portable-linux.md)
- [Installation and harness setup](installation.md): installers, `--setup`, removal
- [Demo](demo.md): preview every card with `--demo`

## Configure

- [Configuration](configuration.md): every setting of `config.toml`
- [Card controls and event appearance](card-controls.md)
- [Artwork selection](visuals.md), [animation format](assets.md), [Codex pets](pets.md)
- [Tray usage and restart](tray-usage.md), [session source labels](session-source.md)
- [Notification click activation](notification-click.md), [active originating session](active-session.md)

## Harnesses

- [Claude Code hooks](claude-hooks.md), [status line performance](statusline-performance.md)
- [Codex hooks](codex-hooks.md), [Codex compatibility](codex-compatibility.md),
  [input waits](codex-input-waits.md), [approval observation](codex-approval-observation.md)
- [OpenCode, Pi, Goose and Crush](harnesses.md), [harness compatibility matrix](harness-compatibility.md)

## Containers, VMs and sandboxes

- [Sandboxes](sandboxes.md): the `[remote]` listener and guest senders per environment
- [Instructions for an agent inside a guest](sandbox-setup-for-agents.md)

## Project

- [Privacy register](privacy.md)
- [Windows application icon](app-icon.md)
- [Engineering notes](https://github.com/proprock/aHerald/tree/master/docs/notes): investigations,
  spikes, milestone and per-ticket acceptance records, and design plans. They are history, kept in
  the repository and left out of release archives; the current behaviour is in the guides above.
