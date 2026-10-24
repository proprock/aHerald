#![cfg(windows)]
use std::{
    io::Write,
    mem::size_of,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::*,
    System::{Diagnostics::ToolHelp::*, Threading::*},
};
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct OwnedProcess(Handle);
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        unsafe {
            let _ = TerminateProcess(self.0.0, 0);
            let _ = WaitForSingleObject(self.0.0, 2000);
        }
    }
}
#[test]
fn hook_process_returns_while_its_finite_or_indefinite_overlay_remains_alive() {
    for duration in [30_000, 0] {
        let path = std::env::temp_dir().join(format!(
            "aherald-hook-life-{}-{duration}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            format!("[events.turn_complete]\nduration_ms={duration}\n[sounds]\nenabled=false"),
        )
        .unwrap();
        let start = Instant::now();
        let mut hook = Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(["--hook", "--config"])
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let parent = hook.id();
        hook.stdin
            .take()
            .unwrap()
            .write_all(include_bytes!("fixtures/stop.json"))
            .unwrap();
        let deadline = start + Duration::from_secs(3);
        loop {
            if hook.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                let _ = hook.kill();
                let _ = hook.wait();
                panic!("hook process blocked");
            }
            std::thread::yield_now();
        }
        let hook_return_ms = start.elapsed().as_millis();
        assert!(hook_return_ms < 3000);
        let output = hook.wait_with_output().unwrap();
        assert!(output.status.success());
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"{}\n");
        // SAFETY: snapshot handles are RAII-owned; only the child of this test's hook is opened.
        let child = unsafe {
            let snapshot = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).unwrap());
            let mut entry = PROCESSENTRY32W {
                dwSize: size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut found = None;
            if Process32FirstW(snapshot.0, &mut entry).is_ok() {
                loop {
                    if entry.th32ParentProcessID == parent {
                        found = Some(OwnedProcess(Handle(
                            OpenProcess(
                                PROCESS_TERMINATE | PROCESS_SYNCHRONIZE,
                                false,
                                entry.th32ProcessID,
                            )
                            .unwrap(),
                        )));
                        break;
                    }
                    if Process32NextW(snapshot.0, &mut entry).is_err() {
                        break;
                    }
                }
            }
            found.expect("hook did not leave its detached notification child alive")
        };
        unsafe {
            assert_eq!(WaitForSingleObject(child.0.0, 0), WAIT_TIMEOUT);
        }

        println!("duration_ms={duration}, hook_return_ms={}", hook_return_ms);
        std::fs::remove_file(&path).unwrap();
    }
}
