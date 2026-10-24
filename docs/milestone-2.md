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
