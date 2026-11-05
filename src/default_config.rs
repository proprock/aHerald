//! Publish a complete default without replacing an existing user file.
use crate::atomic_file::{Temporary, publish, temporary_sibling};
use std::{fs::OpenOptions, io::Write, path::Path};
pub(crate) fn create(path: &Path) -> Result<(), String> {
    let result = (|| -> std::io::Result<()> {
        // Application Support/aHerald does not exist on the first start of a bundle.
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = temporary_sibling(path)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        let temporary = Temporary(temporary);
        file.write_all(include_bytes!("../config.example.toml"))?;
        file.sync_all()?;
        drop(file);
        // Exclusive: never replaces a file that is already there.
        match publish(&temporary.0, path, false) {
            Ok(()) => Ok(()),
            // Another starter or the user published a file first. Never replace it.
            Err(_) if path.try_exists()? => Ok(()),
            Err(e) => Err(e),
        }
    })();
    result.map_err(|e| {
        format!(
            "cannot create default configuration {}: {e}",
            path.display()
        )
    })
}
