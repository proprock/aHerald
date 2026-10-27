//! The card as premultiplied BGRA pixels in memory, and the composition of its artwork
//! frames and countdown badge on top. The macOS counterpart of the Windows `Surface` and
//! `Composition` (`overlay.rs`); only the text engine differs.
use super::text;
use crate::{
    animation::{PreparedAnimation, PreparedFrame},
    card_paint::{self, CardText},
};
use std::sync::Arc;

pub struct Surface {
    pub width: i32,
    pub height: i32,
    pixels: Vec<u8>,
}

impl Surface {
    pub fn allocate(width: i32, height: i32) -> Result<Self, String> {
        if width <= 0 || height <= 0 {
            return Err("card has no size".into());
        }
        Ok(Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        })
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    /// The base card: shapes from `card_paint`, premultiplied, with its text on top.
    pub fn card(
        width: i32,
        height: i32,
        scale: f64,
        text_left: i32,
        content: CardText<'_>,
    ) -> Result<Self, String> {
        let painted = card_paint::paint(width, height, scale, text_left, &content);
        let mut surface = Self::allocate(width, height)?;
        for (to, from) in surface
            .pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(painted.pixels.as_chunks::<4>().0)
        {
            let alpha = u16::from(from[3]);
            *to = [
                ((u16::from(from[0]) * alpha + 127) / 255) as u8,
                ((u16::from(from[1]) * alpha + 127) / 255) as u8,
                ((u16::from(from[2]) * alpha + 127) / 255) as u8,
                from[3],
            ];
        }
        text::draw_runs(&mut surface.pixels, width, height, &painted.text)?;
        Ok(surface)
    }
    /// Height of a body text wrapped to `width`, at least one line.
    pub fn body_height(width: i32, scale: f64, label: &str) -> Result<i32, String> {
        let line = (18.0 * scale) as i32;
        Ok(text::measure_wrapped(width, 18.0 * scale, label)?.max(line))
    }
    /// Draws the remaining-seconds badge over the pixels already in this surface.
    pub fn draw_countdown(&mut self, seconds: u64, scale: f64) -> Result<(), String> {
        let run = card_paint::countdown_run(self.width, seconds, scale);
        text::draw_runs(&mut self.pixels, self.width, self.height, &[run])
    }
    /// Draws `base` with one artwork frame blended over it into this surface, which
    /// must have the same dimensions as `base`.
    pub fn blend_artwork(
        &mut self,
        base: &Surface,
        frame: &PreparedFrame,
        left: i32,
        top: i32,
    ) -> Result<(), String> {
        if (self.width, self.height) != (base.width, base.height)
            || left < 0
            || top < 0
            || left as u32 + frame.width > self.width as u32
            || top as u32 + frame.height > self.height as u32
            || frame.bgra.len() != frame.width as usize * frame.height as usize * 4
        {
            return Err("artwork does not fit notification card".into());
        }
        self.pixels.copy_from_slice(&base.pixels);
        // Frames and card pixels are premultiplied BGRA; the bounds were checked above.
        let width = self.width as usize;
        for y in 0..frame.height as usize {
            for x in 0..frame.width as usize {
                let src = &frame.bgra[(y * frame.width as usize + x) * 4..][..4];
                let offset = ((y + top as usize) * width + x + left as usize) * 4;
                for c in 0..4 {
                    self.pixels[offset + c] = (u16::from(src[c])
                        + (u16::from(self.pixels[offset + c]) * (255 - u16::from(src[3])) + 127)
                            / 255)
                        .min(255) as u8;
                }
            }
        }
        Ok(())
    }
}

/// A card with its artwork frames: the base card is drawn once, each frame blends over it.
pub struct Composition {
    base: Surface,
    work: Surface,
    art: Option<Arc<PreparedAnimation>>,
    art_left: i32,
    /// What `work` currently shows: frame and countdown second.
    shown: Option<(usize, Option<u64>)>,
    pub durations: Vec<u64>,
    pub looping: bool,
    pub text_left: i32,
    /// Artwork was available but the text card was used instead.
    pub degraded: bool,
}

