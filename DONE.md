# aHerald completed tasks

Tasks moved here from TODO.md after every deliverable and acceptance criterion
passed. Open follow-ups and deferred items stay in TODO.md under new IDs.

## Milestone 1 — Functional notification engine

- [x] **AH-001 — Bootstrap the native Rust project.** Dependencies: none.
  Create a testable library and one GUI executable targeting
  `x86_64-pc-windows-msvc`. Use focused modules for CLI/configuration, normalized
  events, process transport, notifications, tray, and Codex adapters as needed;
  avoid empty speculative subsystems. Prefer `windows`, `serde`, `serde_json`,
  `toml`, PNG-only `image`, and `tracing`; use standard-library concurrency and
  commit Cargo.lock. Acceptance: Cargo checks pass and a release executable builds
  without an external GUI framework or runtime dependency.

- [x] **AH-002 — Implement CLI and validated TOML configuration.** Depends on AH-001.
  Support `--hook`, `--resident`, `--transient`, `--event`, `--status`, `--quit`,
  and `--config`; report unimplemented later-milestone operations explicitly.
  Explicit mode arguments override the configured mode; reject conflicting
  explicit operations. Resolve relative assets against the configuration directory.
  Cover event enablement/duration, positioning/scaling/fades, sound volume,
  deduplication, resident options, Codex monitoring, and usage refresh.
  Acceptance: loading, defaults, invalid values, missing/malformed files, path
  resolution, and mode selection are tested; configuration failures cannot crash
  or change permission behavior in hook mode.

- [x] **AH-003 — Define normalized events and bounded hook input.** Depends on AH-002.
  Model `turn_complete`, `permission_required`, `input_required`, `error`, and
  `interrupted`, with priority, source, and optional session/turn/request identity.
  Parse stdin using explicit size/time limits; tolerate unknown JSON fields and
  reject malformed or oversized inputs safely. Verify official hook contracts
  before adding Codex-specific parsing and output. Keep operational diagnostics
  off stdout and never emit approval, denial, continuation, or input rewrites.
  Acceptance: fixtures test normalization, optional identifiers, unknown events,
  malformed input, input bounds, and event-specific non-blocking responses.

- [x] **AH-004 — Implement detached transient delivery.** Depends on AH-003.
  Launch a detached GUI child and hand off only bounded normalized event data
  through a local channel, not sensitive command-line arguments. Bound startup
  and transfer time. Keep `--event` available for demonstrations without Codex.
  Acceptance: an integration test proves the hook returns before an overlay's
  lifetime ends, including `duration_ms = 0`; startup failure exits safely.

- [x] **AH-005 — Render a native layered overlay.** Depends on AH-004.
  Use `WS_POPUP`, `WS_EX_TOPMOST`, `WS_EX_LAYERED`, `WS_EX_TOOLWINDOW`, and
  `WS_EX_NOACTIVATE`, with `UpdateLayeredWindow` and correct alpha handling.
  Implement text fallback, click-to-dismiss, automatic dismissal, smooth fades,
  and message-loop timers. A zero duration persists until dismissal or a verified
  resolution event. Acceptance: no focus theft or Alt-Tab entry, responsive UI,
  transparent background, working timers and dismissal; basic DPI-correct sizing.

- [x] **AH-006 — Load and animate external PNG assets.** Depends on AH-005.
  Support TOML-described sprite sheets in row-major order and deterministic PNG
  frame sequences. Support FPS and optional per-frame duration metadata, looping,
  and nearest-neighbor/bilinear scaling. Bound decoded dimensions, frame counts,
  and memory; handle invalid dimensions, missing files, and corrupt images with
  the text fallback. Acceptance: frame extraction, timing, sequence ordering,
  bounds, alpha conversion, and asset failures have focused tests.

- [x] **AH-007 — Add native asynchronous sound and sample resources.** Depends on AH-006.
  Use a Windows audio API supporting per-playback volume without changing system
  volume. Serialize playback to prevent overlap; honor global/per-event settings
  and repeated-event suppression. Add original sample PNG/WAV resources, metadata,
  and example TOML for the event types. Acceptance: sound does not block the GUI,
  mute/volume work, missing WAV files are harmless, and asset provenance is recorded.

- [x] **AH-008 — Accept the first vertical slice.** Depends on AH-001–AH-007.
  Run the standard Cargo checks and release build. Demonstrate transient visual
  and sound notifications using external resources and text fallback. Record a
  Windows smoke test and hook-return timing under finite and indefinite duration.
  Acceptance: the engine works without Codex installed and all focused tests pass.

## Additional asset support

