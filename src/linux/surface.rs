//! CPU card surface with Unicode text rendered by the desktop's Pango and Cairo libraries.
use crate::{
    card_paint::{self, Align, CardText, TextRun},
    card_surface::CardSurface,
};
use std::ffi::{c_char, c_int, c_void};

#[link(name = "libcairo.so.2", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn cairo_image_surface_create_for_data(
        data: *mut u8,
        format: c_int,
        width: c_int,
        height: c_int,
        stride: c_int,
    ) -> *mut c_void;
    fn cairo_surface_status(surface: *mut c_void) -> c_int;
    fn cairo_surface_mark_dirty(surface: *mut c_void);
    fn cairo_surface_flush(surface: *mut c_void);
    fn cairo_surface_destroy(surface: *mut c_void);
    fn cairo_create(surface: *mut c_void) -> *mut c_void;
    fn cairo_status(cr: *mut c_void) -> c_int;
    fn cairo_destroy(cr: *mut c_void);
    fn cairo_save(cr: *mut c_void);
    fn cairo_restore(cr: *mut c_void);
    fn cairo_rectangle(cr: *mut c_void, x: f64, y: f64, width: f64, height: f64);
    fn cairo_clip(cr: *mut c_void);
    fn cairo_move_to(cr: *mut c_void, x: f64, y: f64);
    fn cairo_set_source_rgb(cr: *mut c_void, r: f64, g: f64, b: f64);
}
#[link(
    name = "libpangocairo-1.0.so.0",
    kind = "dylib",
    modifiers = "+verbatim"
)]
unsafe extern "C" {
    fn pango_cairo_create_layout(cr: *mut c_void) -> *mut c_void;
    fn pango_cairo_show_layout(cr: *mut c_void, layout: *mut c_void);
}
#[link(name = "libpango-1.0.so.0", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn pango_font_description_new() -> *mut c_void;
    fn pango_font_description_free(description: *mut c_void);
    fn pango_font_description_set_family(description: *mut c_void, family: *const c_char);
    fn pango_font_description_set_absolute_size(description: *mut c_void, size: f64);
    fn pango_layout_set_font_description(layout: *mut c_void, description: *const c_void);
    fn pango_layout_set_text(layout: *mut c_void, text: *const c_char, length: c_int);
    fn pango_layout_set_width(layout: *mut c_void, width: c_int);
    fn pango_layout_set_wrap(layout: *mut c_void, wrap: c_int);
    fn pango_layout_set_ellipsize(layout: *mut c_void, mode: c_int);
    fn pango_layout_set_alignment(layout: *mut c_void, alignment: c_int);
    fn pango_layout_set_single_paragraph_mode(layout: *mut c_void, setting: c_int);
    fn pango_layout_get_pixel_size(layout: *mut c_void, width: *mut c_int, height: *mut c_int);
}
#[link(name = "libgobject-2.0.so.0", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn g_object_unref(object: *mut c_void);
}

pub type Composition = crate::card_surface::Composition<Surface>;
pub struct Surface {
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}

impl Surface {
    pub fn card(
        width: i32,
        height: i32,
        scale: f64,
        left: i32,
        content: CardText<'_>,
    ) -> Result<Self, String> {
        let painted = card_paint::paint(width, height, scale, left, &content);
        let mut surface = Self::allocate(width, height)?;
        for (to, from) in surface
            .pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(painted.pixels.as_chunks::<4>().0.iter())
        {
            let a = u16::from(from[3]);
            for i in 0..3 {
                to[i] = ((u16::from(from[i]) * a + 127) / 255) as u8;
            }
            to[3] = from[3];
        }
        surface.draw_runs(&painted.text)?;
        Ok(surface)
    }

