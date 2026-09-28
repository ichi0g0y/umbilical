//! The supervisor keeps one `claude remote-control` per target directory alive.
//!
//! It runs on its own thread. Other threads (the GUI) send [`Command`]s and
//! read a [`Snapshot`].

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::child::{Output, Proc, Spec, now_text, stop_all};
use crate::config::Config;
use crate::detect::Attention;
use crate::discovery::{Target, discover};
use crate::env::{child_path, find_claude, launcher};

const TICK: Duration = Duration::from_millis(500);
const STOP_GRACE: Duration = Duration::from_secs(3);

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
    pub session_url: Option<String>,
    pub command: Option<String>,
    pub spawn: String,
    pub effective_spawn: String,
    pub log_path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub dirs: Vec<DirStatus>,
    pub running: usize,
    /// Enabled targets.
    pub total: usize,
    pub config_path: String,
    pub config_error: Option<String>,
    pub log_dir: String,
    /// The claude binary from the global settings, if found.
    pub claude_bin: Option<String>,
    /// No config file yet. Nothing runs until the user finishes the first setup.
    pub needs_setup: bool,
    /// Set by the GUI when it cannot reach the daemon.
    #[serde(default)]
    pub daemon_error: Option<String>,
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
}

struct Worker {
    config_path: PathBuf,
    config: Config,
    config_mtime: Option<SystemTime>,
    config_error: Option<String>,
    needs_setup: bool,
    path_var: String,
    entries: BTreeMap<String, Entry>,
    shared: Arc<Shared>,
    last_scan: Option<Instant>,
    /// Paths already marked as trusted (so we do not read the file every scan).
    trusted: std::collections::HashSet<PathBuf>,
    trust_error: Option<String>,
}

impl Worker {
    fn new(config_path: PathBuf, shared: Arc<Shared>) -> Self {
        // No config file: first start. Watch nothing until the GUI writes one.
        let needs_setup = !config_path.exists();
        let (config, config_error) = if needs_setup {
            (
                Config {
                    roots: Vec::new(),
                    ..Config::default()
                },
                None,
            )
        } else {
            match Config::load(&config_path) {
                Ok(c) => (c, None),
                Err(e) => (Config::default(), Some(format!("{e:#}"))),
            }
        };
        let worker = Self {
            needs_setup,
            config_mtime: mtime(&config_path),
            config_path,
            config,
            config_error,
            path_var: child_path(),
            entries: BTreeMap::new(),
            shared,
            last_scan: None,
            trusted: std::collections::HashSet::new(),
            trust_error: None,
        };
        worker.log(&format!(
            "supervisor started (config {})",
            worker.config_path.display()
        ));
        if let Some(e) = &worker.config_error {
            worker.log(&format!("config error: {e}"));
        }
        worker
    }

    fn run(mut self, rx: Receiver<Command>) {
        loop {
            match rx.recv_timeout(TICK) {
                Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(cmd) => self.handle(cmd),
                Err(RecvTimeoutError::Timeout) => {}
            }
            // Handle every waiting command before the periodic work.
            while let Ok(cmd) = rx.try_recv() {
                if matches!(cmd, Command::Shutdown) {
                    self.shutdown();
                    return;
                }
                self.handle(cmd);
            }
            self.tick();
        }
        self.shutdown();
    }

    fn handle(&mut self, cmd: Command) {
        self.debug(&format!("command: {cmd:?}"));
        match cmd {
            Command::Restart(key) => {
                if let Some(e) = self.entries.get_mut(&key) {
                    e.manual_stop = false;
                    let proc = e.proc.take();
                    reset_for_start(e, &self.config);
                    stop_all(proc.into_iter().collect(), STOP_GRACE);
                }
            }
            Command::Stop(key) => {
                if let Some(e) = self.entries.get_mut(&key) {
                    e.manual_stop = true;
                    e.next_start = None;
                    e.next_start_unix = None;
                    stop_all(e.proc.take().into_iter().collect(), STOP_GRACE);
                }
            }
            Command::Start(key) => {
                if let Some(e) = self.entries.get_mut(&key) {
                    e.manual_stop = false;
                    if e.proc.is_none() {
                        reset_for_start(e, &self.config);
                    }
                }
            }
            Command::RestartAll => {
                let mut procs = Vec::new();
                for e in self.entries.values_mut() {
                    e.manual_stop = false;
                    procs.extend(e.proc.take());
                    reset_for_start(e, &self.config);
                }
                stop_all(procs, STOP_GRACE);
            }
            Command::Reload => {
                self.reload_config();
                self.last_scan = None;
            }
            Command::Answer { key, text } => {
                if let Some(e) = self.entries.get(&key) {
                    answer(e, &text);
                    self.log(&format!("{}: answered {text:?}", e.target.name));
                }
            }
            Command::AnswerAll { attention, text } => {
                let names: Vec<String> = self
                    .entries
                    .values()
                    .filter(|e| e.output.lock().unwrap().attention == Some(attention))
                    .map(|e| {
                        answer(e, &text);
                        e.target.name.clone()
                    })
                    .collect();
                if !names.is_empty() {
                    self.log(&format!("answered {text:?} for {}", names.join(", ")));
                }
            }
            Command::Shutdown => {}
        }
    }

