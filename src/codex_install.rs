//! Locate an existing native Codex binary without executing package-manager shims.
use std::{ffi::OsStr, path::PathBuf};

pub(crate) fn find(path: &OsStr) -> Option<PathBuf> {
    // Do not search the working directory or relative PATH entries: hooks can run
    // in arbitrary repositories. Only explicitly configured absolute directories qualify.
    let dirs: Vec<_> = std::env::split_paths(path)
        .filter(|dir| dir.is_absolute())
        .collect();
    let name = crate::paths::executable_name("codex");
    if let Some(native) = dirs.iter().map(|dir| dir.join(&name)).find(|p| p.is_file()) {
        return Some(native);
    }
    // The npm launcher layout below only exists on Windows.
    if !cfg!(windows) {
        return None;
    }
    for dir in dirs {
        if !dir.join("codex.cmd").is_file() {
            continue;
        }
        let scope = dir.join("node_modules/@openai");
        // Match the npm launcher's nested/hoisted optional platform dependency,
        // then its bundled-vendor fallback. Never execute or parse the cmd shim.
        for package in [
            scope.join("codex/node_modules/@openai/codex-win32-x64"),
            scope.join("codex-win32-x64"),
            scope.join("codex"),
        ] {
            let native = package.join("vendor/x86_64-pc-windows-msvc/bin/codex.exe");
            if native.is_file() {
                return Some(native);
            }
        }
    }
    None
}

/// Where the Codex desktop app keeps its bundled CLI on macOS; the app does not put it on
/// `PATH`. Fixed absolute locations only, never searched for.
pub(crate) fn app_bundles() -> Vec<PathBuf> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    const INSIDE: &str = "Codex.app/Contents/Resources/codex";
    let mut found = vec![PathBuf::from("/Applications").join(INSIDE)];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && home.is_absolute()
    {
        found.push(home.join("Applications").join(INSIDE));
    }
    found
}

/// The first of `candidates` that is a regular file.
pub(crate) fn find_app_bundle(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|path| path.is_file()).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::Path,
        sync::atomic::{AtomicUsize, Ordering},
    };

    struct Installation(PathBuf);
    impl Installation {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "aherald-codex-discovery-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn file(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"fixture").unwrap();
            path
        }
        fn path(&self, dirs: &[&str]) -> std::ffi::OsString {
            std::env::join_paths(dirs.iter().map(|dir| self.0.join(dir))).unwrap()
        }
    }
    impl Drop for Installation {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[cfg(windows)]
    const NESTED: &str = "npm prefix/node_modules/@openai/codex/node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe";

    fn native_name(dir: &str) -> String {
        format!("{dir}/{}", crate::paths::executable_name("codex"))
    }

    #[cfg(windows)]
    #[test]
    fn native_path_binary_precedes_npm_even_in_later_path_entry() {
        let install = Installation::new();
        install.file("npm prefix/codex.cmd");
        install.file(NESTED);
        let native = install.file("native/codex.exe");
        assert_eq!(find(&install.path(&["npm prefix", "native"])), Some(native));
    }
    #[test]
    fn native_binary_follows_path_order() {
        let install = Installation::new();
        let first = install.file(&native_name("first"));
        install.file(&native_name("second"));
        assert_eq!(find(&install.path(&["first", "second"])), Some(first));
    }
    #[cfg(windows)]
    #[test]
    fn npm_fallback_finds_nested_platform_package_in_spaced_prefix() {
        let install = Installation::new();
        install.file("npm prefix/codex.cmd");
        let native = install.file(NESTED);
        assert_eq!(
            find(&install.path(&["missing", "npm prefix"])),
            Some(native)
        );
    }
    #[cfg(windows)]
    #[test]
    fn npm_fallback_supports_hoisted_and_bundled_vendor_layouts() {
        for relative in [
            "npm prefix/node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe",
            "npm prefix/node_modules/@openai/codex/vendor/x86_64-pc-windows-msvc/bin/codex.exe",
        ] {
            let install = Installation::new();
            install.file("npm prefix/codex.cmd");
            let native = install.file(relative);
            assert_eq!(find(&install.path(&["npm prefix"])), Some(native));
        }
    }
    #[cfg(windows)]
    #[test]
    fn incomplete_installation_does_not_hide_later_working_npm_prefix() {
        let install = Installation::new();
        install.file("broken/codex.cmd");
        install.file("npm prefix/codex.cmd");
        let native = install.file(NESTED);
        assert_eq!(find(&install.path(&["broken", "npm prefix"])), Some(native));
    }
    #[test]
    fn an_app_bundle_is_used_only_when_it_is_a_file_and_the_first_one_wins() {
        let install = Installation::new();
        let first = install.file("first/Codex.app/Contents/Resources/codex");
        let second = install.file("second/Codex.app/Contents/Resources/codex");
        let missing = install.0.join("none/codex");
        let folder = install.0.join("folder");
        fs::create_dir_all(&folder).unwrap();
        assert_eq!(
            find_app_bundle(&[missing.clone(), folder.clone(), second.clone(), first]),
            Some(second)
        );
        assert_eq!(find_app_bundle(&[missing, folder]), None);
        assert_eq!(find_app_bundle(&[]), None);
    }
    #[test]
    fn app_bundle_locations_are_absolute_and_exist_only_on_macos() {
        let bundles = app_bundles();
        assert_eq!(!bundles.is_empty(), cfg!(target_os = "macos"));
        assert!(bundles.iter().all(|path| path.is_absolute()));
    }
    #[test]
    fn missing_binary_and_relative_search_paths_are_unavailable() {
        let install = Installation::new();
        install.file("npm prefix/codex.cmd");
        assert_eq!(find(&install.path(&["npm prefix"])), None);
        assert_eq!(find(OsStr::new("")), None);
        assert_eq!(find(Path::new(".").as_os_str()), None);
        #[cfg(windows)]
        {
            install.file(NESTED);
            fs::remove_file(install.0.join("npm prefix/codex.cmd")).unwrap();
            assert_eq!(find(&install.path(&["npm prefix"])), None);
        }
    }
}
