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
    let selected = match settings.visual {
        EventVisual::Builtin => return Ok(builtin()),
        EventVisual::Custom => settings
            .animation
            .as_deref()
            .and_then(|p| Animation::load(p).ok()),
        EventVisual::Pet => crate::pet::selected(config, event).ok().flatten(),
        EventVisual::Inherit => match config.notifications.visual {
            Visual::Builtin => return Ok(builtin()),
            Visual::Auto | Visual::Pet => crate::pet::selected(config, event).ok().flatten(),
        },
    };
    Ok(selected.or_else(builtin))
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
    let Some(animation) = resolve(config, event, builtin_root)? else {
        return Ok(None);
    };
    let prepare = |a: Animation| {
        a.prepare(
            config.notifications.scale,
            config.notifications.interpolation == crate::config::Interpolation::Bilinear,
        )
        .ok()
    };
    Ok(prepare(animation).or_else(|| {
        Animation::load(&builtin_root.join(event.kind.name()))
            .ok()
            .and_then(prepare)
    }))
}

pub fn prepared_for_event(
    config: &Config,
    event: &Event,
) -> Result<Option<crate::animation::PreparedAnimation>, String> {
    prepare(config, event, &builtin_root()?)
}
