//! Adapter for the standard Codex pet atlas; it reads assets, never installer commands.
use crate::{
    animation::{Animation, MAX_BYTES},
    config::Config,
    events::Kind,
};
use image::{ImageFormat, ImageReader, imageops};
use serde::Deserialize;
use std::{
    fs::File,
    io::{Cursor, Read},
    path::{Component, Path},
};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    #[serde(default = "default_sheet")]
    spritesheet_path: String,
    #[serde(default = "default_version")]
    sprite_version_number: u32,
    frame: Option<serde_json::Value>,
    animations: Option<serde_json::Value>,
}
fn default_sheet() -> String {
    "spritesheet.webp".into()
}
fn default_version() -> u32 {
    1
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "cannot open pet resource")?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read pet resource")?;
    if bytes.len() as u64 > limit {
        return Err("pet resource exceeds size limit".into());
    }
    Ok(bytes)
}
/// Load only the event's occupied cells, avoiding blank cells and scaling unrelated states.
pub fn load(manifest: &Path, kind: Kind) -> Result<Animation, String> {
    let bytes = read_bounded(manifest, 16 * 1024)?;
    let meta: Manifest = serde_json::from_slice(&bytes).map_err(|_| "invalid pet.json")?;
    let rows = match meta.sprite_version_number {
        1 => 9,
        2 => 11,
        _ => return Err("unsupported pet atlas version".into()),
    };
    if meta.frame.is_some()
        || meta
            .animations
            .as_ref()
            .is_some_and(|v| v.as_object().is_none_or(|m| !m.is_empty()))
    {
        return Err("custom pet frame layouts and animation tracks are not supported".into());
    }
    let path = Path::new(&meta.spritesheet_path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        || meta.spritesheet_path.contains(':')
    {
        return Err("pet spritesheet must stay inside its directory".into());
    }
    let parent = manifest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let directory = parent
        .canonicalize()
        .map_err(|_| "cannot resolve pet directory")?;
    let path = directory
        .join(path)
        .canonicalize()
        .map_err(|_| "cannot resolve pet spritesheet")?;
    if !path.starts_with(&directory) {
        return Err("pet spritesheet escapes its directory".into());
    }
    let bytes = read_bounded(&path, 20 * 1024 * 1024)?;
    let format = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| "cannot recognize pet image")?
        .format()
        .ok_or("unknown pet image format")?;
    if !matches!(format, ImageFormat::Png | ImageFormat::WebP) {
        return Err("pet image must be PNG or WebP".into());
    }
    let dimensions = ImageReader::with_format(Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|_| "invalid pet image header")?;
    if dimensions != (1536, 208 * rows) {
        return Err("pet atlas dimensions do not match its version".into());
    }
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1536);
    limits.max_image_height = Some(2288);
    limits.max_alloc = Some(MAX_BYTES);
    reader.limits(limits);
    let sheet = reader
        .decode()
        .map_err(|_| "invalid or oversized pet image")?
        .to_rgba8();
    // aHerald loops the selected notification state. It does not reproduce Codex's chat activity runtime.
    let (row, durations_ms) = match kind {
        Kind::TurnComplete => (3, vec![140, 140, 140, 280]),
        Kind::PermissionRequired => (8, vec![150, 150, 150, 150, 150, 280]),
        Kind::InputRequired => (6, vec![150, 150, 150, 150, 150, 260]),
        Kind::Error => (5, vec![140, 140, 140, 140, 140, 140, 140, 240]),
        Kind::Interrupted => (0, vec![1680, 660, 660, 840, 840, 1920]),
    };
    let frames = (0..durations_ms.len())
        .map(|column| {
            imageops::crop_imm(&sheet, column as u32 * 192, row * 208, 192, 208).to_image()
        })
        .collect();
    Ok(Animation {
        frames,
        durations_ms,
        looping: true,
    })
}
/// Explicit per-event animation overrides the shared pet; invalid assets reach the text fallback.
pub fn for_notification(config: &Config, kind: Kind) -> Result<Option<Animation>, String> {
    if let Some(path) = config
        .events
        .get(kind.name())
        .and_then(|event| event.animation.as_deref())
    {
        return Animation::load(path).map(Some);
    }
    config
        .notifications
        .pet
        .as_deref()
        .map(|path| load(path, kind))
        .transpose()
}

/// Select artwork for an event source.
pub fn for_event(
    config: &Config,
    event: &crate::events::Event,
) -> Result<Option<Animation>, String> {
    let explicit = config
        .events
        .get(event.kind.name())
        .and_then(|e| e.animation.as_ref());
    if explicit.is_none()
        && let Some(path) = config
            .agents
            .get(event.source.agent_key())
            .and_then(|a| a.pet.as_deref())
    {
        return load(path, event.kind).map(Some);
    }
    for_notification(config, event.kind)
}
