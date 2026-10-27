//! Where a card goes on macOS: AppKit's screens (points, origin at the bottom-left of the
//! primary display) in, the window frame (points, bottom-left origin) out. The placement
//! rules themselves are the shared ones in `placement`; this only converts coordinates.
use crate::{
    config::{Monitor, Position},
    placement::Area,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// One display as AppKit reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    pub frame: Rect,
    /// The part not covered by the menu bar or the Dock.
    pub visible: Rect,
    /// Backing pixels per point (2.0 on Retina).
    pub scale: f64,
}

/// A window frame in points, origin at the bottom-left like `NSWindow.setFrame`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub rect: Rect,
    /// Backing pixels per point of the screen the card landed on.
    pub scale: f64,
}

/// Index of the display the next card appears on: the configured one, or the one under
/// the cursor for a card near the cursor. The first screen is the primary one.
pub fn choose(
    screens: &[Screen],
    monitor: Monitor,
    position: Position,
    cursor: (f64, f64),
) -> usize {
    if position == Position::NearCursor
        && let Some(under) = screens.iter().position(|s| contains(&s.frame, cursor))
    {
        return under;
    }
    match monitor {
        Monitor::Index(n) if n < screens.len() => n,
        _ => 0,
    }
}

fn contains(rect: &Rect, (x, y): (f64, f64)) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

/// Frame for a card of `card` backing pixels on `screen`.
pub fn place(screen: &Screen, card: (i32, i32), position: Position, cursor: (f64, f64)) -> Frame {
    let (frame, visible, scale) = (screen.frame, screen.visible, screen.scale);
    // The shared rules work in pixels with the origin at the screen's top-left corner.
    let left = ((visible.x - frame.x) * scale).round() as i32;
    let top = ((frame.y + frame.height - (visible.y + visible.height)) * scale).round() as i32;
    let area = Area {
        left,
        top,
        right: left + (visible.width * scale).round() as i32,
        bottom: top + (visible.height * scale).round() as i32,
    };
    let local_cursor = (
        ((cursor.0 - frame.x) * scale).round() as i32,
        ((frame.y + frame.height - cursor.1) * scale).round() as i32,
    );
    let (x, y, width, height) =
        crate::placement::layout_rendered(area, card, scale, position, local_cursor);
    Frame {
        rect: Rect {
            x: frame.x + f64::from(x) / scale,
            y: frame.y + frame.height - f64::from(y + height) / scale,
            width: f64::from(width) / scale,
            height: f64::from(height) / scale,
        },
        scale,
    }
}

