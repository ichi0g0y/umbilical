//! The supervisor keeps one `claude remote-control` per target directory alive.
//!
//! It runs on its own thread. Other threads (the GUI) send [`Command`]s and
//! read a [`Snapshot`].

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::child::{Output, Proc, now_text};
use crate::config::Config;
use crate::detect::Attention;
use crate::discovery::Target;
use crate::resume::store::SessionStore;

pub use resumer::ResumedSession;

mod auto_trust;
mod children;
mod commands;
mod resume;
mod resumer;
mod scan;
mod snapshot;
mod worker;

const TICK: Duration = Duration::from_millis(500);
const STOP_GRACE: Duration = Duration::from_secs(3);
/// Retry this often while an old registration still holds the folder.
/// The server drops it after a few minutes; a growing backoff would wait too long.
const ALREADY_SERVED_RETRY: Duration = Duration::from_secs(15);

#[derive(Debug)]
pub enum Command {
    Restart(String),
    Stop(String),
    Start(String),
    RestartAll,
    /// Read the config file again now.
    Reload,
    /// Type an answer into one session (for a prompt like "(y/n)").
    Answer {
        key: String,
        text: String,
    },
    /// Type an answer into every session that waits for this prompt.
    AnswerAll {
        attention: Attention,
        text: String,
    },
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Running,
    /// Waiting for the next restart (backoff).
    Waiting,
    /// Stopped by the user.
    Stopped,
    /// Turned off in the settings.
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirStatus {
    pub key: String,
    pub name: String,
    pub path: String,
    pub source: String,
    pub state: RunState,
    pub pid: Option<u32>,
    /// Unix seconds.
    pub started_at: Option<u64>,
    /// Unix seconds.
    pub next_restart_at: Option<u64>,
    pub restarts: u32,
    pub last_exit: Option<String>,
    pub last_error: Option<String>,
    pub attention: Option<Attention>,
    /// Waiting because an old registration still holds the folder (see
    /// [`crate::detect::Signal::AlreadyServed`]). Not an error.
    #[serde(default)]
    pub already_served: bool,
    pub session_url: Option<String>,
    pub command: Option<String>,
    pub spawn: String,
    pub effective_spawn: String,
    pub log_path: String,
    /// Sessions working right now (see `activity`). The GUI fills it in.
    #[serde(default)]
    pub busy: usize,
    /// Remote Control sessions that are kept to resume after a restart (see `resume`).
    #[serde(default)]
    pub kept_sessions: usize,
    /// Remote Control sessions started again in their worktree.
    #[serde(default)]
    pub resumed: Vec<ResumedSession>,
    /// The last session that could not be resumed.
    #[serde(default)]
    pub resume_note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub dirs: Vec<DirStatus>,
    pub running: usize,
    /// Enabled targets.
    pub total: usize,
    pub config_path: String,
    pub config_error: Option<String>,
    /// The config file had an error at start and no good one was loaded
    /// since, so nothing runs (old settings are not there to keep).
    #[serde(default)]
    pub config_held: bool,
    pub log_dir: String,
    /// The claude binary from the global settings, if found.
    pub claude_bin: Option<String>,
    /// No config file yet. Nothing runs until the user finishes the first setup.
    pub needs_setup: bool,
    /// Set by the GUI when it cannot reach the daemon.
    #[serde(default)]
    pub daemon_error: Option<String>,
    /// Set by the GUI: when the config file was last changed (unix ms).
    /// The settings screen reloads, or warns, when it changes.
    #[serde(default)]
    pub config_mtime: Option<u64>,
}

struct Shared {
    snapshot: Mutex<Snapshot>,
    outputs: Mutex<HashMap<String, Arc<Mutex<Output>>>>,
}

pub struct Supervisor {
    tx: Sender<Command>,
    shared: Arc<Shared>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl Supervisor {
    /// Start the supervisor thread.
    pub fn start(config_path: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            snapshot: Mutex::new(Snapshot::default()),
            outputs: Mutex::new(HashMap::new()),
        });
        let worker_shared = shared.clone();
        let handle = thread::Builder::new()
            .name("umbilical-supervisor".into())
            .spawn(move || Worker::new(config_path, worker_shared).run(rx))
            .expect("start supervisor thread");
        Self {
            tx,
            shared,
            handle: Mutex::new(Some(handle)),
        }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }

    pub fn snapshot(&self) -> Snapshot {
        self.shared.snapshot.lock().unwrap().clone()
    }

    /// The last `n` output lines of one directory.
    pub fn tail(&self, key: &str, n: usize) -> Vec<String> {
        let outputs = self.shared.outputs.lock().unwrap();
        outputs
            .get(key)
            .map(|o| o.lock().unwrap().tail(n))
            .unwrap_or_default()
    }

    /// Stop every child and wait for the thread. Safe to call twice.
    pub fn shutdown(&self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(h) = self.handle.lock().unwrap().take() {
            let _ = h.join();
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Entry {
    target: Target,
    proc: Option<Proc>,
    output: Arc<Mutex<Output>>,
    backoff: Duration,
    next_start: Option<Instant>,
    next_start_unix: Option<u64>,
    started_unix: Option<u64>,
    manual_stop: bool,
    restarts: u32,
    last_attention: Option<Attention>,
    last_exit: Option<String>,
    last_error: Option<String>,
    sessions: resume::Sessions,
}

struct Worker {
    config_path: PathBuf,
    config: Config,
    config_mtime: Option<SystemTime>,
    config_error: Option<String>,
    /// No good config was loaded yet: the file had an error at start.
    config_held: bool,
    needs_setup: bool,
    path_var: String,
    entries: BTreeMap<String, Entry>,
    shared: Arc<Shared>,
    last_scan: Option<Instant>,
    /// Paths already marked as trusted (so we do not read the file every scan).
    trusted: std::collections::HashSet<PathBuf>,
    /// When `trusted` was filled. It is cleared after a while to check again.
    trusted_at: Option<Instant>,
    trust_error: Option<String>,
    /// Remote Control sessions to resume, and the file they are kept in.
    store: SessionStore,
    store_path: PathBuf,
    last_session_scan: Option<Instant>,
}

fn reset_for_start(e: &mut Entry, config: &Config) {
    e.backoff = Duration::from_secs(config.backoff_min_secs);
    e.next_start = None;
    e.next_start_unix = None;
    e.started_unix = None;
}

/// Write one line to `<log_dir>/umbilical.log`.
pub fn app_log(log_dir: &Path, msg: &str) {
    let line = format!("{} {msg}", now_text());
    eprintln!("{line}");
    let _ = fs::create_dir_all(log_dir);
    let path = log_dir.join("umbilical.log");
    // Keep it from growing forever (debug logging writes a lot).
    if fs::metadata(&path).is_ok_and(|m| m.len() > 10 * 1024 * 1024) {
        let _ = fs::rename(&path, log_dir.join("umbilical.log.1"));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
}

fn mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
