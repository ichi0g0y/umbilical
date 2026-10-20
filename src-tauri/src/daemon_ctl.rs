//! Find, start and talk to the Umbilical daemon.

use std::path::Path;
#[cfg(not(target_os = "macos"))]
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use umbilical_core::daemon::{Client, Paths, Request, mtime_secs};

const STOPPED: &str = "the Umbilical daemon is stopped";

pub struct DaemonCtl {
    paths: Paths,
    client: Mutex<Option<Client>>,
    // Only one thread starts the daemon at a time.
    starting: Mutex<()>,
    /// Sessions are working: keep an old daemon for now (replacing it restarts them).
    hold: AtomicBool,
    /// We use an old daemon and replace it when nothing works.
    outdated: AtomicBool,
    /// Set by `shutdown`: start no daemon (the status loop would start one
    /// again at once, while the app quits or an update replaces the files).
    suspended: AtomicBool,
    /// Counts `shutdown` calls, so a late `resume_after` can see a newer one.
    shutdowns: AtomicU64,
}

impl DaemonCtl {
    pub fn new() -> Self {
        Self {
            paths: Paths::default(),
            client: Mutex::new(None),
            starting: Mutex::new(()),
            hold: AtomicBool::new(false),
            outdated: AtomicBool::new(false),
            suspended: AtomicBool::new(false),
            shutdowns: AtomicU64::new(0),
        }
    }

    pub fn call<T: DeserializeOwned>(&self, request: &Request) -> Result<T, String> {
        if self.suspended.load(Ordering::Relaxed) {
            return Err(STOPPED.into());
        }
        let cached = self.client.lock().unwrap().clone();
        if let Some(c) = cached
            && let Ok(v) = c.call(request)
        {
            return Ok(v);
        }
        // Not connected, or the daemon went away: (re)start it and try once more.
        let c = self.ensure()?;
        c.call(request).map_err(|e| format!("{e:#}"))
    }

    /// Fire and forget.
    pub fn send(&self, request: Request) {
        let _ = self.call::<serde_json::Value>(&request);
    }

    pub fn set_hold(&self, hold: bool) {
        self.hold.store(hold, Ordering::Relaxed);
    }

    /// An old daemon is kept because sessions were working. Replace it once they stop.
    /// Returns true when it was replaced.
    pub fn replace_if_outdated(&self) -> bool {
        if !self.outdated.load(Ordering::Relaxed)
            || self.hold.load(Ordering::Relaxed)
            || self.suspended.load(Ordering::Relaxed)
        {
            return false;
        }
        self.outdated.store(false, Ordering::Relaxed);
        self.client.lock().unwrap().take();
        self.ensure().is_ok()
    }

    /// Stop every session and the daemon. No daemon starts again until `resume`.
    /// Returns a number for `resume_after`.
    pub fn shutdown(&self) -> u64 {
        self.suspended.store(true, Ordering::Relaxed);
        let id = self.shutdowns.fetch_add(1, Ordering::SeqCst) + 1;
        let _guard = self.starting.lock().unwrap();
        let cached = self.client.lock().unwrap().take();
        let client = cached.or_else(|| Client::connect(&self.paths).ok());
        if let Some(c) = client {
            let _ = c.call::<serde_json::Value>(&Request::Shutdown);
        }
        id
    }

    /// Stop the daemon with every session and start a new one. macOS gives
    /// some permission changes only to a new responsible process (the daemon).
    pub fn restart(&self) -> Result<(), String> {
        {
            let _guard = self.starting.lock().unwrap();
            if self.suspended.load(Ordering::Relaxed) {
                return Err(STOPPED.into());
            }
            let cached = self.client.lock().unwrap().take();
            if let Some(c) = cached.or_else(|| Client::connect(&self.paths).ok()) {
                c.call::<serde_json::Value>(&Request::Shutdown)
                    .map_err(|e| format!("stop daemon: {e:#}"))?;
            }
            self.outdated.store(false, Ordering::Relaxed);
        }
        self.ensure().map(|_| ())
    }

    /// Start daemons again after `shutdown` (an update that did not install).
    pub fn resume(&self) {
        self.suspended.store(false, Ordering::Relaxed);
    }

    /// `resume`, but only if no other `shutdown` came after the one that
    /// returned `id` (for example a second power off, or an update).
    pub fn resume_after(&self, id: u64) -> bool {
        if self.shutdowns.load(Ordering::SeqCst) != id {
            return false;
        }
        self.resume();
        true
    }

    /// Connect to a daemon that runs our binary, or start one.
    pub fn ensure(&self) -> Result<Client, String> {
        let _guard = self.starting.lock().unwrap();
        if self.suspended.load(Ordering::Relaxed) {
            return Err(STOPPED.into());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        if let Ok(c) = Client::connect(&self.paths) {
            if is_current(&c, &exe) {
                *self.client.lock().unwrap() = Some(c.clone());
                return Ok(c);
            }
            if self.hold.load(Ordering::Relaxed) {
                // Replacing restarts the sessions. Wait until none is working.
                self.outdated.store(true, Ordering::Relaxed);
                *self.client.lock().unwrap() = Some(c.clone());
                return Ok(c);
            }
            // Old binary (after an update or a dev rebuild): replace it.
            // The sessions restart once.
            let _ = c.call::<serde_json::Value>(&Request::Shutdown);
        }
        spawn_daemon(&exe)?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Ok(c) = Client::connect(&self.paths) {
                *self.client.lock().unwrap() = Some(c.clone());
                return Ok(c);
            }
            if Instant::now() > deadline {
                return Err("the Umbilical daemon did not start (see umbilical.log)".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn is_current(c: &Client, exe: &Path) -> bool {
    Path::new(&c.info.exe) == exe && c.info.exe_mtime >= mtime_secs(exe)
}

/// Start `<exe> --daemon` so that it does not end with this process.
/// On macOS it is also responsible for itself, so the sessions keep
/// Umbilical's permissions after the GUI quits (see `spawn_mac`).
#[cfg(target_os = "macos")]
fn spawn_daemon(exe: &Path) -> Result<(), String> {
    crate::spawn_mac::spawn_disclaimed(exe, &["--daemon"])
        .map(|_| ())
        .map_err(|e| format!("start daemon: {e}"))
}

/// Start `<exe> --daemon` so that it does not end with this process.
#[cfg(not(target_os = "macos"))]
fn spawn_daemon(exe: &Path) -> Result<(), String> {
    let mut cmd = Command::new(exe);
    cmd.arg("--daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own process group: signals to the GUI's group do not reach it.
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("start daemon: {e}"))
}
