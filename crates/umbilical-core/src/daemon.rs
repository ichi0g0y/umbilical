//! The daemon: one background process that owns every session.
//!
//! The GUI is only a remote control for it. When the GUI quits, the daemon
//! and the sessions keep running. They talk over TCP on 127.0.0.1 with one
//! JSON line per request. A random token in `daemon.json` (mode 600) keeps
//! other users away.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::detect::Attention;
use crate::supervisor::{Command, Supervisor, app_log};

/// Written by the daemon, read by the GUI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonInfo {
    pub port: u16,
    pub token: String,
    pub pid: u32,
    pub version: String,
    /// The daemon binary and its modified time (unix seconds) when it started.
    /// The GUI restarts the daemon when its own binary is newer.
    pub exe: String,
    pub exe_mtime: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Ping,
    Status,
    Tail {
        key: String,
        lines: usize,
    },
    Restart {
        key: String,
    },
    Stop {
        key: String,
    },
    Start {
        key: String,
    },
    RestartAll,
    Reload,
    Answer {
        key: String,
        text: String,
    },
    AnswerAll {
        attention: Attention,
        text: String,
    },
    /// Stop every session and exit.
    Shutdown,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    token: String,
    #[serde(flatten)]
    request: Request,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pong {
    pub pid: u32,
    pub version: String,
}

/// Files of the daemon: `<dir>/daemon.json` and `<dir>/daemon.lock`.
#[derive(Debug, Clone)]
pub struct Paths {
    pub dir: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Self {
            dir: crate::config::home().join(".umbilical"),
        }
    }
}

impl Paths {
    pub fn info(&self) -> PathBuf {
        self.dir.join("daemon.json")
    }
    fn lock(&self) -> PathBuf {
        self.dir.join("daemon.lock")
    }
}

/// Run the daemon until a `Shutdown` request. Fails if another daemon runs.
pub fn serve(config_path: PathBuf, paths: &Paths) -> Result<()> {
    fs::create_dir_all(&paths.dir)?;
    let lock = File::create(paths.lock()).context("create daemon.lock")?;
    if lock.try_lock().is_err() {
        anyhow::bail!("another Umbilical daemon is already running");
    }

    let listener = TcpListener::bind("127.0.0.1:0").context("listen on 127.0.0.1")?;
    let port = listener.local_addr()?.port();
    let exe = std::env::current_exe().unwrap_or_default();
    let info = DaemonInfo {
        port,
        token: random_token()?,
        pid: std::process::id(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        exe_mtime: mtime_secs(&exe),
        exe: exe.to_string_lossy().into_owned(),
    };
    write_private(&paths.info(), &serde_json::to_string_pretty(&info)?)?;

    let sup = Arc::new(Supervisor::start(config_path.clone()));
    let log_dir = crate::Config::load(&config_path)
        .map(|c| c.log_dir())
        .unwrap_or_else(|_| crate::config::default_log_dir());
    app_log(
        &log_dir,
        &format!("daemon started (pid {}, port {port})", info.pid),
    );

    let stop = Arc::new(AtomicBool::new(false));
    for conn in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let Ok(conn) = conn else { continue };
        let sup = sup.clone();
        let token = info.token.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            let shutdown = handle(conn, &sup, &token);
            if shutdown {
                stop.store(true, Ordering::SeqCst);
                // Wake up `incoming()` so the loop sees the flag.
                let _ = TcpStream::connect(("127.0.0.1", port));
            }
        });
    }

    sup.shutdown();
    // Only remove the file if it is still ours.
    if read_info(paths).is_ok_and(|i| i.pid == info.pid) {
        let _ = fs::remove_file(paths.info());
    }
    app_log(&log_dir, "daemon stopped");
    drop(lock);
    Ok(())
}