- [x] **AH-024 — Load local Codex pet packages.** Depends on AH-008.
  Accept `notifications.pet` pointing to local `pet.json`, resolve the sprite path
  inside the package, and decode PNG/WebP standard v1/v2 atlases. Map aHerald events
  to the documented occupied state frames and timing. Keep explicit event animations
  as overrides; resource errors use text fallback. Do not run installer commands.
  Acceptance: focused tests verify paths, frames, alpha, timing, version/size bounds,
  invalid resources and native window recovery; visually demonstrate a real pet.
  User approved extending the existing image decoder with WebP on 2026-10-24.

- [x] **AH-025 — Keep source and signal text beside optional agent artwork.** Depends on AH-024.
  Always render the source and normalized signal in one native card. Add animation
  as an optional decoration, selected by agent with a shared fallback; preserve
  explicit event animation overrides. Invalid artwork keeps the source/text card.
  Acceptance: test source isolation, asset precedence, visible text pixels, alpha
  composition, native card recovery/dismissal; visually inspect the Codex/Becky card.
  This slice does not implement Claude event ingestion, a resident queue, or replies.

## Milestone 2 — Resident architecture

- [x] **AH-009 — Enforce a single resident instance.** Depends on AH-008.
  Scope a named mutex and named pipe to the current Windows user and logon session.
  Apply appropriate security descriptors and reject remote pipe clients. Require
  no elevation. Acceptance: concurrent starts yield one resident; tests exercise
  ownership, access restrictions, and recovery after process termination.

- [x] **AH-010 — Implement bounded, versioned IPC and control operations.** Depends on AH-009.
  Add framing, validated lengths, acknowledgement, connection/read/write deadlines,
  and bounded retries. Forward hook events, start resident when absent, and implement
  `--status`/`--quit`. Acceptance: round-trip, oversized/malformed messages,
  incompatible versions, broken connections, startup races, and graceful lifecycle
  are tested; no TCP listener exists.

- [x] **AH-011 — Queue, deduplicate, and switch notification events.** Depends on AH-010.
  Use a bounded queue with High/Medium/Low priority and FIFO within each priority.
  Show one overlay with count and navigation controls. Persistent waits must not
  prevent presentation of other events; dismissal affects only the selected event.
  Correlate duplicates by available session/turn/request identity and source,
  including cross-source duplicates; avoid merging unrelated sessions.
  Acceptance: simultaneous sessions, persistent events, overflow, repeated delivery,
  switching, priority, and individual dismissal are tested.

- [x] **AH-012 — Add native tray and graceful shutdown.** Depends on AH-011.
  Use `Shell_NotifyIconW`; restore the icon after Explorer restarts. Add usage
  summary/details/refresh, sound and visual toggles, pause, configuration location,
  and exit. Pause suppresses presentation/audio while resolution processing
  continues. Acceptance: menu actions work, Explorer restart recovers, and exit
  stops workers and releases native resources; unavailable usage is explicit.

## Milestone 3 — Verified Codex integration

- [x] **AH-013 — Record version and event-source compatibility.** Depends on AH-008.
  Inspect current official documentation and matching release/protocol schemas.
  Record tested Codex versions, payload fixtures, capabilities, and coverage limits.
  Planning observed `codex-cli 0.164.0-alpha.11.1`; this is not proof of stable
  release support. Acceptance: a technical note distinguishes verified behavior,
  unsupported behavior, and experimental/version-dependent behavior.

- [x] **AH-014 — Deliver lifecycle hook integration and examples.** Depends on AH-010, AH-013.
  Verify `Stop`, `PermissionRequest`, and version-supported `Interrupt`; provide
  Windows hooks.json examples with correct path quoting and response contracts.
  Never decide permissions. Acceptance: mocked and real supported hook invocations
  forward notifications promptly; instructions explain manual installation,
  trust requirements, removal, and preservation of existing hooks.

- [x] **AH-015 — Implement supported input-wait observation.** Depends on AH-013.
  Investigate documented stable sources for actual wait entry and resolution,
  including app-server request/resolution events where legitimately observable.
  Share app-server transport with AH-017 if needed. Do not assume a private server
  observes other CLI sessions, resume their threads, create a proxy client, or
  parse unstable transcripts. Acceptance: supported sources distinguish executing,
  waiting, and resolved states; unobservable sessions report unsupported explicitly.
  Lack of a stable source is documented rather than replaced with heuristic detection.

- [x] **AH-016 — Correlate pending and terminal states.** Depends on AH-011, AH-014, AH-015.
  Resolve pending notifications only on verified matching request/turn lifecycle
  evidence. Deduplicate multiple sources and handle completion/interruption.
  A failed individual tool call is not automatically an agent execution error.
  Acceptance: fixtures cover multiple requests/sessions, late resolution,
  duplicate sources, and unsupported resolution; manual dismissal always works.

## Milestone 4 — Usage and quotas

