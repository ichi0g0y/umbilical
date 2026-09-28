//! Daemon + client over TCP, with a fake `claude` (Unix only).
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use umbilical_core::config::Config;
use umbilical_core::daemon::{Client, Paths, Request, serve};
use umbilical_core::{RunState, Snapshot};

const FAKE: &str = "#!/bin/sh\necho \"fake $*\"\nwhile true; do sleep 1; done\n";

#[test]
fn daemon_serves_and_shuts_down() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("claude");
    fs::write(&bin, FAKE).unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    fs::create_dir_all(tmp.path().join("repos").join("one")).unwrap();
    let config = Config {
        roots: vec![tmp.path().join("repos").to_string_lossy().into()],
        claude_bin: bin.to_string_lossy().into(),
        log_dir: tmp.path().join("logs").to_string_lossy().into(),
        claude_json: tmp.path().join(".claude.json").to_string_lossy().into(),
        scan_interval_secs: 1,
        ..Default::default()
    };
    let config_path = tmp.path().join("config.toml");
    config.save(&config_path).unwrap();
    let paths = Paths {
        dir: tmp.path().join("state"),
    };

    let (p, c) = (paths.clone(), config_path.clone());
    let server = std::thread::spawn(move || serve(c, &p));

    // Wait for the daemon, then for the session.
    let deadline = Instant::now() + Duration::from_secs(15);
    let client = loop {
        if let Ok(c) = Client::connect(&paths) {
            break c;
        }
        assert!(Instant::now() < deadline, "daemon did not start");
        std::thread::sleep(Duration::from_millis(100));
    };
    let mode = fs::metadata(paths.info()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "daemon.json must be private");

    let snap = loop {
        let s: Snapshot = client.call(&Request::Status).unwrap();
        if s.dirs.first().is_some_and(|d| d.state == RunState::Running) {
            break s;
        }
        assert!(Instant::now() < deadline, "session did not start: {s:#?}");
        std::thread::sleep(Duration::from_millis(100));
    };
    let key = snap.dirs[0].key.clone();
    let pid = snap.dirs[0].pid.unwrap();

    // Tail works (poll: the machine may be busy).
    loop {
        let tail: Vec<String> = client
            .call(&Request::Tail {
                key: key.clone(),
                lines: 10,
            })
            .unwrap();
        if tail.iter().any(|l| l.contains("fake remote-control")) {
            break;
        }
        assert!(Instant::now() < deadline, "no output: {tail:?}");
        std::thread::sleep(Duration::from_millis(100));
    }

    // A second daemon refuses to start.
    assert!(serve(config_path.clone(), &paths).is_err());

    // A wrong token is refused.
    let mut bad = client.clone();
    bad.info.token = "nope".into();
    assert!(bad.call::<serde_json::Value>(&Request::Ping).is_err());

    // Shutdown stops the session and the daemon.
    client
        .call::<serde_json::Value>(&Request::Shutdown)
        .unwrap();
    server.join().unwrap().unwrap();
    assert!(!paths.info().exists());
    let alive = std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success();
    assert!(!alive, "session still alive after daemon shutdown");
}
