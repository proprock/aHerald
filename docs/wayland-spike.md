# Linux Wayland investigation (AH-073)

## Observed on Ubuntu

On 2026-10-30, Ubuntu 22.04.5 with GNOME Shell 42.9 reported
`XDG_SESSION_TYPE=wayland` and `DISPLAY=:0`. A read-only Wayland registry roundtrip
listed `xdg_wm_base`, `xdg_activation_v1`, and `wl_output`; it did **not** advertise
`zwlr_layer_shell_v1`. This is a measurement of this session, not every GNOME version.
The registry was queried with `wl_display_get_registry`, a listener and a roundtrip
against the current compositor; no additional package was installed.

An isolated resident sent an `input_required` event through IPC. XWayland reported a
viewable 32-bit window, `_NET_WM_WINDOW_TYPE_NOTIFICATION`, `_NET_WM_STATE_ABOVE`,
`WM_HINTS` input=false and opaque-window opacity at full visibility. The active X11
window ID was unchanged before and after the notification. On the available single
monitor the card was 402x150 at 1x scale. The user previously confirmed that the card
works in this window manager. These properties are hints and measurements, not proof
that every compositor will keep the card above every window. Fullscreen, Xorg and
multi-monitor/mixed-DPI cases were not measured here (AH-093).

The user also confirmed that clicking a synthetic Claude notification focuses the Warp
window. Warp does not restore the formerly selected tab or pane. Other terminal process
layouts and native-Wayland-only applications are not covered by that check.

## Native layer-shell option

The [wlr layer-shell protocol](https://wayland.app/protocols/wlr-layer-shell-unstable-v1)
provides layers, output selection and anchoring, with no keyboard interaction by default.
The [gtk4-layer-shell compatibility list](https://github.com/wmww/gtk4-layer-shell#supported-desktops)
lists KDE Plasma and wlroots-based compositors as supported, and GNOME Wayland as not
supported. This is upstream documentation, not a local KDE or wlroots test.

A native backend would need Wayland surface/buffer rendering, output and scale management,
input, window lifecycle, packaging dependencies and separate desktop acceptance. Its output
selection and anchor model does not automatically provide the existing global cursor and
foreign-window activation behavior; those need compositor-specific verification or a
separate fallback. GNOME would still need XWayland or a different integration. These are
engineering scope estimates, not benchmark results.

## Decision

Agreed with the user on 2026-10-30: retain XWayland as the GNOME path. Defer a native
layer-shell backend until KDE/wlroots sessions are available for protocol and behavioral
checks (AH-095). This spike adds no native Wayland backend or dependency.
