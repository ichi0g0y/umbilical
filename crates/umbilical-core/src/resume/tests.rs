//! Tests for `resume.rs` and `resume/store.rs`.
use super::store::{SessionStore, StoredSession};
use super::*;

#[test]
fn reads_end_lines() {
    let ended = |id: &str| Some(SessionLine::Ended(id.to_string()));
    assert_eq!(
        scan_session_line("Session completed (12m 3s) cse_01JmUEHn9p3wCv58sXKLLKPJ"),
        ended("cse_01JmUEHn9p3wCv58sXKLLKPJ")
    );
    assert_eq!(
        scan_session_line("✗ Session failed: Process exited with error cse_01Abc"),
        ended("cse_01Abc")
    );
    assert_eq!(
        scan_session_line("Shutting down 2 active session(s)…"),
        Some(SessionLine::ShuttingDown)
    );
    assert_eq!(
        scan_session_line("Shutting down 1 active session"),
        Some(SessionLine::ShuttingDown)
    );
    for line in [
        "Session started cse_01Abc",
        "Session completed",
        "Resume this session by running claude remote-control --continue",
        "Connected to cse_01Abc",
    ] {
        assert_eq!(scan_session_line(line), None, "{line}");
    }
}

#[test]
fn session_ids() {
    assert!(is_session_id("cse_01JmUEHn9p3wCv58sXKLLKPJ"));
    assert!(!is_session_id("cse_"));
    assert!(!is_session_id("cse_a-b"));
    assert!(!is_session_id("session_01"));
    let folder = Path::new("/w/app");
    assert_eq!(
        worktree_path(folder, "cse_1"),
        Path::new("/w/app/.claude/worktrees/bridge-cse_1")
    );
    assert_eq!(branch_name("cse_1"), "worktree-bridge-cse_1");
}

#[test]
fn finds_sessions_from_worktrees_and_transcripts() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("app");
    let wt = worktrees_dir(&folder);
    for d in ["bridge-cse_A1", "brave-meitner-1", "bridge-other"] {
        fs::create_dir_all(wt.join(d)).unwrap();
    }
    let projects = tmp.path().join("projects");
    // The plain path is always checked (the real path only on Unix).
    let base = project_dir_name(&folder);
    for (sub, entry) in [
        ("--claude-worktrees-bridge-cse-B2", "sdk-cli"),
        ("--claude-worktrees-bridge-cse-C3", "claude-desktop"),
        ("--claude-worktrees-bridge-cse-A1", "sdk-cli"),
    ] {
        let d = projects.join(format!("{base}{sub}"));
        fs::create_dir_all(&d).unwrap();
        let line = format!(r#"{{"type":"user","entrypoint":"{entry}"}}"#);
        fs::write(d.join("s.jsonl"), line + "\n").unwrap();
    }
    let ids: Vec<String> = find_sessions(&folder, &projects).into_iter().collect();
    assert_eq!(ids, vec!["cse_A1", "cse_B2"]);
}

#[test]
fn chooses_worktree_plan() {
    assert_eq!(WorktreePlan::choose(true, true), WorktreePlan::Reuse);
    assert_eq!(WorktreePlan::choose(true, false), WorktreePlan::Reuse);
    assert_eq!(WorktreePlan::choose(false, true), WorktreePlan::FromBranch);
    assert_eq!(WorktreePlan::choose(false, false), WorktreePlan::NewBranch);
}

fn run_git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn makes_worktree_again() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("repo");
    fs::create_dir_all(&folder).unwrap();
    run_git(&folder, &["init", "-q"]);
    run_git(
        &folder,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "x",
        ],
    );
    let path = std::env::var("PATH").unwrap_or_default();
    let id = "cse_T1";
    let wt = worktree_path(&folder, id);
    let branch = branch_name(id);

    // Nothing there: a new branch from HEAD.
    assert_eq!(
        ensure_worktree(&folder, &wt, &branch, &path),
        Ok(WorktreePlan::NewBranch)
    );
    assert!(wt.join(".git").exists());
    // Still there: use it.
    assert_eq!(
        ensure_worktree(&folder, &wt, &branch, &path),
        Ok(WorktreePlan::Reuse)
    );
    // Removed, branch kept.
    run_git(&folder, &["worktree", "remove", &wt.to_string_lossy()]);
    assert_eq!(
        ensure_worktree(&folder, &wt, &branch, &path),
        Ok(WorktreePlan::FromBranch)
    );
    // The folder was deleted by hand, git still lists it.
    fs::remove_dir_all(&wt).unwrap();
    assert_eq!(
        ensure_worktree(&folder, &wt, &branch, &path),
        Ok(WorktreePlan::FromBranch)
    );
    assert!(wt.join(".git").exists());
}

#[test]
fn store_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let path = SessionStore::path_next_to(&tmp.path().join("config.toml"));
    assert_eq!(path, tmp.path().join("sessions.json"));
    assert_eq!(SessionStore::load(&path).unwrap(), SessionStore::default());

    let s = |id: &str| StoredSession {
        id: id.into(),
        worktree: format!("/w/app/.claude/worktrees/bridge-{id}"),
        branch: branch_name(id),
        since: 1,
    };
    let mut store = SessionStore::default();
    assert!(store.add("/w/app", s("cse_1")));
    assert!(!store.add("/w/app", s("cse_1")));
    assert!(store.add("/w/app", s("cse_2")));
    assert!(store.add("/w/web", s("cse_3")));
    store.save(&path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let mut loaded = SessionStore::load(&path).unwrap();
    assert_eq!(loaded, store);
    assert!(loaded.contains("/w/app", "cse_2"));

    assert!(loaded.remove("/w/app", "cse_1"));
    assert!(!loaded.remove("/w/app", "cse_1"));
    assert!(loaded.remove("/w/web", "cse_3"));
    assert!(!loaded.folders.contains_key("/w/web"));
    assert!(loaded.clear("/w/app"));
    assert!(!loaded.clear("/w/app"));

    fs::write(&path, "{ not json").unwrap();
    assert!(SessionStore::load(&path).is_err());
}
