//! The card as premultiplied BGRA pixels in memory, and the composition of its artwork
//! frames on top of it. Shared by the Windows (GDI DIB section) and macOS (plain buffer)
//! surfaces; each platform supplies only its pixel storage and its countdown text.
use crate::animation::{PreparedAnimation, PreparedFrame};
use std::sync::Arc;

/// A card-sized premultiplied BGRA pixel buffer owned by one platform.
pub trait CardSurface: Sized {
    fn allocate(width: i32, height: i32) -> Result<Self, String>;
    fn width(&self) -> i32;
    fn height(&self) -> i32;
    fn pixels(&self) -> &[u8];
    fn pixels_mut(&mut self) -> &mut [u8];
    /// Called after the pixels change, so a platform can drop anything derived from them.
    fn pixels_changed(&mut self) {}
    /// Draws the remaining-seconds badge over the pixels already in this surface.
    fn draw_countdown(&mut self, seconds: u64, scale: f64) -> Result<(), String>;

    /// Replaces this surface's pixels with `base`'s (same dimensions).
    fn copy_from(&mut self, base: &Self) {
        debug_assert_eq!((self.width(), self.height()), (base.width(), base.height()));
        self.pixels_mut().copy_from_slice(base.pixels());
        self.pixels_changed();
    }

    /// Draws `base` with one artwork frame blended over it into this surface, which must have
    /// the same dimensions as `base`.
    fn blend_artwork(
        &mut self,
        base: &Self,
        frame: &PreparedFrame,
        left: i32,
        top: i32,
    ) -> Result<(), String> {
        let (width, height) = (self.width(), self.height());
        if (width, height) != (base.width(), base.height())
            || left < 0
            || top < 0
            || left as u32 + frame.width > width as u32
            || top as u32 + frame.height > height as u32
            || frame.bgra.len() != frame.width as usize * frame.height as usize * 4
        {
            return Err("artwork does not fit notification card".into());
        }
        self.copy_from(base);
        // Frames and card pixels are premultiplied BGRA; the bounds were checked above.
        let width = width as usize;
        let pixels = self.pixels_mut();
        for y in 0..frame.height as usize {
            for x in 0..frame.width as usize {
                let src = &frame.bgra[(y * frame.width as usize + x) * 4..][..4];
                let offset = ((y + top as usize) * width + x + left as usize) * 4;
                for c in 0..4 {
                    pixels[offset + c] = (u16::from(src[c])
                        + (u16::from(pixels[offset + c]) * (255 - u16::from(src[3])) + 127) / 255)
                        .min(255) as u8;
                }
            }
        }
        Ok(())
    }
}

