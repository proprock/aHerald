//! Two quota bars, rendered independently of the Windows notification area.
use crate::{
    config::{TrayGraph, UsageDisplay},
    usage_service::View,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Levels {
    remaining: [Option<f64>; 2],
    split_right: Option<[Option<f64>; 2]>,
    display: UsageDisplay,
}

impl Levels {
    pub fn from_views(
        codex: &View,
        claude: Option<&View>,
        graph: TrayGraph,
        display: UsageDisplay,
    ) -> Self {
        let codex = Self::from_view(codex, display);
        let claude = claude.map_or(
            Self {
                remaining: [None; 2],
                split_right: None,
                display,
            },
            |v| Self::from_view(v, display),
        );
        match graph {
            TrayGraph::Codex => codex,
            TrayGraph::Claude => claude,
            TrayGraph::Both => Self {
                split_right: Some(codex.remaining),
                ..claude
            },
            TrayGraph::Auto => {
                let worst = |levels: &Self| levels.remaining.into_iter().flatten().reduce(f64::min);
                match (worst(&codex), worst(&claude)) {
                    (Some(c), Some(a)) if a < c => claude,
                    (None, Some(_)) => claude,
                    _ => codex,
                }
            }
        }
    }

    pub fn from_view(view: &View, display: UsageDisplay) -> Self {
        let bucket = view
            .snapshot
            .as_ref()
            .filter(|_| view.enabled)
            .and_then(|s| s.buckets.get("codex").or_else(|| s.buckets.values().next()));
        let remaining = bucket.map_or([None, None], |b| {
            [&b.primary, &b.secondary].map(|window| {
                window
                    .as_ref()
                    .and_then(|w| w.used_percent)
                    .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
                    .map(|v| 100.0 - v)
            })
        });
        Self {
            remaining,
            split_right: None,
            display,
        }
    }

    /// Top-down, premultiplied BGRA pixels for a square notification-area icon.
    pub fn render(&self, size: u32) -> Result<Vec<u8>, &'static str> {
        if !(8..=256).contains(&size) {
            return Err("invalid tray icon size");
        }
        let mut pixels = vec![0; (size * size * 4) as usize];
        let margin = ((size + 8) / 16).max(1);
        let gap = ((size + 4) / 8).max(1);
        let height = size - 2 * margin;
        let space = size - 2 * margin - gap;
        for index in 0..2 {
            let i = index as u32;
            let left = margin + i * space / 2 + i * gap;
            let width = (i + 1) * space / 2 - i * space / 2;
            let agents = if self.split_right.is_some() { 2 } else { 1 };
            for agent in 0..agents {
                let remaining = if agent == 0 {
                    self.remaining[index]
                } else {
                    self.split_right.and_then(|r| r[index])
                };
                let start = left + agent * width / agents;
                let end = left + (agent + 1) * width / agents;
                let color = match remaining {
                    Some(v) if v > 60.0 => [85, 180, 34, 255],
                    Some(v) if v > 20.0 => [0, 165, 216, 255],
                    Some(_) => [68, 68, 224, 255],
                    None => [133, 133, 133, 255],
                };
                let percent = remaining.map_or(100.0, |v| match self.display {
                    UsageDisplay::Remain => v,
                    UsageDisplay::Used => 100.0 - v,
                });
                // A one-pixel baseline keeps a known zero distinct from missing data.
                let fill = ((height as f64 * percent / 100.0).round() as u32).max(1);
                for y in margin..margin + height {
                    for x in start..end {
                        let offset = ((y * size + x) * 4) as usize;
                        // Opaque neutral tracks retain the full scale on light and dark taskbars.
                        let pixel = if y >= margin + height - fill {
                            color
                        } else {
                            [85, 85, 85, 255]
                        };
                        pixels[offset..offset + 4].copy_from_slice(&pixel);
                    }
                }
            }
        }
        Ok(pixels)
    }
}