/// Returns true for a `Shutdown` request.
fn handle(conn: TcpStream, sup: &Supervisor, token: &str) -> bool {
    let _ = conn.set_read_timeout(Some(Duration::from_secs(10)));
    let mut reader = BufReader::new(&conn);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
        return false;
    }
    let (reply, shutdown) = match serde_json::from_str::<Envelope>(&line) {
        Ok(env) if env.token == token => {
            let shutdown = matches!(env.request, Request::Shutdown);
            if shutdown {
                // Stop the sessions before we answer, so the caller can wait on us.
                sup.shutdown();
            }
            (answer(sup, env.request), shutdown)
        }
        Ok(_) => (err("bad token"), false),
        Err(e) => (err(&format!("bad request: {e}")), false),
    };
    let mut w = &conn;
    let _ = writeln!(w, "{reply}");
    let _ = conn.shutdown(Shutdown::Both);
    shutdown
}

fn answer(sup: &Supervisor, request: Request) -> Value {
    let data = match request {
        Request::Ping => serde_json::to_value(Pong {
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
        Request::Status => serde_json::to_value(sup.snapshot()),
        Request::Tail { key, lines } => serde_json::to_value(sup.tail(&key, lines)),
        other => {
            let cmd = match other {
                Request::Restart { key } => Command::Restart(key),
                Request::Stop { key } => Command::Stop(key),
                Request::Start { key } => Command::Start(key),
                Request::RestartAll => Command::RestartAll,
                Request::Reload => Command::Reload,
                Request::Answer { key, text } => Command::Answer { key, text },
                Request::AnswerAll { attention, text } => Command::AnswerAll { attention, text },
                Request::Shutdown => return serde_json::json!({ "ok": true, "data": null }),
                Request::Ping | Request::Status | Request::Tail { .. } => unreachable!(),
            };
            sup.send(cmd);
            Ok(Value::Null)
        }
    };
    match data {
        Ok(d) => serde_json::json!({ "ok": true, "data": d }),
        Err(e) => err(&e.to_string()),
    }
}

fn err(msg: &str) -> Value {
    serde_json::json!({ "ok": false, "error": msg })
}

/// A connection to the running daemon.
#[derive(Debug, Clone)]
pub struct Client {
    pub info: DaemonInfo,
}

impl Client {
    /// Read `daemon.json` and check that the daemon answers.
    pub fn connect(paths: &Paths) -> Result<Self> {
        let client = Self {
            info: read_info(paths)?,
        };
        client.call::<Pong>(&Request::Ping)?;
        Ok(client)
    }

    pub fn call<T: DeserializeOwned>(&self, request: &Request) -> Result<T> {
        let addr = SocketAddr::from(([127, 0, 0, 1], self.info.port));
        let conn = TcpStream::connect_timeout(&addr, Duration::from_secs(1))
            .context("connect to daemon")?;
        // Shutdown waits for every session to stop.
        let wait = if matches!(request, Request::Shutdown) {
            20
        } else {
            5
        };
        conn.set_read_timeout(Some(Duration::from_secs(wait)))?;
        let env = Envelope {
            token: self.info.token.clone(),
            request: request.clone(),
        };
        let mut w = &conn;
        writeln!(w, "{}", serde_json::to_string(&env)?)?;
        let mut line = String::new();
        BufReader::new(&conn)
            .read_line(&mut line)
            .context("read daemon reply")?;
        let reply: Value = serde_json::from_str(&line).context("parse daemon reply")?;
        if reply["ok"] != Value::Bool(true) {
            anyhow::bail!(
                "daemon: {}",
                reply["error"].as_str().unwrap_or("unknown error")
            );
        }
        Ok(serde_json::from_value(reply["data"].clone())?)
    }
}

pub fn read_info(paths: &Paths) -> Result<DaemonInfo> {
    let text = fs::read_to_string(paths.info()).context("daemon is not running")?;
    Ok(serde_json::from_str(&text)?)
}

pub fn mtime_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn random_token() -> Result<String> {
    let mut buf = [0u8; 24];
    getrandom::fill(&mut buf).map_err(|e| anyhow::anyhow!("random: {e}"))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

fn write_private(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(text.as_bytes())?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}
