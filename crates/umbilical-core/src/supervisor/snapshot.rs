//! Build the status snapshot for the GUI.

use std::path::PathBuf;

use super::{DirStatus, RunState, Snapshot, Worker};
use crate::env::find_claude;

impl Worker {
    pub(super) fn publish(&self) {
        let log_dir = self.config.log_dir();
        let dirs: Vec<DirStatus> = self
            .entries
            .iter()
            .map(|(key, e)| {
                let out = e.output.lock().unwrap();
                let state = if e.proc.is_some() {
                    RunState::Running
                } else if !e.target.enabled {
                    RunState::Disabled
                } else if e.manual_stop {
                    RunState::Stopped
                } else {
                    RunState::Waiting
                };
                DirStatus {
                    key: key.clone(),
                    name: e.target.name.clone(),
                    path: e.target.path.to_string_lossy().into_owned(),
                    source: e.target.source.clone(),
                    state,
                    pid: e.proc.as_ref().and_then(|p| p.pid()),
                    started_at: e.started_unix,
                    next_restart_at: e.next_start_unix.filter(|_| state == RunState::Waiting),
                    restarts: e.restarts,
                    last_exit: e.last_exit.clone(),
                    last_error: e.last_error.clone(),
                    attention: out.attention,
                    already_served: out.already_served && state == RunState::Waiting,
                    session_url: out.session_url.clone(),
                    command: e.proc.as_ref().map(|p| p.spec.display()),
                    spawn: e.target.settings.spawn.clone(),
                    effective_spawn: e.target.settings.effective_spawn.clone(),
                    log_path: self.log_path_for(key).to_string_lossy().into_owned(),
                    busy: 0,
                    kept_sessions: self.store.sessions(key).len(),
                    resumed: e
                        .sessions
                        .children
                        .iter()
                        .map(|(id, c)| c.status(id, &self.log_path_for(key)))
                        .collect(),
                    resume_note: e.sessions.note.clone(),
                }
            })
            .collect();
        let snapshot = Snapshot {
            running: dirs.iter().filter(|d| d.state == RunState::Running).count(),
            total: dirs
                .iter()
                .filter(|d| d.state != RunState::Disabled)
                .count(),
            dirs,
            config_path: self.config_path.to_string_lossy().into_owned(),
            config_error: self.config_error.clone(),
            config_held: self.config_held,
            // The GUI fills it in.
            config_mtime: None,
            log_dir: log_dir.to_string_lossy().into_owned(),
            claude_bin: find_claude(&self.config.claude_bin, &self.path_var)
                .map(|p| p.to_string_lossy().into_owned()),
            needs_setup: self.needs_setup,
            daemon_error: None,
        };
        *self.shared.snapshot.lock().unwrap() = snapshot;
    }

    pub(super) fn log_path_for(&self, key: &str) -> PathBuf {
        let name = self
            .entries
            .get(key)
            .map(|e| e.target.name.as_str())
            .unwrap_or("unknown");
        let safe: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || "-_.".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        // A sub folder, so a target named "umbilical" never mixes with the app log.
        self.config
            .log_dir()
            .join("sessions")
            .join(format!("{safe}.log"))
    }

    pub(super) fn claude_debug_path(&self, key: &str) -> PathBuf {
        self.log_path_for(key).with_extension("claude-debug.log")
    }
}
