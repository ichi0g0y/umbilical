//! `~/.umbilical/sessions.json`: the Remote Control sessions that live now, per folder.
//!
//! It is written on every change, not only at stop, because a Mac restart
//! can kill everything with no order. Only Umbilical reads it (mode 600).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionStore {
    /// Folder key -> its live Remote Control sessions.
    pub folders: BTreeMap<String, Vec<StoredSession>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredSession {
    /// `cse_<id>`.
    pub id: String,
    pub worktree: String,
    pub branch: String,
    /// When Umbilical first saw it (unix seconds).
    #[serde(default)]
    pub since: u64,
}

impl SessionStore {
    /// `sessions.json` next to the config file.
    pub fn path_next_to(config_path: &Path) -> PathBuf {
        config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
            .join("sessions.json")
    }

    /// A missing file is an empty store.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => {
                serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        crate::daemon::write_private(path, &serde_json::to_string_pretty(self)?)
            .with_context(|| format!("write {}", path.display()))
    }

    pub fn sessions(&self, folder: &str) -> &[StoredSession] {
        self.folders.get(folder).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn contains(&self, folder: &str, id: &str) -> bool {
        self.sessions(folder).iter().any(|s| s.id == id)
    }

    /// Returns false if it was already there.
    pub fn add(&mut self, folder: &str, session: StoredSession) -> bool {
        if self.contains(folder, &session.id) {
            return false;
        }
        self.folders
            .entry(folder.to_string())
            .or_default()
            .push(session);
        true
    }

    /// Returns false if it was not there.
    pub fn remove(&mut self, folder: &str, id: &str) -> bool {
        let Some(list) = self.folders.get_mut(folder) else {
            return false;
        };
        let before = list.len();
        list.retain(|s| s.id != id);
        let removed = list.len() != before;
        if list.is_empty() {
            self.folders.remove(folder);
        }
        removed
    }

    /// Forget every session of the folder. Returns false if it had none.
    pub fn clear(&mut self, folder: &str) -> bool {
        self.folders.remove(folder).is_some()
    }
}
