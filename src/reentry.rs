//! Reentrancy gate for a window procedure. `ShowWindow` and `SetWindowTextW` send
//! messages to the calling thread's own window synchronously, so a handler can be
//! entered while another one holds `&mut` state. The gate admits one handler at a time
//! and keeps the messages that arrive meanwhile to run once it is done.
use std::{cell::Cell, cell::RefCell, collections::VecDeque};

const BACKLOG: usize = 64;

/// A window message kept for later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Deferred {
    pub msg: u32,
    pub wparam: usize,
    pub lparam: isize,
}

/// GUI-thread only (not `Sync`).
#[derive(Default)]
pub struct Gate {
    busy: Cell<bool>,
    backlog: RefCell<VecDeque<Deferred>>,
}

impl Gate {
    /// `false` while another handler runs; the caller must then `defer` its message.
    pub fn enter(&self) -> bool {
        !self.busy.replace(true)
    }

    pub fn busy(&self) -> bool {
        self.busy.get()
    }

    pub fn leave(&self) {
        self.busy.set(false);
    }

    /// Keeps a message for `next`. A message identical to a pending one is dropped
    /// (repeated timer ticks), and the backlog is bounded: the oldest is discarded.
    pub fn defer(&self, message: Deferred) {
        let mut backlog = self.backlog.borrow_mut();
        if backlog.contains(&message) {
            return;
        }
        if backlog.len() == BACKLOG {
            backlog.pop_front();
        }
        backlog.push_back(message);
    }

    pub fn next(&self) -> Option<Deferred> {
        self.backlog.borrow_mut().pop_front()
    }
}