impl Composition {
    /// Sizes the card by the largest artwork frame. Any composition failure, or a card
    /// too large to hold twice within the byte budget, falls back to the plain text card;
    /// only a failing text card is an error.
    pub fn compose(
        scale: f64,
        art: Option<Arc<PreparedAnimation>>,
        make_card: impl Fn(i32, i32, i32) -> Result<Surface, String>,
    ) -> Result<Self, String> {
        let (base_width, base_height) = ((320.0 * scale) as i32, crate::card::height(scale));
        let has_art = art.as_ref().is_some_and(|a| !a.frames.is_empty());
        if let Some(art) = art.as_ref().filter(|a| !a.frames.is_empty()) {
            let padding = (18.0 * scale) as i32;
            let art_width = art.frames.iter().map(|f| f.width).max().unwrap_or(0) as i32;
            let art_height = art.frames.iter().map(|f| f.height).max().unwrap_or(0) as i32;
            let width = base_width + art_width + padding;
            let height = base_height.max(art_height + padding * 2);
            let bytes = width as u64 * height as u64 * 4 * 2;
            if bytes <= crate::animation::MAX_BYTES
                && let Ok(base) = make_card(width, height, art_width + padding * 2)
                && let Ok(work) = Surface::allocate(width, height)
            {
                return Ok(Self {
                    base,
                    work,
                    art: Some(art.clone()),
                    art_left: padding,
                    shown: None,
                    durations: art.durations_ms.clone(),
                    looping: art.looping,
                    text_left: art_width + padding * 2,
                    degraded: false,
                });
            }
        }
        let text_left = (22.0 * scale) as i32;
        Ok(Self {
            base: make_card(base_width, base_height, text_left)?,
            work: Surface::allocate(base_width, base_height)?,
            art: None,
            art_left: 0,
            shown: None,
            durations: vec![1000],
            looping: false,
            text_left,
            degraded: has_art,
        })
    }
    pub fn frame_count(&self) -> usize {
        self.art.as_ref().map_or(1, |a| a.frames.len())
    }
    pub fn size(&self) -> (i32, i32) {
        (self.base.width, self.base.height)
    }
    /// Draws `frame`, with the countdown badge when `seconds` is given, into the working
    /// surface and returns it. Asking again for what is shown costs nothing.
    pub fn render(
        &mut self,
        frame: usize,
        seconds: Option<u64>,
        scale: f64,
    ) -> Result<&Surface, String> {
        let frame = frame.min(self.frame_count() - 1);
        if self.shown != Some((frame, seconds)) {
            self.shown = None;
            match &self.art {
                Some(art) => {
                    let f = &art.frames[frame];
                    let top = (self.base.height - f.height as i32) / 2;
                    self.work.blend_artwork(&self.base, f, self.art_left, top)?;
                }
                None => self.work.pixels.copy_from_slice(&self.base.pixels),
            }
            if let Some(seconds) = seconds {
                self.work.draw_countdown(seconds, scale)?;
            }
            self.shown = Some((frame, seconds));
        }
        Ok(&self.work)
    }
    /// Redraws the base card's text at the same size; the artwork frames are untouched.
    pub fn update_card(
        &mut self,
        make_card: impl FnOnce(i32, i32, i32) -> Result<Surface, String>,
    ) -> Result<(), String> {
        self.base = make_card(self.base.width, self.base.height, self.text_left)?;
        self.shown = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{card::Heading, config::Color};

    fn text_card(width: i32, height: i32, left: i32) -> Result<Surface, String> {
        Surface::card(
            width,
            height,
            1.0,
            left,
            CardText {
                heading: &Heading::from("Codex"),
                label: "Turn complete",
                body: false,
                index: 0,
                count: 1,
                dismiss_button: false,
                age: None,
                color: Color([101, 220, 178]),
                background_color: Color([28, 35, 45]),
                border: None,
            },
        )
    }
    fn pixel(surface: &Surface, x: usize, y: usize) -> [u8; 4] {
        surface.pixels()[(y * surface.width as usize + x) * 4..][..4]
            .try_into()
            .unwrap()
    }
    /// Frame `index` is an opaque block whose blue channel identifies it.
    fn numbered(count: usize, width: u32, height: u32) -> PreparedAnimation {
        PreparedAnimation {
            frames: (0..count)
                .map(|i| PreparedFrame {
                    width,
                    height,
                    bgra: [i as u8 + 1, 0, 0, 255].repeat((width * height) as usize),
                })
                .collect(),
            durations_ms: vec![40; count],
            looping: true,
        }
    }
    fn region(s: &Surface, x: std::ops::Range<usize>, y: std::ops::Range<usize>) -> Vec<u8> {
        y.flat_map(|y| x.clone().flat_map(move |x| pixel(s, x, y)))
            .collect()
    }

    #[test]
    fn a_card_is_premultiplied_with_transparent_corners_and_an_event_coloured_stripe() {
        let card = text_card(320, 150, 22).unwrap();
        assert_eq!(pixel(&card, 0, 0), [0, 0, 0, 0]);
        let [b, g, r, a] = pixel(&card, 2, 75);
        // The stripe keeps the card's 242 alpha; the colour is premultiplied by it.
        assert_eq!([r, g, b, a], [96, 209, 169, 242]);
        let [b, g, r, a] = pixel(&card, 300, 140);
        assert_eq!(a, 242);
        assert!(b <= a && g <= a && r <= a);
        assert!((i32::from(r) - 28 * 242 / 255).abs() <= 1);
    }

    #[test]
    fn a_card_draws_its_label_in_the_text_area_only() {
        let with = text_card(320, 150, 22).unwrap();
        let without = Surface::card(
            320,
            150,
            1.0,
            22,
            CardText {
                heading: &Heading::from("Codex"),
                label: "",
                body: false,
                index: 0,
                count: 1,
                dismiss_button: false,
                age: None,
                color: Color([101, 220, 178]),
                background_color: Color([28, 35, 45]),
                border: None,
            },
        )
        .unwrap();
        let differing = |x: std::ops::Range<usize>, y: std::ops::Range<usize>| {
            region(&with, x.clone(), y.clone()) != region(&without, x, y)
        };
        assert!(differing(22..300, 40..110));
        assert!(!differing(0..320, 120..150));
    }

    #[test]
    fn a_zero_sized_surface_is_an_error() {
        assert!(Surface::allocate(0, 10).is_err());
        assert!(Surface::allocate(10, -1).is_err());
    }

    #[test]
    fn body_height_grows_with_the_text_and_never_drops_below_one_line() {
        let one = Surface::body_height(300, 1.0, "short").unwrap();
        let many = Surface::body_height(300, 1.0, &"word ".repeat(60)).unwrap();
        assert!(one >= 18 && many > one * 3);
        assert!(Surface::body_height(300, 1.0, "").unwrap() >= 18);
    }

    #[test]
    fn countdown_is_drawn_per_frame_and_removed_without_touching_the_base() {
        let art = Arc::new(numbered(1, 40, 40));
        let mut c = Composition::compose(1.0, Some(art), text_card).unwrap();
        let (w, _) = c.size();
        let badge = |s: &Surface| region(s, w as usize - 80..w as usize - 18, 16..40);
        let plain = badge(c.render(0, None, 1.0).unwrap());
        let counted = badge(c.render(0, Some(7), 1.0).unwrap());
        assert_ne!(plain, counted);
        assert_eq!(badge(c.render(0, None, 1.0).unwrap()), plain);
    }

    #[test]
    fn card_is_sized_by_the_largest_frame() {
        let art = PreparedAnimation {
            frames: vec![
                PreparedFrame {
                    width: 40,
                    height: 40,
                    bgra: vec![255; 40 * 40 * 4],
                },
                PreparedFrame {
                    width: 64,
                    height: 150,
                    bgra: vec![255; 64 * 150 * 4],
                },
            ],
            durations_ms: vec![40, 40],
            looping: true,
        };
        let c = Composition::compose(1.0, Some(Arc::new(art)), text_card).unwrap();
        assert_eq!(c.frame_count(), 2);
        assert_eq!(c.size(), (402, 186));
        assert_eq!((c.text_left, c.looping, c.degraded), (100, true, false));
        assert_eq!(c.durations, [40, 40]);
    }

    #[test]
    fn rendering_a_frame_leaves_no_trace_of_the_previous_one() {
        let mut c =
            Composition::compose(1.0, Some(Arc::new(numbered(3, 40, 40))), text_card).unwrap();
        let (x, y) = (30, 75);
        assert_eq!(pixel(c.render(1, None, 1.0).unwrap(), x, y)[0], 2);
        assert_eq!(pixel(c.render(2, None, 1.0).unwrap(), x, y)[0], 3);
        assert_eq!(pixel(c.render(99, None, 1.0).unwrap(), x, y)[0], 3);
        assert_eq!(pixel(c.render(0, None, 1.0).unwrap(), x, y)[0], 1);
    }

    #[test]
    fn missing_or_failing_artwork_falls_back_to_one_text_card() {
        let none = Composition::compose(1.0, None, text_card).unwrap();
        assert_eq!(
            (none.size(), none.frame_count(), none.degraded),
            ((320, 150), 1, false)
        );
        let art = Some(Arc::new(numbered(2, 40, 40)));
        let failing = |w: i32, h: i32, l: i32| {
            if w > 320 {
                Err("no".into())
            } else {
                text_card(w, h, l)
            }
        };
        let c = Composition::compose(1.0, art, failing).unwrap();
        assert_eq!(
            (c.size(), c.frame_count(), c.degraded),
            ((320, 150), 1, true)
        );
        assert!(Composition::compose(1.0, None, |_, _, _| Err("no".into())).is_err());
    }

    #[test]
    fn artwork_that_does_not_fit_is_an_error() {
        let base = text_card(320, 150, 22).unwrap();
        let mut work = Surface::allocate(320, 150).unwrap();
        let frame = PreparedFrame {
            width: 40,
            height: 40,
            bgra: vec![0; 40 * 40 * 4],
        };
        assert!(work.blend_artwork(&base, &frame, 300, 0).is_err());
        assert!(work.blend_artwork(&base, &frame, -1, 0).is_err());
        assert!(work.blend_artwork(&base, &frame, 10, 10).is_ok());
    }

    #[test]
    fn artwork_blends_over_the_card_by_its_alpha() {
        let base = text_card(320, 150, 22).unwrap();
        let mut work = Surface::allocate(320, 150).unwrap();
        let half = PreparedFrame {
            width: 2,
            height: 2,
            bgra: [0, 0, 100, 100].repeat(4),
        };
        work.blend_artwork(&base, &half, 150, 60).unwrap();
        let under = pixel(&base, 150, 60);
        let [b, _, r, a] = pixel(&work, 150, 60);
        assert_eq!(b, ((u16::from(under[0]) * 155 + 127) / 255) as u8);
        assert_eq!(r, (100 + (u16::from(under[2]) * 155 + 127) / 255) as u8);
        assert_eq!(a, 100 + ((u16::from(under[3]) * 155 + 127) / 255) as u8);
    }

    #[test]
    fn updating_the_card_keeps_frames_and_size() {
        let mut c =
            Composition::compose(1.0, Some(Arc::new(numbered(2, 40, 40))), text_card).unwrap();
        let size = c.size();
        c.update_card(text_card).unwrap();
        assert_eq!((c.size(), c.frame_count()), (size, 2));
        assert_eq!(pixel(c.render(1, None, 1.0).unwrap(), 30, 75)[0], 2);
    }
}
