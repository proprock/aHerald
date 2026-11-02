# Active originating session — AH-098

Two independent options default to `true`:

```toml
[notifications]
suppress_when_focused = true

[sounds]
suppress_when_focused = true
```

Each option controls only its own output. Suppression requires positive evidence
that the originating window is focused **and** its active session matches the event.
Unfocused windows, different sessions, unknown focus, unknown session identity and
failed platform queries retain delivery. Existing global/event output switches,
volume, configured sound paths and pause still apply.

The resident samples focus at event admission; transient mode samples in the
display child, after its readiness acknowledgement. Suppressed cards never appear
later after focus changes (agreed 2026-11-01). The resident keeps hidden waits for
deduplication and matching terminal evidence, expires hidden timed events, and
excludes hidden entries from navigation/counts. Visible events can displace hidden
entries at capacity, so a hidden high-priority wait cannot block another session's
visible event. Sounds are considered once, independently of card selection.
Hook replies, permission decisions and lifecycle adapters are unchanged. No new
worker, dependency, desktop permission, transcript access or payload logging is used.

## Identification coverage

| Platform/source | Evidence | Behavior |
| --- | --- | --- |
| Windows, Claude hook in a classic console | Existing hook ancestor-console capture supplies a visible `ConsoleWindowClass` HWND; a valid main-session ID is required; foreground HWND must match | Suppression supported for this single console surface |
| Windows, Claude hook in Windows Terminal/other tabbed hosts | Root-owner HWND can identify a host, but not its selected tab/session | Deliver; no session guessed from host focus |
| Windows, Claude Desktop | Existing `cliSessionId` mapping supplies a deep link, not active-session evidence | Deliver; no additional session-file fields read |
| Windows, Codex CLI/Desktop | Session ID alone does not identify the active client window/session; hooks may run under an app-server | Deliver; no private server started to observe another client |
| macOS, both agents | Existing terminal capture identifies an application PID, not a selected window/tab; no active desktop-session adapter | Deliver |
| Linux, both agents | Existing X11 capture can identify a host window, not its selected tab/session; no Wayland active-session adapter | Deliver |
| Any source with missing/invalid window or session, subagent, demo, application notice or usage | Insufficient origin evidence or no originating agent session | Deliver according to existing output rules |

Classic-console support assumes the hook belongs to the session using that console
surface; concurrent background agents sharing a single console do not have a native
selected-session concept. This implementation does not extend suppression to them
or to terminal multiplexers. Desktop same/different/unknown session outcomes are
covered by the pure policy tests; they do **not** establish a working desktop adapter.

## Sources checked 2026-11-01

