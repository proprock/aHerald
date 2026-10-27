//! Card text drawn with AppKit into a premultiplied BGRA buffer.
use crate::card_paint::TextRun;

use crate::card_paint::Align;
use objc2::{AllocAnyThread, rc::Retained, runtime::AnyObject};
use objc2_app_kit::{
    NSBitmapFormat, NSBitmapImageRep, NSColor, NSDeviceRGBColorSpace, NSFont, NSFontAttributeName,
    NSFontWeightMedium, NSForegroundColorAttributeName, NSGraphicsContext, NSLineBreakMode,
    NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSRectClip, NSStringDrawingOptions,
    NSStringNSExtendedStringDrawing, NSTextAlignment,
};
use objc2_foundation::{NSAttributedStringKey, NSDictionary, NSPoint, NSRect, NSSize, NSString};

/// Segoe UI medium on Windows; the system font at the same weight here.
fn attributes(
    size: f64,
    gray: u8,
    align: Align,
    mode: NSLineBreakMode,
) -> Retained<NSDictionary<NSAttributedStringKey, AnyObject>> {
    // SAFETY: plain AppKit constructors and the documented attribute key constants.
    unsafe {
        let font = NSFont::systemFontOfSize_weight(size, NSFontWeightMedium);
        let white = f64::from(gray) / 255.0;
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(white, white, white, 1.0);
        let style = NSMutableParagraphStyle::new();
        style.setAlignment(match align {
            Align::Left => NSTextAlignment::Left,
            Align::Center => NSTextAlignment::Center,
            Align::Right => NSTextAlignment::Right,
        });
        style.setLineBreakMode(mode);
        NSDictionary::from_slices(
            &[
                NSFontAttributeName,
                NSForegroundColorAttributeName,
                NSParagraphStyleAttributeName,
            ],
            &[&*font as &AnyObject, &*color, &*style],
        )
    }
}

fn options() -> NSStringDrawingOptions {
    NSStringDrawingOptions::UsesLineFragmentOrigin
}

/// Height in pixels `text` takes wrapped to `width`.
fn text_height(
    text: &NSString,
    width: f64,
    attrs: &NSDictionary<NSAttributedStringKey, AnyObject>,
) -> f64 {
    // SAFETY: `attrs` holds the font, colour and paragraph style that AppKit expects.
    let rect = unsafe {
        text.boundingRectWithSize_options_attributes_context(
            NSSize::new(width, 0.0),
            options(),
            Some(attrs),
            None,
        )
    };
    rect.size.height.ceil()
}

/// Draws `runs` over `pixels`, a `width` x `height` premultiplied BGRA buffer.
pub fn draw_runs(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    runs: &[TextRun],
) -> Result<(), String> {
    if width <= 0 || height <= 0 || pixels.len() != width as usize * height as usize * 4 {
        return Err("text buffer does not match its size".into());
    }
    if runs.iter().all(|run| run.text.is_empty()) {
        return Ok(());
    }
    let stride = width as usize * 4;
    // SAFETY: the representation allocates and owns its bitmap; every access below stays
    // inside `height * stride` bytes, and the graphics state is restored before returning.
    unsafe {
        let rep = NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            width as isize,
            height as isize,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            NSBitmapFormat::empty(),
            stride as isize,
            32,
        )
        .ok_or("cannot create text bitmap")?;
        let data = rep.bitmapData();
        if data.is_null() {
            return Err("text bitmap has no storage".into());
        }
        let bitmap = std::slice::from_raw_parts_mut(data, height as usize * stride);
        // The bitmap is premultiplied RGBA; the card is premultiplied BGRA.
        for (to, from) in bitmap
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(pixels.as_chunks::<4>().0)
        {
            *to = [from[2], from[1], from[0], from[3]];
        }
        let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)
            .ok_or("cannot create text context")?;
        NSGraphicsContext::saveGraphicsState_class();
        NSGraphicsContext::setCurrentContext(Some(&context));
        for run in runs.iter().filter(|run| !run.text.is_empty()) {
            let (left, right) = (f64::from(run.area.left), f64::from(run.area.right));
            let (top, bottom) = (f64::from(run.area.top), f64::from(run.area.bottom));
            let mode = if run.wrap {
                NSLineBreakMode::ByWordWrapping
            } else if run.ellipsis {
                NSLineBreakMode::ByTruncatingTail
            } else {
                NSLineBreakMode::ByClipping
            };
            let attrs = attributes(run.size, run.gray, run.align, mode);
            let text = NSString::from_str(&run.text);
            let mut top_edge = top;
            if run.center_vertical {
                let line = text_height(&text, right - left, &attrs);
                top_edge = top + ((bottom - top - line) / 2.0).max(0.0);
            }
            // AppKit's bitmap origin is the bottom-left corner.
            let area = NSRect::new(
                NSPoint::new(left, f64::from(height) - bottom),
                NSSize::new(right - left, bottom - top),
            );
            context.saveGraphicsState();
            NSRectClip(area);
            let rect = NSRect::new(
                NSPoint::new(left, f64::from(height) - bottom),
                NSSize::new(right - left, bottom - top_edge),
            );
            text.drawWithRect_options_attributes_context(rect, options(), Some(&attrs), None);
            context.restoreGraphicsState();
        }
        context.flushGraphics();
        NSGraphicsContext::restoreGraphicsState_class();
        for (to, from) in pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(bitmap.as_chunks::<4>().0)
        {
            *to = [from[2], from[1], from[0], from[3]];
        }
    }
    Ok(())
}