/// A card with its artwork frames: the base card is drawn once, each frame blends over it.
pub struct Composition<S: CardSurface> {
    base: S,
    work: S,
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

impl<S: CardSurface> Composition<S> {
    /// Sizes the card by the largest artwork frame. Any composition failure, or a card too
    /// large to hold twice within the byte budget, falls back to the plain text card; only a
    /// failing text card is an error. The artwork's own size is already bounded when it is
    /// prepared.
    pub fn compose(
        scale: f64,
        art: Option<Arc<PreparedAnimation>>,
        make_card: impl Fn(i32, i32, i32) -> Result<S, String>,
    ) -> Result<Self, String> {
        let _timer = crate::perf::span(crate::perf::Span::CompositionBuild);
        crate::perf::count(crate::perf::Counter::CompositionRebuild);
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
                && let Ok(work) = S::allocate(width, height)
            {
                crate::perf::gauge(crate::perf::Gauge::CompositionBytes, bytes);
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
        crate::perf::gauge(
            crate::perf::Gauge::CompositionBytes,
            base_width as u64 * base_height as u64 * 4 * 2,
        );
        Ok(Self {
            base: make_card(base_width, base_height, text_left)?,
            work: S::allocate(base_width, base_height)?,
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
        (self.base.width(), self.base.height())
    }

    /// Draws `frame`, with the countdown badge when `seconds` is given, into the working
    /// surface and returns it. Asking again for what is shown costs nothing.
    pub fn render(&mut self, frame: usize, seconds: Option<u64>, scale: f64) -> Result<&S, String> {
        let frame = frame.min(self.frame_count() - 1);
        if self.shown != Some((frame, seconds)) {
            let _timer = crate::perf::span(crate::perf::Span::FrameRender);
            self.shown = None;
            match &self.art {
                Some(art) => {
                    let f = &art.frames[frame];
                    let top = (self.base.height() - f.height as i32) / 2;
                    self.work.blend_artwork(&self.base, f, self.art_left, top)?;
                }
                None => self.work.copy_from(&self.base),
            }
            if let Some(seconds) = seconds {
                self.work.draw_countdown(seconds, scale)?;
            }
            self.shown = Some((frame, seconds));
        }
        Ok(&self.work)
    }

    /// Redraws the base card's text (heading, counter, age) at the same size; the artwork is
    /// blended at render time, so it and its frames are untouched.
    pub fn update_card(
        &mut self,
        make_card: impl FnOnce(i32, i32, i32) -> Result<S, String>,
    ) -> Result<(), String> {
        self.base = make_card(self.base.width(), self.base.height(), self.text_left)?;
        self.shown = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain buffer whose countdown marks the first pixel, so tests need no text engine.
    struct Plain {
        width: i32,
        height: i32,
        pixels: Vec<u8>,
        changes: u32,
    }
    impl CardSurface for Plain {
        fn allocate(width: i32, height: i32) -> Result<Self, String> {
            if width <= 0 || height <= 0 {
                return Err("card has no size".into());
            }
            Ok(Self {
                width,
                height,
                pixels: vec![0; width as usize * height as usize * 4],
                changes: 0,
            })
        }
        fn width(&self) -> i32 {
            self.width
        }
        fn height(&self) -> i32 {
            self.height
        }
        fn pixels(&self) -> &[u8] {
            &self.pixels
        }
        fn pixels_mut(&mut self) -> &mut [u8] {
            &mut self.pixels
        }
        fn pixels_changed(&mut self) {
            self.changes += 1;
        }
        fn draw_countdown(&mut self, seconds: u64, _scale: f64) -> Result<(), String> {
            self.pixels[0] = seconds as u8;
            Ok(())
        }
    }

    /// A text card: a flat opaque colour whose red channel records the text offset.
    fn text_card(width: i32, height: i32, left: i32) -> Result<Plain, String> {
        let mut card = Plain::allocate(width, height)?;
        for pixel in card.pixels.as_chunks_mut::<4>().0 {
            *pixel = [10, 20, left as u8, 255];
        }
        Ok(card)
    }
    fn pixel(surface: &Plain, x: usize, y: usize) -> [u8; 4] {
        surface.pixels[(y * surface.width as usize + x) * 4..][..4]
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
        assert!(Composition::<Plain>::compose(1.0, None, |_, _, _| Err("no".into())).is_err());
    }

    #[test]
    fn artwork_beyond_the_byte_budget_keeps_the_text_card() {
        let huge = Arc::new(numbered(1, 4000, 4000));
        let c = Composition::compose(1.0, Some(huge), text_card).unwrap();
        assert_eq!(
            (c.size(), c.frame_count(), c.degraded),
            ((320, 150), 1, true)
        );
    }

    #[test]
    fn countdown_is_drawn_per_frame_and_removed_without_touching_the_base() {
        let mut c =
            Composition::compose(1.0, Some(Arc::new(numbered(1, 40, 40))), text_card).unwrap();
        // Pixel 0 lies outside the artwork, so it is the base card's blue channel (10).
        assert_eq!(c.render(0, None, 1.0).unwrap().pixels[0], 10);
        assert_eq!(c.render(0, Some(7), 1.0).unwrap().pixels[0], 7);
        assert_eq!(c.render(0, None, 1.0).unwrap().pixels[0], 10);
    }

    #[test]
    fn rendering_what_is_already_shown_draws_nothing_again() {
        let mut c = Composition::compose(1.0, None, text_card).unwrap();
        c.render(0, None, 1.0).unwrap();
        let changes = c.work.changes;
        c.render(0, None, 1.0).unwrap();
        assert_eq!(c.work.changes, changes);
    }

    #[test]
    fn artwork_that_does_not_fit_is_an_error() {
        let base = text_card(320, 150, 22).unwrap();
        let mut work = Plain::allocate(320, 150).unwrap();
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
        let mut work = Plain::allocate(320, 150).unwrap();
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
        assert!(
            work.changes >= 1,
            "derived data is dropped when pixels change"
        );
    }

    #[test]
    fn updating_the_card_keeps_frames_and_size() {
        let mut c =
            Composition::compose(1.0, Some(Arc::new(numbered(2, 40, 40))), text_card).unwrap();
        let (size, left) = (c.size(), c.text_left);
        c.update_card(|w, h, left| text_card(w, h, left + 1))
            .unwrap();
        assert_eq!((c.size(), c.frame_count()), (size, 2));
        assert_eq!(pixel(c.render(1, None, 1.0).unwrap(), 30, 75)[0], 2);
        assert_eq!(
            pixel(c.render(1, None, 1.0).unwrap(), 300, 75)[2],
            left as u8 + 1
        );
    }
}
