//! Premultiplied BGRA pixels as an `NSImage`.
use objc2::{AllocAnyThread, rc::Retained};
use objc2_app_kit::{NSBitmapFormat, NSBitmapImageRep, NSDeviceRGBColorSpace, NSImage};
use objc2_foundation::NSSize;

/// The picture of a `width` x `height` pixel buffer, shown `size` points large so the
/// pixels land one to one on a display with that backing scale.
pub fn from_bgra(
    pixels: &[u8],
    width: i32,
    height: i32,
    size: (f64, f64),
) -> Result<Retained<NSImage>, String> {
    from_bgra_as(pixels, width, height, size, false)
}

/// As [`from_bgra`]; a `template` image uses only its alpha and is tinted by the system.
pub fn from_bgra_as(
    pixels: &[u8],
    width: i32,
    height: i32,
    size: (f64, f64),
    template: bool,
) -> Result<Retained<NSImage>, String> {
    if width <= 0 || height <= 0 || pixels.len() != width as usize * height as usize * 4 {
        return Err("image buffer does not match its size".into());
    }
    // SAFETY: the representation owns its storage; the copy below stays inside it.
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
            width as isize * 4,
            32,
        )
        .ok_or("cannot create image bitmap")?;
        let data = rep.bitmapData();
        if data.is_null() {
            return Err("image bitmap has no storage".into());
        }
        let target = std::slice::from_raw_parts_mut(data, pixels.len());
        for (to, from) in target
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(pixels.as_chunks::<4>().0)
        {
            *to = [from[2], from[1], from[0], from[3]];
        }
        rep.setSize(NSSize::new(size.0, size.1));
        let image = NSImage::initWithSize(NSImage::alloc(), NSSize::new(size.0, size.1));
        image.addRepresentation(&rep);
        image.setTemplate(template);
        Ok(image)
    }
}
