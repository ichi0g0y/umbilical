//! Environment for child processes: PATH and the claude binary.

use std::path::{Path, PathBuf};

use crate::config::{expand_home, home};

/// PATH to give the children.
///
/// Apps started at login on macOS get a very small PATH, so tools from
/// mise or Homebrew are not found. We ask the login shell for the real PATH.
/// On Windows, GUI apps already get the user PATH.
pub fn child_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    #[cfg(unix)]
    {
        if let Some(shell_path) = login_shell_path() {
            return merge_paths(&shell_path, &current);
        }
    }
    current
}

#[cfg(unix)]
fn login_shell_path() -> Option<String> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const MARK: &str = "__UMBILICAL_PATH__=";
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/zsh".into());
    let mut child = Command::new(shell)
        .args(["-lic", &format!("printf '%s%s\\n' '{MARK}' \"$PATH\"")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // A broken shell profile must not block start-up.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let out = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .rev()
        .find_map(|l| l.strip_prefix(MARK))
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
}

/// Join two PATH values. Keeps the order and drops duplicates.
pub fn merge_paths(first: &str, second: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let parts: Vec<PathBuf> = std::env::split_paths(first)
        .chain(std::env::split_paths(second))
        .filter(|p| !p.as_os_str().is_empty() && seen.insert(p.clone()))
        .collect();
    std::env::join_paths(parts)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| first.to_string())
}

/// Find the claude binary. `configured` wins if it is not empty.
pub fn find_claude(configured: &str, path_var: &str) -> Option<PathBuf> {
    if !configured.trim().is_empty() {
        return Some(expand_home(configured));
    }
    if let Some(p) = std::env::var_os("CLAUDE_BIN").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd", "claude"]
    } else {
        &["claude"]
    };
    let local_bin = home().join(".local").join("bin");
    std::iter::once(local_bin)
        .chain(std::env::split_paths(path_var))
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| p.is_file())
}

/// Program and first arguments to run `bin`. On Windows, `.cmd` and `.bat`
/// files must run through `cmd.exe`.
pub fn launcher(bin: &Path) -> (PathBuf, Vec<String>) {
    let ext = bin
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    if cfg!(windows) && matches!(ext.as_deref(), Some("cmd" | "bat")) {
        return (
            PathBuf::from("cmd.exe"),
            vec!["/C".into(), bin.to_string_lossy().into_owned()],
        );
    }
    (bin.to_path_buf(), Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_drops_duplicates() {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let a = format!("/a{sep}/b");
        let b = format!("/b{sep}/c");
        assert_eq!(merge_paths(&a, &b), format!("/a{sep}/b{sep}/c"));
    }

    #[test]
    fn configured_claude_wins() {
        assert_eq!(
            find_claude("/opt/claude", ""),
            Some(PathBuf::from("/opt/claude"))
        );
    }
}