/// Frame for a card hanging under the status item `anchor` (global points), right-aligned
/// with it and kept inside the visible area, shrunk when it does not fit.
pub fn place_below(screen: &Screen, anchor: Rect, card: (i32, i32)) -> Frame {
    let (visible, scale) = (screen.visible, screen.scale);
    let (mut width, mut height) = (f64::from(card.0) / scale, f64::from(card.1) / scale);
    let gap = 6.0;
    let room = (visible.height - gap).max(1.0);
    let factor = (visible.width / width.max(1.0))
        .min(room / height.max(1.0))
        .min(1.0);
    (width, height) = (width * factor, height * factor);
    let right = (anchor.x + anchor.width).min(visible.x + visible.width);
    let x = (right - width).clamp(
        visible.x,
        (visible.x + visible.width - width).max(visible.x),
    );
    // The item sits in the menu bar, which lies above the visible area.
    let top = anchor.y.min(visible.y + visible.height) - gap;
    let y = (top - height).max(visible.y);
    Frame {
        rect: Rect {
            x,
            y,
            width,
            height,
        },
        scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }
    /// 1440x900 points, Retina, 25-point menu bar on top.
    fn laptop() -> Screen {
        Screen {
            frame: rect(0.0, 0.0, 1440.0, 900.0),
            visible: rect(0.0, 0.0, 1440.0, 875.0),
            scale: 2.0,
        }
    }
    /// A 1920x1080 external display to the right, not Retina, its top aligned with the laptop's.
    fn external() -> Screen {
        Screen {
            frame: rect(1440.0, -180.0, 1920.0, 1080.0),
            visible: rect(1440.0, -180.0, 1920.0, 1080.0),
            scale: 1.0,
        }
    }

    #[test]
    fn a_centred_card_is_halved_to_points_and_centred_in_the_visible_area() {
        let f = place(&laptop(), (640, 300), Position::Center, (0.0, 0.0));
        assert_eq!(f.scale, 2.0);
        assert_eq!((f.rect.width, f.rect.height), (320.0, 150.0));
        assert_eq!(f.rect.x, 560.0);
        assert_eq!(f.rect.y, (875.0 - 150.0) / 2.0);
    }

    #[test]
    fn a_top_centre_card_sits_a_margin_below_the_menu_bar() {
        let f = place(&laptop(), (640, 300), Position::TopCenter, (0.0, 0.0));
        // 24 points of margin at scale 2.0 are 48 pixels.
        assert_eq!(f.rect.y + f.rect.height, 875.0 - 24.0);
    }

    #[test]
    fn a_bottom_right_card_keeps_its_margin_from_both_edges() {
        let f = place(&laptop(), (640, 300), Position::BottomRight, (0.0, 0.0));
        assert_eq!(f.rect.x + f.rect.width, 1440.0 - 24.0);
        assert_eq!(f.rect.y, 24.0);
    }

    #[test]
    fn a_card_larger_than_the_screen_is_shrunk_to_fit() {
        let f = place(&laptop(), (4000, 300), Position::Center, (0.0, 0.0));
        assert_eq!((f.rect.x, f.rect.width), (0.0, 1440.0));
    }

    #[test]
    fn a_card_on_a_screen_with_an_offset_origin_uses_global_coordinates() {
        let f = place(&external(), (320, 150), Position::BottomRight, (0.0, 0.0));
        assert_eq!(f.scale, 1.0);
        assert_eq!(f.rect.x + f.rect.width, 1440.0 + 1920.0 - 24.0);
        assert_eq!(f.rect.y, -180.0 + 24.0);
    }

    #[test]
    fn a_card_near_the_cursor_starts_a_little_below_and_right_of_it() {
        // Cursor at (700, 600) in AppKit coordinates, 300 points below the screen's top.
        let f = place(&laptop(), (640, 300), Position::NearCursor, (700.0, 600.0));
        assert_eq!(f.rect.x, 716.0);
        assert_eq!(f.rect.y + f.rect.height, 600.0 - 16.0);
    }

    #[test]
    fn the_configured_monitor_wins_and_a_missing_one_falls_back_to_the_primary() {
        let screens = [laptop(), external()];
        let cursor = (2000.0, 0.0);
        assert_eq!(
            choose(&screens, Monitor::Primary, Position::Center, cursor),
            0
        );
        assert_eq!(
            choose(&screens, Monitor::Index(1), Position::Center, cursor),
            1
        );
        assert_eq!(
            choose(&screens, Monitor::Index(5), Position::Center, cursor),
            0
        );
        assert_eq!(choose(&[], Monitor::Primary, Position::Center, cursor), 0);
    }

    #[test]
    fn near_cursor_follows_the_display_under_the_cursor() {
        let screens = [laptop(), external()];
        assert_eq!(
            choose(
                &screens,
                Monitor::Primary,
                Position::NearCursor,
                (2000.0, 0.0)
            ),
            1
        );
        assert_eq!(
            choose(
                &screens,
                Monitor::Primary,
                Position::NearCursor,
                (100.0, 100.0)
            ),
            0
        );
        assert_eq!(
            choose(
                &screens,
                Monitor::Primary,
                Position::NearCursor,
                (-500.0, 0.0)
            ),
            0
        );
    }

    #[test]
    fn a_card_hangs_under_the_status_item_and_ends_at_its_right_edge() {
        let anchor = rect(1300.0, 875.0, 40.0, 25.0);
        let f = place_below(&laptop(), anchor, (1520, 700));
        assert_eq!((f.rect.width, f.rect.height, f.scale), (760.0, 350.0, 2.0));
        assert_eq!(f.rect.x + f.rect.width, 1340.0);
        // A small gap below the menu bar.
        assert_eq!(f.rect.y + f.rect.height, 875.0 - 6.0);
    }

    #[test]
    fn a_card_under_an_item_near_an_edge_stays_on_screen() {
        let left = place_below(&laptop(), rect(100.0, 875.0, 40.0, 25.0), (1520, 700));
        assert_eq!(left.rect.x, 0.0);
        let right = place_below(&laptop(), rect(1420.0, 875.0, 40.0, 25.0), (1520, 700));
        assert_eq!(right.rect.x + right.rect.width, 1440.0);
    }

    #[test]
    fn a_card_taller_than_the_visible_area_is_shrunk_in_proportion() {
        let f = place_below(&laptop(), rect(1300.0, 875.0, 40.0, 25.0), (1520, 4000));
        assert!(f.rect.height <= 875.0 - 6.0 && f.rect.y >= 0.0);
        assert!((f.rect.width / f.rect.height - 1520.0 / 4000.0).abs() < 0.01);
    }
}
