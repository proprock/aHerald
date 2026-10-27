//! Serialized sound playback on Unix: one sound at a time per user across all aHerald
//! processes, and a repeat of the same event inside the dedup window stays silent.
//!
//! The gates are `flock` locks (`local::Scope`), so they vanish with the process holding them.
//! Producing the sound is the caller's `output`: AudioQueue on macOS, a stand-in in tests.
use super::Wave;
use crate::local::{Ownership, Scope};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// How long a sound waits for the one before it before it gives up.
const SERIAL_WAIT: Duration = Duration::from_secs(6);
const POLL: Duration = Duration::from_millis(20);

/// The name of the cross-process gate that serializes playback.
pub const SERIAL_GATE: &str = "audio";

/// Takes the gate `name`, waiting up to `timeout`. `Ok(None)` when it stayed taken or
/// `cancel` was raised.
pub fn acquire(
    name: &str,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<Option<Ownership>, String> {
    let scope = Scope::current(name)?;
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.load(Ordering::Acquire) {
            return Ok(None);
        }
        if let Some(owner) = scope.acquire()? {
            return Ok(Some(owner));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        std::thread::sleep(remaining.min(POLL));
    }
}

/// Plays `wave` through `output` once the serial gate `gate` is free.
///
/// With `dedup_ms > 0` a second call for the same `key` while the first still plays, or within
/// `dedup_ms` of its start, returns without sound. The call itself lasts until then, so it
/// belongs on a thread of its own.
pub fn play_with(
    gate: &str,
    wave: &Wave,
    key: &[u8],
    dedup_ms: u64,
    cancel: &AtomicBool,
    output: impl FnOnce(&Wave, &AtomicBool) -> Result<(), String>,
) -> Result<(), String> {
    let hash = key.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    });
    let started = Instant::now();
    let _repeat = if dedup_ms > 0 {
        let Some(guard) = acquire(&format!("{gate}-r-{hash:016x}"), Duration::ZERO, cancel)? else {
            return Ok(());
        };
        Some(guard)
    } else {
        None
    };
    let result = match acquire(gate, SERIAL_WAIT, cancel)? {
        Some(_serial) if !cancel.load(Ordering::Acquire) => output(wave, cancel),
        _ => Ok(()),
    };
    if dedup_ms > 0 {
        cooldown(started, dedup_ms, cancel);
    }
    result
}

