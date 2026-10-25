# Artwork selection

Standard animations load automatically from `assets/` beside `aherald.exe`.
Cargo stages the repository resources there for debug and release builds.
Distribute the executable together with `assets/`; artwork stays external and
replaceable. Runtime never searches the repository or working directory.
Sound selection is independent and still requires configured WAV paths.

## All builtin (default)

No configuration is required. `notifications.visual = 'auto'` without a pet uses
the standard animation for each event. To keep builtin as the main style even
when pet manifests are configured, set `[notifications] visual = 'builtin'`.

## All pet

```toml
[notifications]
pet = 'characters/my-pet/pet.json'
```

Default `visual = 'auto'` switches all inherited events to the selected pet.
`[agents.codex].pet` takes priority over the shared manifest. Stored event
`animation` paths do not interfere. Shared/agent `pet_animations` maps select
the track for each event; see [pet tracks](pets.md).

## Hybrid

Use pet generally, with builtin error artwork:

```toml
[notifications]
pet = 'characters/my-pet/pet.json'

[events.error]
visual = 'builtin'
```

Or builtin generally, with pet for completion only:

```toml
[notifications]
visual = 'builtin'
pet = 'characters/my-pet/pet.json'

[events.turn_complete]
visual = 'pet'
```

For an external custom animation:

```toml
[events.error]
visual = 'custom'
animation = 'my-artwork/error'
```

## Rules and migration

Global `notifications.visual`: `auto` (default), `builtin`, `pet`.
Per-event `visual`: `inherit` (default), `builtin`, `pet`, `custom`.
Explicit event choices override the main style. Only `custom` uses `animation`;
it requires a path. Unknown choices are configuration errors.
See the complete commented [`aHerald.example.toml`](../aHerald.example.toml).

Missing/corrupt/unsupported selected artwork, including scale/memory-limit failures,
falls back to the event's builtin
animation, then a text card if that resource is unavailable too. Explicit `pet`
without a manifest also uses this recovery. A broken agent pet does not silently
switch characters by using the shared manifest. Loading stays on existing workers.

Previously, any `animation` path took priority over a pet. Add `visual = 'custom'`
to events whose external artwork should remain an intentional exception. Paths
with `visual = 'inherit'` are now ignored, so adding a pet replaces inherited
artwork automatically. The resource demo retains only scale, sound paths and
volume overrides. Restart resident/demo to reload configuration or changed assets.

## Verification

Red: the source-selection test failed because defaults yielded no artwork, and
the configuration test rejected `visual = 'auto'`. Executable tests cover all
five builtin/pet selections, both hybrid directions, unused legacy paths,
explicit custom pixels and builtin-to-text recovery. Native tests inspect
artwork-sized windows for default and failed-custom selections, then a text-sized
window from an isolated executable with no adjacent assets.

On Windows 11 build 22621, `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`,
`cargo test --all-targets -- --test-threads=1` (112 passed, 7 opt-in checks ignored),
and `cargo build --release --target x86_64-pc-windows-msvc` passed.
Visual inspection confirmed an inherited cat pet for permission waits, builtin
red error artwork, and an explicit purple custom animation for input waits.
The opt-in demo preview closes after one pass through its five event types and
navigation scene; its 45-second deadline is a failure-safety bound.
The user-facing demo remains continuous.
The bounded preview completed successfully in 21.67 seconds; its ordinary
nonactivating/lifecycle test, formatting and clippy also passed after this change.

The first full run encountered an app-server test timeout during concurrent
compilation; that test and the subsequent serial full run passed. Another run
exposed a native navigation test's fixed coordinates after default artwork
expanded the card. The test now waits for artwork and clicks relative to window
bounds, also asserting that navigation retains both queued events.
No new Codex protocol integration was introduced. Other Windows versions and
additional monitor/DPI arrangements were not manually verified for this change.
