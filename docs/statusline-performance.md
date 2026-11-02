# Status line wrapper performance

Claude Code runs the `statusLine` command after each assistant message (debounced to about
300 ms) and at its `refreshInterval`, so the wrapper's start-up time is paid on a hot path.
`--statusline` does little work: it parses a bounded payload, tries one IPC send to a running
resident (never starting one) and, with `--then`, runs the user's command in parallel. With no
`--then` it reads the configuration only to find `[claude.statusline] format`.

## Measurements (2026-11-02, Ubuntu 22.04.5, release build, 500 runs, `hyperfine`)

Taken with a resident running. Without one the wrapper used to take about 160 ms: the IPC
client waited up to its 150 ms timeout for a resident to start, which the status line never
wants. `request_if_running` now fails at once when nobody listens (4.2 ms measured), and
`tests/perf_statusline.rs` runs against an empty private runtime directory so it fails if that
wait returns. The Codex `PostModelRequest` context hook had the same wait and uses the same call.

| Command | median | p95 |
|---|---|---|
| minimal Rust program (std only) | 0.38 ms | 0.63 ms |
| `aherald --version` | 3.53 ms | 4.54 ms |
| `aherald --statusline` | 3.93 ms | 5.38 ms |
| `aherald --statusline --then true` | 4.45 ms | 5.72 ms |

The wrapper's own work is about 0.4 ms. The remaining ~3.5 ms is the dynamic loader mapping
63 shared libraries (cairo, pango, pulse, X11, ...) that `--statusline` never calls. A separate
tiny wrapper executable would start in about 0.5 ms; it is not built, because saving ~3 ms on a
refresh that happens at most every 300 ms does not justify a second executable in the
installed layout and the receipt's ownership rules. `tests/perf_statusline.rs` detects a
regression that would change that decision.

## Regression guard

`cargo test --test perf_statusline -- --nocapture` interleaves `--version` (reference),
`--statusline`, `--statusline` with a format and `--statusline --then true`, 80 rounds each,
and fails when a median is more than 6 ms over the reference or any median exceeds 50 ms.
Measured on the debug test build the overhead is 0.8-1.7 ms. The check compares against the
same binary in the same run, so machine speed cancels; the 3.5x headroom absorbs CI noise.
Windows is excluded because the wrapper's shell differs there.

To measure by hand (hyperfine 1.12 has no `--input`, so redirect stdin):

```sh
hyperfine --warmup 30 --runs 500 \
  'target/release/aherald --version' \
  'target/release/aherald --statusline < payload.json' \
  'target/release/aherald --statusline --then true < payload.json'
```

## macOS: `--then` costs more

On a GitHub macOS runner (2026-11-02) the wrapper without `--then` costs about 1.2 ms over
`aherald --version`, like on Linux, but `--then true` costs about 9 ms more than that (8.4 and
9.6 ms in two runs), although starting `sh -c true` from the test costs only 0.08 ms. Spawning
a child from the aHerald process is slower there; the cause has not been investigated.
`tests/perf_statusline.rs` allows 12 ms extra for that case on macOS only. A status line
refresh happens at most every 300 ms, so this is tolerable, but it is a candidate for a look
with a profiler on real hardware.
