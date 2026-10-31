use std::{env, fs, path::Path};
fn main() {
    let args: Vec<_> = env::args().collect();
    if args.get(1).map(String::as_str) == Some("--delayed-child") {
        std::thread::sleep(std::time::Duration::from_millis(500));
        fs::write(&args[2], b"complete").unwrap();
        return;
    }
    if args.get(1).map(String::as_str) == Some("--bootstrap") {
        std::process::Command::new(env::current_exe().unwrap())
            .args(["--delayed-child", &args[2]])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        return;
    }
    let dir = env::args()
        .find_map(|arg| arg.strip_prefix("/DIR=").map(str::to_owned))
        .unwrap();
    let dir = Path::new(dir.trim_matches('"'));
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("aherald.exe"), b"partial installation").unwrap();
    std::process::exit(1);
}