- [x] **AH-017 — Connect to the official app-server.** Depends on AH-013.
  Manage one long-lived stdio child with handshake, bounded JSON messages,
  request IDs, timeout, reconnect/backoff, and shutdown. Handle Codex absence,
  authentication unavailability, and unsupported methods without blocking UI.
  Acceptance: mock transport tests cover normal responses, timeouts, disconnects,
  malformed messages, and reconnect; credentials are never read or logged directly.
  Codex discovery: prefer codex.exe in absolute PATH directories, then fall back
  to the existing npm installation's native x64 binary; never copy/install Codex
  or execute npm shims. Verify precedence, spaced prefixes, missing/incomplete
  installations, and a live usage read with the ordinary Windows PATH.

- [x] **AH-018 — Normalize available rate-limit data.** Depends on AH-017.
  Implement `account/rateLimits/read` and supported updates, preferring multi-bucket
  data when available. Preserve primary/secondary windows, percentages, Unix reset
  timestamps, additional buckets, and officially exposed credits. Distinguish
  missing data from zero. Do not consume reset credits or add account mutations.
  Acceptance: fixtures cover legacy/multi-bucket, optional fields, unknown fields,
  unavailable values, and local-time reset conversion including DST boundaries.

- [x] **AH-019 — Present usage independently of notifications.** Depends on AH-012, AH-018.
  Show compact tray usage and a native details window for all available buckets.
  Use local reset times, default 300-second refresh, manual refresh, and explicit
  cached values and unavailable states. Acceptance: usage failure leaves notifications working,
  polling is bounded/backed off, and no session data goes to third parties.

## Milestone 5 — Polish and portable release

- [x] **AH-026 — Preview the current configuration in a looping demo.**
  Support `--demo [SECONDS]`, with a three-second default pause between scenarios.
  Cycle enabled event types and a multi-event navigation scenario through the shared
  native engine; retain configured resources, sounds, position, scale and finite
  durations. Advance persistent waits automatically. Keep demo events isolated from
  the resident and allow tray pause/exit. Acceptance: CLI validation, deterministic
  timing/repetition/disabled-event tests, native lifecycle/isolation tests, visual
  inspection and standard Cargo checks. No configuration is written.
  Verified on Windows 11 build 22621 on 2026-10-25; see `docs/demo.md` for
  commands, manual observations, cold-cache behavior and test limitations.

- [x] **AH-027 — Simplify cards and expose visual event settings.**
  Hide count/navigation for one event; show bottom-right `<`/`>` for multiple.
  Dismiss informational cards by body click; retain an explicit Dismiss button
  only when body dismissal is disabled. Show remaining seconds for finite events.
  Use configurable per-event accent colors and shared/agent pet state overrides
  covering all nine standard tracks. Acceptance: behavioral layout/input/timer,
  configuration and atlas extraction tests, native pixel/lifecycle checks, visual
  inspection and standard Cargo checks. See `docs/card-controls.md`.

- [x] **AH-028 — Make artwork sources explicit and defaults usable.**
  Support global auto/builtin/pet and per-event inherit/builtin/pet/custom.
  Enable full builtin, full pet and both hybrid directions without clearing old
  animation paths. Stage external builtin resources beside Cargo executables.
  Recover failed pet/custom assets to builtin, then text. Acceptance: executable
  source-selection/configuration/fallback tests, native default-resource checks,
  visual inspection and standard Cargo checks. See `docs/visuals.md`.

- [x] **AH-029 — Expire grouped demo cards individually.**
  Give each selected grouped card its own preview timeout; retain the remaining
  cards until they expire or are dismissed. Hide the window only after the last
  card, then begin the configured quiet interval. Use the queued lifetime for
  countdowns and fades, including demo waits. Acceptance: a controlled-time
  three-card regression, repeat/navigation checks, one visual pass and Cargo checks.
  See `docs/demo.md`.

- [x] **AH-030 — Identify the originating Codex session.**
  Show `Codex: <stored thread name>` in resident and transient cards, falling back
  to a short session ID when unavailable. Resolve only supported read-only metadata
  in the background, without delaying hooks or restarting card lifetimes. Bound
  cache/queues and isolate concurrent sessions. Acceptance: protocol, cache and
  native rendering checks, one visual inspection, and standard Cargo verification.
  See `docs/session-source.md`; automatic sidebar captions may have no stored name.

- [x] **AH-021 — Harden resources and recovery.** Depends on AH-016, AH-019, AH-020.
  Add bounded resident asset caching and operational logging with no credentials,
  full conversation content, or sensitive tool arguments. Exercise queue floods,
  duplicate storms, malformed assets, resident crashes, broken IPC, and unavailable
  Codex. Acceptance: limits and recovery behavior are documented and tested;
  repeated notifications do not leak handles, GDI objects, threads, or memory.

