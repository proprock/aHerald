//! The window procedure is entered again by messages that `ShowWindow` and
//! `SetWindowTextW` send synchronously. The gate admits one handler at a time and keeps
//! the messages that arrived meanwhile, bounded and in order, to run afterwards.
use aherald::reentry::{Deferred, Gate};

fn message(msg: u32) -> Deferred {
    Deferred {
        msg,
        wparam: 0,
        lparam: 0,
    }
}

#[test]
fn only_one_handler_runs_at_a_time() {
    let gate = Gate::default();
    assert!(!gate.busy());
    assert!(gate.enter());
    assert!(gate.busy());
    assert!(!gate.enter(), "a nested handler must be refused");
    gate.leave();
    assert!(!gate.busy());
    assert!(gate.enter(), "the gate reopens after the handler leaves");
}

#[test]
fn deferred_messages_replay_in_arrival_order() {
    let gate = Gate::default();
    gate.defer(message(3));
    gate.defer(message(1));
    gate.defer(message(2));
    let replay: Vec<u32> = std::iter::from_fn(|| gate.next()).map(|m| m.msg).collect();
    assert_eq!(replay, [3, 1, 2]);
    assert!(gate.next().is_none());
}

#[test]
fn an_identical_pending_message_is_kept_once() {
    let gate = Gate::default();
    for _ in 0..100 {
        gate.defer(message(0x113)); // a timer firing while a handler runs
    }
    gate.defer(Deferred {
        msg: 0x100,
        wparam: 0x25,
        lparam: 0,
    });
    gate.defer(Deferred {
        msg: 0x100,
        wparam: 0x27,
        lparam: 0,
    });
    assert_eq!(std::iter::from_fn(|| gate.next()).count(), 3);
}

#[test]
fn the_backlog_is_bounded() {
    let gate = Gate::default();
    for i in 0..10_000 {
        gate.defer(Deferred {
            msg: 0x100,
            wparam: i,
            lparam: 0,
        });
    }
    let kept = std::iter::from_fn(|| gate.next()).count();
    assert!(kept > 0 && kept <= 64, "kept {kept}");
}
