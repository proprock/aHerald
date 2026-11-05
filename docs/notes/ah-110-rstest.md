# AH-110 — rstest pilot

Decision (2026-11-02, owner approved): add `rstest` as a dev-dependency only and keep a
small pilot of named parameterized cases. Ordinary `#[test]` functions stay the default.

## Dependency record

| Item | Value |
| --- | --- |
| Crate | `rstest` `=0.28.0` (pinned), proc-macro `rstest_macros` 0.28.0 |
| License | MIT OR Apache-2.0 |
| Maintenance | `la10736/rstest`, established and widely used |
| MSRV | 1.85 (project: 1.88) |
| Features | `default-features = false` (drops `async-timeout`, `crate-name`, `futures-*`) |
| Transitive (build-time only) | `regex`, `regex-automata`, `regex-syntax`, `aho-corasick`, `glob`, `relative-path` |
| Windows | Built and tested on `x86_64-pc-windows-msvc`, Rust 1.98.1 |
| Distributed exe | No runtime dependency: `cargo tree -e normal --target all` has no `rstest`; release build unaffected |

## Pilot

| File | Test | Before | After |
| --- | --- | --- | --- |
| `tests/hooks.rs` | `malformed_and_unknown_hooks_are_silent_success` | loop, 3 payloads | 3 named cases |
| `tests/hooks.rs` | `permission_and_interrupt_do_not_emit_decisions_or_payloads` | loop, 6 events | 6 named cases |
| `tests/config.rs` | `event_duration_seconds_are_converted_to_internal_milliseconds` | loop, 4 pairs | 4 named cases |

All input/expected pairs are unchanged. The two rejection assertions that shared the config
test moved to `event_duration_outside_the_supported_form_is_rejected`. Hook cases still
use the bounded `invoke` helper, so timeouts, process ownership and the `DO_NOT_LOG`
privacy assertion are unchanged. Counts: `config` 33 → 37, `hooks` 12 → 19; full
`cargo test --all-targets` 705 passed, 0 failed.

## Findings

- Diagnostics: a failing payload now names itself
  (`...::case_4_pre_tool_use`) and runs alone:
  `cargo test --test hooks -- case_4_pre_tool_use` (1 passed, 18 filtered). The loop form
  stopped at the first bad value and could not be selected.
- Readability: one case per line with a name beats an unnamed array in a loop for
  payload/value matrices; it is slightly more verbose for 2–3 trivial values.
- Build time: package-cold `cargo test --test hooks --no-run` (aherald, rstest and
  rstest_macros rebuilt) took 17 s on this machine; the rstest crates are a small part
  of it. Not isolated from aherald's own rebuild, so treat it as an upper bound.
- Cargo.lock grows by 8 packages (proc-macro tool chain only).

## Retained and not converted

Loops that assert several related values in one scenario, mixed positive/negative
assertions, and simple one-case tests stay as ordinary tests. Convert further matrices
only where a separate failing case clearly helps diagnosis; avoid Cartesian products.