- [x] **AH-031 — Simplify the tray menu and compact usage summary.**
  Show `Codex: 5h 25% / 2:35, 7d 10% / 6d 11h`; configure `[usage] display`
  as `remain` (default) or `used`. Use days and whole hours, H:MM below one day, and minutes
  below one hour, with explicit missing values and retained last received values. Remove Refresh usage and
  the separate Usage details action. The summary adds one persistent usage card
  to the shared notification queue beside the tray: plan, both windows, local
  minute-precision reset dates, workspace credits and all reset credits. Queue
  arrows switch cards; mouse wheel pages long details; body click dismisses only
  the usage card. Open configuration through its Windows association, falling
  back to Notepad. Restart gracefully replaces the process and reloads settings.
  Verified with domain/native lifecycle tests, synthetic visual inspection of
  details and queue navigation, and standard Cargo checks. See `docs/tray-usage.md`.

- [x] **AH-032 — Switch to the agent application on notification click.**
  Configure `[notifications] activate_agent_on_click = true` by default.
  Body click opens `codex://threads/<thread-id>` with validated, encoded identity;
  missing identity or shell failure tries an existing Codex window. Dismissal
  remains controlled independently by `click_to_dismiss`. Arrows and Dismiss
  never activate; demo and usage stay isolated. Resident and transient share the
  activation adapter. Failures retain card behavior and record a fixed diagnostic.
  Verified with focused/native tests, standard Cargo checks, and user-confirmed
  transition from a transient test card to the correct chat. See
  `docs/notification-click.md` for versions and manual-check limitations.

- [x] **AH-033 — Default to config.toml beside the executable.**
  Without `--config`, use `config.toml` beside `aherald.exe`, independently of
  the working directory. Create an absent standard file with current defaults;
  preserve existing files. No APPDATA search or automatic migration. Explicit
  missing custom files remain errors. Resolve resources from the selected file.
  Report creation errors with target path and reason. Simplify standard hook
  examples and document manual migration. Verified with process tests under
  Unicode/spaced paths, concurrent creation, preservation and failures, and
  standard Cargo checks. See `docs/configuration.md`.

- [x] **AH-034 — Show usage levels in a dynamic tray icon.**
  Two columns show primary/5h and secondary/week. Fill follows usage.display;
  remaining quota sets green (>60%), yellow (>20%) or red, with unavailable gray.
  Last received values remain current throughout the icon, tooltip, menu and card;
  age and refresh errors remain in details. Native sizes 16/20/24/32 were visually
  checked. Tests verify updates, pixel/handle reuse, TaskbarCreated recovery and
  restoring the current icon after shell deletion. Standard Cargo checks and the
  Windows x64 release build passed. See `docs/tray-usage.md` for evidence and limits.

- [x] **AH-035 — Configure card background colors.**
  Add global `[notifications] background_color` with the existing #1C232D default.
  Optional `[events.NAME] background_color` and `[usage] background_color` override
  it by card type; omitted overrides inherit the global setting. Accept #RRGGBB
  as for accent color, preserving card opacity, rounded transparency and artwork.
  Verified: parsing/inheritance and native rendered pixels pass functional tests;
  inherited event, overridden event and usage backgrounds were visually checked.
  Standard Cargo checks and Windows x64 release build passed. See `docs/card-controls.md`.

- [x] **AH-036 — Claude Code hook notifications.**
  `Agent` enum (`src/agent.rs`), `--hook --agent claude`, `[claude] hooks_enabled`,
  mapping of `Notification`/`Stop`/`StopFailure`/`PermissionRequest` per
  `docs/claude-hooks.md`; the hook never answers Claude Code. Automated tests pass.
  Live checks (Claude Code, 2026-10-25 and 2026-10-26, interactive session with project
  hooks and `--setting-sources project`): `PermissionRequest`, `Notification`
  (`permission_prompt`, `idle_prompt`) and `Stop` payloads matched the documented
  fields; permission required and turn complete cards appeared, the permission card was
  cleared by `Stop`, and the hook exited 0 with empty stdout. `PermissionRequest` and
  `permission_prompt` arrived 6 s apart and showed two cards; fixed so an unresolved
  Claude card absorbs repeats (`claude_notification_arriving_seconds_after_permission_request_joins_the_pending_card`
  in tests/queue.rs, red before the fix). After the fix one card was observed.
  Not observed live: `StopFailure` (follow-up AH-054).

- [x] **AH-039 — Select tray graph agents.**
  Add `[tray] graph = "auto" | "both" | "claude" | "codex"`, default auto.
  Auto shows the agent with the lowest remaining percentage across 5h/week;
  ties prefer Codex, disabled/no-data agents are excluded, missing windows stay gray.
  Both splits each window's bar by width: Claude left, Codex right, independent
  full-height fills and fixed positions. Exclusive modes never fall back.
  Preserve display modes, colors, cached values, DPI sizing and icon reuse/restore.
  Acceptance: config/selection/raster/native tests, four-mode visual preview at
  16/20/24/32 px, isolated Explorer restoration and standard Cargo checks pass.
  Graph selection does not enable polling or change menu/card behavior.
  Verified on Windows 11 build 22621 with synthetic data; see `docs/tray-usage.md`.

