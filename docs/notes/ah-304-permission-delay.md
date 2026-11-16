# AH-304 — default of `[codex] permission_delay_sec`

Date: 2026-11-16. Codex CLI on the measuring machine: `codex-cli 0.169.0`.

## Sample

Source: the owner's live Watchdog instance, table `event_facts`, opened read-only.

| Provider | Hook rows from | to | Days | `PermissionRequest` rows |
| --- | --- | --- | --- | --- |
| Codex | 2026-09-07 | 2026-11-15 | 69 | 0 |
| Claude | 2026-09-08 | 2026-11-16 | 69 | 0 |

Hook profiles checked: the global Codex hook file and the global Claude Code settings on that
machine. In both, `PermissionRequest` (and, for Codex, `PostModelRequest`) is bound only to
aHerald; Watchdog's own hook entries do not subscribe to it, so it records no such row. The
Watchdog table has `surface = 'unknown'` on every row, so CLI and desktop rows cannot be told
apart.

No percentiles or histogram can be computed: the sample has fewer than 50 requests and no first
group of automatic reviews.

## Chosen value

The ticket's fallback for a sample under 50 requests is 8 seconds. The owner chose **3 seconds**
instead (2026-11-16) when approving the plan. `src/config.rs` and `config.example.toml` hold 3;
`tests/config.rs` checks that they agree.

## Claude

No Claude `PermissionRequest` rows either, so whether Claude needs the delay stays open; it is a
separate decision (see `TODO.md`).

## To measure later

Binding Watchdog's hook entries to Codex `PermissionRequest` and `PostModelRequest` would let the
measurement in AH-304 run as written. Changing the owner's live hook profiles is outside this
ticket; see `TODO.md`.
