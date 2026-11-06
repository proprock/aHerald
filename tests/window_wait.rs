#![cfg(windows)]
mod support;
use std::{sync::mpsc, time::Duration};
use windows::{
    Win32::{Foundation::*, UI::WindowsAndMessaging::*},
    core::w,
};

struct Window {
    hwnd: HWND,
    hide: mpsc::Sender<Duration>,
    done: mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Window {
    fn new() -> Self {
        let (ready, receive) = mpsc::channel();
        let (hide, hidden) = mpsc::channel::<Duration>();
        let (done, finish) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            struct Owned(HWND);
            impl Drop for Owned {
                fn drop(&mut self) {
                    // SAFETY: this thread created and owns this window.
                    unsafe {
                        let _ = DestroyWindow(self.0);
                    }
                }
            }
            // SAFETY: STATIC is a system window class; strings live through the call.
            let window = Owned(
                unsafe {
                    CreateWindowExW(
                        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                        w!("STATIC"),
                        w!("AH-092 wait fixture"),
                        WS_POPUP | WS_VISIBLE,
                        0,
                        0,
                        2,
                        2,
                        None,
                        None,
                        None,
                        None,
                    )
                }
                .unwrap(),
            );
            ready.send(window.0.0 as isize).unwrap();
            if let Ok(delay) = hidden.recv_timeout(Duration::from_secs(3)) {
                // Deliberate delayed effect: prove the wait tolerates asynchronous hiding.
                std::thread::sleep(delay);
                // SAFETY: `window.0` is the fixture window created on this thread and kept alive
                // until `finish` arrives.
                unsafe {
                    let _ = ShowWindow(window.0, SW_HIDE);
                }
                let _ = finish.recv_timeout(Duration::from_secs(3));
            }
        });
        let hwnd = HWND(receive.recv_timeout(Duration::from_secs(3)).unwrap() as *mut _);
        Self {
            hwnd,
            hide,
            done,
            thread: Some(thread),
        }
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.hide.send(Duration::ZERO);
        let _ = self.done.send(());
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

#[test]
fn hidden_wait_accepts_a_delayed_native_visibility_change() {
    let window = Window::new();
    // SAFETY: `window.hwnd` is the fixture window, alive until `window` is dropped.
    assert!(unsafe { IsWindowVisible(window.hwnd) }.as_bool());
    window.hide.send(Duration::from_millis(100)).unwrap();
    support::window::wait_hidden(window.hwnd, Duration::from_secs(2)).unwrap();
    // SAFETY: `window.hwnd` is the fixture window, alive until `window` is dropped.
    assert!(!unsafe { IsWindowVisible(window.hwnd) }.as_bool());
}

#[test]
fn hidden_wait_rejects_a_window_that_stays_visible_with_last_state() {
    let window = Window::new();
    let error = support::window::wait_hidden(window.hwnd, Duration::from_millis(50)).unwrap_err();
    assert!(
        error.contains("visible=false") && error.contains("last visible=true"),
        "{error}"
    );
}
