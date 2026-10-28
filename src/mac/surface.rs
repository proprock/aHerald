//! The card as premultiplied BGRA pixels in memory, drawn with AppKit text. The artwork
//! composition on top of it is shared with Windows (`card_surface`).
use super::text;
use crate::{
    card_paint::{self, CardText},
    card_surface::CardSurface,
};

pub type Composition = crate::card_surface::Composition<Surface>;

pub struct Surface {
    pub width: i32,
    pub height: i32,
    pixels: Vec<u8>,
}

impl Surface {
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
}

impl CardSurface for Surface {
    fn allocate(width: i32, height: i32) -> Result<Self, String> {
        if width <= 0 || height <= 0 {
            return Err("card has no size".into());
        }
        Ok(Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
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
    fn draw_countdown(&mut self, seconds: u64, scale: f64) -> Result<(), String> {
        let run = card_paint::countdown_run(self.width, seconds, scale);
        text::draw_runs(&mut self.pixels, self.width, self.height, &[run])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        animation::{PreparedAnimation, PreparedFrame},
        card::Heading,
        config::Color,
    };
    use std::sync::Arc;

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
}
