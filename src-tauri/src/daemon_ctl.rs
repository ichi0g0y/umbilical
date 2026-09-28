//! Find, start and talk to the Umbilical daemon.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use umbilical_core::daemon::{Client, Paths, Request, mtime_secs};

pub struct DaemonCtl {
    paths: Paths,
    client: Mutex<Option<Client>>,
    // Only one thread starts the daemon at a time.
    starting: Mutex<()>,
}

impl DaemonCtl {
    pub fn new() -> Self {
        Self {
            paths: Paths::default(),
            client: Mutex::new(None),
            starting: Mutex::new(()),
        }
    }

    pub fn call<T: DeserializeOwned>(&self, request: &Request) -> Result<T, String> {
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

    /// Stop every session and the daemon.
    pub fn shutdown(&self) {
        let cached = self.client.lock().unwrap().take();
        let client = cached.or_else(|| Client::connect(&self.paths).ok());
        if let Some(c) = client {
            let _ = c.call::<serde_json::Value>(&Request::Shutdown);
        }
    }

    /// Connect to a daemon that runs our binary, or start one.
    pub fn ensure(&self) -> Result<Client, String> {
        let _guard = self.starting.lock().unwrap();
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        if let Ok(c) = Client::connect(&self.paths) {
            if is_current(&c, &exe) {
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
