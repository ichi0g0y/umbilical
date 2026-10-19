//! One `claude remote-control --session-id <id>`: a Remote Control session started
//! again in its worktree, next to the server of its folder.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::resume::{Cx, Folder};
use super::{Entry, RunState, unix_now};
use crate::child::{Output, Proc, Spec};
use crate::resume::ensure_worktree;
use crate::resume::store::StoredSession;

/// Failed starts in a row before a session is forgotten.
const MAX_TRIES: u32 = 5;

/// A resumed session, for the GUI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumedSession {
    pub id: String,
    pub state: RunState,
    pub pid: Option<u32>,
    /// Unix seconds.
    pub started_at: Option<u64>,
    /// Unix seconds.
    pub next_restart_at: Option<u64>,
    /// Failed starts in a row.
    pub tries: u32,
    pub last_exit: Option<String>,
    pub last_error: Option<String>,
    pub session_url: Option<String>,
    pub log_path: String,
}

/// What every resume child of a folder shares: program, the args before
/// `remote-control`, env and permission mode.
pub(super) type Base = Result<(PathBuf, Vec<String>, BTreeMap<String, String>, String), String>;

/// Cut the server's command line down to the shared part.
pub(super) fn base_of(spec: Result<Spec, String>, e: &Entry) -> Base {
    let spec = spec?;
    let n = spec
        .args
        .iter()
        .position(|a| a == "remote-control")
        .unwrap_or(spec.args.len());
    let mode = e.target.settings.permission_mode.clone();
    Ok((spec.program, spec.args[..n].to_vec(), spec.env, mode))
}

pub(super) struct Resumer {
    pub(super) proc: Option<Proc>,
    pub(super) output: Arc<Mutex<Output>>,
    tries: u32,
    next_start: Option<std::time::Instant>,
    next_start_unix: Option<u64>,
    started_unix: Option<u64>,
    last_exit: Option<String>,
    last_error: Option<String>,
    /// Its session ended: never start it again.
    pub(super) ended: bool,
}

impl Resumer {
    pub(super) fn new(now: std::time::Instant) -> Self {
        Self {
            proc: None,
            output: Arc::default(),
            tries: 0,
            next_start: Some(now),
            next_start_unix: None,
            started_unix: None,
            last_exit: None,
            last_error: None,
            ended: false,
        }
    }

    /// Notice an exit. `ended`: an end line for this session waits for its check.
    pub(super) fn check_exit(
        &mut self,
        id: &str,
        ended: bool,
        f: &Folder,
        cx: &mut Cx,
        note: &mut Option<String>,
    ) {
        let Some(p) = self.proc.as_mut() else { return };
        let Some(exit) = p.try_exit() else { return };
        let uptime = p.started.elapsed();
        self.proc = None;
        self.started_unix = None;
        self.last_exit = Some(exit.clone());
        self.last_error = self.output.lock().unwrap().tail(1).pop();
        cx.logs.push(format!(
            "{}: resumed session {id} {exit} after {}s",
            f.name,
            uptime.as_secs()
        ));
        if ended {
            self.ended = true;
        } else if !self.ended && cx.store.contains(f.key, id) {
            if uptime >= Duration::from_secs(cx.config.stable_reset_secs) {
                self.tries = 0;
            }
            let why = self.last_error.clone().unwrap_or(exit);
            self.failed(id, why, f, cx, note);
        }
    }

    /// Start it when it is time, or again when its command line changed.
    pub(super) fn run(
        &mut self,
        st: &StoredSession,
        base: &Base,
        f: &Folder,
        cx: &mut Cx,
        note: &mut Option<String>,
    ) {
        let spec = base.clone().map(|(program, mut args, env, mode)| {
            args.extend(
                [
                    "remote-control",
                    "--session-id",
                    &st.id,
                    "--permission-mode",
                    &mode,
                ]
                .map(String::from),
            );
            Spec {
                program,
                args,
                cwd: PathBuf::from(&st.worktree),
                env,
            }
        });
        if let (Some(p), Ok(spec)) = (&self.proc, &spec)
            && *spec != p.spec
        {
            cx.to_stop.extend(self.proc.take());
            self.next_start = Some(cx.now);
            cx.logs.push(format!(
                "{}: settings changed, resuming {} again",
                f.name, st.id
            ));
        }
        if self.proc.is_some() || self.ended || self.next_start.is_none_or(|t| t > cx.now) {
            return;
        }
        let worktree = PathBuf::from(&st.worktree);
        let started =
            ensure_worktree(f.path, &worktree, &st.branch, cx.path_var).and_then(|plan| {
                let spec = spec?;
                let log = cx.log_base.with_extension(format!("{}.log", st.id));
                let p = Proc::spawn(spec, self.output.clone(), &log, cx.config.log_max_bytes)
                    .map_err(|e| format!("{e:#}"))?;
                Ok((plan, p))
            });
        match started {
            Ok((plan, p)) => {
                cx.logs.push(format!(
                    "{}: resuming session {} ({}, pid {:?})",
                    f.name,
                    st.id,
                    plan.text(),
                    p.pid()
                ));
                self.proc = Some(p);
                self.started_unix = Some(unix_now());
                self.next_start = None;
                self.next_start_unix = None;
            }
            Err(e) => {
                self.last_error = Some(e.clone());
                self.failed(&st.id, e, f, cx, note);
            }
        }
    }

    /// Count a failed try. Plan the next one, or give up and forget the session.
    fn failed(
        &mut self,
        id: &str,
        why: String,
        f: &Folder,
        cx: &mut Cx,
        note: &mut Option<String>,
    ) {
        self.tries += 1;
        if self.tries >= MAX_TRIES {
            self.ended = true;
            if cx.store.remove(f.key, id) {
                cx.changed = true;
            }
            let msg = format!("session {id} could not be resumed and was forgotten: {why}");
            cx.logs.push(format!("{}: {msg}", f.name));
            *note = Some(msg);
            return;
        }
        let min = cx.config.backoff_min_secs.max(1);
        let wait = Duration::from_secs((min << (self.tries - 1)).min(cx.config.backoff_max_secs));
        self.next_start = Some(cx.now + wait);
        self.next_start_unix = Some(unix_now() + wait.as_secs());
        cx.logs.push(format!(
            "{}: resume of {id} failed (try {}), again in {}s: {why}",
            f.name,
            self.tries,
            wait.as_secs()
        ));
    }

    pub(super) fn status(&self, id: &str, log_base: &std::path::Path) -> ResumedSession {
        let state = if self.proc.is_some() {
            RunState::Running
        } else if self.ended || self.next_start.is_none() {
            RunState::Stopped
        } else {
            RunState::Waiting
        };
        ResumedSession {
            id: id.to_string(),
            state,
            pid: self.proc.as_ref().and_then(|p| p.pid()),
            started_at: self.started_unix,
            next_restart_at: self.next_start_unix.filter(|_| state == RunState::Waiting),
            tries: self.tries,
            last_exit: self.last_exit.clone(),
            last_error: self.last_error.clone(),
            session_url: self.output.lock().unwrap().session_url.clone(),
            log_path: log_base
                .with_extension(format!("{id}.log"))
                .to_string_lossy()
                .into_owned(),
        }
    }
}