- [x] **AH-040 — Resolve stale Claude pending cards.** Depends on AH-036.
  Done: Claude `Stop`/`StopFailure` resolve same-session pending cards; Claude pending
  events dedupe per session; every new event sounds once (agreed with the user).
  The live double-fire of `PermissionRequest` + `Notification` is still unobserved;
  dedup is safe either way. Original brief:
  Claude permission/input cards are persistent and never receive terminal evidence
  (`turn_id` is absent; `is_terminal_evidence` accepts only Codex hooks). They stay
  until dismissed, hold the High-rank selection and silence every later sound; with
  visuals off they cannot be dismissed at all. A Claude `Stop`/`StopFailure`
  resolves pending cards of the same `session_id` only. Deduplicate a
  `PermissionRequest` and `Notification` `permission_prompt` pair for one prompt if
  the AH-036 payload captures show both fire. Decide with the user how queued,
  unselected events announce themselves. Overlaps the terminal-evidence part of
  AH-038. Acceptance: queue tests for same-session clearing, cross-session and
  cross-agent isolation, the duplicate pair, and a later sound after a hidden
  persistent card; standard Cargo checks. Source: `audit-20261026.md` E1.

- [x] **AH-041 — Keep the resident alive on card render failures.** Depends on AH-021.
  Done: shared `Composition::compose` (largest frame, byte budget, text fallback) used by
  both modes; a resident draw failure hides the window, logs `render_failed` once,
  keeps the queue and retries next tick. Placement failure shows no card until placement
  succeeds (no fabricated position). Native fault tests use a debug-build-only
  `aherald.fault` file beside the configuration. Transient already cached countdown
  frames, set the title and scaled mouse input. Original brief:
  Card, artwork, placement and countdown errors propagate through `Ui::draw` and
  stop the resident, while transient mode falls back to a text card. Extract one
  card composition shared by both modes: size by the largest frame, enforce the
  animation byte budget, fall back to text, cache countdown frames, update the
  window title and scale mouse coordinates. Stop the resident only when its window
  or message loop is lost, and record a fixed diagnostic. Acceptance: an injected
  composition or placement failure leaves the resident accepting events and showing
  a text card; native rendering and leak tests and standard Cargo checks pass.
  Source: `audit-20261026.md` E2, A1.

- [x] **AH-042 — Wake the resident only for pending work.** Depends on AH-041.
  Done (part 1, agreed with the user; the rest is AH-051): `serve_until` waits on
  overlapped accept plus a stop event; IPC posts `WAKE` to the window; the 16 ms timer
  runs only while a card is visible or a timed card counts down (a 15 s housekeeping
  timer remains only while usage polling is enabled, so the tray tooltip/icon may lag
  by up to 15 s when idle); `Driver::wait` lets the usage worker sleep to its deadline
  (a live app-server connection is still polled every 500 ms for pushed updates);
  tooltip text is applied only on change. A test counts `WM_TIMER` deliveries on the
  idle GUI thread. Original brief:
  An idle resident wakes about 100 times per second: the 16 ms GUI timer, the 50 ms
  pipe accept poll with a new event per attempt, and the 50 ms usage worker loop per
  enabled agent. Post new IPC events to the window, run the timer only while a card
  is visible or animating, wait on overlapped accept plus a stop event, and sleep
  the usage worker until its next deadline. Present a static card only when frame,
  alpha, countdown second or placement changes; update the tooltip only when its
  text changes; cache usage-card layout and tray placement while the card is open.
  Move "which card, frame, alpha and sounds now" into a pure presenter driven by
  queue state and a controlled clock, leaving `Ui` a thin Win32 adapter, and stop
  the window procedure from borrowing `Ui` while `draw` holds it across reentrant
  `ShowWindow`/`SetWindowTextW` calls. Acceptance: presenter unit tests; a native
  test observes no periodic wakeups while idle and prompt display of a forwarded
  event; lifecycle, tray and leak tests and standard Cargo checks pass.
  Source: `audit-20261026.md` P1, P2, A6, E7.

