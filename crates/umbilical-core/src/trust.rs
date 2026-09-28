//! Mark folders as trusted in Claude Code, so the trust dialog never blocks
//! a session.
//!
//! Claude Code keeps this in `~/.claude.json` (or `$CLAUDE_CONFIG_DIR/.claude.json`)
//! as `projects["<path>"].hasTrustDialogAccepted`. Running claude processes
//! also write this file, so we change only that one key, only when needed,
//! and write with a temp file + rename.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Map, Value};

use crate::config::home;

pub fn claude_json_path() -> PathBuf {
    match std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        Some(dir) => PathBuf::from(dir).join(".claude.json"),
        None => home().join(".claude.json"),
    }
}

/// Make sure every path in `dirs` is trusted. Returns the paths that were changed.
pub fn ensure_trusted(file: &Path, dirs: &[PathBuf]) -> Result<Vec<String>> {
    let text = match fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}".into(),
        Err(e) => return Err(e).with_context(|| format!("read {}", file.display())),
    };
    let mut root: Value =
        serde_json::from_str(&text).with_context(|| format!("parse {}", file.display()))?;
    let Some(obj) = root.as_object_mut() else {
        anyhow::bail!("{} is not a JSON object", file.display());
    };
    let projects = obj
        .entry("projects")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(projects) = projects.as_object_mut() else {
        anyhow::bail!("projects in {} is not an object", file.display());
    };

    let mut changed = Vec::new();
    for dir in dirs {
        let key = key_for(dir);
        let entry = projects
            .entry(key.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        let Some(entry) = entry.as_object_mut() else {
            continue;
        };
        if entry.get("hasTrustDialogAccepted") != Some(&Value::Bool(true)) {
            entry.insert("hasTrustDialogAccepted".into(), Value::Bool(true));
            changed.push(key);
        }
    }
    if changed.is_empty() {
        return Ok(changed);
    }

    let out = serde_json::to_string_pretty(&root)?;
    let mut tmp = file.as_os_str().to_owned();
    tmp.push(".umbilical-tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, out).with_context(|| format!("write {}", tmp.display()))?;
    // Keep the old file mode (it is private).
    #[cfg(unix)]
    if let Ok(meta) = fs::metadata(file) {
        let _ = fs::set_permissions(&tmp, meta.permissions());
    }
    fs::rename(&tmp, file).with_context(|| format!("replace {}", file.display()))?;
    Ok(changed)
}

/// Claude Code uses the plain path as the key.
fn key_for(dir: &Path) -> String {
    let s = dir.to_string_lossy();
    let s = s.trim_end_matches(['/', '\\']);
    if cfg!(windows) {
        // Assumed from how Claude Code writes paths on Windows. Not checked yet.
        s.replace('\\', "/")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_trust_and_keeps_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".claude.json");
        fs::write(
            &file,
            r#"{"numStartups": 3, "projects": {"/a": {"hasTrustDialogAccepted": false, "lastCost": 1.5}, "/b": {"hasTrustDialogAccepted": true}}}"#,
        )
        .unwrap();

        let changed = ensure_trusted(
            &file,
            &[
                PathBuf::from("/a"),
                PathBuf::from("/b"),
                PathBuf::from("/c/"),
            ],
        )
        .unwrap();
        assert_eq!(changed, vec!["/a", "/c"]);

        let v: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["numStartups"], 3);
        assert_eq!(v["projects"]["/a"]["hasTrustDialogAccepted"], true);
        assert_eq!(v["projects"]["/a"]["lastCost"], 1.5);
        assert_eq!(v["projects"]["/c"]["hasTrustDialogAccepted"], true);
        // Keys keep their order.
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.find("numStartups").unwrap() < text.find("projects").unwrap());
    }

    #[test]
    fn no_write_when_nothing_changes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".claude.json");
        let original = r#"{"projects":{"/a":{"hasTrustDialogAccepted":true}}}"#;
        fs::write(&file, original).unwrap();
        assert!(
            ensure_trusted(&file, &[PathBuf::from("/a")])
                .unwrap()
                .is_empty()
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
    }

    #[test]
    fn missing_file_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".claude.json");
        ensure_trusted(&file, &[PathBuf::from("/x")]).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["projects"]["/x"]["hasTrustDialogAccepted"], true);
    }

    #[test]
    fn broken_file_is_not_touched() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".claude.json");
        fs::write(&file, "{not json").unwrap();
        assert!(ensure_trusted(&file, &[PathBuf::from("/x")]).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "{not json");
    }
}
