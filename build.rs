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
    // Stamp the build time into the binary for `--version` and the About card.
    // SOURCE_DATE_EPOCH keeps reproducible builds reproducible: CI and release builds set it to the
    // commit time (`.github/actions/setup-rust`), so two builds of one commit print the same line.
    // Without it the stamp is the time this script last ran, which a local incremental build does
    // not refresh: "built ... ago" can then be older than the binary.
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    let built = env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|elapsed| elapsed.as_secs())
        })
        .unwrap_or(0);
    println!("cargo:rustc-env=AHERALD_BUILD_UNIX={built}");
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=resources/windows/aherald.rc");
    println!("cargo:rerun-if-changed=resources/windows/aherald.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for(
            "resources/windows/aherald.rc",
            ["aherald"],
            embed_resource::NONE,
        )
        .manifest_required()?;
    }
    let output = env::var_os("OUT_DIR").ok_or("Cargo OUT_DIR is unavailable")?;
    let profile = Path::new(&output)
        .ancestors()
        .nth(3)
        .ok_or("unexpected Cargo output layout")?;
    copy_directory(Path::new("assets"), &profile.join("assets"))?;
    Ok(())
}
