//! Remove Umbilical: "Uninstall Umbilical…" in the menus (macOS and Linux),
//! and `umbilical --stop-daemon` for the Windows uninstaller
//! (`windows/hooks.nsh`).
//!
//! It removes only what Umbilical made. Folders, repositories and Remote
//! Control worktrees are never touched.

use std::fs::File;
use std::io::ErrorKind;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use umbilical_core::daemon::{Client, Paths, Request};

#[cfg(not(windows))]
mod data;
#[cfg(not(windows))]
mod flow;
#[cfg(target_os = "macos")]
mod trash_mac;

#[cfg(not(windows))]
pub use flow::start;

pub const STOP_DAEMON_FLAG: &str = "--stop-daemon";

/// The error of commands that would write files during an uninstall.
pub const BUSY: &str = "Umbilical is being uninstalled";

/// How long we wait for the sessions and the daemon to stop.
const STOP_WAIT: Duration = Duration::from_secs(20);

/// Set while an uninstall runs. Then the menus do nothing, and nothing
/// writes a log line at exit (it would make the deleted log folder again).
static STARTED: AtomicBool = AtomicBool::new(false);

pub fn started() -> bool {
    STARTED.load(Ordering::SeqCst)
}

/// Windows: start the NSIS uninstaller next to the exe. Its hooks close
/// this app and stop the sessions (`windows/hooks.nsh`).
#[cfg(windows)]
pub fn start_windows_uninstaller() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let uninstaller = exe.with_file_name("uninstall.exe");
    if !uninstaller.exists() {
        return Err("Uninstall Umbilical in Settings > Apps.".into());
    }
    std::process::Command::new(uninstaller)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("start the uninstaller: {e}"))
}

/// `umbilical --stop-daemon`: stop every session and the daemon, and wait
/// until it is gone. Returns false when it still runs.
pub fn stop_daemon() -> bool {
    // A daemon that is just starting has the lock but no daemon.json yet,
    // so ask again until the lock is free.
    wait_daemon_gone(&Paths::default(), STOP_WAIT)
}

/// Send `Shutdown` to any daemon that answers, until no daemon holds
/// `daemon.lock`.
fn wait_daemon_gone(paths: &Paths, timeout: Duration) -> bool {
    let lock = paths.dir.join("daemon.lock");
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(c) = Client::connect(paths) {
            let _ = c.call::<serde_json::Value>(&Request::Shutdown);
        }
        let free = match File::open(&lock) {
            // The daemon holds the lock while it runs.
            Ok(f) => f.try_lock().is_ok(),
            // No file: no daemon ever ran here.
            Err(e) => e.kind() == ErrorKind::NotFound,
        };
        if free {
            return true;
        }
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
