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
        self.paint(
            size,
            |remaining| match remaining {
                Some(v) if v > 60.0 => [85, 180, 34, 255],
                Some(v) if v > 20.0 => [0, 165, 216, 255],
                Some(_) => [68, 68, 224, 255],
                None => [133, 133, 133, 255],
            },
            [85, 85, 85, 255],
        )
    }

    /// The same bars in black with only the alpha varying, for a template image: macOS tints
    /// it black or white to match the menu bar. Missing data is a dimmer full-height bar.
    pub fn render_mono(&self, size: u32) -> Result<Vec<u8>, &'static str> {
        self.paint(
            size,
            |remaining| match remaining {
                Some(_) => [0, 0, 0, 255],
                None => [0, 0, 0, 110],
            },
            [0, 0, 0, 70],
        )
    }

    fn paint(
        &self,
        size: u32,
        fill_color: impl Fn(Option<f64>) -> [u8; 4],
        track: [u8; 4],
    ) -> Result<Vec<u8>, &'static str> {
        if !(8..=256).contains(&size) {
            return Err("invalid tray icon size");
        }
        let mut pixels = vec![0; (size * size * 4) as usize];
        let margin = ((size + 8) / 16).max(1);
        let gap = ((size + 4) / 8).max(1);
        let height = size - 2 * margin;
        let space = size - 2 * margin - gap;
        // One group per agent (two when split), each holding its 5h then 7d bar,
        // so a split icon reads as two single-agent icons side by side.
        let mut bars = Vec::with_capacity(4);
        match self.split_right {
            Some(right) => {
                for (group, remaining) in [self.remaining, right].into_iter().enumerate() {
                    let g = group as u32;
                    let left = margin + g * space / 2 + g * gap;
                    let width = (g + 1) * space / 2 - g * space / 2;
                    for (window, remaining) in remaining.into_iter().enumerate() {
                        let w = window as u32;
                        bars.push((remaining, left + w * width / 2, left + (w + 1) * width / 2));
                    }
                }
            }
            None => {
                for (window, remaining) in self.remaining.into_iter().enumerate() {
                    let w = window as u32;
                    let left = margin + w * space / 2 + w * gap;
                    bars.push((remaining, left, left + (w + 1) * space / 2 - w * space / 2));
                }
            }
        }
        for (remaining, start, end) in bars {
            let color = fill_color(remaining);
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
                        track
                    };
                    pixels[offset..offset + 4].copy_from_slice(&pixel);
                }
            }
        }
        Ok(pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels(remaining: [Option<f64>; 2]) -> Levels {
        Levels {
            remaining,
            split_right: None,
            display: UsageDisplay::Remain,
        }
    }

    fn alpha(pixels: &[u8], size: u32, x: u32, y: u32) -> u8 {
        pixels[((y * size + x) * 4 + 3) as usize]
    }

    #[test]
    fn the_monochrome_icon_is_black_with_alpha_only_so_the_system_can_tint_it() {
        let size = 32;
        let pixels = levels([Some(50.0), None]).render_mono(size).unwrap();
        assert_eq!(pixels.len(), (size * size * 4) as usize);
        // Colour channels stay zero everywhere (premultiplied black).
        assert!(pixels.chunks(4).all(|p| p[..3] == [0, 0, 0]));
        let (left, right, bottom, top) = (6, 24, size - 5, 5);
        // Known level: filled part opaque, empty part a faint track.
        assert_eq!(alpha(&pixels, size, left, bottom), 255);
        let track = alpha(&pixels, size, left, top);
        assert!((1..255).contains(&track), "{track}");
        // Missing data is drawn as a full-height dim bar, apart from a known level.
        let unknown = alpha(&pixels, size, right, bottom);
        assert!(unknown > track && unknown < 255, "{unknown}");
        assert_eq!(unknown, alpha(&pixels, size, right, top));
        assert_eq!(alpha(&pixels, size, 0, 0), 0);
    }

    #[test]
    fn the_monochrome_icon_rejects_sizes_the_colour_icon_rejects() {
        assert!(levels([None; 2]).render_mono(4).is_err());
    }
}
