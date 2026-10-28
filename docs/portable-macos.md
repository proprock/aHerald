# aHerald for macOS

aHerald shows a small card when a coding agent (Codex, Claude Code) finishes a turn, needs
permission or input, fails or is interrupted. On macOS it is `aHerald.app`: a menu-bar
program with no Dock icon (`LSUIElement`). Nothing is installed and no administrator rights
are needed. In the release archive this file is also `START-HERE.md`.

Tested on: macOS 26.6 on an Apple Silicon MacBook Pro (M1), Codex.app 26.519, Claude Code
2.1.308 (terminal and Claude Desktop sessions). Intel Macs are not supported: the archive holds an
arm64-only build.

## What is in the archive

| Path | Purpose |
| --- | --- |
| `aHerald.app` | The program. Its `Contents/Resources/assets` holds the default animations. |
| `aHerald.example.toml` | Complete configuration reference with the built-in defaults. |
| `examples/` | Hook scripts and settings for Codex and Claude Code (`*.macos.json`, `*.sh`). |
| `docs/` | Detailed documentation; `docs/configuration.md` is the best next read. |
| `MANIFEST.txt` | Version and SHA-256 of every file. |

Compare the archive with the `.sha256` file next to it: `shasum -a 256 -c aherald-*.zip.sha256`.

## First run

1. Extract the archive and move `aHerald.app` to `/Applications` (any folder works).
2. The app is not signed with a Developer ID and not notarized, so Gatekeeper blocks the
   first start of a downloaded copy. Either right-click the app and choose Open, or remove
   the quarantine mark once:

   ```bash
   xattr -dr com.apple.quarantine /Applications/aHerald.app
   ```

3. Try it without any agent:

   ```bash
   /Applications/aHerald.app/Contents/MacOS/aherald --demo
   ```

   Cards for every event loop on screen. Exit through the menu-bar icon.
4. Show one real card: `/Applications/aHerald.app/Contents/MacOS/aherald --event turn_complete`.

## Where things live

The first start creates `config.toml` in `~/Library/Application Support/aHerald/`, next to
`aherald.log`. Nothing is written inside the app (that would break its signature, and
`/Applications` may be read-only). Relative paths in the configuration resolve from that
folder, so give your own animations and sounds as absolute paths. A running resident keeps
the configuration it started with; restart it from the menu-bar icon after editing.

Running the bare `aherald` program outside an app bundle keeps the portable behavior:
`config.toml` and `assets/` beside the executable.

Opening the app from Finder or `open` starts nothing visible until an event arrives; use the
menu-bar icon to reach settings, usage and quit. The command line is the same as on Windows;
see `aherald --help`.

## Hooks

aHerald never edits agent configuration. Add the hooks yourself:

1. Copy `examples/aherald-codex-hook.sh` and `examples/aherald-claude-hook.sh` somewhere
   stable, keep them executable, and fix the app path inside if it is not in `/Applications`.
2. Codex: merge `examples/hooks.macos.json` into `~/.codex/hooks.json`, with the script's
   absolute path. Start a new Codex session, open `/hooks` and trust the command.
3. Claude Code: merge `examples/claude-settings.macos.json` into `~/.claude/settings.json`,
   again with the absolute script path.

Remove them by deleting those entries. See `docs/codex-hooks.md` and `docs/claude-hooks.md`.

## Click-to-activate and quota

- Clicking a card brings the session's terminal or Claude Desktop/Codex.app window forward.
  macOS may ask for permission the first time.
- Claude quota (`[claude.usage]`, off by default) reads the Claude Code login from the
  Keychain item `Claude Code-credentials`; macOS shows its own access prompt. The token is
  sent only to `api.anthropic.com` and never stored or logged.

## Autostart and removal

To start at login, add `aHerald.app` under System Settings, General, Login Items. aHerald
never adds itself.

To remove it: quit from the menu-bar icon, delete `aHerald.app`, delete
`~/Library/Application Support/aHerald`, remove the hook entries and any Login Item.

## Limitations

- Not signed with a Developer ID and not notarized (see First run).
- Apple Silicon only.
- No Dock icon or window by design; there is no installer.
- Linux and Windows behavior is documented in `docs/portable.md`.
