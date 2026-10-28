//! A `monitor` that is not connected is said once for each set of connected displays. One test
//! only: the notice sink is process-wide.
use aherald::{config::Monitor, notice};
use std::sync::{Arc, Mutex};

#[test]
fn a_missing_monitor_is_reported_once_for_each_set_of_displays() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    notice::install(move |event| sink.lock().unwrap().push(event.message.unwrap_or_default()));
    let mut watch = notice::MonitorWatch::new();
    for _ in 0..3 {
        watch.check(Monitor::Index(2), 1);
    }
    watch.check(Monitor::Index(2), 2); // the set of displays changed
    watch.check(Monitor::Index(1), 2); // now it exists
    watch.check(Monitor::Primary, 2);
    watch.check(Monitor::Index(2), 2); // missing again after being found
    notice::clear();
    let line = "Monitor 2 is not connected; using the main display";
    assert_eq!(*seen.lock().unwrap(), vec![line; 3]);
}
