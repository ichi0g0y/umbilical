//! Resume Remote Control sessions after `claude remote-control` stops.
//!
//! In spawn mode `worktree`, a session made from claude.ai runs in its own
//! worktree `<folder>/.claude/worktrees/bridge-cse_<id>` on the branch
//! `worktree-bridge-cse_<id>`. When the server stops (restart, update, Mac
//! restart), it archives every session. `claude remote-control --session-id
//! cse_<id>` started in that worktree brings the session back with its
//! history. One folder can have only one server, so the resume must run in
//! the worktree, not in the folder.
//!
//! This module has the parts without state: reading output lines, finding
//! sessions on disk and making the worktree again.

use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;

use regex::Regex;

use crate::activity::project_dir_name;

pub mod store;

/// `Session completed (...) cse_<id>` or `Session failed: ... cse_<id>`.
static ENDED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bsession (?:completed|failed)\b.*?\b(cse_[A-Za-z0-9]+)").unwrap()
});

/// `Shutting down 2 active session(s)`.
static SHUTTING_DOWN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bshutting down \d+ active session").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionLine {
    /// This session ended.
    Ended(String),
    /// The server stops and ends every session. Ends after this do not count.
    ShuttingDown,
}

pub fn scan_session_line(line: &str) -> Option<SessionLine> {
    if SHUTTING_DOWN.is_match(line) {
        return Some(SessionLine::ShuttingDown);
    }
    ENDED
        .captures(line)
        .map(|c| SessionLine::Ended(c[1].to_string()))
}

/// `cse_` and letters or digits.
pub fn is_session_id(id: &str) -> bool {
    id.strip_prefix("cse_")
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric()))
}

pub fn worktrees_dir(folder: &Path) -> PathBuf {
    folder.join(".claude").join("worktrees")
}

pub fn worktree_path(folder: &Path, id: &str) -> PathBuf {
    worktrees_dir(folder).join(format!("bridge-{id}"))
}

pub fn branch_name(id: &str) -> String {
    format!("worktree-bridge-{id}")
}

/// Remote Control sessions of this folder that are on disk now: a `bridge-cse_<id>`
/// worktree, or a transcript folder of one with `"entrypoint": "sdk-cli"`.
pub fn find_sessions(folder: &Path, projects: &Path) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    if let Ok(entries) = fs::read_dir(worktrees_dir(folder)) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some(id) = name.strip_prefix("bridge-").filter(|id| is_session_id(id))
                && e.path().is_dir()
            {
                ids.insert(id.to_string());
            }
        }
    }
    let mut bases = vec![project_dir_name(folder)];
    // claude runs in the real path, so a symlinked folder has the real name.
    if let Some(real) = fs::canonicalize(folder).ok().filter(|_| cfg!(unix)) {
        let name = project_dir_name(&real);
        if !bases.contains(&name) {
            bases.push(name);
        }
    }
    let Ok(entries) = fs::read_dir(projects) else {
        return ids;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        // `_` becomes `-` in the folder name: `...-bridge-cse-<rest>`.
        let Some(rest) = bases.iter().find_map(|b| {
            name.strip_prefix(b.as_str())?
                .strip_prefix("--claude-worktrees-bridge-cse-")
        }) else {
            continue;
        };
        let id = format!("cse_{rest}");
        if is_session_id(&id) && !ids.contains(&id) && is_remote_control_project(&e.path()) {
            ids.insert(id);
        }
    }
    ids
}

/// Does a transcript in this folder come from `claude remote-control`?
fn is_remote_control_project(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .any(|p| {
            let Ok(file) = fs::File::open(&p) else {
                return false;
            };
            BufReader::new(file)
                .lines()
                .take(20)
                .map_while(Result::ok)
                .filter_map(|l| serde_json::from_str::<serde_json::Value>(&l).ok())
                .find_map(|v| v.get("entrypoint")?.as_str().map(|e| e == "sdk-cli"))
                .unwrap_or(false)
        })
}

/// How to get the worktree back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreePlan {
    /// It is still there (it had changes or commits, so the server kept it).
    Reuse,
    /// The worktree is gone, the branch is still there.
    FromBranch,
    /// Both are gone: make a new branch from HEAD.
    NewBranch,
}

impl WorktreePlan {
    pub fn choose(worktree_exists: bool, branch_exists: bool) -> Self {
        match (worktree_exists, branch_exists) {
            (true, _) => Self::Reuse,
            (false, true) => Self::FromBranch,
            (false, false) => Self::NewBranch,
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Self::Reuse => "worktree kept",
            Self::FromBranch => "worktree made again from its branch",
            Self::NewBranch => "worktree made again from HEAD",
        }
    }
}

/// Make sure the worktree exists. `path_var` is the PATH for git.
pub fn ensure_worktree(
    folder: &Path,
    worktree: &Path,
    branch: &str,
    path_var: &str,
) -> Result<WorktreePlan, String> {
    let exists = worktree.join(".git").exists();
    let branch_exists = !exists
        && git(
            folder,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
            path_var,
        )
        .is_ok();
    let plan = WorktreePlan::choose(exists, branch_exists);
    let wt = worktree.to_string_lossy().into_owned();
    let args: Vec<&str> = match plan {
        WorktreePlan::Reuse => return Ok(plan),
        WorktreePlan::FromBranch => vec!["worktree", "add", &wt, branch],
        WorktreePlan::NewBranch => vec!["worktree", "add", &wt, "-b", branch],
    };
    match git(folder, &args, path_var) {
        Ok(()) => Ok(plan),
        // The folder is gone but git still lists it: add it again in its place.
        Err(e) if e.contains("already registered") => {
            let mut forced = args.clone();
            forced.insert(2, "-f");
            git(folder, &forced, path_var).map(|()| plan)
        }
        Err(e) => Err(e),
    }
}

fn git(dir: &Path, args: &[&str], path_var: &str) -> Result<(), String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(args)
        .env("PATH", path_var)
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window from the background daemon.
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(format!("git {}: {err}", args.join(" ")))
    }
}

#[cfg(test)]
mod tests;
