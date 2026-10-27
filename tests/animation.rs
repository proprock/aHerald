use aherald::animation::{Animation, premultiplied_bgra, scaled};
use image::{Rgba, RgbaImage};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new(metadata: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "aherald-png-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("animation.toml"), metadata).unwrap();
        Self(path)
    }
    fn png(&self, name: &str, image: &RgbaImage) {
        image.save(self.0.join(name)).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
const SHEET: &str = "type='spritesheet'\nimage='sheet.png'\nframe_width=1\nframe_height=1\nframes=4\nfps=10\nloop=false";
#[test]
fn sprite_extraction_is_row_major_and_timing_is_deterministic() {
    let fixture = Fixture::new(SHEET);
    let image = RgbaImage::from_fn(2, 2, |x, y| Rgba([(x + y * 2) as u8, 0, 0, 255]));
    fixture.png("sheet.png", &image);
    let animation = Animation::load(&fixture.0).unwrap();
    assert_eq!(animation.frames.len(), 4);
    for i in 0..4 {
        assert_eq!(animation.frames[i].get_pixel(0, 0).0, [i as u8, 0, 0, 255]);
    }
    assert_eq!(animation.index_at(99), 0);
    assert_eq!(animation.index_at(100), 1);
    assert_eq!(animation.index_at(400), 3);
}
#[test]
fn sequence_order_and_per_frame_durations_are_respected() {
    let fixture = Fixture::new("type='sequence'\nfps=20\ndurations_ms=[50,200]\nloop=true");
    fixture.png("02.png", &RgbaImage::from_pixel(2, 2, Rgba([2, 0, 0, 255])));
    fixture.png("01.png", &RgbaImage::from_pixel(2, 2, Rgba([1, 0, 0, 255])));
    let animation = Animation::load(&fixture.0).unwrap();
    assert_eq!(animation.frames[0].get_pixel(0, 0).0[0], 1);
    assert_eq!(animation.index_at(49), 0);
    assert_eq!(animation.index_at(50), 1);
    assert_eq!(animation.index_at(249), 1);
    assert_eq!(animation.index_at(250), 0);
}
#[test]
fn alpha_and_interpolation_are_observable() {
    assert_eq!(
        premultiplied_bgra(&[255, 128, 0, 128, 255, 255, 255, 0]),
        vec![0, 64, 128, 128, 0, 0, 0, 0]
    );
    let frame = RgbaImage::from_fn(2, 1, |x, _| {
        if x == 0 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        }
    });
    let nearest = scaled(&frame, 2.0, false).unwrap();
    let bilinear = scaled(&frame, 2.0, true).unwrap();
    assert_eq!(nearest.dimensions(), (4, 2));
    assert_eq!(nearest.get_pixel(1, 0).0, [255, 0, 0, 255]);
    assert_ne!(bilinear.get_pixel(1, 0), nearest.get_pixel(1, 0));
    assert!(scaled(&frame, f64::NAN, false).is_err());
}
#[test]
fn missing_corrupt_and_out_of_bounds_assets_are_errors() {
    let fixture = Fixture::new(SHEET);
    assert!(Animation::load(&fixture.0).is_err());
    fs::write(fixture.0.join("sheet.png"), b"not PNG").unwrap();
    assert!(Animation::load(&fixture.0).is_err());
    fixture.png("sheet.png", &RgbaImage::new(2, 2));
    for text in [
        SHEET.replace("frames=4", "frames=257"),
        SHEET.replace("frame_width=1", "frame_width=3"),
        SHEET.replace("fps=10", "fps=nan"),
        SHEET.replace("fps=10", "fps=0"),
        format!("{SHEET}\ndurations_ms=[1]"),
        SHEET.replace("frame_width=1", "frame_width=4097"),
    ] {
        fs::write(fixture.0.join("animation.toml"), text).unwrap();
        assert!(Animation::load(&fixture.0).is_err());
    }
}

#[test]
fn bilinear_prepare_keeps_translucent_edges_premultiplied() {
    // An opaque red pixel next to a transparent one: the blended edge is half red.
    let frame = RgbaImage::from_fn(2, 1, |x, _| {
        if x == 0 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 0, 0])
        }
    });
    let animation = Animation {
        frames: vec![frame],
        durations_ms: vec![100],
        looping: true,
    };
    let prepared = animation.prepare(2.0, true).unwrap();
    let bgra = &prepared.frames[0].bgra;
    assert_eq!(
        (prepared.frames[0].width, prepared.frames[0].height),
        (4, 2)
    );
    for pixel in bgra.chunks(4) {
        let (blue, green, red, alpha) = (pixel[0], pixel[1], pixel[2], pixel[3]);
        assert!(blue <= alpha && green <= alpha && red <= alpha, "{pixel:?}");
        assert_eq!((blue, green), (0, 0));
        // Premultiplied red equals alpha for pure red, within one rounding step.
        assert!(red.abs_diff(alpha) <= 1, "{pixel:?}");
    }
    assert_eq!(&bgra[..4], &[0, 0, 255, 255]);
    assert_eq!(&bgra[12..16], &[0, 0, 0, 0]);
}
