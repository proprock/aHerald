# Session warnings implementation plan

**Goal:** Claude context-size warnings and independent Claude/Codex idle reminders,
keyed by agent and session. Idle time starts at the end of an agent response
(maintainer decision, 2026-10-30).

**Architecture:** A bounded, deterministic state machine in each resident's shared
state observes identity-only lifecycle evidence and Claude context counts. The
existing native loops check deadlines and deliver ordinary warning kinds through
the shared queue. No additional processes, dependencies or network listeners.

**Tech stack:** Rust, Serde/TOML, existing native presentation and IPC.

## Proposed configuration and boundaries

Optional `[claude.warnings] context_tokens = "120k"` and per-agent
`[claude.warnings] idle_minutes = 55` / `[codex.warnings] idle_minutes = 25`;
missing idle values retain these agent defaults, and zero disables an idle timer.
Absent context thresholds disable context warnings. Accept positive integer tokens or integer strings, with
`k = 1000`; reject fractions, overflow and zero. Codex context is explicitly
unsupported. Warn strictly above thresholds, once per episode. New user activity
cancels an idle deadline; a new Stop rearms it. Error, interruption and session
end cancel it. Tool/permission activity cancels provisional Stop deadlines where
observable, because another Stop hook can continue a turn.

Resident-only, memory-only state; restart requires fresh evidence. Overdue reminders
are delivered on the next native-loop tick, including after resume when the clock
reports elapsed time. No wake from sleep or portable suspend-time guarantee.
Pause postpones delivery; resuming does not restart the deadline. Keep at most 64
sessions and evict oldest activity. Context warnings rearm only after a known
reading at/below the threshold; null is unknown. Preserve agent/session identity,
and never let subagent activity end or reset the main conversation's timer.

## Execution

1. **Configuration** — `src/config.rs`, `tests/config.rs`.
   Add parsing/validation tests for thresholds, defaults, independence and invalid
   values. Run `cargo test --test config session_warning` before implementation,
   confirm the missing-settings failure; then implement the smallest parser.
2. **Session state** — new `src/session_warnings.rs`, `src/lib.rs`.
   Test strict boundaries, once-only delivery, activity cancellation, new completion,
   duplicate completion, cancellation/end, isolation, bounded state, context rearm
   and missing readings with controlled time. Observe red, implement, rerun.
3. **Adapters** — `src/events.rs`, `src/agent.rs`, `src/statusline.rs`,
   `src/main.rs`, `src/hook_setup.rs`, examples and plugin hooks.
   First test identity-only activity, subagent filtering, end evidence, no hook
   decisions and context extraction. Verify matching Codex schemas and document
   synthetic fixtures/runtime limits. Extend AH-059 only for context counters.
4. **Resident delivery** — `src/resident_request.rs`, resident shared state and
   presentation loops on Windows/macOS/Linux, queue and artwork kind mappings.
   Test handler/queue effects with controlled time, including disabled completion
   presentation, empty queue, pause and session cleanup. New lifecycle evidence
   must never become a notification. Keep native loops awake for pending deadlines.
5. **Documentation and verification** — config example, configuration and hook
   guides, TODO research notes. Run focused tests, formatting, Clippy and full
   tests. Attempt the required Windows release build; report host/toolchain limits.
   Manually inspect available native warning cards; leave unavailable integration
   acceptance open. No commits, pushes or release publication in this task.
