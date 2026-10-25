# External animation format

Set an event's `visual = "custom"` and `animation` to a directory, relative to the configuration file.
The directory contains `animation.toml` and PNG images. Assets remain external.

```toml
type = "spritesheet"
image = "sheet.png"
frame_width = 64
frame_height = 64
frames = 8
fps = 12
loop = true
# durations_ms = [80, 80, 80, 80, 80, 80, 80, 160]
```

Sprites are extracted left-to-right, then top-to-bottom. For a frame sequence use
`type = "sequence"`, omit `image`, and name frames `001.png`, `002.png`, etc.
PNG file names are sorted lexically; every frame must have identical dimensions.
Optional `frames`, `frame_width`, and `frame_height` must match the sequence.
Per-frame durations override FPS. A non-looping animation holds its last frame.

Limits: 256 frames, 1024 pixels per source frame dimension, 4096 pixels per source
PNG dimension, 16 MiB per PNG file, 32 MiB decoded frames and 32 MiB scaled frames,
2048 pixels per scaled dimension, metadata 16 KiB, FPS 1–120, durations 1–60000 ms.
Nearest-neighbor and premultiplied-alpha bilinear interpolation are supported.
Decoding/scaling runs on a worker before the GUI message loop. Rendering uses
prepared premultiplied BGRA surfaces. Missing/corrupt artwork falls back to the
event's builtin animation, then text. See [source selection](visuals.md).

## Sound format and scheduling

Sound files must be RIFF/WAVE PCM, signed 16-bit little-endian mono/stereo,
8000–48000 Hz, at most five seconds and 2 MiB. Invalid/missing files are ignored.
Volume scales a private sample buffer; system/device volume is never modified.
A worker owns the waveOut device, stable sample/header buffers and completion
handle. Playback and completion waits stay off the GUI thread.

A named mutex serializes playback across transient processes in the current logon
scope. A second event-identity mutex suppresses repeats for `deduplicate_ms` when
`suppress_repeats` is enabled. Only a hash appears in that mutex name. Local names
include the Windows token authentication LUID and use the token default DACL;
there is no network endpoint. Queue waits are bounded to six seconds; sounds that
cannot acquire playback in time are skipped. Repeat suppression may keep a child
alive after its window closes, for at most the configured 60-second cooldown.

Use `examples/resources.toml` for all five original PNG/WAV examples. Ordinary
tests do not require an audio device; the ignored native device smoke test is
explicitly opt-in. Missing devices degrade to visual notifications.
