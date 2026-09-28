//! One `claude remote-control` process inside a pty.

use std::collections::{BTreeMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::detect::{Attention, LineSplitter, Signal, scan_line, terminal_replies};

/// How many output lines we keep in memory for each directory.
pub const TAIL_LINES: usize = 1000;

/// Lines this recent are not written again (TUI redraws).
const RECENT: usize = 12;

/// Everything needed to start the process. If this changes, we restart it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
}

impl Spec {
    pub fn display(&self) -> String {
        let mut s = self.program.to_string_lossy().into_owned();
        for a in &self.args {
            s.push(' ');
            if a.contains(' ') {
                s.push_str(&format!("{a:?}"));
            } else {
                s.push_str(a);
            }
        }
        s
    }
}

/// Output of one directory. It lives across restarts.
#[derive(Debug, Default)]
pub struct Output {
    pub lines: VecDeque<String>,
    pub attention: Option<Attention>,
    pub session_url: Option<String>,
}

impl Output {
    fn push(&mut self, line: String) {
        if self.lines.len() >= TAIL_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    /// The last lines, newest last.
    pub fn tail(&self, n: usize) -> Vec<String> {
        let skip = self.lines.len().saturating_sub(n);
        self.lines.iter().skip(skip).cloned().collect()
    }
}

pub struct Proc {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    // Keep the master open while the child runs. Dropping it hangs up the pty.
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub started: Instant,
    pub spec: Spec,
}

impl Proc {
    pub fn spawn(
        spec: Spec,
        output: Arc<Mutex<Output>>,
        log_path: &Path,
        log_max: u64,
    ) -> Result<Self> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 50,
                // Wide, so long URLs are not cut into pieces.
                cols: 400,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("open pty")?;

        let mut cmd = CommandBuilder::new(&spec.program);
        cmd.args(&spec.args);
        cmd.cwd(&spec.cwd);
        // If Umbilical itself was started from inside a Claude Code session,
        // do not pass that session's variables on. The child is its own session.
        for (k, _) in std::env::vars_os() {
            let k = k.to_string_lossy();
            if is_parent_claude_var(&k) {
                cmd.env_remove(k.as_ref());
            }
        }
        cmd.env("TERM", "xterm-256color");
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("start {}", spec.program.display()))?;
        drop(pair.slave);

        let reader = pair.master.try_clone_reader().context("pty reader")?;
        let writer = Arc::new(Mutex::new(pair.master.take_writer().context("pty writer")?));

        {
            let mut out = output.lock().unwrap();
            out.attention = None;
            out.session_url = None;
            out.push(format!("{} ===== start: {}", now_text(), spec.display()));
        }
        let mut log = LogFile::open(log_path.to_path_buf(), log_max);
        log.write_line(&format!("{} ===== start: {}", now_text(), spec.display()));

        let reply_writer = writer.clone();
        thread::Builder::new()
            .name("umbilical-pty-reader".into())
            .spawn(move || read_loop(reader, reply_writer, output, log))
            .context("start reader thread")?;

        Ok(Self {
            child,
            master: Some(pair.master),
            writer,
            started: Instant::now(),
            spec,
        })
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.process_id()
    }

    /// `Some(text)` when the process has exited.
    pub fn try_exit(&mut self) -> Option<String> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(if status.success() {
                "exited with code 0".into()
            } else {
                format!("exited with code {}", status.exit_code())
            }),
            Ok(None) => None,
            Err(e) => Some(format!("wait failed: {e}")),
        }
    }

    /// Type text into the process, then Enter.
    pub fn write_line(&self, text: &str) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(text.as_bytes());
            let _ = w.write_all(b"\r");
            let _ = w.flush();
        }
    }

    fn send_interrupt(&self) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(b"\x03");
            let _ = w.flush();
        }
    }

    fn force_kill(&mut self) {
        let _ = self.child.kill();
        // Closing the master sends a hangup to the whole pty session,
        // so session processes started by claude also stop.
        self.master.take();
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            self.force_kill();
        }
    }
}

