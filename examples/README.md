# Focused examples

The complete configuration reference lives at [`aHerald.example.toml`](../aHerald.example.toml)
in the repository root. It documents every setting and uses built-in default values.
These files serve separate scenarios and do not load automatically or combine together:

| File | Purpose |
| --- | --- |
| `resources.toml` | Runnable demo preset with automatic builtin/pet artwork, scale 3, bundled WAV paths and volume 0.5. Also used by the opt-in native demo preview test. |
| `pet.toml` | Minimal pet setup and per-event state overrides. Requires a separately downloaded pet package and a corrected local path; disables sound. |
| `hooks.windows.json` | Manual Codex hook integration example. Replace executable/configuration paths before merging into your hook configuration. This is not an aHerald TOML config. |
| `generate_assets.rs` | Developer utility that regenerates the original bundled PNG/WAV assets; run with `cargo run --example generate_assets`. Not a user configuration or runtime dependency. |

Choose one TOML file with `--config PATH`. Settings omitted from a preset use built-in
defaults. Relative paths resolve from that selected file's directory; when copying a
preset elsewhere, adjust its paths. Nothing in this directory installs hooks, resources,
user configuration or startup entries automatically.

See [artwork selection](../docs/visuals.md) for full builtin, full pet and hybrid
setups. Adding a pet to `resources.toml` switches all events to it automatically.
