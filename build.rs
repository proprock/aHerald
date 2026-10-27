//! Stage replaceable bundled resources beside Cargo's application executable.
use std::{env, fs, io, path::Path};

fn copy_directory(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if entry.file_type()?.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=assets");
    let output = env::var_os("OUT_DIR").ok_or("Cargo OUT_DIR is unavailable")?;
    let profile = Path::new(&output)
        .ancestors()
        .nth(3)
        .ok_or("unexpected Cargo output layout")?;
    copy_directory(Path::new("assets"), &profile.join("assets"))?;
    Ok(())
}
