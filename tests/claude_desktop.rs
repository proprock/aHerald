use aherald::activation::claude_desktop::local_session_id;
use std::{fs, path::PathBuf};

struct Root(PathBuf);
impl Root {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("aherald-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, relative: &str, text: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn finds_the_desktop_session_that_owns_a_cli_session_id() {
    let root = Root::new("desktop-find");
    root.write(
        "org/acct/local_aaa.json",
        r#"{"sessionId":"local_aaa","cliSessionId":"other","title":"SECRET prompt"}"#,
    );
    root.write(
        "org/acct/local_bbb.json",
        r#"{"sessionId":"local_bbb","cliSessionId":"wanted","extra":{"nested":[1,2]}}"#,
    );
    assert_eq!(
        local_session_id(&root.0, "wanted").as_deref(),
        Some("local_bbb")
    );
    assert_eq!(local_session_id(&root.0, "missing"), None);
}

#[test]
fn skips_unreadable_oversized_and_misnamed_files() {
    let root = Root::new("desktop-skip");
    root.write("a/b/local_bad.json", "{ not json");
    root.write(
        "a/b/notes.json",
        r#"{"sessionId":"local_x","cliSessionId":"wanted"}"#,
    );
    root.write(
        "a/b/local_big.json",
        &format!(
            r#"{{"cliSessionId":"wanted","pad":"{}"}}"#,
            "x".repeat(2 * 1024 * 1024)
        ),
    );
    root.write("a/b/local_ok.json", r#"{"cliSessionId":"wanted"}"#);
    assert_eq!(
        local_session_id(&root.0, "wanted").as_deref(),
        Some("local_ok")
    );
    assert_eq!(local_session_id(&root.0.join("absent"), "wanted"), None);
}

#[test]
fn the_identity_comes_from_the_file_name_not_from_file_contents() {
    let root = Root::new("desktop-name");
    root.write(
        "a/b/local_real.json",
        r#"{"sessionId":"local_spoofed","cliSessionId":"wanted"}"#,
    );
    assert_eq!(
        local_session_id(&root.0, "wanted").as_deref(),
        Some("local_real")
    );
}

#[cfg(windows)]
#[test]
#[ignore = "reads real Claude Desktop sessions; requires AHERALD_TEST_CLI_SESSION"]
fn manual_real_desktop_lookup() {
    let id = std::env::var("AHERALD_TEST_CLI_SESSION").expect("set a real hook session_id");
    let base = std::env::var_os("LOCALAPPDATA").unwrap();
    let found = aherald::activation::claude_desktop::session_roots(std::path::Path::new(&base))
        .iter()
        .find_map(|root| local_session_id(root, &id));
    println!("desktop session: {found:?}");
    assert!(found.is_some());
}
