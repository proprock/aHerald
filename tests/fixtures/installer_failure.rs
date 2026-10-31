use std::{env, fs, path::Path};
fn main() {
    let dir = env::args()
        .find_map(|arg| arg.strip_prefix("/DIR=").map(str::to_owned))
        .unwrap();
    let dir = Path::new(dir.trim_matches('"'));
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("aherald.exe"), b"partial installation").unwrap();
    std::process::exit(1);
}
