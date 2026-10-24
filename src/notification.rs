use crate::{config::Config, events::Event};

#[cfg(windows)]
#[path = "overlay.rs"]
mod overlay;
pub fn show(event: &Event, config: &Config) -> Result<(), String> {
    crate::process::acknowledge()?;
    #[cfg(windows)]
    {
        overlay::show(event, config)
    }
    #[cfg(not(windows))]
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
