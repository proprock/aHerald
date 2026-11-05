//! Rebuild the static application icon from the native `both` tray renderer.
//! Run from the repository root: `cargo run --example generate_app_icon`.
use aherald::{
    config::{TrayGraph, UsageDisplay},
    tray_usage::Levels,
    usage::Snapshot,
    usage_service::View,
};
use image::{ImageEncoder, codecs::png::PngEncoder};
use serde_json::json;
use std::{fs, time::Duration};

fn view(primary: f64, secondary: f64) -> Result<View, Box<dyn std::error::Error>> {
    let mut view = View::new(true, Duration::from_secs(300));
    view.snapshot = Some(Snapshot::parse(json!({"rateLimitsByLimitId":{"codex":{
        "primary":{"usedPercent":primary},"secondary":{"usedPercent":secondary}
    }}}))?);
    Ok(view)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Fixed synthetic levels are an emblem, never an account's usage snapshot.
    let levels = Levels::from_views(
        &view(55.0, 85.0)?,
        Some(&view(15.0, 45.0)?),
        TrayGraph::Both,
        UsageDisplay::Remain,
    );
    let sizes = [16u32, 20, 24, 32, 40, 48, 64, 96, 128, 256];
    let mut directory = vec![0, 0, 1, 0];
    directory.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut images = Vec::new();
    for size in sizes {
        let mut rgba = levels.render(size)?;
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let mut png = Vec::new();
        PngEncoder::new(&mut png).write_image(
            &rgba,
            size,
            size,
            image::ExtendedColorType::Rgba8,
        )?;
        directory.extend_from_slice(&[size as u8, size as u8, 0, 0]);
        directory.extend_from_slice(&1u16.to_le_bytes());
        directory.extend_from_slice(&32u16.to_le_bytes());
        directory.extend_from_slice(&(png.len() as u32).to_le_bytes());
        directory.extend_from_slice(&((6 + sizes.len() * 16 + images.len()) as u32).to_le_bytes());
        if size == 256 {
            fs::create_dir_all("resources/windows")?;
            fs::write("resources/windows/aherald.png", &png)?;
        }
        images.extend_from_slice(&png);
    }
    directory.extend_from_slice(&images);
    fs::write("resources/windows/aherald.ico", directory)?;
    Ok(())
}
