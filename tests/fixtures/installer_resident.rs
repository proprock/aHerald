// Test-only old application boundary. No real aHerald IPC or user settings.
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() {
    let root = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let file = |name: &str| -> PathBuf { root.join(name) };
    match std::env::args().nth(1).as_deref() {
        Some("--status") => {
            let pid = std::fs::read_to_string(file("resident.pid"))
                .ok()
                .and_then(|s| s.parse::<u32>().ok());
            if let Some(pid) = pid {
                println!(
                    "{{\"result\":\"status\",\"pid\":{pid},\"queued\":0,\"paused\":false,\"sounds\":true,\"visuals\":true}}"
                )
            } else {
                println!("{{\"running\":false}}")
            }
        }
        // The final uninstall shutdown check selects no harnesses; this boundary
        // fixture has no hook registrations to remove.
        Some("--unsetup") => println!("{{}}"),
        Some("--quit") => {
            std::fs::write(file("quit-called"), "yes").unwrap();
            if !file("block-quit").exists() {
                std::fs::write(file("quit-request"), "yes").unwrap();
            }
            println!("{{\"result\":\"bye\"}}");
        }
        Some("--resident") => {
            std::fs::write(file("resident.pid"), std::process::id().to_string()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(120);
            while !file("quit-request").exists() {
                if Instant::now() >= deadline {
                    std::process::exit(2)
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            // Keep the image locked after the acknowledgement to exercise real exit waiting.
            std::thread::sleep(Duration::from_millis(200));
            std::fs::write(file("graceful-exit"), "yes").unwrap();
        }
        _ => std::process::exit(3),
    }
}