- [x] **AH-043 — Report configuration errors and guard Restart.**
  Done: `invalid configuration: <table.key> <reason>` messages without values (see
  `docs/configuration.md`); tray Restart validates first and shows the error on the
  details card; fixed log codes for invalid configuration, render fallback/failure,
  resident startup failure and usage failure. Original brief:
  Parsing reports only "invalid configuration fields" and validation one generic
  message; the resident and Restart run without a console, so errors vanish and an
  invalid file makes Restart exit silently. Name the failing field and reason
  without echoing values. Validate the file before the tray Restart replaces the
  process; on failure keep the running resident and show the error on a card. Add
  fixed operational-log codes for invalid configuration, render fallback, resident
  startup failure and usage failure, without payloads. Acceptance: config tests
  assert field-level messages; a process test makes the file invalid, requests
  Restart and observes the original resident still serving; standard Cargo checks.
  Source: `audit-20261026.md` E5, A8.

- [x] **AH-044 — Separate usage sources and slow Claude retries.** Depends on AH-037.
  Done (agreed with the user; the tail is AH-052): `usage_service::UsageSource`
  (`read`, `listen`, `updates`, `is_live`) is implemented by `CodexSource` (the
  unchanged `Connection` and rolling updates) and `claude_usage::ClaudeSource` (one
  typed HTTPS read, no JSON-RPC emulation, no second backoff layer);
  `View` carries an `Agent`, and card text is chosen by it, not by label. HTTP 429 maps
  to `Error::RateLimited`, and the driver then waits at least `refresh_interval_sec`
  (manual refresh included); other failures keep the 10 s..300 s backoff. Fake-clock
  driver tests cover both; Codex tests and texts are unchanged. Original brief:
  `ClaudeTransport` emulates the Codex JSON-RPC app-server and reshapes HTTP data
  into Codex JSON, which adds pre-formatted `Bucket.details`, a second backoff layer
  and Codex-worded errors for Claude. Introduce a `UsageSource` boundary implemented
  by the Codex client and the Claude HTTPS client; select card text by `Agent`, not
  by label; remove the test-only `summary`/`details`/`brief` path and move its
  assertions to the shown card text. After a Claude HTTP 429 the next scheduled
  request waits at least `refresh_interval_sec`; manual refresh stays rate-limited.
  `Retry-After` and the 15-minute cap remain in AH-038. Acceptance: driver tests with
  a fake clock show no early Claude request after 429; Codex usage behavior and
  error text are unchanged; standard Cargo checks. Source: `audit-20261026.md` A3,
  A4, E4.

- [x] **AH-046 — Accept concurrent hook bursts.** Depends on AH-010.
  Done: `sixteen_concurrent_hooks_reach_one_resident_exactly_once` (tests/resident.rs)
  starts 16 `resident::forward` calls together against a cold resident; all succeed
  and one resident queues all 16. No event was lost in 28 runs at 16 hooks or 8 runs at
  48, so the server (single pipe instance, `ensure` before send) is unchanged; the
  test guards it, and `WaitNamedPipeW` or several instances stay the fallback if a
  slower machine loses events. Original brief: The resident pipe has one instance; busy clients retry with 10 ms sleeps, the
  status probe allows 30 ms and the event 150 ms, and each forward makes two round
  trips. Parallel subagent hooks may start extra residents or drop notifications
  while the hook still reports success. First add a test that forwards 16 hook
  events concurrently. If any is lost, serve several pipe instances or wait with
  `WaitNamedPipeW`, and send the event before probing status. Acceptance: one
  resident queues all 16 events exactly once and every hook returns within its
  existing timeout; IPC security and race tests and standard Cargo checks pass.
  Source: `audit-20261026.md` E3.

- [x] **AH-047 — Fix small resident and transient edge cases.**
  Done (decisions agreed with the user): the sound slot loss was already fixed by
  `Queue::take_unannounced(room)` (covered in tests/queue.rs), so no change;
  `[resident] tray_enabled` was removed from the config, the example and INFO.md
  (a file that still sets it is now rejected as an unknown key; no compatibility is
  kept, there are no external users); `overlay::register_class` accepts an existing
  class; simultaneous transient cards stack (`placement::stack`: downward, upward for
  `bottom_right`) by looking at visible `aHerald.Overlay` windows, with placement
  serialized across hook processes by a named mutex held until the card is shown.
  Tests: `stacking::*` in tests/placement.rs, `simultaneous_transient_cards_do_not_overlap`
  in tests/native_overlay.rs (3 real cards), `class_tests` in src/overlay.rs (no red
  phase observed: the helper is new). Not covered: the two-process race at the mutex
  timeout (500 ms). Each transient card still starts its own `codex app-server` for
  the session name. Original brief: The resident marks an event as sounded before checking for a free audio slot, so a
  fifth simultaneous sound is lost. `[resident] tray_enabled` is parsed and
  documented but has no effect; decide with the user whether to implement or
  deprecate it, keeping existing configuration files loadable. Transient window
  registration does not tolerate an existing class, and simultaneous transient cards
  overlap at one position; decide whether they stack. Acceptance: a sound waiting
  for a slot plays once a slot frees; `tray_enabled` behaves as documented; tests
  cover the chosen class and stacking behavior; standard Cargo checks.
  Source: `audit-20261026.md` E8.

