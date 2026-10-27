//! Shared notification controls and timer presentation, independent of Win32.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Previous,
    Next,
    Dismiss,
}
pub struct Area {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
pub struct Card {
    pub heading: String,
    pub navigation: bool,
    pub dismiss_button: bool,
}
impl Card {
    pub fn new(source: &str, index: usize, count: usize, click: bool) -> Self {
        Self {
            heading: if count > 1 {
                format!("{source} [{}/{}]", index + 1, count)
            } else {
                source.into()
            },
            navigation: count > 1,
            dismiss_button: !click,
        }
    }
    pub fn buttons(&self, width: i32, height: i32, scale: f64) -> Vec<(Action, Area)> {
        let mut right = width - (18.0 * scale) as i32;
        let mut buttons = Vec::new();
        let mut add = |action, size: f64| {
            let left = right - (size * scale) as i32;
            buttons.push((
                action,
                Area {
                    left,
                    top: height - (34.0 * scale) as i32,
                    right,
                    bottom: height - (10.0 * scale) as i32,
                },
            ));
            right = left;
        };
        if self.navigation {
            add(Action::Next, 42.0);
            add(Action::Previous, 42.0);
        }
        if self.dismiss_button {
            add(Action::Dismiss, 96.0);
        }
        buttons
    }
    pub fn hit_test(&self, width: i32, height: i32, scale: f64, x: i32, y: i32) -> Option<Action> {
        if x < 0 || y < 0 || x >= width || y >= height {
            return None;
        }
        self.buttons(width, height, scale)
            .into_iter()
            .find_map(|(action, r)| {
                (x >= r.left && x < r.right && y >= r.top && y < r.bottom).then_some(action)
            })
            .or_else(|| (!self.dismiss_button).then_some(Action::Dismiss))
    }
}
pub fn remaining_seconds(duration_ms: u64, elapsed_ms: u64) -> Option<u64> {
    (duration_ms > elapsed_ms).then(|| (duration_ms - elapsed_ms).div_ceil(1000))
}