    fn tick(&mut self) {
        if mtime(&self.config_path) != self.config_mtime {
            self.reload_config();
            self.last_scan = None;
        }
        let scan_every = Duration::from_secs(self.config.scan_interval_secs.max(1));
        if self.last_scan.is_none_or(|t| t.elapsed() >= scan_every) {
            self.rescan();
            self.last_scan = Some(Instant::now());
        }
        self.check_children();
        self.publish();
    }

    fn reload_config(&mut self) {
        self.config_mtime = mtime(&self.config_path);
        if self.needs_setup && !self.config_path.exists() {
            return;
        }
        match Config::load(&self.config_path) {
            Ok(c) => {
                if c != self.config {
                    self.log("config reloaded");
                }
                self.config = c;
                self.config_error = None;
                self.needs_setup = false;
                self.debug(&format!(
                    "config: roots={:?} dirs={:?} exclude={:?} spawn={} permission={} auto_trust={} overrides={}",
                    self.config.roots,
                    self.config.dirs,
                    self.config.exclude,
                    self.config.spawn,
                    self.config.permission_mode,
                    self.config.auto_trust,
                    self.config.overrides.len()
                ));
            }
            Err(e) => {
                // Keep the old config. A typo must not stop running sessions.
                let msg = format!("{e:#}");
                if self.config_error.as_deref() != Some(&msg) {
                    self.log(&format!("config error: {msg}"));
                }
                self.config_error = Some(msg);
            }
        }
    }

    fn rescan(&mut self) {
        let targets = discover(&self.config);
        let keep: std::collections::HashSet<_> = targets.iter().map(|t| t.key.clone()).collect();

        let mut to_stop = Vec::new();
        let removed: Vec<String> = self
            .entries
            .keys()
            .filter(|k| !keep.contains(*k))
            .cloned()
            .collect();
        for key in removed {
            if let Some(mut e) = self.entries.remove(&key) {
                self.log(&format!("{}: directory removed, stopping", e.target.name));
                to_stop.extend(e.proc.take());
                self.shared.outputs.lock().unwrap().remove(&key);
            }
        }

        for target in targets {
            match self.entries.get_mut(&target.key) {
                Some(e) => {
                    e.target = target;
                }
                None => {
                    let output = Arc::new(Mutex::new(Output::default()));
                    self.shared
                        .outputs
                        .lock()
                        .unwrap()
                        .insert(target.key.clone(), output.clone());
                    self.log(&format!("{}: new target {}", target.name, target.key));
                    self.entries.insert(
                        target.key.clone(),
                        Entry {
                            target,
                            proc: None,
                            output,
                            backoff: Duration::from_secs(self.config.backoff_min_secs),
                            next_start: None,
                            next_start_unix: None,
                            started_unix: None,
                            manual_stop: false,
                            restarts: 0,
                            last_attention: None,
                            last_exit: None,
                            last_error: None,
                        },
                    );
                }
            }
        }

        self.ensure_trust();

        // Settings changed: restart with the new command line.
        // Turned off: stop.
        let keys: Vec<String> = self.entries.keys().cloned().collect();
        for key in keys {
            let spec = self.spec_for(&key);
            let e = self.entries.get_mut(&key).unwrap();
            let Some(p) = &e.proc else { continue };
            if !e.target.enabled {
                to_stop.extend(e.proc.take());
                continue;
            }
            if let Ok(spec) = spec
                && spec != p.spec
            {
                let name = e.target.name.clone();
                to_stop.extend(e.proc.take());
                reset_for_start(e, &self.config);
                self.log(&format!("{name}: settings changed, restarting"));
            }
        }
        stop_all(to_stop, STOP_GRACE);
    }

