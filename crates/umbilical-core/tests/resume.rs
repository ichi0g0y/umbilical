//! Resume of Remote Control sessions, end to end with a fake `claude` script (Unix only).
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command as Git;
use std::time::{Duration, Instant};

use umbilical_core::config::Config;
use umbilical_core::resume::store::SessionStore;
use umbilical_core::{Command, DirStatus, RunState, Snapshot, Supervisor};

/// The server: on Ctrl-C it ends every session, like the real one.
/// With the file `end-server`, the session in `end-id` ends while it runs.
/// The resume process (`--session-id`): ends its session when the file `end`
/// is in its worktree.
const FAKE: &str = r#"#!/bin/sh
case "$*" in
*--session-id*)
  sid=$(echo "$*" | sed 's/.*--session-id \([^ ]*\).*/\1/')
  echo "resumed $sid: $*"
  trap 'echo "Skipping archive+deregister to allow resume"; exit 0' INT
  while true; do
    if [ -f end ]; then echo "Session failed: Process exited with error $sid"; exit 1; fi
    sleep 0.2
  done;;
esac
echo "server $*"
trap 'echo "Shutting down 1 active session(s)"; for d in .claude/worktrees/bridge-*; do echo "Session completed (2s) ${d##*bridge-}"; done; exit 0' INT
while true; do
  if [ -f end-server ]; then rm end-server; echo "Session completed (5s) $(cat end-id)"; fi
  sleep 0.2
done
"#;

fn git(dir: &Path, args: &[&str]) {
    let out = Git::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

fn wait_for(sup: &Supervisor, what: &str, f: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let s = sup.snapshot();
        if f(&s) {
            return s;
        }
        assert!(Instant::now() < deadline, "timeout: {what}: {s:#?}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn one(s: &Snapshot) -> &DirStatus {
    s.dirs.iter().find(|d| d.name == "one").unwrap()
}

fn resumed_runs(s: &Snapshot, id: &str) -> bool {
    s.dirs
        .iter()
        .flat_map(|d| &d.resumed)
        .any(|r| r.id == id && r.state == RunState::Running)
}

fn stored(path: &Path) -> Vec<String> {
    let store = SessionStore::load(path).unwrap();
    store
        .folders
        .values()
        .flatten()
        .map(|s| s.id.clone())
        .collect()
}

#[test]
fn remote_control_sessions_come_back_after_a_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("claude");
    fs::write(&bin, FAKE).unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    let folder = tmp.path().join("repos").join("one");
    fs::create_dir_all(&folder).unwrap();
    git(&folder, &["init", "-q"]);
    git(
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
    let config = Config {
        roots: vec![tmp.path().join("repos").to_string_lossy().into()],
        claude_bin: bin.to_string_lossy().into(),
        log_dir: tmp.path().join("logs").to_string_lossy().into(),
        claude_json: tmp.path().join(".claude.json").to_string_lossy().into(),
        scan_interval_secs: 1,
        backoff_min_secs: 1,
        backoff_max_secs: 2,
        ..Default::default()
    };
    let config_path = tmp.path().join("config.toml");
    config.save(&config_path).unwrap();
    let store = tmp.path().join("sessions.json");

    let sup = Supervisor::start(config_path.clone());
    wait_for(&sup, "server runs", |s| {
        s.dirs.first().is_some_and(|d| d.state == RunState::Running)
    });
    // Let the first look at the disk pass: what is there before is old.
    std::thread::sleep(Duration::from_millis(1500));

    // A Remote Control session starts: claude makes its worktree.
    let wt = folder.join(".claude/worktrees/bridge-cse_T1");
    git(
        &folder,
        &[
            "worktree",
            "add",
            "-q",
            &wt.to_string_lossy(),
            "-b",
            "worktree-bridge-cse_T1",
        ],
    );
    wait_for(&sup, "session kept", |s| one(s).kept_sessions == 1);
    assert_eq!(stored(&store), vec!["cse_T1"]);

    // Restart: the server ends the session on its way down. It must not be forgotten.
    let key = one(&sup.snapshot()).key.clone();
    sup.send(Command::Restart(key.clone()));
    let s = wait_for(&sup, "session resumed", |s| resumed_runs(s, "cse_T1"));
    assert_eq!(stored(&store), vec!["cse_T1"]);
    let tail = sup.tail(&key, 200).join("\n");
    assert!(tail.contains("Shutting down 1 active session"), "{tail}");
    let log = fs::read_to_string(&one(&s).resumed[0].log_path).unwrap();
    assert!(
        log.contains(
            "resumed cse_T1: remote-control --session-id cse_T1 --permission-mode bypassPermissions"
        ),
        "{log}"
    );

    // The daemon stops and starts again (update, Mac restart): it comes back again.
    sup.shutdown();
    drop(sup);
    assert_eq!(stored(&store), vec!["cse_T1"]);
    let sup = Supervisor::start(config_path.clone());
    wait_for(&sup, "session resumed after a daemon restart", |s| {
        resumed_runs(s, "cse_T1")
    });

    // The user archives it in claude.ai: the resume process ends. Forget it.
    fs::write(wt.join("end"), "").unwrap();
    wait_for(&sup, "resumed session gone", |s| {
        one(s).resumed.is_empty() && one(s).kept_sessions == 0
    });
    assert!(stored(&store).is_empty());

    // A second session ends in the server while it runs normally.
    let wt2 = folder.join(".claude/worktrees/bridge-cse_T2");
    git(
        &folder,
        &[
            "worktree",
            "add",
            "-q",
            &wt2.to_string_lossy(),
            "-b",
            "worktree-bridge-cse_T2",
        ],
    );
    wait_for(&sup, "second session kept", |s| one(s).kept_sessions == 1);
    fs::write(folder.join("end-id"), "cse_T2").unwrap();
    fs::write(folder.join("end-server"), "").unwrap();
    wait_for(&sup, "second session gone", |s| one(s).kept_sessions == 0);
    // Its worktree is still there, but it is not added again.
    std::thread::sleep(Duration::from_millis(2000));
    assert_eq!(one(&sup.snapshot()).kept_sessions, 0);

    sup.shutdown();
}
