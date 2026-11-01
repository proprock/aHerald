# perf_baseline DPI investigation (2026-11-01)

## Finding

The current artwork baseline failure is explained by an uncontrolled render scale
in the unoptimized resident test. On this machine the primary monitor is 144 DPI
(150%). The resident multiplies notification scale by monitor DPI / 96, so its
nominal scale=1 prepares 96x96 frames, while perf_artwork prepares 64x64 frames.
The workloads take different paths through image's resizer.

This is a controlled explanation of the present failure, not evidence that a
particular commit changed monitor settings or caused a new performance regression.
The original AH-082 report did not record DPI. Its 8.0-8.8 ms reference agrees with
today's 1x timings, but that agreement alone does not establish its historical DPI.

## Environment and method

- Windows 11 build 22621, Rust 1.98.1, image 0.25.10.
- Current code at 046e4f2 plus the pre-existing working-tree changes; branch
  feature/perf-artwork-check. No application source was changed for this investigation.
- All measurements were run outside the Codex sandbox with normal Cargo profiles.
- Native GetDpiForMonitor with per-monitor-v2 awareness returned 144x144 for the
  primary monitor. The baseline selects primary by default.
- SHA-256 matched each PNG and animation.toml across assets, target/debug/assets and
  tests/fixtures/perf_artwork. They each contain eight 64x64 frames.
- A temporary diagnostic test measured Animation::load, Animation::prepare with
  nearest interpolation, and the full artwork::prepare_at independently. Each scale
  had one warmup round and 12 measured rounds of four kinds: 48 samples per stage.
  Prepare excludes file reads/decode; full includes both. Calls execute real code,
  and returned animations are consumed with black_box and dropped between calls.
- A second temporary target copied perf_baseline verbatim and changed only its
  temporary configuration: scale=0.6666666666666666. Multiplied by 1.5 DPI, that
  produces an effective scale of 1. No thresholds, assertions, system display
  settings or installed configuration were changed.

## Unoptimized stage measurements

All values are mean milliseconds across 48 samples; stages were timed separately,
so load + prepare need not exactly equal the independently measured full call.

| Effective scale | Frame size | Read/decode | Prepare | Full artwork call |
|---|---|---:|---:|---:|
| 1.00 | 64x64 | 5.48 | 2.62 | 8.24 |
| 1.50 (native DPI) | 96x96 | 5.08 | 34.90 | 39.93 |
| 1.25 | 80x80 | 6.74 | 30.75 | 37.01 |
| 1.50 (second measurement) | 96x96 | 4.67 | 31.38 | 36.43 |
| 2.00 | 128x128 | 4.46 | 52.20 | 55.98 |

At 1.5x the full-call median was 38.03 ms (35.48 ms on the second measurement).
At 1x it was 7.52 ms. The cost increase occurs during prepare, independently of
resident IPC, GUI activity or debug log aggregation. File/decode cost stays near
5 ms. Some outliers remain: e.g. the 1.25x load maximum was 40.76 ms. DPI explains
the systematic cost difference; scheduling/I/O noise still affects individual runs.

## Resident control

Each row below is one run of the same 24-event resident scenario. The control
changes only the configuration scale and retains every original assertion.

| Measurement | Default config, effective 1.5x | Control config, effective 1x |
|---|---:|---:|
| artwork.prepare mean | 39.182 ms | 7.46 ms |
| composition.build mean | 25.83 ms | 10.22 ms |
| frame.render mean | 2.33 ms | 1.04 ms |
| Cache misses / hits | 4 / 20 | 4 / 20 |
| Composition rebuilds | 24 | 24 |
| Outcome | Failed artwork 30 ms budget | All original assertions passed |

The default run stops at the artwork assertion; its composition mean also exceeds
that span's 25 ms budget. The control did not remove work, skip artwork or increase
cache hits. It restored the original-size rendering workload. This is an experiment,
not a proposed change to user-visible notification size.

## Release-stage control

The same probe was also executed with `cargo test --release --test perf_diagnose
-- --nocapture`. It completed successfully; compilation took 3m 28s and the probe
itself took 4.19s. Each row again has 48 independently timed full-call samples.

| Effective scale | Full-call mean | Full-call median |
|---|---:|---:|
| 1.00 | 1.39 ms | 1.03 ms |
| 1.50 (native DPI) | 10.92 ms | 8.29 ms |
| 1.25 | 5.57 ms | 4.93 ms |
| 1.50 (second measurement) | 6.74 ms | 6.12 ms |
| 2.00 | 12.84 ms | 11.21 ms |

The 1.5x full-call maximum was 61.74 ms in the first batch and 13.77 ms in the
second. Optimization reduces the normal cost substantially; it does not remove
wall-clock outliers or the scale-dependent algorithm path. These are artwork-only
measurements, not a measurement of the complete release notification latency.

## Code explanation