    /// Trust the roots and every enabled target before they start.
    fn ensure_trust(&mut self) {
        if !self.config.auto_trust {
            return;
        }
        let mut want: Vec<PathBuf> = self
            .config
            .roots
            .iter()
            .filter(|r| !r.trim().is_empty())
            .map(|r| crate::config::expand_home(r))
            .collect();
        want.extend(
            self.entries
                .values()
                .filter(|e| e.target.enabled)
                .map(|e| e.target.path.clone()),
        );
        want.retain(|p| !self.trusted.contains(p));
        if want.is_empty() {
            return;
        }
        let file = if self.config.claude_json.trim().is_empty() {
            crate::trust::claude_json_path()
        } else {
            crate::config::expand_home(&self.config.claude_json)
        };
        match crate::trust::ensure_trusted(&file, &want) {
            Ok(changed) => {
                for key in &changed {
                    self.log(&format!("trusted {key} in {}", file.display()));
                }
                self.trusted.extend(want);
                self.trust_error = None;
            }
            Err(e) => {
                let msg = format!("could not update trust: {e:#}");
                if self.trust_error.as_deref() != Some(&msg) {
                    self.log(&msg);
                }
                self.trust_error = Some(msg);
            }
        }
    }

    fn spec_for(&self, key: &str) -> Result<Spec, String> {
        let e = &self.entries[key];
        let s = &e.target.settings;
        let bin = find_claude(&s.claude_bin, &self.path_var)
            .ok_or_else(|| "claude binary not found (set claude_bin in settings)".to_string())?;
        let (program, mut args) = launcher(&bin);
        args.extend(s.args(&e.target.name));
        if self.config.debug {
            args.push("--debug-file".into());
            args.push(self.claude_debug_path(key).to_string_lossy().into_owned());
        }
        let mut env = s.env.clone();
        env.entry("PATH".into())
            .or_insert_with(|| self.path_var.clone());
        Ok(Spec {
            program,
            args,
            cwd: e.target.path.clone(),
            env,
        })
    }

    fn check_children(&mut self) {
        let now = Instant::now();
        let keys: Vec<String> = self.entries.keys().cloned().collect();
        for key in keys {
            let spec = self.spec_for(&key);
            let log_path = self.log_path_for(&key);
            let config = &self.config;
            let e = self.entries.get_mut(&key).unwrap();

            let attention = e.output.lock().unwrap().attention;
            if attention != e.last_attention {
                if config.debug {
                    app_log(
                        &config.log_dir(),
                        &format!(
                            "[debug] {}: attention {:?} -> {attention:?}",
                            e.target.name, e.last_attention
                        ),
                    );
                }
                e.last_attention = attention;
            }

            // Exited?
            if let Some(p) = &mut e.proc
                && let Some(exit) = p.try_exit()
            {
                let uptime = p.started.elapsed();
                e.proc = None;
                e.started_unix = None;
                let tail = e.output.lock().unwrap().tail(3);
                if config.debug {
                    let last = e.output.lock().unwrap().tail(8).join(" | ");
                    app_log(
                        &config.log_dir(),
                        &format!("[debug] {}: last output before exit: {last}", e.target.name),
                    );
                }
                e.last_exit = Some(exit.clone());
                e.last_error = tail.last().cloned();
                if uptime >= Duration::from_secs(config.stable_reset_secs) {
                    e.backoff = Duration::from_secs(config.backoff_min_secs);
                }
                let wait = e.backoff;
                e.backoff = (e.backoff * 2).min(Duration::from_secs(config.backoff_max_secs));
                e.next_start = Some(now + wait);
                e.next_start_unix = Some(unix_now() + wait.as_secs());
                let msg = format!(
                    "{}: {exit} after {}s, restart in {}s",
                    e.target.name,
                    uptime.as_secs(),
                    wait.as_secs()
                );
                self.log(&msg);
                continue;
            }

            let should_run = e.target.enabled && !e.manual_stop;
            if e.proc.is_some() || !should_run {
                continue;
            }
            if e.next_start.is_some_and(|t| t > now) {
                continue;
            }

            match spec.and_then(|spec| {
                Proc::spawn(spec, e.output.clone(), &log_path, config.log_max_bytes)
                    .map_err(|err| format!("{err:#}"))
            }) {
                Ok(p) => {
                    if e.last_exit.is_some() {
                        e.restarts += 1;
                    }
                    if config.debug {
                        let env_keys: Vec<&str> = p.spec.env.keys().map(String::as_str).collect();
                        let msg = format!(
                            "[debug] {}: started pid {:?}: {} (cwd {}, env keys {env_keys:?})",
                            e.target.name,
                            p.pid(),
                            p.spec.display(),
                            p.spec.cwd.display()
                        );
                        app_log(&config.log_dir(), &msg);
                    }
                    e.proc = Some(p);
                    e.started_unix = Some(unix_now());
                    e.next_start = None;
                    e.next_start_unix = None;
                }
                Err(err) => {
                    let wait = e.backoff;
                    e.backoff = (e.backoff * 2).min(Duration::from_secs(config.backoff_max_secs));
                    e.next_start = Some(now + wait);
                    e.next_start_unix = Some(unix_now() + wait.as_secs());
                    e.last_error = Some(err.clone());
                    let msg = format!("{}: start failed: {err}", e.target.name);
                    self.log(&msg);
                }
            }
        }
    }

