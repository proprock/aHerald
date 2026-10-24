# External animation format

Set an event's `animation` to a directory, relative to the configuration file.
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
prepared premultiplied BGRA surfaces. Missing/corrupt/oversized assets use text.
