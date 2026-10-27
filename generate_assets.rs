//! Rebuild the original MIT-licensed aHerald sample artwork and tones.
//! Run from the repository root with `cargo run --example generate_assets`.
use image::{Rgba, RgbaImage};
use std::{f64::consts::TAU, fs, path::Path};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let events = [
        ("turn_complete", [65, 210, 140], 660.0),
        ("permission_required", [250, 180, 55], 440.0),
        ("input_required", [80, 165, 250], 550.0),
        ("error", [245, 85, 100], 220.0),
        ("interrupted", [180, 130, 240], 330.0),
    ];
    for (kind, color, frequency) in events {
        let directory = root.join(kind);
        fs::create_dir_all(&directory)?;
        let sheet = RgbaImage::from_fn(64 * 8, 64, |sx, y| {
            let frame = sx / 64;
            let x = (sx % 64) as f64 - 31.5;
            let y = y as f64 - 31.5;
            let radius = 24.0 + (f64::from(frame) * TAU / 8.0).sin() * 2.0;
            let distance = x.hypot(y);
            let alpha = ((radius + 1.0 - distance).clamp(0.0, 1.0) * 230.0) as u8;
            let mark = match kind {
                "turn_complete" => {
                    ((y - (x + 9.0)).abs() < 2.5 && (-15.0..=-5.0).contains(&x))
                        || ((y - (-x - 1.0)).abs() < 2.5 && (-5.0..=15.0).contains(&x))
                }
                "error" => (x.abs() - y.abs()).abs() < 2.0 && x.abs() < 12.0 && y.abs() < 12.0,
                "interrupted" => (x.abs() - 6.0).abs() < 3.0 && y.abs() < 12.0,
                _ => x.abs() < 3.0 && ((-13.0..=4.0).contains(&y) || (9.0..=13.0).contains(&y)),
            };
            if mark {
                Rgba([255, 255, 255, 255])
            } else {
                Rgba([color[0], color[1], color[2], alpha])
            }
        });
        sheet.save(directory.join("sheet.png"))?;
        fs::write(
            directory.join("animation.toml"),
            format!(
                "name = \"{kind}\"\ntype = \"spritesheet\"\nimage = \"sheet.png\"\nframe_width = 64\nframe_height = 64\nframes = 8\nfps = 12\nloop = true\n"
            ),
        )?;
        let rate = 22050u32;
        let count = rate / 3;
        let mut pcm = Vec::new();
        for i in 0..count {
            let t = f64::from(i) / f64::from(rate);
            let envelope = (t / 0.015).min(1.0) * ((1.0 / 3.0 - t) / 0.08).clamp(0.0, 1.0);
            let sample = ((TAU * frequency * t).sin() * envelope * 9000.0) as i16;
            pcm.extend(sample.to_le_bytes());
        }
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend((36 + pcm.len() as u32).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(rate.to_le_bytes());
        wav.extend((rate * 2).to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend((pcm.len() as u32).to_le_bytes());
        wav.extend(pcm);
        fs::write(directory.join("sound.wav"), wav)?;
    }
    Ok(())
}