/// Keeps the repeat gate until `dedup_ms` after `started` (at most a minute), or until cancelled.
fn cooldown(started: Instant, dedup_ms: u64, cancel: &AtomicBool) {
    let deadline = started + Duration::from_millis(dedup_ms.min(60_000));
    while !cancel.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()).min(POLL));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, atomic::AtomicUsize};

    fn wave() -> Wave {
        Wave {
            channels: 1,
            sample_rate: 8000,
            samples: vec![0; 800],
        }
    }
    fn gate(tag: &str) -> String {
        format!("audio-test-{tag}-{}", std::process::id())
    }

    #[test]
    fn two_sounds_never_play_at_the_same_time() {
        let name = gate("serial");
        let spans = Arc::new(Mutex::new(Vec::new()));
        let workers: Vec<_> = (0..3)
            .map(|i| {
                let (name, spans) = (name.clone(), spans.clone());
                std::thread::spawn(move || {
                    play_with(&name, &wave(), &[i], 0, &AtomicBool::new(false), |_, _| {
                        let start = Instant::now();
                        std::thread::sleep(Duration::from_millis(60));
                        spans.lock().unwrap().push((start, Instant::now()));
                        Ok(())
                    })
                    .unwrap();
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let mut spans = spans.lock().unwrap().clone();
        assert_eq!(spans.len(), 3);
        spans.sort();
        for pair in spans.windows(2) {
            assert!(pair[0].1 <= pair[1].0, "playback overlapped");
        }
    }

    #[test]
    fn a_repeat_inside_the_dedup_window_is_silent_and_another_event_is_not() {
        let name = gate("dedup");
        let plays = Arc::new(AtomicUsize::new(0));
        let count = {
            let plays = plays.clone();
            move |_: &Wave, _: &AtomicBool| {
                plays.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        };
        let cancel = AtomicBool::new(false);
        let first = {
            let (name, count) = (name.clone(), count.clone());
            std::thread::spawn(move || {
                play_with(&name, &wave(), b"turn", 400, &AtomicBool::new(false), count).unwrap()
            })
        };
        std::thread::sleep(Duration::from_millis(100));
        play_with(&name, &wave(), b"turn", 400, &cancel, count.clone()).unwrap();
        assert_eq!(plays.load(Ordering::SeqCst), 1);
        play_with(&name, &wave(), b"other", 0, &cancel, count.clone()).unwrap();
        assert_eq!(plays.load(Ordering::SeqCst), 2);
        first.join().unwrap();
        // Without suppression the same event sounds every time.
        play_with(&name, &wave(), b"free", 0, &cancel, count.clone()).unwrap();
        play_with(&name, &wave(), b"free", 0, &cancel, count).unwrap();
        assert_eq!(plays.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn a_repeat_may_sound_again_once_its_window_has_passed() {
        let name = gate("window");
        let plays = AtomicUsize::new(0);
        let cancel = AtomicBool::new(false);
        let count = |_: &Wave, _: &AtomicBool| {
            plays.fetch_add(1, Ordering::SeqCst);
            Ok(())
        };
        play_with(&name, &wave(), b"turn", 100, &cancel, count).unwrap();
        play_with(&name, &wave(), b"turn", 100, &cancel, count).unwrap();
        assert_eq!(plays.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_cancelled_sound_neither_waits_for_the_gate_nor_plays() {
        let name = gate("cancel");
        let owner = acquire(&name, Duration::ZERO, &AtomicBool::new(false))
            .unwrap()
            .unwrap();
        let plays = AtomicUsize::new(0);
        let started = Instant::now();
        let cancel = AtomicBool::new(true);
        play_with(&name, &wave(), b"x", 0, &cancel, |_, _| {
            plays.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        assert!(started.elapsed() < Duration::from_millis(300));
        assert_eq!(plays.load(Ordering::SeqCst), 0);
        drop(owner);
    }

    #[test]
    fn cancelling_while_waiting_for_the_gate_ends_the_wait_at_once() {
        let name = gate("waiting");
        let owner = acquire(&name, Duration::ZERO, &AtomicBool::new(false))
            .unwrap()
            .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let waiter = std::thread::spawn(move || {
            let started = Instant::now();
            play_with(&name, &wave(), b"x", 0, &flag, |_, _| {
                panic!("must not play")
            })
            .unwrap();
            started.elapsed()
        });
        std::thread::sleep(Duration::from_millis(100));
        cancel.store(true, Ordering::Release);
        assert!(waiter.join().unwrap() < Duration::from_millis(500));
        drop(owner);
    }

    #[test]
    fn the_gate_is_released_after_a_failed_output_and_after_the_owner_dropped_it() {
        let name = gate("release");
        let cancel = AtomicBool::new(false);
        let failed = play_with(&name, &wave(), b"x", 0, &cancel, |_, _| {
            Err("no device".into())
        });
        assert_eq!(failed, Err("no device".into()));
        assert!(
            acquire(&name, Duration::from_millis(500), &cancel)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn a_sound_that_cannot_get_the_gate_in_time_is_dropped_not_queued_forever() {
        let name = gate("timeout");
        let cancel = AtomicBool::new(false);
        let owner = acquire(&name, Duration::ZERO, &cancel).unwrap().unwrap();
        assert!(
            acquire(&name, Duration::from_millis(100), &cancel)
                .unwrap()
                .is_none()
        );
        drop(owner);
    }
}
