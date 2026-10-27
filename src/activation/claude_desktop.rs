//! Maps a Claude Code hook `session_id` to the Claude Desktop session that hosts it.
//!
//! Desktop keeps one JSON file per session (`local_<uuid>.json`) whose `cliSessionId` equals the
//! hook's `session_id`. Only that field is deserialized; prompts, titles and every other field in
//! the file are ignored and never stored or logged. The file layout is undocumented.
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

const MAX_FILE: u64 = 1024 * 1024;
const MAX_FILES: usize = 5000;
const MAX_DEPTH: usize = 4;

/// `local_` followed by ASCII letters, digits and hyphens: safe as one link path component.
pub fn is_local_id(id: &str) -> bool {
    id.strip_prefix("local_").is_some_and(|rest| {
        !rest.is_empty()
            && id.len() <= 128
            && rest.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

/// The Desktop session id (taken from the file name) whose file names `cli_session_id`, newest first.
pub fn local_session_id(root: &Path, cli_session_id: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Session {
        #[serde(rename = "cliSessionId")]
        cli_session_id: Option<String>,
    }
    let mut files = Vec::new();
    collect(root, 0, &mut files);
    files.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));
    files.into_iter().find_map(|(path, _)| {
        let id = path.file_stem()?.to_str()?;
        if !is_local_id(id) {
            return None;
        }
        let bytes = fs::read(&path).ok()?;
        let session: Session = serde_json::from_slice(&bytes).ok()?;
        (session.cli_session_id.as_deref() == Some(cli_session_id)).then(|| id.to_owned())
    })
}

fn collect(dir: &Path, depth: usize, files: &mut Vec<(PathBuf, SystemTime)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if files.len() >= MAX_FILES {
            return;
        }
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            if depth < MAX_DEPTH {
                collect(&path, depth + 1, files);
            }
        } else if meta.len() <= MAX_FILE
            && path.extension().is_some_and(|e| e == "json")
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("local_"))
        {
            files.push((path, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)));
        }
    }
}

/// Session folders of installed Claude Desktop builds: `%LOCALAPPDATA%\Claude-Data\*\claude-code-sessions`.
pub fn session_roots(local_app_data: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(local_app_data.join("Claude-Data")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path().join("claude-code-sessions"))
        .filter(|p| p.is_dir())
        .collect()
}