/// Height in pixels of `text` wrapped to `width`, in the body font of `size` pixels.
pub fn measure_wrapped(width: i32, size: f64, text: &str) -> Result<i32, String> {
    let attrs = attributes(size, 0xf5, Align::Left, NSLineBreakMode::ByWordWrapping);
    Ok(text_height(&NSString::from_str(text), f64::from(width.max(1)), &attrs) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{card::Area, card_paint::Align};

    fn run(text: &str, area: Area) -> TextRun {
        TextRun {
            text: text.into(),
            size: 18.0,
            gray: 0xf5,
            area,
            align: Align::Left,
            wrap: false,
            center_vertical: false,
            ellipsis: false,
        }
    }
    fn area(left: i32, top: i32, right: i32, bottom: i32) -> Area {
        Area {
            left,
            top,
            right,
            bottom,
        }
    }
    fn inked(pixels: &[u8], width: i32, x: std::ops::Range<i32>, y: std::ops::Range<i32>) -> usize {
        let mut count = 0;
        for yy in y {
            for xx in x.clone() {
                if pixels[((yy * width + xx) * 4 + 3) as usize] > 0 {
                    count += 1;
                }
            }
        }
        count
    }

    #[test]
    fn text_is_drawn_inside_its_area_only() {
        let (w, h) = (240, 80);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        draw_runs(&mut pixels, w, h, &[run("Hello", area(20, 10, 200, 40))]).unwrap();
        assert!(inked(&pixels, w, 20..200, 10..40) > 50);
        assert_eq!(inked(&pixels, w, 0..w, 40..h), 0);
        assert_eq!(inked(&pixels, w, 0..20, 0..h), 0);
        assert_eq!(inked(&pixels, w, 200..w, 0..h), 0);
    }

    #[test]
    fn text_is_light_gray_and_premultiplied() {
        let (w, h) = (240, 80);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        draw_runs(&mut pixels, w, h, &[run("HHHH", area(10, 10, 200, 40))]).unwrap();
        let solid = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .find(|p| p[3] == 255)
            .expect("a fully covered pixel");
        assert!(
            solid[0] >= 0xf0 && solid[1] >= 0xf0 && solid[2] >= 0xf0,
            "{solid:?}"
        );
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[..3].iter().all(|c| *c <= p[3]))
        );
    }

    #[test]
    fn right_alignment_puts_the_ink_at_the_right_edge() {
        let (w, h) = (240, 60);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let mut r = run("7s", area(0, 10, 200, 40));
        r.align = Align::Right;
        draw_runs(&mut pixels, w, h, &[r]).unwrap();
        assert_eq!(inked(&pixels, w, 0..100, 0..h), 0);
        assert!(inked(&pixels, w, 150..200, 10..40) > 0);
    }

    #[test]
    fn an_overlong_line_is_cut_inside_its_area() {
        let (w, h) = (240, 60);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let mut r = run(&"long ".repeat(40), area(10, 10, 120, 40));
        r.ellipsis = true;
        draw_runs(&mut pixels, w, h, &[r]).unwrap();
        assert!(inked(&pixels, w, 10..120, 10..40) > 0);
        assert_eq!(inked(&pixels, w, 120..w, 0..h), 0);
        assert_eq!(inked(&pixels, w, 0..w, 40..h), 0);
    }

    #[test]
    fn wrapped_text_uses_several_lines() {
        let (w, h) = (200, 160);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let mut r = run(&"word ".repeat(30), area(10, 10, 190, 150));
        r.wrap = true;
        draw_runs(&mut pixels, w, h, &[r]).unwrap();
        assert!(inked(&pixels, w, 10..190, 60..150) > 0);
    }

    #[test]
    fn empty_text_and_empty_runs_draw_nothing() {
        let (w, h) = (100, 40);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        draw_runs(&mut pixels, w, h, &[]).unwrap();
        draw_runs(&mut pixels, w, h, &[run("", area(0, 0, 100, 40))]).unwrap();
        assert!(pixels.iter().all(|b| *b == 0));
    }

    #[test]
    fn a_wrong_buffer_size_is_an_error() {
        assert!(draw_runs(&mut [0u8; 16], 100, 40, &[run("x", area(0, 0, 10, 10))]).is_err());
    }

    #[test]
    fn measured_height_grows_with_the_text() {
        let one = measure_wrapped(300, 18.0, "short").unwrap();
        let many = measure_wrapped(300, 18.0, &"word ".repeat(60)).unwrap();
        assert!(one >= 18);
        assert!(many > one * 3, "{one} vs {many}");
        assert!(measure_wrapped(300, 18.0, "").unwrap() >= 0);
    }
}