    fn draw_runs(&mut self, runs: &[TextRun]) -> Result<(), String> {
        // SAFETY: Cairo borrows this fixed-size buffer only until the surface is destroyed;
        // all pointers created below are checked and released before the Vec can move.
        unsafe {
            let surface = cairo_image_surface_create_for_data(
                self.pixels.as_mut_ptr(),
                0,
                self.width,
                self.height,
                self.width * 4,
            );
            if surface.is_null() {
                return Err("cannot create Cairo surface".into());
            }
            if cairo_surface_status(surface) != 0 {
                cairo_surface_destroy(surface);
                return Err("cannot create Cairo image surface".into());
            }
            cairo_surface_mark_dirty(surface);
            let cr = cairo_create(surface);
            if cr.is_null() || cairo_status(cr) != 0 {
                if !cr.is_null() {
                    cairo_destroy(cr);
                }
                cairo_surface_destroy(surface);
                return Err("cannot create Cairo context".into());
            }
            let layout = pango_cairo_create_layout(cr);
            let font = pango_font_description_new();
            if layout.is_null() || font.is_null() {
                if !layout.is_null() {
                    g_object_unref(layout);
                }
                if !font.is_null() {
                    pango_font_description_free(font);
                }
                cairo_destroy(cr);
                cairo_surface_destroy(surface);
                return Err("cannot create Pango layout".into());
            }
            pango_font_description_set_family(font, c"Sans".as_ptr());
            for run in runs {
                let a = run.area;
                let width = (a.right - a.left).max(0);
                let height = (a.bottom - a.top).max(0);
                if width == 0 || height == 0 {
                    continue;
                }
                pango_font_description_set_absolute_size(font, run.size * 1024.0);
                pango_layout_set_font_description(layout, font);
                pango_layout_set_text(
                    layout,
                    run.text.as_ptr().cast(),
                    run.text.len().min(i32::MAX as usize) as i32,
                );
                pango_layout_set_width(layout, width.saturating_mul(1024));
                pango_layout_set_wrap(layout, 2); // PANGO_WRAP_WORD_CHAR
                pango_layout_set_ellipsize(layout, if run.ellipsis { 3 } else { 0 });
                pango_layout_set_alignment(
                    layout,
                    match run.align {
                        Align::Left => 0,
                        Align::Center => 1,
                        Align::Right => 2,
                    },
                );
                pango_layout_set_single_paragraph_mode(layout, (!run.wrap) as i32);
                let mut text_height = 0;
                pango_layout_get_pixel_size(layout, std::ptr::null_mut(), &mut text_height);
                let y = a.top
                    + if run.center_vertical {
                        (height - text_height).max(0) / 2
                    } else {
                        0
                    };
                cairo_save(cr);
                cairo_rectangle(cr, a.left as f64, a.top as f64, width as f64, height as f64);
                cairo_clip(cr);
                let gray = f64::from(run.gray) / 255.0;
                cairo_set_source_rgb(cr, gray, gray, gray);
                cairo_move_to(cr, a.left as f64, y as f64);
                pango_cairo_show_layout(cr, layout);
                cairo_restore(cr);
            }
            let status = cairo_status(cr);
            g_object_unref(layout);
            pango_font_description_free(font);
            cairo_destroy(cr);
            cairo_surface_flush(surface);
            cairo_surface_destroy(surface);
            if status != 0 {
                return Err("cannot draw Cairo text".into());
            }
        }
        Ok(())
    }
}
impl CardSurface for Surface {
    fn allocate(width: i32, height: i32) -> Result<Self, String> {
        if width <= 0 || height <= 0 {
            return Err("card has no size".into());
        }
        let bytes = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or("card size overflow")?;
        Ok(Self {
            width,
            height,
            pixels: vec![0; bytes],
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
        self.draw_runs(&[card_paint::countdown_run(self.width, seconds, scale)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{card::Heading, config::Color};

    #[test]
    fn unicode_text_is_drawn_over_premultiplied_card_pixels() {
        let heading = Heading::from("Claude — Привіт");
        let make = |label| {
            Surface::card(
                320,
                150,
                1.0,
                22,
                CardText {
                    heading: &heading,
                    label,
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
            .unwrap()
        };
        let with_text = make("Turn complete — готово");
        let without_text = make("");
        assert_eq!(&with_text.pixels()[..4], &[0, 0, 0, 0]);
        assert_ne!(with_text.pixels(), without_text.pixels());
        assert!(
            with_text
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3])
        );
    }
}
