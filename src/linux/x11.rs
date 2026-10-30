//! X11 window and monitor primitives for the Linux notification overlay.
use x11rb::{
    connection::{Connection, RequestConnection},
    protocol::{
        Event,
        randr::ConnectionExt as _,
        render::ConnectionExt as _,
        xproto::{
            self, AtomEnum, ColormapAlloc, ConfigureWindowAux, ConnectionExt as _, CreateGCAux,
            CreateWindowAux, EventMask, ImageFormat, PropMode, WindowClass,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

use crate::{
    config::{Monitor as MonitorChoice, Position},
    placement::Area,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Monitor {
    pub area: Area,
    pub primary: bool,
    pub scale: f64,
}

/// A single non-activating ARGB notification window.
pub struct Window {
    connection: RustConnection,
    screen: usize,
    root: u32,
    window: u32,
    gc: u32,
    opacity: u32,
}

fn err(e: impl std::fmt::Display) -> String {
    format!("X11: {e}")
}

fn xft_scale(resources: &str) -> f64 {
    resources
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            if key.trim().eq_ignore_ascii_case("Xft.dpi") {
                value
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|dpi| dpi.is_finite() && (48.0..=768.0).contains(dpi))
            } else {
                None
            }
        })
        .map_or(1.0, |dpi| dpi / 96.0)
}

fn intersection(a: Area, b: Area) -> Option<Area> {
    let area = Area {
        left: a.left.max(b.left),
        top: a.top.max(b.top),
        right: a.right.min(b.right),
        bottom: a.bottom.min(b.bottom),
    };
    (area.right > area.left && area.bottom > area.top).then_some(area)
}

/// Select a configured display, or the display at the pointer for near-cursor placement.
pub fn select_monitor(
    monitors: &[Monitor],
    choice: MonitorChoice,
    position: Position,
    pointer: (i32, i32),
) -> Option<Monitor> {
    if position == Position::NearCursor
        && choice == MonitorChoice::Primary
        && let Some(monitor) = monitors.iter().find(|m| {
            pointer.0 >= m.area.left
                && pointer.0 < m.area.right
                && pointer.1 >= m.area.top
                && pointer.1 < m.area.bottom
        })
    {
        return Some(*monitor);
    }
    match choice {
        MonitorChoice::Index(index) => monitors
            .get(index)
            .copied()
            .or_else(|| monitors.first().copied()),
        MonitorChoice::Primary => monitors.first().copied(),
    }
}

impl Window {
    pub fn new() -> Result<Self, String> {
        let (connection, screen) = x11rb::connect(None).map_err(err)?;
        let root = connection.setup().roots[screen].root;
        let formats = connection
            .render_query_pict_formats()
            .map_err(err)?
            .reply()
            .map_err(err)?;
        let pictscreen = formats
            .screens
            .get(screen)
            .ok_or("X11: missing screen formats")?;
        let visual = pictscreen
            .depths
            .iter()
            .filter(|d| d.depth == 32)
            .flat_map(|d| &d.visuals)
            .find(|v| {
                formats.formats.iter().any(|f| {
                    f.id == v.format
                        && f.depth == 32
                        && f.direct.red_shift == 16
                        && f.direct.green_shift == 8
                        && f.direct.blue_shift == 0
                        && f.direct.alpha_shift == 24
                        && f.direct.alpha_mask == 255
                        && f.direct.red_mask == 255
                        && f.direct.green_mask == 255
                        && f.direct.blue_mask == 255
                })
            })
            .ok_or("X11: no compatible ARGB visual")?
            .visual;
        let colormap = connection.generate_id().map_err(err)?;
        connection
            .create_colormap(ColormapAlloc::NONE, colormap, root, visual)
            .map_err(err)?
            .check()
            .map_err(err)?;
        let window = connection.generate_id().map_err(err)?;
        connection
            .create_window(
                32,
                window,
                root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                visual,
                &CreateWindowAux::new()
                    .colormap(colormap)
                    .background_pixel(0)
                    .border_pixel(0)
                    .event_mask(
                        EventMask::EXPOSURE | EventMask::BUTTON_PRESS | EventMask::STRUCTURE_NOTIFY,
                    ),
            )
            .map_err(err)?
            .check()
            .map_err(err)?;
        let atom = |name: &[u8]| -> Result<u32, String> {
            Ok(connection
                .intern_atom(false, name)
                .map_err(err)?
                .reply()
                .map_err(err)?
                .atom)
        };
        let window_type = atom(b"_NET_WM_WINDOW_TYPE")?;
        let notification = atom(b"_NET_WM_WINDOW_TYPE_NOTIFICATION")?;
        let state = atom(b"_NET_WM_STATE")?;
        let above = atom(b"_NET_WM_STATE_ABOVE")?;
        let skip_taskbar = atom(b"_NET_WM_STATE_SKIP_TASKBAR")?;
        let skip_pager = atom(b"_NET_WM_STATE_SKIP_PAGER")?;
        let opacity = atom(b"_NET_WM_WINDOW_OPACITY")?;
        connection
            .change_property32(
                PropMode::REPLACE,
                window,
                window_type,
                AtomEnum::ATOM,
                &[notification],
            )
            .map_err(err)?
            .check()
            .map_err(err)?;
        connection
            .change_property32(
                PropMode::REPLACE,
                window,
                state,
                AtomEnum::ATOM,
                &[above, skip_taskbar, skip_pager],
            )
            .map_err(err)?
            .check()
            .map_err(err)?;
        // ICCCM WM_HINTS: InputHint is set and input is false, so the WM should not focus it.
        connection
            .change_property32(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_HINTS,
                AtomEnum::WM_HINTS,
                &[1, 0, 0, 0, 0, 0, 0, 0, 0],
            )
            .map_err(err)?
            .check()
            .map_err(err)?;
        connection
            .change_property8(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                b"aHerald notification",
            )
            .map_err(err)?
            .check()
            .map_err(err)?;
        let gc = connection.generate_id().map_err(err)?;
        connection
            .create_gc(gc, window, &CreateGCAux::new())
            .map_err(err)?
            .check()
            .map_err(err)?;
        connection.flush().map_err(err)?;
        Ok(Self {
            connection,
            screen,
            root,
            window,
            gc,
            opacity,
        })
    }

    /// Current RandR monitors in stable primary-first order, with root geometry as fallback.
    pub fn monitors(&self) -> Result<Vec<Monitor>, String> {
        let root = &self.connection.setup().roots[self.screen];
        let resource = self
            .connection
            .get_property(
                false,
                self.root,
                AtomEnum::RESOURCE_MANAGER,
                AtomEnum::STRING,
                0,
                65536,
            )
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| String::from_utf8_lossy(&r.value).into_owned())
            .unwrap_or_default();
        let scale = xft_scale(&resource);
        let mut monitors = self
            .connection
            .randr_get_monitors(self.root, true)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|r| r.monitors)
            .unwrap_or_default()
            .into_iter()
            .filter(|m| m.width > 0 && m.height > 0)
            .map(|m| Monitor {
                area: Area {
                    left: i32::from(m.x),
                    top: i32::from(m.y),
                    right: i32::from(m.x) + i32::from(m.width),
                    bottom: i32::from(m.y) + i32::from(m.height),
                },
                primary: m.primary,
                // XWayland often reports fabricated physical sizes.
                scale,
            })
            .collect::<Vec<_>>();
        if monitors.is_empty() {
            monitors.push(Monitor {
                area: Area {
                    left: 0,
                    top: 0,
                    right: i32::from(root.width_in_pixels),
                    bottom: i32::from(root.height_in_pixels),
                },
                primary: true,
                scale,
            });
        }
        // EWMH gives one work area per desktop in root coordinates. It can be absent on
        // compositors without a full EWMH implementation; monitor bounds remain usable.
        let atom = |name: &[u8]| {
            self.connection
                .intern_atom(true, name)
                .ok()?
                .reply()
                .ok()
                .map(|r| r.atom)
                .filter(|a| *a != 0)
        };
        let property = |a| {
            self.connection
                .get_property(false, self.root, a, AtomEnum::CARDINAL, 0, 4096)
                .ok()?
                .reply()
                .ok()
                .and_then(|r| r.value32().map(|v| v.collect::<Vec<_>>()))
        };
        if let (Some(desktops), Some(workareas)) = (
            atom(b"_NET_CURRENT_DESKTOP").and_then(property),
            atom(b"_NET_WORKAREA").and_then(property),
        ) {
            let index = desktops.first().copied().unwrap_or(0) as usize;
            if let Some(values) =
                workareas.get(index.saturating_mul(4)..index.saturating_mul(4).saturating_add(4))
            {
                let (x, y) = (values[0] as i32, values[1] as i32);
                let area = Area {
                    left: x,
                    top: y,
                    right: x.saturating_add(values[2].min(i32::MAX as u32) as i32),
                    bottom: y.saturating_add(values[3].min(i32::MAX as u32) as i32),
                };
                for monitor in &mut monitors {
                    if let Some(work) = intersection(monitor.area, area) {
                        monitor.area = work;
                    }
                }
            }
        }
        monitors.sort_by_key(|m| (!m.primary, m.area.left, m.area.top));
        Ok(monitors)
    }

    pub fn pointer(&self) -> Result<(i32, i32), String> {
        let p = self
            .connection
            .query_pointer(self.root)
            .map_err(err)?
            .reply()
            .map_err(err)?;
        Ok((i32::from(p.root_x), i32::from(p.root_y)))
    }

    /// Present a premultiplied BGRA image. Each request contains whole rows and stays
    /// below the server's negotiated maximum request size.
    pub fn show(
        &self,
        x: i32,
        y: i32,
        width: u16,
        height: u16,
        pixels: &[u8],
        alpha: u8,
    ) -> Result<(), String> {
        let row = usize::from(width) * 4;
        if width == 0 || height == 0 || pixels.len() != row * usize::from(height) {
            return Err("X11: invalid image dimensions".into());
        }
        if self.connection.setup().image_byte_order != xproto::ImageOrder::LSB_FIRST {
            return Err("X11: unsupported image byte order".into());
        }
        let max = self.connection.maximum_request_bytes().saturating_sub(64);
        let rows = (max / row).min(usize::from(u16::MAX));
        if rows == 0 {
            return Err("X11: image row exceeds request limit".into());
        }
        let pixmap = self.connection.generate_id().map_err(err)?;
        self.connection
            .create_pixmap(32, pixmap, self.window, width, height)
            .map_err(err)?
            .check()
            .map_err(err)?;
        self.connection
            .configure_window(
                self.window,
                &ConfigureWindowAux::new()
                    .x(x)
                    .y(y)
                    .width(u32::from(width))
                    .height(u32::from(height)),
            )
            .map_err(err)?
            .check()
            .map_err(err)?;
        self.connection
            .change_property32(
                PropMode::REPLACE,
                self.window,
                self.opacity,
                AtomEnum::CARDINAL,
                &[u32::from(alpha) * 0x01010101],
            )
            .map_err(err)?
            .check()
            .map_err(err)?;
        for (i, chunk) in pixels.chunks(row * rows).enumerate() {
            let start = i * rows;
            let count = chunk.len() / row;
            let offset = i16::try_from(start).map_err(|_| "X11: image too tall")?;
            self.connection
                .put_image(
                    ImageFormat::Z_PIXMAP,
                    pixmap,
                    self.gc,
                    width,
                    count as u16,
                    0,
                    offset,
                    0,
                    32,
                    chunk,
                )
                .map_err(err)?
                .check()
                .map_err(err)?;
        }
        self.connection
            .change_window_attributes(
                self.window,
                &xproto::ChangeWindowAttributesAux::new().background_pixmap(pixmap),
            )
            .map_err(err)?
            .check()
            .map_err(err)?;
        self.connection
            .free_pixmap(pixmap)
            .map_err(err)?
            .check()
            .map_err(err)?;
        self.connection
            .clear_area(false, self.window, 0, 0, width, height)
            .map_err(err)?
            .check()
            .map_err(err)?;
        self.connection
            .map_window(self.window)
            .map_err(err)?
            .check()
            .map_err(err)?;
        self.connection.flush().map_err(err)
    }

    pub fn hide(&self) -> Result<(), String> {
        self.connection
            .unmap_window(self.window)
            .map_err(err)?
            .check()
            .map_err(err)?;
        self.connection.flush().map_err(err)
    }

    pub fn poll(&self) -> Result<Option<Event>, String> {
        self.connection.poll_for_event().map_err(err)
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.connection.destroy_window(self.window);
        let _ = self.connection.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xft_dpi_uses_valid_values_and_rejects_bad_resources() {
        assert_eq!(xft_scale("other: 123\nXft.dpi: 144\n"), 1.5);
        assert_eq!(xft_scale("Xft.dpi: NaN"), 1.0);
        assert_eq!(xft_scale("Xft.dpi: 99999"), 1.0);
        assert_eq!(xft_scale(""), 1.0);
    }

    #[test]
    fn work_area_is_clipped_to_the_monitor_with_negative_coordinates() {
        let monitor = Area {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        let work = Area {
            left: -1920,
            top: 40,
            right: 1920,
            bottom: 1080,
        };
        assert_eq!(
            intersection(monitor, work),
            Some(Area { top: 40, ..monitor })
        );
        assert_eq!(
            intersection(
                monitor,
                Area {
                    left: 0,
                    top: 0,
                    right: 10,
                    bottom: 10
                }
            ),
            None
        );
    }

    #[test]
    fn monitor_selection_honors_explicit_index_and_near_cursor() {
        let monitors = [
            Monitor {
                area: Area {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                },
                primary: true,
                scale: 1.0,
            },
            Monitor {
                area: Area {
                    left: -1280,
                    top: 0,
                    right: 0,
                    bottom: 1024,
                },
                primary: false,
                scale: 1.0,
            },
        ];
        assert_eq!(
            select_monitor(
                &monitors,
                MonitorChoice::Primary,
                Position::NearCursor,
                (-200, 400)
            ),
            Some(monitors[1])
        );
        assert_eq!(
            select_monitor(
                &monitors,
                MonitorChoice::Index(0),
                Position::NearCursor,
                (-200, 400)
            ),
            Some(monitors[0])
        );
        assert_eq!(
            select_monitor(&monitors, MonitorChoice::Index(9), Position::Center, (0, 0)),
            Some(monitors[0])
        );
        assert_eq!(
            select_monitor(&[], MonitorChoice::Primary, Position::Center, (0, 0)),
            None
        );
    }

    #[test]
    #[ignore = "requires an interactive X11 or XWayland display"]
    fn live_window_is_argb_non_focusing_and_has_notification_properties() {
        let w = Window::new().expect("open X11 notification window");
        let focus = w
            .connection
            .get_input_focus()
            .unwrap()
            .reply()
            .unwrap()
            .focus;
        let pixels = [0u8, 0, 128, 128].repeat(80 * 40);
        w.show(20, 20, 80, 40, &pixels, 255).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let state = w
                .connection
                .get_window_attributes(w.window)
                .unwrap()
                .reply()
                .unwrap()
                .map_state;
            if state == xproto::MapState::VIEWABLE {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "window was not mapped by the window manager"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let geometry = w
            .connection
            .get_geometry(w.window)
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!(geometry.depth, 32);
        let hints = w
            .connection
            .get_property(
                false,
                w.window,
                AtomEnum::WM_HINTS,
                AtomEnum::WM_HINTS,
                0,
                9,
            )
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!(hints.value32().unwrap().take(2).collect::<Vec<_>>(), [1, 0]);
        let type_atom = w
            .connection
            .intern_atom(false, b"_NET_WM_WINDOW_TYPE")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        let notification = w
            .connection
            .intern_atom(false, b"_NET_WM_WINDOW_TYPE_NOTIFICATION")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        let property = w
            .connection
            .get_property(false, w.window, type_atom, AtomEnum::ATOM, 0, 1)
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!(property.value32().unwrap().next(), Some(notification));
        assert_eq!(
            w.connection
                .get_input_focus()
                .unwrap()
                .reply()
                .unwrap()
                .focus,
            focus
        );
        assert!(!w.monitors().unwrap().is_empty());
        w.hide().unwrap();
    }
}
