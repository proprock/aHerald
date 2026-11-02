# Windows application icon (AH-105)

The static application emblem uses the existing `tray.graph = "both"` geometry
with fixed synthetic quota levels. It contains no account data and does not
change with the resident tray icon. The artwork is original project artwork,
licensed under the repository's MIT license.

`resources/windows/aherald.ico` contains separate 32-bit RGBA PNG frames at
16, 20, 24, 32, 40, 48, 64, 96, 128 and 256 pixels. The 20/24/40/48 frames
also cover common 125%/150% icon sizes without upscaling a smaller frame.
`aherald.png` is the 256-pixel preview. Rebuild both files from the repository
root with `cargo run --example generate_app_icon`; the generator reuses the
tray renderer and the existing PNG encoder.

The build script compiles `aherald.rc` only for Windows targets and links the
resource only into `aherald.exe`. The approved MIT build dependency
`embed-resource` 3.0.12 discovers the Windows SDK resource compiler. Its declared
MSRV is 1.76, below this project's Rust 1.88; the actual build was tested with
Rust 1.98.1. The icon is embedded, so distribution does not require the ICO file.

## Verification on 2026-11-01

Environment: Windows 11 build 22621, Rust 1.98.1, Windows x64 MSVC.

- Red: `cargo test --test app_icon` failed because the executable had no resource
  section before the resource was added.
- Green: the same test loads the built executable as data (without running it),
  loads resource ID 1 at all ten sizes, and checks the native icon bitmap's
  dimensions and 32-bit color depth. Owned native resources are released by guards.
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings` and
  `cargo test --all-targets --no-fail-fast` passed.
- `cargo build --release --target x86_64-pc-windows-msvc` and
  `cargo test --release --target x86_64-pc-windows-msvc --test app_icon` passed;
  the latter loads the actual release executable at all ten sizes.
- User-supplied screenshots confirm the icon in Explorer's file list and in the
  executable Properties dialog, including its small title-bar and larger preview.
  These screenshots do not identify the executable profile or display scaling.

AH-105 is complete as agreed on 2026-11-01. The embedded multi-size resource,
successful native loading from the release executable at all ten sizes, and
Explorer screenshots satisfy acceptance. Windows handles DPI-dependent resource
selection and display; changing system display scaling is not an additional
acceptance requirement for this task. No further user action is required.
