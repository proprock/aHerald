# Milestone 2 verification

## Implementation plan

1. AH-009: user/logon-scoped ownership and local-only named pipe, explicit ACLs,
   native lifetime and access tests.
2. AH-010: versioned bounded frames, cancellable overlapped I/O, bounded retries,
   detached startup, event forwarding, status and quit.
3. AH-011: bounded priority/FIFO queue with deterministic identity deduplication;
   one resident card with event count, navigation and selected dismissal.
4. AH-012: native tray, Explorer recreation, runtime pause/toggles, usage-unavailable
   actions and orderly shutdown of owned workers/resources.

Keep the shared transient renderer and local assets. Never send permission decisions.
No new third-party dependency or asynchronous runtime is introduced. Additional
`windows` features expose only the Win32 IPC/security APIs required by this milestone.

## AH-009

The production resource scope combines the current token's user SID and logon SID.
The mutex is in the Local namespace and the pipe is local-only. A protected DACL
allows data access to the logon SID, explicitly denies NETWORK, and limits implicit
owner rights to READ_CONTROL so another logon of the same user cannot rewrite it.
Handles are noninheritable. The pipe rejects remote clients and uses first-instance
creation; an existing server cannot be silently replaced. Ownership is RAII and
thread-affine because Windows mutex release must occur on its owning thread.

Native tests exercise a live competing owner, duplicate pipe rejection, scope
identity, process termination/reacquisition and an actual denied mutex/pipe open
against a foreign-logon ACL. No other user's account or remote host is required.
The initial ownership tests were added alongside the implementation; no observed
behavioral red phase is claimed for this slice. Compilation setup failures were
corrected separately and are not counted as a red phase.

References checked 2026-10-24:
- https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights
- https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
- https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-connectnamedpipe
- https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-server-using-overlapped-i-o

Tests run under the existing Windows 11 build 22621 environment. Cross-account,
remote-host and separate interactive-logon end-to-end tests remain environmental
limits; the foreign-logon ACL test verifies the local access denial directly.

Passed for AH-009: formatting, Clippy with warnings denied, all 46 tests (one audio-device test ignored), and the x64 MSVC release build.

## AH-010

Red: the frame round-trip test failed against the framing stub; the startup-race
test failed against the resident-startup stub. Native exchange then reproduced
ERROR_NO_DATA because DisconnectNamedPipe can discard unread response bytes.
The final protocol waits for the client's response-consumption acknowledgement
before disconnecting. No blocking FlushFileBuffers is used.

Frames contain little-endian version 1 (u16), payload length (u32), and at most
4096 bytes of validated JSON. Read/write stages have 250ms deadlines and connect
waits are separately bounded. Pending named-pipe I/O is canceled and drained
before borrowed OVERLAPPED/buffer storage is released. The client uses identification
SQOS; the server never impersonates a client. Slow, disconnected, oversized,
incompatible and malformed clients do not terminate the listener. The server uses
one connection at a time and a bounded event channel; later queue policy is AH-011.

Resident startup retries are bounded to 1.5 seconds plus an in-flight bounded
exchange. Competing detached starters use the same scoped ownership; only one
serves. Hooks acknowledge transport independently of presentation and never emit
permission decisions. --status/--quit do not start an absent resident. An internal
namespace allows isolated process integration tests without touching the user's
normal resident. Public operation uses the fixed resident namespace for this logon.

Native tests cover four concurrent starters, same PID status, event acknowledgement,
graceful quit/resource release, stalled clients, malformed frames and subsequent
successful exchanges. Ordinary transient delivery remains unchanged. At this commit
the resident owns temporary display children; AH-011 replaces these with the single
resident event card. No child survives a graceful quit.

Passed for AH-010: formatting, Clippy with warnings denied, all 49 tests (one audio-device test ignored), and the x64 MSVC release build.

## AH-011

