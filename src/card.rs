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
/// The two heading rows of a card: `<harness>[, <folder>]` and, when the session has a
/// readable name, the name. A session ID is never shown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Heading {
    pub title: String,
    pub name: Option<String>,
}
impl Heading {
    pub fn new(harness: &str, folder: Option<&str>, name: Option<&str>) -> Self {
        Self {
            title: match folder {
                Some(folder) => format!("{harness}, {folder}"),
                None => harness.into(),
            },
            name: name.map(Into::into),
        }
    }
    /// A usage card's first line, `<agent>: <summary>`, as two rows. Text that does not
    /// start with a single short word and a colon stays whole in the first row.
    pub fn from_summary(line: &str) -> Self {
        match line.split_once(": ") {
            Some((agent, summary))
                if !agent.is_empty()
                    && agent.len() <= 12
                    && !agent.contains(char::is_whitespace)
                    && !summary.trim().is_empty() =>
            {
                Self::new(agent, None, Some(summary.trim()))
            }
            _ => Self::from(line),
        }
    }
    /// The same text as one line, for the window title that assistive tools read.
    pub fn window_text(&self) -> String {
        match &self.name {
            Some(name) => format!("{}: {name}", self.title),
            None => self.title.clone(),
        }
    }
}
impl From<&str> for Heading {
    fn from(title: &str) -> Self {
        Self::new(title, None, None)
    }
}
/// Card height in logical pixels: three text rows and the button row, whatever the queue holds.
const HEIGHT: f64 = 150.0;
pub fn height(scale: f64) -> i32 {
    (HEIGHT * scale) as i32
}
pub struct Card {
    pub heading: Heading,
    pub index: usize,
    pub count: usize,
    pub navigation: bool,
    pub dismiss_button: bool,
    /// Left edge of the text column in pixels; `None` is the artwork-less default.
    pub text_left: Option<i32>,
}
impl Card {
    pub fn new(heading: impl Into<Heading>, index: usize, count: usize, click: bool) -> Self {
        Self {
            heading: heading.into(),
            index,
            count,
            navigation: count > 1,
            dismiss_button: !click,
            text_left: None,
        }
    }
    /// `n/m` for the queue position, shown only when several cards are queued.
    pub fn counter(&self) -> Option<String> {
        self.navigation
            .then(|| format!("{}/{}", self.index + 1, self.count))
    }
    fn row(height: i32, scale: f64) -> (i32, i32) {
        (
            height - (34.0 * scale) as i32,
            height - (10.0 * scale) as i32,
        )
    }
    fn text_column(&self, scale: f64) -> i32 {
        self.text_left.unwrap_or((22.0 * scale) as i32)
    }
    /// Bordered buttons of the bottom row: Dismiss at the left, the arrows at the right
    /// with the counter between them.
    pub fn buttons(&self, width: i32, height: i32, scale: f64) -> Vec<(Action, Area)> {
        let (top, bottom) = Self::row(height, scale);
        let area = |left, right| Area {
            left,
            top,
            right,
            bottom,
        };
        let mut buttons = Vec::new();
        if self.dismiss_button {
            let left = self.text_column(scale);
            buttons.push((Action::Dismiss, area(left, left + (96.0 * scale) as i32)));
        }
        if self.navigation {
            let next = width - (18.0 * scale) as i32;
            let next_left = next - (28.0 * scale) as i32;
            let previous = next_left - (44.0 * scale) as i32;
            buttons.push((Action::Next, area(next_left, next)));
            buttons.push((
                Action::Previous,
                area(previous - (28.0 * scale) as i32, previous),
            ));
        }
        buttons
    }
    /// Where the counter text goes: between the arrows.
    pub fn counter_area(&self, width: i32, height: i32, scale: f64) -> Area {
        let (top, bottom) = Self::row(height, scale);
        let right = width - (18.0 * scale) as i32 - (28.0 * scale) as i32;
        Area {
            left: right - (44.0 * scale) as i32,
            top,
            right,
            bottom,
        }
    }
    /// Where the age text goes: right of the Dismiss button (or at the text column without
    /// one) and clear of the arrows.
    pub fn age_area(&self, width: i32, height: i32, scale: f64) -> Area {
        let (top, bottom) = Self::row(height, scale);
        let gap = (10.0 * scale) as i32;
        let left = if self.dismiss_button {
            self.text_column(scale) + (96.0 * scale) as i32 + gap
        } else {
            self.text_column(scale)
        };
        let limit = if self.navigation {
            self.counter_area(width, height, scale).left - (28.0 * scale) as i32 - gap
        } else {
            width - (18.0 * scale) as i32
        };
        Area {
            left,
            top,
            right: limit.max(left),
            bottom,
        }
    }
    /// Body excludes controls, independent of the dismissal setting.
    pub fn body_hit_test(&self, width: i32, height: i32, scale: f64, x: i32, y: i32) -> bool {
        x >= 0
            && y >= 0
            && x < width
            && y < height
            && !self
                .buttons(width, height, scale)
                .into_iter()
                .any(|(_, r)| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
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
/// How long ago an event arrived, coarse enough that the card redraws once a minute at most.
pub fn age_text(elapsed_ms: u64) -> String {
    const MINUTE: u64 = 60_000;
    match elapsed_ms {
        0..MINUTE => "just now".into(),
        ms if ms < 60 * MINUTE => format!("{} min ago", ms / MINUTE),
        ms if ms < 24 * 60 * MINUTE => format!("{} h ago", ms / (60 * MINUTE)),
        ms => format!("{} d ago", ms / (24 * 60 * MINUTE)),
    }
}
pub fn remaining_seconds(duration_ms: u64, elapsed_ms: u64) -> Option<u64> {
    (duration_ms > elapsed_ms).then(|| (duration_ms - elapsed_ms).div_ceil(1000))
}