- [x] **AH-055 — Claude click-to-activate (split from AH-038).** Depends on AH-036.
  Clicking a Claude card opens its own session. Desktop: the hook `session_id` is matched
  to `cliSessionId` in Claude Desktop's session files (only that field is read; user-approved
  2026-10-26) and `claude://claude.ai/epitaxy/local_<id>` is opened, with the Desktop window
  as fallback. Terminal: the hook records the visible console window of its own or an ancestor
  process (`console_window`), accepted at click time only for `conhost.exe`, `OpenConsole.exe`
  and `WindowsTerminal.exe`. Automated: policy with a fake agent, Desktop lookup in temp dirs,
  ancestor walk, event window field (`tests/activation.rs`, `tests/claude_desktop.rs`,
  `tests/click_target.rs`). Manual 2026-10-26/27 (classic console host, Claude Desktop):
  a card raised its terminal window; two Desktop sessions answered with a delay produced two
  cards, and each click switched to its own session. Not verified: Windows Terminal and two
  terminal windows (tracked in AH-056).

- [x] **AH-056 — Verify Claude click in Windows Terminal (split from AH-055).**
  Depends on AH-055. Manual 2026-10-27 (Windows Terminal 1.24.12741.0, Claude CLI): a card
  raised the Windows Terminal window; with two sessions in two windows, each click raised
  its own window, and both restored from minimized. The first click was 6.7 s late because
  the Desktop session scan (703 files, cold cache) ran before the recorded window; the
  window is now tried first (`tests/activation.rs`), and clicks log
  `debug agent_activated ms=N` (`tests/operational_log.rs`); clicks after the fix took
  1 to 226 ms. Known limit, accepted: tabs are not selected, UI Automation tab selection
  is not built (fragile across Windows Terminal versions). Automated: 270 passed, 0 failed,
  14 ignored; `fmt` and `clippy -D warnings` clean. Follow-ups: AH-074, AH-075.

- [x] **AH-037 — Opt-in Claude 5h/7d quota.** Depends on AH-036.
  `[claude.usage] enabled` (default false, interval 300..86400 s) polls the undocumented
  `GET api.anthropic.com/api/oauth/usage` over WinHTTP with the token Claude Code stores
  in `.credentials.json`. Agreed exception to the token-reading rule (see AGENTS.md):
  token used only for that call, never logged, stored or refreshed. Parsing, credential,
  status and driver behavior are tested with fakes (no network). Live check 2026-10-25
  (signed-in Claude Code, `anthropic-beta: oauth-2025-04-20`): HTTP 200 with
  `five_hour`/`seven_day` `{utilization, resets_at}`; a stale token gave 401. Tested with
  Claude Code 2.1.310. Manual 2026-10-25/27: separate Claude menu line, details card with
  plan, 5h/Week, Extra usage and Spend, forced refresh on opening details, and tray
  tooltip/menu/card with both agents enabled.

- [x] **AH-053 — Test the transient placement mutex timeout race (split from AH-047).**
  Depends on AH-047. `tests/native_overlay.rs` holds the session-wide placement mutex from
  the test process, delivers a transient card and asserts that the card still appears
  (after the 500 ms wait) while the lock is held, then dismisses it and observes a clean
  exit. Checked by mutation: a placement wait that never times out makes the test fail
  with "card missing". Card-showing tests in that file now run one at a time because they
  share that mutex and screen area.

- [x] **AH-052 — Finish the usage-source cleanup (split from AH-044).** Depends on AH-044.
  Removed the test-only `View::summary`/`details`, `Service::summary`/`details` and
  `brief`; their assertions in `tests/usage_service.rs` and `tests/tray_usage.rs` now check
  the shown `card_text`. `Bucket.details` (pre-formatted lines) became typed
  `Bucket.extra_usage`/`spend` (`usage::Allowance`: enabled, used, limit, currency);
  the card renders the same text from them. `claude_usage::parse_response` builds the
  `Snapshot` directly (no Codex-shaped JSON, `snapshot_from_response`/`with_plan` gone).
  Codex card text is unchanged. Note: the dropped `details` view also showed reset-credit
  `granted` times and full-length reset timestamps, which the card never did.

- [x] **AH-051 — Pure card presenter and P2 redraw limits (split from AH-042).** Depends on AH-042.
  Done (part 1, agreed with the user; the reentrancy half is AH-057): `presenter::Presenter`
  turns the selected queue entry, animation clock and fade settings into `Shown`
  (frame, alpha, countdown second) and reports a card only when that differs from what is
  on screen; `Ui` presents and calls `ShowWindow` only then. The usage card measures its
  height and asks Explorer for the tray rectangle once per layout, not every 16 ms, and
  the cached placement is dropped on display, DPI, setting and taskbar changes.
  `tests/presenter.rs` covers once-only static cards, countdown seconds, animation frames,
  fade alpha and invalidation. Not covered by an automated test: the present-call count
  of the live window. Original brief: see TODO.md history; source `audit-20261026.md`
  A6, E7, P2.

