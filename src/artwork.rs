//! Select notification artwork independently of rendering and sound.
use crate::{
    animation::Animation,
    config::{Config, EventVisual, Visual},
    events::Event,
};
use std::path::{Path, PathBuf};

/// Resolve against the application's external assets directory; failures degrade to text.
pub fn resolve(
    config: &Config,
    event: &Event,
    builtin_root: &Path,
) -> Result<Option<Animation>, String> {
    let settings = &config.events[event.kind];
    let builtin = || Animation::load(&builtin_root.join(event.kind.name())).ok();
    // An aHerald notice belongs to no agent, so no per-event, pet or custom picture applies.
    if event.source == crate::events::Source::App {
        return Ok(builtin());
    }
    let selected = match settings.visual {
        EventVisual::Builtin => return Ok(builtin()),
        EventVisual::Custom => {
            settings
                .animation
                .as_deref()
                .and_then(|p| match Animation::load(p) {
                    Ok(animation) => Some(animation),
                    Err(_) => {
                        reject("custom", p);
                        None
                    }
                })
        }
        EventVisual::Pet => pet(config, event),
        EventVisual::Inherit => match config.notifications.visual {
            Visual::Builtin => return Ok(builtin()),
            Visual::Auto | Visual::Pet => pet(config, event),
        },
    };
    Ok(selected.or_else(builtin))
}

/// A configured picture that cannot be used is reported once per file state; the built-in
/// artwork takes its place.
fn reject(which: &'static str, path: &Path) {
    let what = if which == "pet" {
        "Pet cannot be used"
    } else {
        "Animation cannot be used"
    };
    crate::notice::report_file(
        "artwork",
        path,
        crate::operational_log::Record::ArtworkRejected(which),
        what,
    );
}

fn pet(config: &Config, event: &Event) -> Option<Animation> {
    match crate::pet::selected(config, event) {
        Ok(animation) => animation,
        Err(_) => {
            if let Some(path) = config.notifications.pet.as_deref() {
                reject("pet", path);
            }
            None
        }
    }
}

pub fn for_event(config: &Config, event: &Event) -> Result<Option<Animation>, String> {
    resolve(config, event, &builtin_root()?)
}

fn builtin_root() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|_| "cannot locate application resources")?;
    let directory = executable
        .parent()
        .ok_or("application directory is unavailable")?;
    Ok(directory.join("assets"))
}

pub fn prepare(
    config: &Config,
    event: &Event,
    builtin_root: &Path,
) -> Result<Option<crate::animation::PreparedAnimation>, String> {
    prepare_at(config, event, builtin_root, config.notifications.scale)
}

/// Prepares artwork at an explicit render scale (the configured scale times the
/// monitor's DPI scale). Artwork too large to prepare at that scale is prepared at the
/// configured scale instead, so a high-DPI monitor never costs the picture.
pub fn prepare_at(
    config: &Config,
    event: &Event,
    builtin_root: &Path,
    scale: f64,
) -> Result<Option<crate::animation::PreparedAnimation>, String> {
    let _timer = crate::perf::span(crate::perf::Span::ArtworkPrepare);
    let Some(animation) = resolve(config, event, builtin_root)? else {
        return Ok(None);
    };
    let bilinear = config.notifications.interpolation == crate::config::Interpolation::Bilinear;
    let base = config.notifications.scale;
    // Preparing consumes the animation, so a retry at the configured scale reloads it.
    let at_base = |load: &dyn Fn() -> Option<Animation>| {
        (scale > base)
            .then(|| load().and_then(|a| a.prepare(base, bilinear).ok()))
            .flatten()
    };
    let selected = || resolve(config, event, builtin_root).ok().flatten();
    let builtin = || Animation::load(&builtin_root.join(event.kind.name())).ok();
    Ok(animation
        .prepare(scale, bilinear)
        .ok()
        .or_else(|| {
            let smaller = at_base(&selected);
            if smaller.is_some() {
                crate::perf::count(crate::perf::Counter::ArtworkBudgetFallback);
            }
            smaller
        })
        .or_else(|| builtin().and_then(|a| a.prepare(scale, bilinear).ok()))
        .or_else(|| at_base(&builtin)))
}

pub fn prepared_for_event(
    config: &Config,
    event: &Event,
    scale: f64,
) -> Result<Option<crate::animation::PreparedAnimation>, String> {
    prepare_at(config, event, &builtin_root()?, scale)
}
