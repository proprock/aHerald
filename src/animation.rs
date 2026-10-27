use image::{
    ImageFormat, ImageReader, RgbaImage,
    imageops::{self, FilterType},
};
use serde::Deserialize;
use std::{
    fs::{self, File},
    io::{BufReader, Read},
    path::Path,
};
pub const MAX_FRAMES: usize = 256;
pub const MAX_BYTES: u64 = 32 * 1024 * 1024;
#[derive(Debug)]
pub struct Animation {
    pub frames: Vec<RgbaImage>,
    pub durations_ms: Vec<u64>,
    pub looping: bool,
}
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Metadata {
    #[serde(rename = "type")]
    kind: String,
    image: String,
    frame_width: u32,
    frame_height: u32,
    frames: usize,
    fps: f64,
    #[serde(rename = "loop")]
    looping: bool,
    durations_ms: Vec<u64>,
    name: Option<String>,
}
impl Default for Metadata {
    fn default() -> Self {
        Self {
            kind: String::new(),
            image: String::new(),
            frame_width: 0,
            frame_height: 0,
            frames: 0,
            fps: 16.0,
            looping: true,
            durations_ms: Vec::new(),
            name: None,
        }
    }
}
fn png(path: &Path) -> Result<RgbaImage, String> {
    let file = File::open(path).map_err(|_| "cannot open PNG")?;
    if file.metadata().map_err(|_| "cannot inspect PNG")?.len() > 16 * 1024 * 1024 {
        return Err("PNG file exceeds limit".into());
    }
    let mut reader = ImageReader::with_format(BufReader::new(file), ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(MAX_BYTES);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "invalid or oversized PNG")?
        .to_rgba8();
    if image.is_empty() || image.as_raw().len() as u64 > MAX_BYTES {
        return Err("decoded PNG exceeds limit".into());
    }
    Ok(image)
}
impl Animation {
    pub fn load(directory: &Path) -> Result<Self, String> {
        let file = File::open(directory.join("animation.toml"))
            .map_err(|_| "cannot open animation metadata")?;
        let mut text = String::new();
        file.take(16_385)
            .read_to_string(&mut text)
            .map_err(|_| "invalid animation metadata")?;
        if text.len() > 16_384 {
            return Err("animation metadata exceeds limit".into());
        }
        let meta: Metadata = toml::from_str(&text).map_err(|_| "invalid animation metadata")?;
        if !meta.fps.is_finite()
            || !(1.0..=120.0).contains(&meta.fps)
            || meta.frames > MAX_FRAMES
            || meta.frame_width > 1024
            || meta.frame_height > 1024
            || meta.name.as_ref().is_some_and(|name| name.len() > 256)
        {
            return Err("animation settings exceed limits".into());
        }
        let frames = match meta.kind.as_str() {
            "spritesheet" => {
                let (w, h) = (meta.frame_width, meta.frame_height);
                if w == 0 || h == 0 || meta.frames == 0 || meta.image.is_empty() {
                    return Err("invalid sprite dimensions".into());
                }
                let sheet = png(&directory.join(&meta.image))?;
                if sheet.width() % w != 0
                    || sheet.height() % h != 0
                    || meta.frames as u64 > (sheet.width() / w) as u64 * (sheet.height() / h) as u64
                    || meta.frames as u64 * w as u64 * h as u64 * 4 > MAX_BYTES
                {
                    return Err("sprite layout exceeds image or memory bounds".into());
                }
                (0..meta.frames)
                    .map(|i| {
                        imageops::crop_imm(
                            &sheet,
                            (i as u32 % (sheet.width() / w)) * w,
                            (i as u32 / (sheet.width() / w)) * h,
                            w,
                            h,
                        )
                        .to_image()
                    })
                    .collect()
            }
            "sequence" => {
                let mut paths = Vec::new();
                for (index, entry) in fs::read_dir(directory)
                    .map_err(|_| "cannot list frame sequence")?
                    .enumerate()
                {
                    if index >= 4096 {
                        return Err("frame directory exceeds entry limit".into());
                    }
                    let entry = entry.map_err(|_| "cannot inspect frame entry")?;
                    if entry
                        .file_type()
                        .map_err(|_| "cannot inspect frame type")?
                        .is_file()
                        && entry
                            .path()
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
                    {
                        paths.push(entry.path());
                        if paths.len() > MAX_FRAMES {
                            return Err("too many PNG frames".into());
                        }
                    }
                }
                paths.sort();
                if paths.is_empty() || (meta.frames != 0 && meta.frames != paths.len()) {
                    return Err("invalid sequence frame count".into());
                }
                let mut frames: Vec<RgbaImage> = Vec::new();
                let mut bytes = 0;
                for path in paths {
                    let frame = png(&path)?;
                    bytes += frame.as_raw().len() as u64;
                    if bytes > MAX_BYTES
                        || frames
                            .first()
                            .is_some_and(|first| first.dimensions() != frame.dimensions())
                        || (meta.frame_width != 0 && meta.frame_width != frame.width())
                        || (meta.frame_height != 0 && meta.frame_height != frame.height())
                    {
                        return Err("inconsistent sequence dimensions or memory limit".into());
                    }
                    frames.push(frame);
                }
                frames
            }
            _ => return Err("unsupported animation type".into()),
        };
        let durations_ms = if meta.durations_ms.is_empty() {
            vec![(1000.0 / meta.fps).round() as u64; frames.len()]
        } else {
            if meta.durations_ms.len() != frames.len()
                || meta
                    .durations_ms
                    .iter()
                    .any(|ms| !(1..=60_000).contains(ms))
            {
                return Err("invalid frame durations".into());
            }
            meta.durations_ms
        };
        Ok(Self {
            frames,
            durations_ms,
            looping: meta.looping,
        })
    }
    pub fn index_at(&self, elapsed_ms: u64) -> usize {
        frame_index(&self.durations_ms, self.looping, elapsed_ms)
    }
}
pub fn frame_index(durations: &[u64], looping: bool, elapsed_ms: u64) -> usize {
    let total: u64 = durations.iter().sum();
    if total == 0 {
        return 0;
    }
    if !looping && elapsed_ms >= total {
        return durations.len().saturating_sub(1);
    }
    let mut remaining = elapsed_ms % total;
    for (i, duration) in durations.iter().enumerate() {
        if remaining < *duration {
            return i;
        }
        remaining -= duration;
    }
    0
}
pub fn premultiplied_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(rgba.len());
    for pixel in rgba.as_chunks::<4>().0 {
        let alpha = pixel[3] as u16;
        bytes.extend_from_slice(&[
            ((pixel[2] as u16 * alpha + 127) / 255) as u8,
            ((pixel[1] as u16 * alpha + 127) / 255) as u8,
            ((pixel[0] as u16 * alpha + 127) / 255) as u8,
            pixel[3],
        ]);
    }
    bytes
}
fn scaled_size(frame: &RgbaImage, scale: f64) -> Result<(u32, u32), String> {
    if !scale.is_finite() || !(0.25..=16.0).contains(&scale) || frame.is_empty() {
        return Err("invalid render scale".into());
    }
    let w = (frame.width() as f64 * scale).round().max(1.0) as u32;
    let h = (frame.height() as f64 * scale).round().max(1.0) as u32;
    if w > 2048 || h > 2048 {
        return Err("scaled dimensions exceed limit".into());
    }
    Ok((w, h))
}
pub fn scaled(frame: &RgbaImage, scale: f64, bilinear: bool) -> Result<RgbaImage, String> {
    let (w, h) = scaled_size(frame, scale)?;
    if !bilinear {
        return Ok(imageops::resize(frame, w, h, FilterType::Nearest));
    }
    let mut result = resized_premultiplied(frame, w, h);
    for pixel in result.pixels_mut() {
        let a = pixel[3] as u16;
        for c in &mut pixel.0[..3] {
            *c = ((*c as u16 * 255).checked_div(a).unwrap_or(0)).min(255) as u8;
        }
    }
    Ok(result)
}
/// Bilinear resize in premultiplied space, which keeps transparent RGB from bleeding
/// into opaque edges. The result stays premultiplied.
fn resized_premultiplied(frame: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let mut premultiplied = frame.clone();
    for pixel in premultiplied.pixels_mut() {
        let a = pixel[3] as u16;
        for c in &mut pixel.0[..3] {
            *c = ((*c as u16 * a + 127) / 255) as u8;
        }
    }
    imageops::resize(&premultiplied, w, h, FilterType::Triangle)
}
/// Premultiplied RGBA to the BGRA order GDI expects, without premultiplying again.
fn bgra_from_premultiplied(rgba: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(rgba.len());
    for pixel in rgba.as_chunks::<4>().0 {
        bytes.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    bytes
}

pub struct PreparedFrame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}
pub struct PreparedAnimation {
    pub frames: Vec<PreparedFrame>,
    pub durations_ms: Vec<u64>,
    pub looping: bool,
}
impl Animation {
    pub fn prepare(self, scale: f64, bilinear: bool) -> Result<PreparedAnimation, String> {
        let mut total = 0u64;
        for frame in &self.frames {
            let width = (frame.width() as f64 * scale).round().max(1.0) as u64;
            let height = (frame.height() as f64 * scale).round().max(1.0) as u64;
            total = total.saturating_add(width.saturating_mul(height).saturating_mul(4));
        }
        if total > MAX_BYTES {
            return Err("scaled animation exceeds memory limit".into());
        }
        let mut frames = Vec::with_capacity(self.frames.len());
        for frame in self.frames {
            let (width, height, bgra) = if bilinear {
                // `scaled` validates the scale and size; the bilinear result is already
                // premultiplied, so it is not converted to straight alpha and back.
                let (w, h) = scaled_size(&frame, scale)?;
                let resized = resized_premultiplied(&frame, w, h);
                (w, h, bgra_from_premultiplied(resized.as_raw()))
            } else {
                let frame = scaled(&frame, scale, false)?;
                (
                    frame.width(),
                    frame.height(),
                    premultiplied_bgra(frame.as_raw()),
                )
            };
            frames.push(PreparedFrame {
                width,
                height,
                bgra,
            });
        }
        Ok(PreparedAnimation {
            frames,
            durations_ms: self.durations_ms,
            looping: self.looping,
        })
    }
}
