//! Problems aHerald can name but the user has to fix, shown as a card of their own.
//!
//! Only a resident installs a sink; hook and transient processes have none, so they log and stay
//! silent. A file that fails is reported once for each of its states (`first_time`), the same
//! rule as its log line.
use crate::{events::Event, operational_log::Record};
use std::{path::Path, sync::Mutex};

type Sink = Box<dyn Fn(Event) + Send + Sync>;
static SINK: Mutex<Option<Sink>> = Mutex::new(None);

/// Cards for notices go to `sink` from now on (the resident's queue).
pub fn install(sink: impl Fn(Event) + Send + Sync + 'static) {
    *SINK.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(sink));
}

/// Stops delivering notices.
pub fn clear() {
    *SINK.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Shows `message` as an aHerald card, if a resident is listening.
pub fn post(message: &str) {
    if let Some(sink) = SINK.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        sink(Event::notice(message));
    }
}

/// The last component of `path`, which is what the card names (the folder stays out of it).
fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// Reports a failing file: the log line `record` and a card saying `what` and the file's name,
/// the first time the file is seen failing in its current state under `topic`.
pub fn report_file(topic: &'static str, path: &Path, record: Record, what: &str) {
    if crate::operational_log::first_time(topic, path) {
        crate::operational_log::log(record);
        post(&format!("{what}: {}", file_name(path)));
    }
}

/// Remembers which missing `monitor` was already reported, so a card placed on the main display
/// at every event is one report for each combination of the setting and the connected displays.
#[derive(Debug, Default)]
pub struct MonitorWatch(Option<(usize, usize)>);

impl MonitorWatch {
    pub const fn new() -> Self {
        Self(None)
    }

    /// Call at every placement with the configured monitor number and the connected count. A
    /// monitor that is not connected is logged and shown as a card the first time.
    pub fn check(&mut self, configured: crate::config::Monitor, connected: usize) {
        match configured {
            crate::config::Monitor::Index(n) if n >= connected => {
                if self.0 != Some((n, connected)) {
                    self.0 = Some((n, connected));
                    crate::operational_log::log(Record::MonitorMissing);
                    post(&format!(
                        "Monitor {n} is not connected; using the main display"
                    ));
                }
            }
            _ => self.0 = None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex as StdMutex};

    /// One test only: the sink is process-wide.
    #[test]
    fn a_notice_reaches_the_installed_sink_only_and_is_cut_to_one_line() {
        post("nobody listens");
        let seen = Arc::new(StdMutex::new(Vec::new()));
        let sink = seen.clone();
        install(move |event| sink.lock().unwrap().push(event));
        post("Sound over 5 s:\nalert.wav\u{7}");
        clear();
        post("after clear");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0].message.as_deref(),
            Some("Sound over 5 s: alert.wav")
        );
        assert_eq!(seen[0].source, crate::events::Source::App);
        assert_eq!(seen[0].kind, crate::events::Kind::Error);
    }

    #[test]
    fn the_card_names_the_file_not_its_folder() {
        assert_eq!(
            file_name(Path::new("/home/me/secret/alert.wav")),
            "alert.wav"
        );
        assert_eq!(file_name(Path::new("")), "");
    }
}
