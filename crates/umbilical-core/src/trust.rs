//! Mark folders as trusted in Claude Code, so the trust dialog never blocks
//! a session.
//!
//! Claude Code keeps this in `~/.claude.json` (or `$CLAUDE_CONFIG_DIR/.claude.json`)
//! as `projects["<path>"].hasTrustDialogAccepted`. Running claude processes
//! also write this file, so we change only that one key, only when needed,
//! and write with a temp file + rename.

use std::fs;
use std::io::Write;
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

/// How often to try again when the file changed while we worked on it.
const TRIES: usize = 3;

/// Make sure every path in `dirs` is trusted. Returns the keys that were changed.
pub fn ensure_trusted(file: &Path, dirs: &[PathBuf]) -> Result<Vec<String>> {
    ensure_trusted_with(file, dirs, || {})
}

/// `before_write` runs between the read and the write (tests use it to change
/// the file in that window).
fn ensure_trusted_with(
    file: &Path,
    dirs: &[PathBuf],
    mut before_write: impl FnMut(),
) -> Result<Vec<String>> {
    // A symlinked file (dotfiles) keeps its link: write the real file.
    let file = fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let keys = keys_for(dirs);
    for _ in 0..TRIES {
        let seen = stamp(&file);
        let Some((out, changed)) = add_trust(&file, &keys)? else {
            return Ok(Vec::new());
        };
        before_write();
        // Claude Code writes this file too. If it did so since our read,
        // read again, so its change is not lost.
        if stamp(&file) != seen {
            continue;
        }
        write_atomic(&file, &out)?;
        return Ok(changed);
    }
    anyhow::bail!(
        "{} keeps changing; trust is tried again on the next scan",
        file.display()
    )
}

/// The new file text and the changed keys, or None when all keys are trusted.
fn add_trust(file: &Path, keys: &[String]) -> Result<Option<(String, Vec<String>)>> {
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
    for key in keys {
        let entry = projects
            .entry(key.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        let Some(entry) = entry.as_object_mut() else {
            continue;
        };
        if entry.get("hasTrustDialogAccepted") != Some(&Value::Bool(true)) {
            entry.insert("hasTrustDialogAccepted".into(), Value::Bool(true));
            changed.push(key.clone());
        }
    }
    if changed.is_empty() {
        return Ok(None);
    }
    Ok(Some((serde_json::to_string_pretty(&root)?, changed)))
}

/// Size and modified time, to see if someone else wrote the file.
fn stamp(file: &Path) -> Option<(u64, std::time::SystemTime)> {
    let m = fs::metadata(file).ok()?;
    Some((m.len(), m.modified().ok()?))
}

/// Temp file (private from the start) + fsync + rename, so a crash or a power
/// loss never leaves a half or empty file.
fn write_atomic(file: &Path, text: &str) -> Result<()> {
    let mut tmp = file.as_os_str().to_owned();
    tmp.push(".umbilical-tmp");
    let tmp = PathBuf::from(tmp);
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mode = fs::metadata(file)
            .map(|m| m.permissions().mode() & 0o777)
            .unwrap_or(0o600);
        opts.mode(mode);
    }
    let mut f = opts
        .open(&tmp)
        .with_context(|| format!("write {}", tmp.display()))?;
    f.write_all(text.as_bytes())
        .and_then(|()| f.sync_all())
        .with_context(|| format!("write {}", tmp.display()))?;
    drop(f);
    fs::rename(&tmp, file).with_context(|| format!("replace {}", file.display()))
}

/// The key for each folder, and also for its real path when it is a symlink
/// (claude runs in the real path).
fn keys_for(dirs: &[PathBuf]) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for dir in dirs {
        // On Windows `canonicalize` gives a `\\?\C:\...` path, which Claude
        // Code never uses as a key. Symlinked folders are a Unix case.
        let real = if cfg!(unix) {
            fs::canonicalize(dir).ok()
        } else {
            None
        };
        for key in std::iter::once(key_for(dir)).chain(real.map(|r| key_for(&r))) {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys
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
    fn a_write_by_claude_in_between_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".claude.json");
        fs::write(&file, r#"{"numStartups": 1}"#).unwrap();
        let mut first = true;
        ensure_trusted_with(&file, &[PathBuf::from("/x")], || {
            if first {
                first = false;
                // Claude Code writes the file between our read and our write.
                fs::write(&file, r#"{"numStartups": 2, "tipsShown": true}"#).unwrap();
            }
        })
        .unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["numStartups"], 2);
        assert_eq!(v["tipsShown"], true);
        assert_eq!(v["projects"]["/x"]["hasTrustDialogAccepted"], true);
    }

    #[cfg(unix)]
    #[test]
    fn keeps_a_symlink_and_the_file_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("dotfiles-claude.json");
        fs::write(&real, "{}").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
        let link = dir.path().join(".claude.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        ensure_trusted(&link, &[PathBuf::from("/x")]).unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let v: Value = serde_json::from_str(&fs::read_to_string(&real).unwrap()).unwrap();
        assert_eq!(v["projects"]["/x"]["hasTrustDialogAccepted"], true);
        let mode = fs::metadata(&real).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn trusts_the_real_path_of_a_symlinked_folder() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        fs::create_dir(&project).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&project, &link).unwrap();
        let file = dir.path().join(".claude.json");

        ensure_trusted(&file, std::slice::from_ref(&link)).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        let real = key_for(&fs::canonicalize(&project).unwrap());
        assert_eq!(
            v["projects"][&key_for(&link)]["hasTrustDialogAccepted"],
            true
        );
        assert_eq!(v["projects"][&real]["hasTrustDialogAccepted"], true);
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
