//! What every resident starts around its window loop and takes down after it: notices, the
//! messenger sinks, the `[remote]` listener and the local IPC listener thread. Dropping the
//! [`Runtime`] tears them down in one order on every OS.
use crate::{
    config::Config,
    ipc::{Response, pipe},
    local::Scope,
    resident_request::{self, Shared},
};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

const STARTUP_DEADLINE: Duration = Duration::from_secs(1);

pub struct Runtime {
    stop: Arc<AtomicBool>,
    stop_event: Arc<pipe::StopEvent>,
    listener: Option<JoinHandle<()>>,
    remote: Option<crate::remote::Remote>,
}

/// Starts the notice sink, forwarding, the `[remote]` listener and the IPC listener. `wake` runs
/// with the state locked, whenever something changed that the window loop must see: a card, a
/// warning, a quit. Returns once the pipe is listening, or the error that kept it from doing so.
/// The caller holds the scope's ownership and runs its loop until [`Runtime::stop_flag`] is set.
pub fn start<T: Send + 'static>(
    scope: Scope,
    config: &Config,
    path: &Path,
    shared: &Arc<Mutex<Shared<T>>>,
    started: Instant,
    wake: impl Fn(&Shared<T>) + Clone + Send + Sync + 'static,
) -> Result<Runtime, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_event = Arc::new(pipe::StopEvent::new()?);
    // Whatever fails from here on drops `runtime`, which undoes what was started.
    let mut runtime = Runtime {
        stop: stop.clone(),
        stop_event: stop_event.clone(),
        listener: None,
        remote: None,
    };
    // aHerald's own notices (a refused sound, unusable artwork) become cards of this resident.
    crate::notice::install(resident_request::notice_sink(
        shared.clone(),
        config.clone(),
        started,
        wake.clone(),
    ));
    crate::forward::start(&config.forward);
    runtime.remote = crate::remote::start_if_enabled(
        config,
        path,
        resident_request::remote_deliver(shared.clone(), config.clone(), started, wake.clone()),
    );
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (state, settings) = (shared.clone(), config.clone());
    let log_directory = path.parent().unwrap_or_else(|| Path::new(".")).to_owned();
    runtime.listener = Some(std::thread::spawn(move || {
        use crate::operational_log::{Record, init, log};
        init(&log_directory, settings.general.logging);
        let server = match scope.create_pipe() {
            Ok(server) => server,
            Err(e) => {
                let _ = ready_tx.send(Err(e));
                return;
            }
        };
        let _ = ready_tx.send(Ok(()));
        log(Record::Started);
        let mut diagnostics = resident_request::Diagnostics::default();
        let mut ipc_failing = false;
        let mut snapshot_clock = crate::perf::Every::new(Duration::from_secs(60));
        while !stop.load(Ordering::Acquire) {
            crate::perf::tick();
            if snapshot_clock.due(Instant::now()) {
                #[cfg(windows)]
                crate::process_stats::log_snapshot();
            }
            let result = server.serve_until(&stop_event, |request| {
                let _timer = crate::perf::span(crate::perf::Span::IpcHandle);
                let Ok(mut state) = state.lock() else {
                    return Response::Rejected;
                };
                let now = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
                let (response, effects) =
                    resident_request::respond(request, &mut *state, &settings, now);
                if effects.quit {
                    stop.store(true, Ordering::Release);
                }
                if effects.wake {
                    wake(&state);
                }
                drop(state);
                diagnostics.record(&response, effects);
                response
            });
            if result.is_err() && !ipc_failing {
                log(Record::IpcFailure);
            }
            ipc_failing = result.is_err();
        }
        log(Record::Stopped);
    }));
    ready_rx
        .recv_timeout(STARTUP_DEADLINE)
        .unwrap_or_else(|_| Err("resident listener startup deadline exceeded".into()))?;
    Ok(runtime)
}

impl Runtime {
    /// Set by a quit request; the window loop ends when it sees it, and may set it itself.
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        crate::notice::clear();
        drop(self.remote.take());
        crate::forward::stop();
        self.stop.store(true, Ordering::Release);
        self.stop_event.signal();
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{events::Event, ipc::Request};
    use std::sync::atomic::AtomicUsize;

    fn running(name: &str) -> (Scope, Runtime, Arc<AtomicUsize>, Arc<Mutex<Shared<()>>>) {
        let scope = Scope::current(&format!("{name}-{}", std::process::id())).unwrap();
        let config = Config::default();
        let shared = Arc::new(Mutex::new(Shared::new(&config, ())));
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let runtime = start(
            scope.clone(),
            &config,
            &std::env::temp_dir().join("aherald-runtime-test.toml"),
            &shared,
            Instant::now(),
            move |_: &Shared<()>| {
                counter.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
        (scope, runtime, wakes, shared)
    }

    #[test]
    fn a_request_over_the_pipe_reaches_the_state_and_wakes_the_loop() {
        let (scope, runtime, wakes, shared) = running("runtime-event");
        let event = Event::demo("turn_complete").unwrap();
        let reply = pipe::request(&scope, &Request::Event { event }, Duration::from_secs(2));
        assert!(matches!(reply, Ok(Response::Accepted)), "{reply:?}");
        assert!(wakes.load(Ordering::SeqCst) >= 1);
        assert!(shared.lock().unwrap().queue.selected().is_some());
        assert!(!runtime.stop_flag().load(Ordering::SeqCst));
    }

    #[test]
    fn a_quit_request_sets_the_stop_flag_and_dropping_the_runtime_closes_the_pipe() {
        let (scope, runtime, wakes, _shared) = running("runtime-quit");
        let reply = pipe::request(&scope, &Request::Quit, Duration::from_secs(2));
        assert!(reply.is_ok(), "{reply:?}");
        assert!(runtime.stop_flag().load(Ordering::SeqCst));
        assert!(wakes.load(Ordering::SeqCst) >= 1);
        drop(runtime);
        assert!(
            pipe::request_if_running(&scope, &Request::Status, Duration::from_millis(300)).is_err()
        );
    }
}
