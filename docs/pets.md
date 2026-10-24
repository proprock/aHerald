# Codex pet assets

Use a downloaded local package containing `pet.json` and its PNG/WebP sprite sheet:

```toml
[notifications]
pet = 'C:\Users\you\.codex\pets\becky--natewanggg\pet.json'
scale = 1.0
interpolation = 'bilinear'
```

Set the actual absolute path, or a path relative to the configuration file. Environment
variables and `~` are not expanded. See `examples/pet.toml` for a relative example.
Loading the package neither starts nor changes Codex. Node.js, npm, network access,
and a Codex restart are unnecessary for aHerald playback. Download/install resources
separately if needed; aHerald does not interpret a configuration value as a command.

An explicit `[events.NAME].animation` overrides the shared pet for that event.
An invalid selected animation or pet produces the existing text fallback. Sound,
fade, scale, placement, click and duration settings still apply. The filename in
`spritesheetPath` is relative to the manifest directory and must remain within it,
including after filesystem link resolution. The default filename is `spritesheet.webp`.

## Supported package contract

- Standard v1: 1536x1872 atlas, eight columns and nine rows; version omitted or 1.
- Standard v2: 1536x2288 atlas, eight columns and eleven rows; `spriteVersionNumber: 2`.
- Every cell is 192x208. The v2 directional rows are accepted but not animated here.
- PNG and WebP are recognized from their contents, not merely the filename extension.
- Metadata is bounded to 16 KiB, encoded image to 20 MiB, decode allocation to 32 MiB.
  Dimensions must match the declared version before full decoding. Only occupied
  cells for the chosen state are extracted; scaled frames retain the engine limits.
- Unrecognized metadata fields are tolerated. Other versions, custom `frame` layouts,
  and nonempty custom `animations` are unsupported and use text fallback.

| aHerald event | Pet state | Zero-based row | Occupied frames |
| --- | --- | --- | --- |
| turn_complete | waving | 3 | 4 |
| permission_required | review | 8 | 6 |
| input_required | waiting | 6 | 6 |
| error | failed | 5 | 8 |
| interrupted | idle | 0 | 6 |

Frame durations follow the inspected standard tracks. aHerald loops the selected
notification state for its configured duration; this is aHerald's policy, not an
implementation of Codex's chat-status transitions, directional movement, or controls.

## Sources and version boundary

Checked 2026-10-24:

- [OpenAI standard package contract](https://github.com/openai/skills/blob/main/skills/.curated/hatch-pet/references/codex-pet-contract.md).
- [Codex frame catalog](https://github.com/openai/codex/blob/970111f6cb62cc02fe36a82ea7d71b37a2889c97/codex-rs/tui/src/pets/catalog.rs)
  and [track definitions](https://github.com/openai/codex/blob/970111f6cb62cc02fe36a82ea7d71b37a2889c97/codex-rs/tui/src/pets/model.rs).
  The latter was fetched at this exact revision to check row/count/timing data.
  This is a source snapshot, not a claim of full compatibility with any installed
  Codex release or a stable agent-observation API.
- [Awesome Codex Pet v1/v2 package contract](https://github.com/legeling/awesome-codex-pet/blob/main/docs/en/CONTRIBUTING.md).
- [Becky manifest](https://github.com/legeling/awesome-codex-pet/blob/main/pets/becky--natewanggg/pet.json).

The implementation consumes local artwork only; it does not use Codex authentication,
app-server, transcripts, hooks, or process state. Existing hook version notes remain
unchanged in `docs/codex-hooks.md`.

## Verification

Tests generate original synthetic PNG/WebP atlases, inspect extracted RGBA pixels,
track counts/timing and premultiplied alpha, and exercise both v1/v2 dimensions,
path escape, malformed/oversized/missing resources, precedence and relative paths.
A Windows integration test observes a real frame-sized pet window, click dismissal,
and recovery to text after corrupting its owned WebP fixture.

Red: the configuration test rejected the new field and the loader test failed against
the stub. Green: all seven feature tests pass. The full suite has 40 passing tests
and one ignored opt-in audio-device test. Formatting, Clippy and the MSVC release build pass.
The release packaging check passes: x64 Windows GUI, 1,483,264 bytes, static CRT, and only system DLL imports.

Manual Windows 11 build 22621: Becky was downloaded into an isolated temporary demo
folder, not installed into Codex. The user confirmed seeing the animated cat with a
transparent background. Third-party Becky artwork is not bundled or relicensed;
ordinary tests use only the synthetic fixtures. Windows 10 and other display setups
remain untested as recorded in the milestone report.

WebP decoding is enabled in the existing `image` crate with `image-webp` 0.2.4
(MIT OR Apache-2.0; MSRV 1.80.1, within the project's declared Rust 1.88 baseline).
The decoder is pure Rust and adds no runtime DLL dependency. The user approved this
feature expansion. Package maintenance/versions are locked in Cargo.lock.
