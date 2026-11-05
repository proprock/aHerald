# AH-098: active originating session

Branch: `feature/ah-098-active-session`.

1. Add default-on, independent `suppress_when_focused` options to notifications and sounds.
2. Test output decisions for all option combinations, known/unknown focus, and
   same/different/unknown active sessions before implementing the policy.
3. Require positive platform evidence. Preserve delivery for ambiguous desktop
   sessions, terminal tabs, missing identities, subagents and failed native queries.
4. Apply the visual policy without losing queued lifecycle state or blocking other
   sessions. Apply the sound policy before asset loading and playback.
5. Run focused tests, formatting, Clippy, the full test suite and Windows release build.
6. Visually check a focused/unfocused real terminal, independent output options,
   and fail-open desktop delivery. Record synthetic-hook evidence separately from
   actual agent dispatch. Leave AH-098 open if its live acceptance is incomplete.

No dependency additions, hook response changes, transcript inspection, desktop
session-file expansion or global setup changes are needed.
