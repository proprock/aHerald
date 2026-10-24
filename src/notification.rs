use crate::{config::Config, events::Event};

pub fn show(event: &Event, config: &Config) -> Result<(), String> {
    crate::process::acknowledge()?;
    let duration = config.events[event.kind.name()].duration_ms;
    if duration == 0 {
        loop {
            std::thread::park();
        }
    }
    std::thread::park_timeout(std::time::Duration::from_millis(duration));
    Ok(())
}
