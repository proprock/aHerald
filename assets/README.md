# Original sample assets

All PNG artwork, animation metadata and PCM WAV tones in this directory were
created specifically for aHerald, from the deterministic Rust generator
`examples/generate_assets.rs`. They contain no third-party artwork or recordings.
They are distributed under the repository MIT license; see ../LICENSE.

Regenerate with `cargo run --example generate_assets`. Each event has eight
64x64 RGBA pulse frames in a row-major sprite sheet and a short mono 16-bit PCM
22050 Hz synthesized tone. No resource is embedded in the executable.

Cargo stages this directory beside its application executable. Runtime builtin
artwork loads from that adjacent `assets/` directory automatically; include it
when moving/distributing `aherald.exe`. Sounds need explicit WAV paths in config.