    fn publish(&self) {
        let log_dir = self.config.log_dir();
        let dirs: Vec<DirStatus> = self
            .entries
            .iter()
            .map(|(key, e)| {
                let out = e.output.lock().unwrap();
                let state = if e.proc.is_some() {
                    RunState::Running
                } else if !e.target.enabled {
                    RunState::Disabled
                } else if e.manual_stop {
                    RunState::Stopped
                } else {
                    RunState::Waiting
                };
                DirStatus {
                    key: key.clone(),
                    name: e.target.name.clone(),
                    path: e.target.path.to_string_lossy().into_owned(),
                    source: e.target.source.clone(),
                    state,
                    pid: e.proc.as_ref().and_then(|p| p.pid()),
                    started_at: e.started_unix,
                    next_restart_at: e.next_start_unix.filter(|_| state == RunState::Waiting),
                    restarts: e.restarts,
                    last_exit: e.last_exit.clone(),
                    last_error: e.last_error.clone(),
                    attention: out.attention,
                    session_url: out.session_url.clone(),
                    command: e.proc.as_ref().map(|p| p.spec.display()),
                    spawn: e.target.settings.spawn.clone(),
                    effective_spawn: e.target.settings.effective_spawn.clone(),
                    log_path: self.log_path_for(key).to_string_lossy().into_owned(),
                }
            })
            .collect();
        let snapshot = Snapshot {
            running: dirs.iter().filter(|d| d.state == RunState::Running).count(),
            total: dirs
                .iter()
                .filter(|d| d.state != RunState::Disabled)
                .count(),
            dirs,
            config_path: self.config_path.to_string_lossy().into_owned(),
            config_error: self.config_error.clone(),
            log_dir: log_dir.to_string_lossy().into_owned(),
            claude_bin: find_claude(&self.config.claude_bin, &self.path_var)
                .map(|p| p.to_string_lossy().into_owned()),
            needs_setup: self.needs_setup,
            daemon_error: None,
        };
        *self.shared.snapshot.lock().unwrap() = snapshot;
    }

    fn log_path_for(&self, key: &str) -> PathBuf {
        let name = self
            .entries
            .get(key)
            .map(|e| e.target.name.as_str())
            .unwrap_or("unknown");
        let safe: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || "-_.".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        // A sub folder, so a target named "umbilical" never mixes with the app log.
        self.config
            .log_dir()
            .join("sessions")
            .join(format!("{safe}.log"))
    }

    fn log(&self, msg: &str) {
        app_log(&self.config.log_dir(), msg);
    }

    /// Only when `debug = true` in the config.
    fn debug(&self, msg: &str) {
        if self.config.debug {
            self.log(&format!("[debug] {msg}"));
        }
    }

    fn claude_debug_path(&self, key: &str) -> PathBuf {
        self.log_path_for(key).with_extension("claude-debug.log")
    }

    fn shutdown(&mut self) {
        self.log("shutting down, stopping all children");
        let procs: Vec<Proc> = self
            .entries
            .values_mut()
            .filter_map(|e| e.proc.take())
            .collect();
        stop_all(procs, STOP_GRACE);
        self.publish();
        self.log("stopped");
    }
}

fn answer(e: &Entry, text: &str) {
    if let Some(p) = &e.proc {
        p.write_line(text);
        e.output.lock().unwrap().attention = None;
    }
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
