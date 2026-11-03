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

## macOS: what `--then` costs

On a GitHub macOS runner (2026-11-02) `--then true` looked about 9 ms slower than the bare
wrapper while the test's own `sh -c true` seemed to cost 0.08 ms, which suggested that
spawning a child from the aHerald process was slow on macOS. It is not.

Measured on a MacBook Pro M1 (macOS 26, 2026-11-03, `cargo test --test perf_statusline --
--nocapture`, 80 rounds, same result in debug and release builds, three runs):

| Command | median (debug build) |
|---|---|
| `aherald --version` | 4.50 ms |
| `aherald --statusline` | 4.9-5.7 ms |
| `aherald --statusline --then true` | 8.03 ms |
| `sh -c true` alone | 3.28 ms |

`--then true` costs 3.3-3.5 ms more than the bare wrapper, which is what starting `sh` costs
(3.28 ms): the wrapper starts the user's shell after its own start, so the two add up and the
spawn itself (`posix_spawn` with piped stdin and stdout) adds nothing measurable. On macOS a
shell start is slower than on Linux (about 1 ms there), and a loaded runner makes it slower
still (8 ms). The earlier comparison was misleading: it subtracted the `--version` median,
and `sh` starts faster than the GUI-linked aHerald executable, so the shell's cost was
clamped to zero and the whole shell start looked like a spawn penalty.

`tests/perf_statusline.rs` therefore compares the `--then` case against the absolute median
of `sh -c true` measured in the same run, with the usual 6 ms budget on top, and the 12 ms
macOS-only allowance is removed. Nothing to fix in the wrapper: the cost is the user's shell.
