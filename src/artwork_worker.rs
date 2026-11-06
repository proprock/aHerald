//! Prepares card artwork off the window thread. The loop submits what the card on screen needs and
//! is woken when the result is ready; the worker blocks while idle, so an idle resident stays idle.
use crate::{animation::PreparedAnimation, config::Config, events::Event};
use std::{
    sync::{Arc, mpsc},
    thread::JoinHandle,
};

/// The card id, the render scale the artwork was prepared at, and the artwork (none: the card has
/// no artwork, or it could not be prepared).
pub type Prepared = (u64, f64, Option<Arc<PreparedAnimation>>);

pub struct ArtworkWorker {
    jobs: Option<mpsc::SyncSender<(u64, Event, f64)>>,
    pub results: mpsc::Receiver<Prepared>,
    thread: Option<JoinHandle<()>>,
}

impl ArtworkWorker {
    /// `notify` runs on the worker thread after each result is queued.
    pub fn spawn(config: &Config, notify: impl Fn() + Send + 'static) -> Self {
        let (jobs, requests) = mpsc::sync_channel::<(u64, Event, f64)>(1);
        let (sender, results) = mpsc::channel();
        let config = config.clone();
        let thread = std::thread::spawn(move || {
            let new_cache =
                || crate::resource_cache::AssetCache::new(16, crate::animation::MAX_BYTES as usize);
            let mut cache = new_cache();
            // Artwork is prepared for one render scale at a time.
            let mut cached_scale = 0.0;
            // Ends when the sender is dropped.
            while let Ok((id, event, scale)) = requests.recv() {
                if scale != cached_scale {
                    cache = new_cache();
                    cached_scale = scale;
                }
                let art = cache.get_or_load((event.source.agent_key(), event.kind.name()), || {
                    crate::artwork::prepared_for_event(&config, &event, scale)
                        .ok()
                        .flatten()
                });
                if sender.send((id, scale, art)).is_err() {
                    return;
                }
                notify();
            }
        });
        Self {
            jobs: Some(jobs),
            results,
            thread: Some(thread),
        }
    }

    /// Hands the worker a job; false when it is still busy with the previous one.
    pub fn submit(&self, id: u64, event: &Event, scale: f64) -> bool {
        self.jobs
            .as_ref()
            .is_some_and(|jobs| jobs.try_send((id, event.clone(), scale)).is_ok())
    }
}

impl Drop for ArtworkWorker {
    fn drop(&mut self) {
        drop(self.jobs.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    #[test]
    fn a_submitted_card_comes_back_with_its_id_and_scale_and_wakes_the_loop() {
        let woken = Arc::new(AtomicUsize::new(0));
        let counter = woken.clone();
        let worker = ArtworkWorker::spawn(&Config::default(), move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        let event = Event::demo("turn_complete").unwrap();
        assert!(worker.submit(7, &event, 1.5));
        let (id, scale, _art) = worker
            .results
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        assert_eq!((id, scale), (7, 1.5));
        // `notify` runs just after the result is queued.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while woken.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(woken.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn dropping_the_worker_ends_its_thread_while_idle_and_busy() {
        let worker = ArtworkWorker::spawn(&Config::default(), || {});
        drop(worker);
        let busy = ArtworkWorker::spawn(&Config::default(), || {});
        let event = Event::demo("turn_complete").unwrap();
        busy.submit(1, &event, 1.0);
        // The result is never read; the drop must still return.
        drop(busy);
    }
}
