//! End-to-end tests with a fake `claude` script (Unix only).
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

use umbilical_core::config::Config;
use umbilical_core::{Command, RunState, Snapshot, Supervisor};

/// Prints its args and a URL, then sleeps. Exits early if `$EXIT_AFTER` is set.
const FAKE: &str = r#"#!/bin/sh
echo "fake claude $*"
echo "Open https://claude.ai/code/session_$(basename "$PWD")"
if [ -n "$EXIT_AFTER" ]; then sleep "$EXIT_AFTER"; echo "bye"; exit 3; fi
while true; do sleep 1; done
"#;

fn setup(extra: impl FnOnce(&mut Config)) -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("claude");
    fs::write(&bin, FAKE).unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    let repos = tmp.path().join("repos");
    fs::create_dir_all(repos.join("one")).unwrap();
    let mut config = Config {
        roots: vec![repos.to_string_lossy().into()],
        claude_bin: bin.to_string_lossy().into(),
        log_dir: tmp.path().join("logs").to_string_lossy().into(),
        scan_interval_secs: 1,
        backoff_min_secs: 1,
        backoff_max_secs: 2,
        // Never touch the real ~/.claude.json in tests.
        claude_json: tmp.path().join(".claude.json").to_string_lossy().into(),
        ..Default::default()
    };
    extra(&mut config);
    let path = tmp.path().join("config.toml");
    config.save(&path).unwrap();
    (tmp, path)
}

fn wait_for(sup: &Supervisor, what: &str, f: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let s = sup.snapshot();
        if f(&s) {
            return s;
        }
        assert!(
            Instant::now() < deadline,
            "timeout waiting for {what}: {s:#?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn state_of(s: &Snapshot, name: &str) -> Option<RunState> {
    s.dirs.iter().find(|d| d.name == name).map(|d| d.state)
}

#[test]
fn starts_restarts_and_follows_directories() {
    let (tmp, config_path) = setup(|_| {});
    let sup = Supervisor::start(config_path.clone());

    let s = wait_for(&sup, "one running with url", |s| {
        s.dirs
            .iter()
            .any(|d| d.name == "one" && d.state == RunState::Running && d.session_url.is_some())
    });
    let one = s.dirs.iter().find(|d| d.name == "one").unwrap();
    assert_eq!(
        one.session_url.as_deref(),
        Some("https://claude.ai/code/session_one")
    );
    // No .git, so worktree falls back to same-dir.
    assert_eq!(one.effective_spawn, "same-dir");
    let trust: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(tmp.path().join(".claude.json")).unwrap())
            .unwrap();
    assert_eq!(trust["projects"][&one.key]["hasTrustDialogAccepted"], true);
    let tail = sup.tail(&one.key, 50).join("\n");
    assert!(
        tail.contains(
            "remote-control --name one --permission-mode bypassPermissions --spawn same-dir"
        ),
        "{tail}"
    );
    assert!(Path::new(&one.log_path).exists());

    // Kill the child from outside: it must come back.
    let pid = one.pid.unwrap();
    unsafe_kill(pid);
    wait_for(&sup, "one restarted", |s| {
        s.dirs
            .iter()
            .any(|d| d.name == "one" && d.state == RunState::Running && d.restarts == 1)
    });

    // New directory: new session.
    fs::create_dir_all(tmp.path().join("repos").join("two")).unwrap();
    wait_for(&sup, "two running", |s| {
        state_of(s, "two") == Some(RunState::Running)
    });

    // Manual stop and start.
    let key = sup
        .snapshot()
        .dirs
        .iter()
        .find(|d| d.name == "two")
        .unwrap()
        .key
        .clone();
    sup.send(Command::Stop(key.clone()));
    wait_for(&sup, "two stopped", |s| {
        state_of(s, "two") == Some(RunState::Stopped)
    });
    sup.send(Command::Start(key.clone()));
    wait_for(&sup, "two running again", |s| {
        state_of(s, "two") == Some(RunState::Running)
    });

    // Disable in config: stop. Removed directory: gone.
    let mut config = Config::load(&config_path).unwrap();
    config.overrides.insert(
        key.clone(),
        umbilical_core::config::DirOverride {
            enabled: Some(false),
            ..Default::default()
        },
    );
    config.save(&config_path).unwrap();
    sup.send(Command::Reload);
    wait_for(&sup, "two disabled", |s| {
        state_of(s, "two") == Some(RunState::Disabled)
    });
    assert_eq!(sup.snapshot().total, 1);

    fs::remove_dir_all(tmp.path().join("repos").join("two")).unwrap();
    wait_for(&sup, "two removed", |s| state_of(s, "two").is_none());

    let pid = sup.snapshot().dirs[0].pid.unwrap();
    sup.shutdown();
    assert!(!alive(pid), "child still alive after shutdown");
}

#[test]
fn backoff_after_crash() {
    let (_tmp, config_path) = setup(|c| {
        c.env.insert("EXIT_AFTER".into(), "0".into());
    });
    let sup = Supervisor::start(config_path);
    let s = wait_for(&sup, "waiting after crash", |s| {
        s.dirs
            .iter()
            .any(|d| d.state == RunState::Waiting && d.last_exit.is_some())
    });
    let d = &s.dirs[0];
    assert_eq!(d.last_exit.as_deref(), Some("exited with code 3"));
    assert_eq!(
        d.last_error.as_deref().map(|e| e.ends_with("bye")),
        Some(true)
    );
    assert!(d.next_restart_at.is_some());
    wait_for(&sup, "restarted a few times", |s| s.dirs[0].restarts >= 2);
    sup.shutdown();
}

#[test]
fn missing_binary_is_reported() {
    let (_tmp, config_path) = setup(|c| c.claude_bin = "/nope/claude".into());
    let sup = Supervisor::start(config_path);
    let s = wait_for(&sup, "error shown", |s| {
        s.dirs.iter().any(|d| d.last_error.is_some())
    });
    assert_eq!(s.dirs[0].state, RunState::Waiting);
    sup.shutdown();
}

fn unsafe_kill(pid: u32) {
    std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .unwrap();
}

fn alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn first_start_waits_for_config() {
    let (tmp, config_path) = setup(|_| {});
    // With no config the default log folder is used. Keep it out of the real one.
    // SAFETY: the other tests set `log_dir` in their config and do not read this.
    unsafe { std::env::set_var("UMBILICAL_LOG_DIR", tmp.path().join("default-logs")) };
    let text = fs::read_to_string(&config_path).unwrap();
    fs::remove_file(&config_path).unwrap();

    let sup = Supervisor::start(config_path.clone());
    let s = wait_for(&sup, "setup state", |s| s.needs_setup);
    std::thread::sleep(Duration::from_millis(1500));
    assert!(sup.snapshot().dirs.is_empty(), "{s:#?}");
    assert!(
        !config_path.exists(),
        "config must not be created by the supervisor"
    );

    // The GUI writes the config: sessions start.
    fs::write(&config_path, text).unwrap();
    wait_for(&sup, "one running after setup", |s| {
        !s.needs_setup && state_of(s, "one") == Some(RunState::Running)
    });
    sup.shutdown();
    drop(tmp);
}