- src/resident_overlay.rs, Ui::refresh_scale: effective scale = configured scale *
  monitor DPI scale. The artwork worker receives that effective value.
- src/placement.rs, target: monitor DPI scale = max(dx, 96) / 96.
- src/animation.rs, scaled: nearest interpolation calls imageops::resize.
- image 0.25.10, src/imageops/sample.rs, resize: when output dimensions match input,
  it copies the image and returns. When dimensions differ, including for Nearest,
  it uses vertical_sample into a floating-point RGBA intermediate followed by
  horizontal_sample. This is substantially more work in an unoptimized build.

The frame area grows 2.25 times at 1.5x, but the cost increase is also an algorithm
path change: 1x avoids resampling entirely. This is why simply scaling the old
budget linearly by DPI or pixel count is not justified.

Historical source check: src/animation.rs is identical to commit 797e7b6, which
introduced perf_baseline; that commit also used image 0.25.10 and the same
configured-scale times DPI rule. No changed nearest-resize algorithm or image
version was found to explain the current difference from the recorded reference.
That does not identify the conditions of the old reference run.

## Implications

The existing 30 ms artwork budget is not independent of display configuration.
The failure is reproducible under normal rendering at 150% DPI; it cannot be
classified as pure random noise or attributed to the sandbox. Conversely, a
35-50 ms result does not establish a regression between commits.

Keep perf_artwork as the fixed-workload cost check. For a future revision of the
resident baseline, record effective scale/frame size and either calibrate a
specific workload or establish separately measured budgets for supported scales.
Do not silently set scale=2/3 in the real baseline or raise its limit: the control
above only isolates the cause and changes the rendered physical workload.

The investigation itself changed no production code, original baseline test, budget or dependency.
Temporary diagnostic targets were removed after measurement. The threshold changes
authorized afterward and their verification are recorded below.

## Follow-up decision: retain native DPI with at most 10% headroom

On 2026-11-01 the maintainer chose to retain real monitor DPI and raise the
resident's limits, then rejected the proposed 100 ms artwork limit as too wide.
The accepted calibration rule is observed maximum mean plus at most 10%, rounded
down to a whole millisecond. It applies to mean cost, not to the slowest sample.

| Span | Original budget | Observed maximum mean used | Maximum + 10% | New budget | Actual headroom |
|---|---:|---:|---:|---:|---:|
| artwork.prepare | 30 ms | 55.805 ms | 61.3855 ms | 61 ms | 9.31% |
| composition.build | 25 ms | 25.83 ms | 28.413 ms | 28 ms | 8.40% |

The artwork reference includes the earlier full-suite run recorded in
[the routine check verification](milestone-5.md#routine-artwork-performance-check).
Composition's maximum is the default-DPI reproduction in the resident-control table
above. These observations are retained rather than discarding the more expensive
runs and calibrating only to the quieter series below.

After the initial full-suite check passed with the intermediate 100 ms artwork
budget, ten sequential isolated baseline runs were collected on the same binary,
without simultaneous Cargo compilation, at the measured native 144 DPI. Each run
still has four artwork cache misses and twenty hits.

| Run | Artwork mean, ms | Composition mean, ms |
|---|---:|---:|
| 1 | 36.43 | 24.16 |
| 2 | 40.42 | 25.58 |
| 3 | 38.31 | 25.37 |
| 4 | 39.64 | 22.35 |
| 5 | 38.57 | 24.50 |
| 6 | 36.48 | 21.05 |
| 7 | 38.66 | 24.03 |
| 8 | 38.84 | 21.58 |
| 9 | 35.37 | 24.23 |
| 10 | 38.07 | 22.40 |

Artwork's mean across runs is 38.079 ms; sample standard deviation is 1.554 ms
(CV 4.08%), range 35.37-40.42 ms. Runs 2 and 3 failed the old composition limit;
the precise failing means were 25.585 and 25.375 ms. DPI affects composition as
well as artwork, so its limit is also adjusted by the same measured rule.

The reference set is a small local sample, not a portable percentile or a guarantee
for every DPI/load. In particular, 10% headroom above an observed upper bound does
not guarantee detection of a 10% slowdown from the usual or median result: existing
noise already overlaps small regressions. Monitor/DPI changes require a fresh
comparison on the same workload. The diagnostic 2x artwork probe is useful context,
but a complete resident baseline at 2x and higher DPI has not been calibrated.

The fixed-workload perf_artwork keeps its 30 ms per-kind mean limit at 1x. All other
resident budgets and cache/rebuild/frame assertions remain unchanged. No workload
reduction, application change, dependency change or altered display setting is used.

Final verification with the 61/28 ms budgets: formatting, Clippy with warnings
denied, the full `cargo test --all-targets --no-fail-fast` suite, the Windows MSVC
release build and diff hygiene passed. The focused run also passed, with artwork
mean 43.38 ms and composition mean 22.53 ms. The complete suite passed both
perf_artwork and perf_baseline; explicit manual/platform ignores were retained.
