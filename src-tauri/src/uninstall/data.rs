//! Delete the settings and the logs. Only the files Umbilical writes: the
//! default root `~/.umbilical/repos` and a log folder from the config can
//! hold the user's own files.

use std::fs;
use std::path::{Path, PathBuf};

/// The files Umbilical writes next to the config file.
const OWN_FILES: &[&str] = &[
    "config.toml",
    "config.toml.tmp",
    "daemon.json",
    "daemon.json.tmp",
    "daemon.lock",
    "sessions.json",
    "sessions.json.tmp",
    "update-failed",
];

/// Returns a note for each folder that is kept because it holds other files.
/// `app_dirs` are deleted with all they hold: only Umbilical uses them.
pub fn delete(
    config_dir: &Path,
    log_dir: &Path,
    own_log_dir: bool,
    app_dirs: &[PathBuf],
) -> Vec<String> {
    // The logs first: a log folder from the config can be in `config_dir`.
    if own_log_dir {
        let _ = fs::remove_dir_all(log_dir);
    } else {
        delete_logs(log_dir);
        let _ = fs::remove_dir(log_dir);
    }
    for dir in app_dirs {
        let _ = fs::remove_dir_all(dir);
    }
    for name in OWN_FILES {
        let _ = fs::remove_file(config_dir.join(name));
    }
    // `remove_dir` removes only an empty folder.
    let _ = fs::remove_dir(config_dir.join("repos"));
    let _ = fs::remove_dir(config_dir);

    let mut notes = Vec::new();
    if config_dir.exists() {
        notes.push(format!(
            "{} is kept: it holds other files (for example repositories in repos).",
            config_dir.display()
        ));
    }
    if log_dir.exists() {
        notes.push(format!(
            "{} is kept: it holds other files.",
            log_dir.display()
        ));
    }
    notes
}

/// The app log, the login log and the session logs (`sessions/*.log`,
/// rotated `*.log.1`).
fn delete_logs(dir: &Path) {
    for name in ["umbilical.log", "umbilical.log.1", "login.log"] {
        let _ = fs::remove_file(dir.join(name));
    }
    let sessions = dir.join("sessions");
    if let Ok(entries) = fs::read_dir(&sessions) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let is_log = name.ends_with(".log") || name.ends_with(".log.1");
            if is_log && entry.file_type().is_ok_and(|t| t.is_file()) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    let _ = fs::remove_dir(&sessions);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("umbilical-uninstall-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }

    #[test]
    fn deletes_everything_of_ours() {
        let root = scratch("ours");
        let config = root.join(".umbilical");
        let logs = root.join("Logs").join("Umbilical");
        touch(&config.join("config.toml"));
        touch(&config.join("daemon.lock"));
        fs::create_dir_all(config.join("repos")).unwrap();
        touch(&logs.join("umbilical.log"));
        touch(&logs.join("sessions").join("a.log"));
        let webkit = root.join("WebKit").join("sh.ich.umbilical");
        touch(&webkit.join("LocalStorage").join("x"));

        let notes = delete(&config, &logs, true, std::slice::from_ref(&webkit));
        assert!(notes.is_empty(), "{notes:?}");
        assert!(!config.exists());
        assert!(!logs.exists());
        assert!(!webkit.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn keeps_repositories_and_other_files() {
        let root = scratch("keep");
        let config = root.join(".umbilical");
        let logs = root.join("my-logs");
        touch(&config.join("config.toml"));
        touch(&config.join("repos").join("app").join(".git").join("HEAD"));
        touch(&logs.join("umbilical.log"));
        touch(&logs.join("sessions").join("a.log.1"));
        touch(&logs.join("sessions").join("notes.txt"));
        touch(&logs.join("mine.txt"));

        let notes = delete(&config, &logs, false, &[]);
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(!config.join("config.toml").exists());
        assert!(config.join("repos/app/.git/HEAD").exists());
        assert!(!logs.join("umbilical.log").exists());
        assert!(!logs.join("sessions/a.log.1").exists());
        assert!(logs.join("sessions/notes.txt").exists());
        assert!(logs.join("mine.txt").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn log_folder_inside_the_config_folder() {
        let root = scratch("inside");
        let config = root.join(".umbilical");
        let logs = config.join("logs");
        touch(&config.join("config.toml"));
        touch(&logs.join("login.log"));

        let notes = delete(&config, &logs, false, &[]);
        assert!(notes.is_empty(), "{notes:?}");
        assert!(!config.exists());
        let _ = fs::remove_dir_all(&root);
    }
}
