use crate::{config::Config, events::Event};

#[cfg(windows)]
#[path = "overlay.rs"]
pub(crate) mod overlay;
/// A failed interactive startup has no resident sink. Show only its diagnostic card (`text`,
/// from `notice::startup_failure_text` or `run_failure_text`), without a display-child
/// readiness byte or any agent/usage activity.
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
pub fn startup_notice(text: &str) -> Result<(), String> {
    let event = Event::notice(text);
    let mut config = Config::default();
    config.sounds.enabled = false;
    config.events[event.kind].duration_ms = crate::resident_request::NOTICE_MS;
    #[cfg(windows)]
    return overlay::show(&event, &config);
    #[cfg(target_os = "linux")]
    return crate::linux::resident::show_transient(&event, &config);
    #[cfg(target_os = "macos")]
    return crate::mac::resident::run_notice(event, config);
}

pub fn show(event: &Event, config: &Config) -> Result<(), String> {
    crate::process::acknowledge()?;
    let outputs = crate::focus::outputs(config, crate::focus::observe(event, config));
    let mut effective = config.clone();
    effective.notifications.enabled &= outputs.window;
    effective.sounds.enabled &= outputs.sound;
    let config = &effective;
    #[cfg(windows)]
    {
        let audio = crate::audio::start(event, config);
        let result = if config.notifications.enabled {
            overlay::show(event, config)
        } else {
            Ok(())
        };
        if let Some(audio) = audio {
            let _ = audio.join();
        }
        result
    }
    #[cfg(target_os = "linux")]
    {
        let audio = crate::audio::start(event, config);
        let result = crate::linux::resident::show_transient(event, config);
        if let Some(audio) = audio {
            let _ = audio.join();
        }
        result
    }
    #[cfg(all(not(windows), not(target_os = "linux")))]
    {
        let _ = (event, config);
        Err("Windows is required".into())
    }
}
pub fn presentation(
    elapsed_ms: u64,
    duration_ms: u64,
    fade_in_ms: u64,
    fade_out_ms: u64,
    dismissed_at: Option<u64>,
) -> (u8, bool) {
    let end = dismissed_at.or(if duration_ms > 0 {
        Some(duration_ms)
    } else {
        None
    });
    if let Some(end) = end
        && elapsed_ms >= end
    {
        let fade = elapsed_ms - end;
        if fade >= fade_out_ms {
            return (0, true);
        }
        let base = end
            .saturating_mul(255)
            .checked_div(fade_in_ms)
            .unwrap_or(255)
            .min(255);
        return (((fade_out_ms - fade) * base / fade_out_ms) as u8, false);
    }
    if fade_in_ms > 0 && elapsed_ms < fade_in_ms {
        return ((elapsed_ms * 255 / fade_in_ms) as u8, false);
    }
    (255, false)
}
#[cfg(test)]
mod tests {
    use super::presentation;
    #[test]
    fn fades_and_automatic_dismissal_follow_controlled_time() {
        assert_eq!(presentation(0, 1000, 100, 200, None), (0, false));
        assert_eq!(presentation(50, 1000, 100, 200, None), (127, false));
        assert_eq!(presentation(500, 1000, 100, 200, None), (255, false));
        assert_eq!(presentation(1100, 1000, 100, 200, None), (127, false));
        assert_eq!(presentation(1200, 1000, 100, 200, None), (0, true));
    }
    #[test]
    fn persistent_wait_requires_dismissal() {
        assert_eq!(presentation(999_999, 0, 100, 200, None), (255, false));
        assert_eq!(presentation(400, 0, 100, 200, Some(300)), (127, false));
        assert_eq!(presentation(500, 0, 100, 200, Some(300)), (0, true));
        assert_eq!(presentation(10, 10, 0, 0, None), (0, true));
    }
}