- [x] **AH-057 — Reentrancy-safe resident Ui (split from AH-051).** Depends on AH-051.
  The resident window now owns `Ui` behind a `reentry::Gate`: the procedure borrows it
  only when no handler is active, so messages that `ShowWindow`/`SetWindowTextW` send
  during `draw` are kept (identical ones merged, backlog capped at 64) and handled after
  it, and the tray popup menu's nested loop runs with the gate open. The tray
  `TaskbarCreated` id lives outside `Ui`. `presenter::sound_slots`/`MAX_SOUNDS` replace the
  inline 4-sound cap. Tests: `tests/reentry.rs` (gate), `sound_slots_cap_concurrent_playback`,
  and a debug-build native test that sends a message mid-draw (`aherald.fault` = `reenter`).
  Limit: aliasing undefined behavior is not observable at run time, so the native test
  proves the deferral path stays alive, not the absence of aliasing. Source:
  `audit-20261026.md` E7, A6.

- [x] **AH-049 — Reuse app-server clients and decoded sounds.**
  Done (agreed with the user; transient mode is AH-058): with Codex usage polling on, the
  resident reads session names over the usage worker's connection
  (`usage_service::NameLookup`, `Connection::thread_name`), so one `codex app-server`
  child runs instead of two; with polling off the name worker keeps its own child.
  `audio::SoundCache` (8 entries, keyed by path, volume, size and modification time)
  replaces a disk read and parse per event. `Queue::push` returns `queue::Outcome`
  (Queued, Resolved, Duplicate, Obsolete, Full, Invalid) and the IPC listener no longer
  repeats the terminal, duplicate and obsolete checks (a repeated open Claude wait now
  answers Accepted instead of Rejected). Bilinear `Animation::prepare` premultiplies once
  and `activation::find` reuses one path buffer per enumeration. Tests:
  `tests/usage_shared.rs`, `tests/queue_outcome.rs`, `audio::cache` and a bilinear
  `prepare` test. Limit: the child count is asserted through a fake transport's connection
  factory (one factory call is one child), not by counting real processes; the
  allocation reductions have no automated test. Source: `audit-20261026.md` P5, P6.

- [x] **AH-050 — Tighten internal types and remove dead code.**
  Done: `general.mode`, `general.logging`, `notifications.position`, `monitor` and
  `interpolation` are the enums `Mode`, `LogLevel`, `Position`, `Monitor` and
  `Interpolation`; `Config::events` is `config::Events`, indexed by `events::Kind`
  (`Kind::ALL`, `Kind::from_name`; `EVENT_NAMES` is gone), so a missing key cannot panic.
  The TOML format is unchanged and bad values still fail without echoing them. In
  `pipe.rs` a private `Failure` enum replaces matching on `"local pipe deadline
  exceeded"`; public pipe functions still return the same `String` errors.
  `audio_windows` uses `local::Handle`; `overlay.rs` has `Font::segoe`, `draw_text`,
  `restore_alpha` and safe `Surface::pixels`/`pixels_mut`, so pixel loops run outside
  `unsafe`. Removed `Agent::hook_source` and the unused `now` argument of
  `View::status` and `View::compact`. Tests: `enumerated_settings_*` in
  `tests/config.rs`, `a_connected_client_that_stalls_*` in `tests/ipc.rs`; existing
  pixel and config tests guard the refactors. Limits: the `https.rs` handle closes with
  `WinHttpCloseHandle`, so it cannot share `local::Handle`; `[resident] tray_enabled`
  was already gone; test-only helpers (`pet::for_notification`, `Animation::index_at`,
  `AssetCache::retained_bytes`) stay because tests use them. Source:
  `audit-20261026.md` A2, A5, A7.

- [x] **AH-038 — Claude follow-ups.**
  Done (2026-10-27, `feature/AH-038`): Claude quota polling honors `Retry-After` (seconds
  form) capped at 15 minutes and never earlier than `refresh_interval_sec` (driver tests with
  a fake clock, loopback header test); `SessionEnd` clears that session's pending cards
  without a card or sound and reaches only a running resident (`Event.ended`; queue,
  hook-parse and resident IPC tests); `SubagentStop` stays ignored by decision; terminal
  evidence, per-session waits, session names, console window capture and activation
  selection go through `Agent`. Also fixed: an empty header slice reached
  `WinHttpSendRequest` as a dangling pointer. Not done, split to AH-059: Claude session
  names and the statusLine quota fallback. Not verified live: `SessionEnd` and a real 429
  `Retry-After` from Claude Code.