- [Official Codex hook documentation](https://learn.chatgpt.com/docs/hooks):
  shared session IDs and event-specific responses. It does not establish a native
  active-window/session query for this integration.
- Installed `codex-cli 0.166.0-alpha.5.1`; generated its current app-server JSON
  schemas with `codex app-server generate-json-schema --out target/ah-098/protocol`.
  No `activeSession`, `active_session`, `foreground`, `windowId` or `window_id` match
  was found in that export. This is a bounded schema check, not proof that no such
  capability exists elsewhere. No Codex hook schema or adapter was changed.
- [Microsoft GetConsoleWindow](https://learn.microsoft.com/en-us/windows/console/getconsolewindow):
  a pseudoconsole's handle is a non-displayed message-queue surrogate. Matching its
  owner to a foreground terminal is not evidence of a selected tab.
- Existing aHerald `activation/claude_desktop`, Windows `console_window`, macOS
  `terminal` and Linux `activation` adapters were inspected. Their activation
  capabilities are not treated as active-session observation capabilities.

## Automated evidence

Windows 11, OS build `10.0.22621`; installed Codex version above was read, but no
new live Codex or Claude session dispatch was claimed.

- Red: the new configuration keys failed parsing before implementation.
- Red: independent-output policy failed before implementation.
- Red: a suppressed wait hid another session's event; a separate capacity-one
  test reproduced rejection of a visible event behind a hidden wait.
- Green: defaults and all independent option combinations; focused/unfocused/unknown
  focus crossed with same/different/unknown sessions; retained output decisions
  through resident admission, once-only announcements, terminal resolution even
  with completion disabled, hidden-event expiration/navigation, deduplication and
  capacity isolation. Muted events consume no playback slots; hidden cards are
  not charged for time before arrival. Both edge cases failed before their fixes.
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all-targets`, and
  `cargo build --release --target x86_64-pc-windows-msvc`: passed.
  Final full suite: **510 passed, 16 ignored** (the ignored checks include manual
  audio/UI and opt-in live-account checks; they were not counted as passes).

The full test run in the sandbox stopped on
`simultaneous_transient_cards_do_not_overlap`; the focused test reproduced that
failure there. Without code changes, the focused test and then the full suite
passed outside the sandbox on the interactive Windows desktop. Root cause inside
the sandbox remains undiagnosed. The successful full log is
`target/ah-098/tests-all-targets.log`. Existing installed aHerald processes were
left untouched. No macOS/Linux build or native UI result is claimed.

## Live acceptance progress (2026-11-01)

The user ran the native synthetic-event probe on the interactive Windows desktop.
Read-only native queries confirmed that HWND `3151618` was a visible, valid
`ConsoleWindowClass` window. With both suppression options enabled:

- Focused `claude`: `focused=true`, `session=Same`, `window=false`, `sound=false`,
  `accepted=true`. The user confirmed no card or sound, and no deferred card after
  changing focus.
- Unfocused `delayed`: `focused=false`, `session=Same`, `window=true`, `sound=true`,
  `accepted=true`. The user confirmed the expected card and sound without focus theft.

The records were read from `target/ah-098/live-both.jsonl` and match the user's
reported output. Two earlier records in the same file have unknown focus/session:
read-only inspection of HWND `6818932` found `CASCADIA_HOSTING_WINDOW_CLASS`
(Windows Terminal). The user supplied screenshots of cards for both commands in
that host. Audio there was not separately confirmed. This establishes visible
fail-open delivery, not selected-tab identification.

The four test configurations initially used unsupported `visual = 'text'`;
this was corrected to `auto`. All four then passed configuration loading and
private-resident startup/quit through the native probe. A fresh build and four
focused policy/configuration/queue tests passed in this continuation.

At that point, independent output combinations, unknown-origin checks and real
terminal/Desktop hook dispatch remained to be observed. Synthetic events do not
establish actual agent hook capture.

## Live acceptance progress (2026-11-02)

The user reported routine real-hook completion and permission delivery from
terminal/Desktop sessions. Interruption, input-request and failure cards were
not observed; no manual acceptance of those separate adapter events is claimed.

During follow-up, the installed resident was found to be version 0.5.0, which
rejected the new suppression settings. It was updated to the tested 0.9.0-beta
build from this branch. The existing configuration was backed up, its five
duration values converted from milliseconds to seconds without changing their
times, and loaded successfully by the new executable. The restarted installed
resident answered status with visuals and sounds enabled.

After that update, the user ran the two-tab Windows Terminal scenario: start a
real agent task in one tab, switch to another tab of the same focused window,
and wait for completion. For both Claude and Codex, the user confirmed a visible
card, audible sound and preserved focus. This is user-observed real-hook
fail-open delivery when active-tab identity is unknown, not a selected-tab
identification result. Agent versions were not recorded in this follow-up.

The earlier daily-use report predated discovery of the unsupported installed
version. The user subsequently repeated the focused classic-console check on
the updated build and explicitly confirmed that card and sound suppression work
both together and independently. This is user-reported manual evidence; no new
automated capture of those observations is claimed. Fully missing origin is
covered by automated tests; the live tabbed-host case above provides
unknown-session evidence.

AH-098 is complete on 2026-11-02 with the identification coverage documented
above. The combined and independent output checks, prior focused/unfocused
observations and no-deferred-card check, real terminal/Desktop delivery report,
and two-agent tabbed-terminal acceptance complete the manual evidence. The
pre-PR Windows checks passed: formatting, Clippy, 538 tests (17 ignored), release
build and native release-icon loading. macOS/Linux execution and separate live
interruption/input-request/failure adapter events were not newly verified.

## Initial manual attempt and repeatable checks

After explicit user authorization, the dedicated native probe was launched on
2026-11-01, then restarted with a normal visible-window launch. Its process and
private resident started, but Computer Use returned no targetable probe window.
The evidence file remained empty. Computer Use also prohibits terminal automation;
the user was asked to enter the probe commands and report card/audio observations.
The user was away and could not observe the probe. Both owned test processes were
then stopped; the installed resident was left running.
That initial attempt supplied no screenshot, audible result or live terminal/Desktop acceptance.
The later user observations above supersede this initial attempt's evidence gap.

Build the opt-in probe with:

```text
cargo build --bin aherald --example active_session_probe
```

Run it from a classic Windows console, supplying absolute paths:

```text
active_session_probe.exe AHERALD_EXE CONFIG EVIDENCE_JSONL
```

The probe creates a private resident scope and sends synthetic sanitized events;
it does not edit global hooks, user configuration, accounts or autostart. Type
`quit` for cleanup. A no-argument UI launch uses the build's `target/ah-098/preview.toml`
and `evidence.jsonl`; create the config first. Configure a real WAV, a 15-second
completion duration and zero fades to make observation unambiguous.

1. With both options true, `claude` in the focused classic console must produce no
   card/sound. `delayed` waits eight seconds: focus another app before delivery;
   the card and sound must now appear without stealing focus.
2. Repeat with `(notifications, sounds).suppress_when_focused` set to
   `(false, true)`, `(true, false)`, and `(false, false)`, restarting the probe with
   each config. Observe only the independently allowed outputs.
3. After a suppressed event, change focus and verify it never appears later.
   `other` supplies no originating window and must still notify.
4. `codex` represents an unknown Codex origin and must notify even while Codex
   Desktop is focused. Also check real hook dispatch from a terminal and from
   Desktop; a synthetic event alone cannot establish real hook capture.
5. In a tabbed terminal, focus another tab in the same host. Events must still
   notify because active-tab identity is unknown. Desktop events from either the
   active or another session currently deliver for the same reason.

Record actual app versions, observed foreground/window identity, card visibility,
audio and hook response/lifetime separately. Do not check AH-098 until the agreed
live acceptance is observed, including its documented fail-open coverage.