/// Stop processes. First ask nicely with Ctrl-C (twice, the TUI may ask for a
/// second one), then kill what is still alive. All processes stop in parallel.
pub fn stop_all(mut procs: Vec<Proc>, grace: Duration) {
    if procs.is_empty() {
        return;
    }
    for p in &procs {
        p.send_interrupt();
    }
    thread::sleep(Duration::from_millis(300));
    for p in &procs {
        p.send_interrupt();
    }
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        procs.retain_mut(|p| p.try_exit().is_none());
        if procs.is_empty() {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    for p in &mut procs {
        p.force_kill();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline && procs.iter_mut().any(|p| p.try_exit().is_none()) {
        thread::sleep(Duration::from_millis(50));
    }
}

fn read_loop(
    mut reader: Box<dyn Read + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    output: Arc<Mutex<Output>>,
    mut log: LogFile,
) {
    let mut buf = [0u8; 8192];
    let mut splitter = LineSplitter::default();
    // A TUI redraws the same block of lines again and again. Skip a line
    // if it was one of the last few lines.
    let mut recent: VecDeque<String> = VecDeque::with_capacity(RECENT);
    let mut last_pending = String::new();
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let chunk = &buf[..n];
        let reply = terminal_replies(chunk);
        if !reply.is_empty()
            && let Ok(mut w) = writer.lock()
        {
            let _ = w.write_all(&reply);
            let _ = w.flush();
        }
        for line in splitter.push(chunk) {
            if recent.contains(&line) {
                continue;
            }
            if recent.len() == RECENT {
                recent.pop_front();
            }
            recent.push_back(line.clone());
            let stamped = format!("{} {line}", now_text());
            log.write_line(&stamped);
            let mut out = output.lock().unwrap();
            for signal in scan_line(&line) {
                match signal {
                    Signal::Attention(a) => out.attention = Some(a),
                    Signal::SessionUrl(u) => out.session_url = Some(u),
                }
            }
            out.push(stamped);
        }
        // A prompt that waits with no line break, like "Enable Remote Control? (y/n)".
        if let Some(pending) = splitter.pending()
            && pending != last_pending
        {
            last_pending = pending.clone();
            let signals = scan_line(&pending);
            if !signals.is_empty() {
                let stamped = format!("{} {pending}", now_text());
                log.write_line(&stamped);
                let mut out = output.lock().unwrap();
                for signal in signals {
                    match signal {
                        Signal::Attention(a) => out.attention = Some(a),
                        Signal::SessionUrl(u) => out.session_url = Some(u),
                    }
                }
                out.push(stamped);
            }
        }
    }
}

/// Append-only log file with simple size rotation (`x.log` -> `x.log.1`).
struct LogFile {
    path: PathBuf,
    max: u64,
    file: Option<File>,
    size: u64,
}

impl LogFile {
    fn open(path: PathBuf, max: u64) -> Self {
        let mut log = Self {
            path,
            max,
            file: None,
            size: 0,
        };
        log.reopen();
        log
    }

    fn reopen(&mut self) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        self.size = fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        if self.max > 0 && self.size > self.max {
            self.rotate();
            return;
        }
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .ok();
    }

    fn rotate(&mut self) {
        self.file = None;
        let mut old = self.path.clone().into_os_string();
        old.push(".1");
        let _ = fs::rename(&self.path, old);
        self.size = 0;
        self.file = File::create(&self.path).ok();
    }

    fn write_line(&mut self, line: &str) {
        if self.max > 0 && self.size > self.max {
            self.rotate();
        }
        if let Some(f) = &mut self.file
            && writeln!(f, "{line}").is_ok()
        {
            self.size += line.len() as u64 + 1;
        }
    }
}

fn is_parent_claude_var(name: &str) -> bool {
    name == "CLAUDECODE"
        || name == "CLAUDE_PID"
        || name == "AI_AGENT"
        || name.starts_with("CLAUDE_CODE_")
        || name.starts_with("CODEX_COMPANION_")
}

pub fn now_text() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_claude_vars() {
        assert!(is_parent_claude_var("CLAUDECODE"));
        assert!(is_parent_claude_var("CLAUDE_CODE_SESSION_ID"));
        assert!(!is_parent_claude_var("CLAUDE_BIN"));
        assert!(!is_parent_claude_var("PATH"));
    }
}