Red: deterministic queue tests failed against the queue stub. The native resident
window test failed against AH-010, which created separate display processes.
Green: priority/FIFO ordering, overflow, identity-based cross-source deduplication,
unrelated sessions, dismissed-event history, persistent selection, selected-only
dismissal, navigation and paused display budgets pass. Queue capacity is 64;
higher-priority arrivals evict the oldest lowest-priority entry when full.

The resident now owns one nonactivating layered card and no display children.
Its title and visible source show the selected index/count; Prev, Next and Dismiss
operate on the selected event. Asset decoding uses a bounded worker channel.
Native integration exchanges real IPC messages, observes one window for two events,
switches selection, dismisses one entry and verifies the surviving event and count.
It also checks that the card does not become the foreground window. This is
programmatic Windows UI evidence; visual inspection of the final tray/card remains
part of AH-012's manual acceptance.

Passed: formatting, Clippy with warnings denied, all 53 tests (one audio-device
test ignored), and the x64 MSVC release build.

## AH-012

Red: the resident integration test failed because WM_COMMAND Pause had no effect.
The cancellation regression failed because an already-canceled audio job still
waited for the serial playback gate. Both pass after implementation. Native menu
inspection checks all eight actions and the current sound/visual/pause checkmarks.
The resident integration test executes Pause/Resume, accepts events while paused,
toggles sound/visual state, opens each usage/configuration action and exits while
an information window remains open. Ownership is then reacquired. Audio tests
interrupt pending completion and repeat cooldown without requiring an audio device.

Shell_NotifyIconW uses a shared system icon and version 4 callbacks. TaskbarCreated
re-adds it; a bounded periodic retry also tolerates Explorer being absent at startup.
Native menus, icon registration, windows, audio handles, decoding worker and IPC
listener have explicit lifetime cleanup. Information windows are modeless and owned
by the resident so they cannot prevent Quit. Runtime toggles are not persisted.
Usage is explicitly unavailable; no polling or fabricated counters are introduced.
Pause freezes selected display time and suppresses presentation/audio while IPC
validation/queueing continues. Actual terminal-event resolution is deferred to M3.

The opt-in Explorer integration test passed in the interactive user session:
it observes the registered icon, deletes only that icon, sends the registered
TaskbarCreated message, waits for Explorer's asynchronous layout, observes the
restored icon, quits the isolated resident and observes icon removal. The original
immediate layout assertion was corrected to a bounded wait; the expected restored
icon was not changed. The sandbox could not register a notification-area icon;
this desktop check therefore ran outside the sandbox. No Explorer process was
restarted, and no unrelated tray icon or application was changed.

References checked 2026-10-24:
- https://learn.microsoft.com/en-us/windows/win32/shell/taskbar
- https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw
- https://learn.microsoft.com/en-us/windows/win32/shell/notification-area

Passed for AH-012: cargo fmt --all -- --check, cargo clippy --all-targets -- -D warnings, cargo test --all-targets (57 passed, three opt-in tests ignored), cargo build --release --target x86_64-pc-windows-msvc, and check-release.ps1. The Explorer-icon and manual-window opt-in tests also passed separately in the interactive user session. The release is a 1,723,904-byte x64 Windows GUI executable with system DLL imports and no separately distributed CRT.

Additional control regression: an invalid notification TOML originally prevented
--status (and the shared --quit path) from executing. The test failed with that
configuration error, then passed after control operations were moved before TOML
loading. Runtime-disabled resident notifications still accept events so tray
switches can enable presentation later.

Manual evidence: after automated UI fixtures finished, the release executable was
copied to a temporary portable demo folder and started with the external Becky
package and two persistent events. A read-only native probe found one visible
`aHerald.Resident` window, title `aHerald demo - Permission required [1/2]`, plus
only the invisible system IME window. The user confirmed: one window and a working
tray menu. The initial preview was inconclusive because the user reported seeing
small and large cards; the repeated, isolated preview resolved that ambiguity.
The demo was subsequently stopped through --quit. No user hooks/config/autostart
were installed. Windows 11 build 22621 is the tested platform; Windows 10,
multiple monitors/mixed DPI and a complete Explorer process restart remain untested.
