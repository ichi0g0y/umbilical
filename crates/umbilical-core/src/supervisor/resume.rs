//! Keep track of Remote Control sessions, and start them again after the server of
//! their folder stopped (see [`crate::resume`]).
//!
//! - A new `bridge-cse_<id>` worktree (or transcript) while the server runs
//!   is a new session: it goes into `sessions.json`.
//! - `Session completed|failed ... cse_<id>` while the server runs normally
//!   ends it: it leaves the file. Ends during a stop do not count.
//! - When a server runs, every stored session that is not in it gets its
//!   own `claude remote-control --session-id <id>` in its worktree.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::resumer::{Base, Resumer, base_of};
use super::{Entry, STOP_GRACE, Worker, unix_now};
use crate::activity::projects_dir;
use crate::child::{Proc, stop_all};
use crate::config::Config;
use crate::resume::store::{SessionStore, StoredSession};
use crate::resume::{branch_name, find_sessions, worktree_path};

/// An end line counts only if the daemon still runs this long after it.
/// A Mac restart stops the sessions and the daemon at about the same time.
const END_CONFIRM: Duration = Duration::from_secs(3);

/// The Remote Control sessions of one folder.
#[derive(Default)]
pub(super) struct Sessions {
    /// Ids seen on disk (also old ones from before), so they are not added again.
    known: HashSet<String>,
    /// The first look at the disk is done. What it found is old.
    scanned: bool,
    /// When the server we watch started. A new server has none of the sessions.
    server: Option<Instant>,
    /// Sessions that started in that server.
    in_server: HashSet<String>,
    /// End lines that wait for [`END_CONFIRM`].
    ends: Vec<(String, Instant)>,
    pub(super) children: BTreeMap<String, Resumer>,
    /// A session that could not be resumed, for the GUI.
    pub(super) note: Option<String>,
}

/// The folder being checked.
pub(super) struct Folder<'a> {
    pub(super) key: &'a str,
    pub(super) name: &'a str,
    pub(super) path: &'a Path,
}

/// What the checks need from the worker. The results are collected here.
pub(super) struct Cx<'a> {
    pub(super) config: &'a Config,
    pub(super) store: &'a mut SessionStore,
    pub(super) path_var: &'a str,
    pub(super) projects: Option<&'a Path>,
    /// The folder's log file. Each resume child writes next to it.
    pub(super) log_base: PathBuf,
    pub(super) now: Instant,
    pub(super) to_stop: Vec<Proc>,
    pub(super) logs: Vec<String>,
    /// The store changed and must be saved.
    pub(super) changed: bool,
}

impl Worker {
    pub(super) fn check_sessions(&mut self) {
        let now = Instant::now();
        let every = Duration::from_secs(self.config.scan_interval_secs.max(1));
        let projects = self
            .last_session_scan
            .is_none_or(|t| now.duration_since(t) >= every)
            .then(projects_dir);
        if projects.is_some() {
            self.last_session_scan = Some(now);
        }
        let keys: Vec<String> = self.entries.keys().cloned().collect();
        let mut to_stop = Vec::new();
        let mut changed = false;
        for key in keys {
            let base: Base = base_of(self.spec_for(&key), &self.entries[&key]);
            let log_base = self.log_path_for(&key);
            let mut cx = Cx {
                config: &self.config,
                store: &mut self.store,
                path_var: &self.path_var,
                projects: projects.as_deref(),
                log_base,
                now,
                to_stop: Vec::new(),
                logs: Vec::new(),
                changed: false,
            };
            folder_step(self.entries.get_mut(&key).unwrap(), base, &mut cx);
            changed |= cx.changed;
            to_stop.extend(cx.to_stop);
            for line in cx.logs {
                self.log(&line);
            }
        }
        if changed && let Err(e) = self.store.save(&self.store_path) {
            self.log(&format!("save sessions: {e:#}"));
        }
        stop_all(to_stop, STOP_GRACE);
    }
}

/// Take every resume child of the folder, to stop it. The sessions stay
/// stored, so they come back when the folder runs again.
pub(super) fn take_children(e: &mut Entry) -> Vec<Proc> {
    std::mem::take(&mut e.sessions.children)
        .into_values()
        .filter_map(|c| c.proc)
        .collect()
}

fn folder_step(e: &mut Entry, base: Base, cx: &mut Cx) {
    let s = &mut e.sessions;
    // Take the end lines out, also when resume is off, so they do not pile up.
    let mut ends = std::mem::take(&mut e.output.lock().unwrap().session_ends);
    for c in s.children.values() {
        ends.append(&mut c.output.lock().unwrap().session_ends);
    }
    s.ends.extend(ends);

    let folder = Folder {
        key: &e.target.key,
        name: &e.target.name,
        path: &e.target.path,
    };
    let settings = &e.target.settings;
    if !settings.resume_sessions || settings.effective_spawn != "worktree" {
        turn_off(s, &folder, cx);
        return;
    }
    confirm_ends(s, &folder, cx);
    if !e.target.enabled || e.manual_stop {
        cx.to_stop.extend(take_children(e));
        return;
    }

    let server = e.proc.as_ref().map(|p| p.started);
    if server != s.server {
        s.server = server;
        s.in_server.clear();
    }
    // A stored session stays known after it is forgotten, so its worktree
    // (kept, or made again by us) never adds it again.
    s.known
        .extend(cx.store.sessions(folder.key).iter().map(|st| st.id.clone()));
    if server.is_some()
        && let Some(projects) = cx.projects
    {
        let found = find_sessions(folder.path, projects);
        if s.scanned {
            add_new(s, &folder, found, cx);
        } else {
            s.known.extend(found);
            s.scanned = true;
        }
    }

    let pending: HashSet<String> = s.ends.iter().map(|(id, _)| id.clone()).collect();
    for (id, c) in s.children.iter_mut() {
        c.check_exit(id, pending.contains(id), &folder, cx, &mut s.note);
    }
    // Done with it: no process, and the session is not stored any more.
    s.children
        .retain(|id, c| c.proc.is_some() || cx.store.contains(folder.key, id));
    if server.is_none() {
        return;
    }
    let stored: Vec<StoredSession> = cx.store.sessions(folder.key).to_vec();
    for st in stored.iter().filter(|st| !s.in_server.contains(&st.id)) {
        let c = s
            .children
            .entry(st.id.clone())
            .or_insert_with(|| Resumer::new(cx.now));
        c.run(st, &base, &folder, cx, &mut s.note);
    }
}

/// Resume is off for this folder: forget its sessions. Running resume
/// children go on until their session ends, but never start again.
fn turn_off(s: &mut Sessions, folder: &Folder, cx: &mut Cx) {
    if cx.store.clear(folder.key) {
        cx.changed = true;
        cx.logs.push(format!(
            "{}: resume is off, sessions forgotten",
            folder.name
        ));
    }
    let mut children = std::mem::take(&mut s.children);
    children.retain(|_, c| c.proc.is_some());
    children.values_mut().for_each(|c| c.ended = true);
    *s = Sessions {
        children,
        ..Sessions::default()
    };
}

/// End lines that stayed long enough are real: forget those sessions.
fn confirm_ends(s: &mut Sessions, folder: &Folder, cx: &mut Cx) {
    let (done, wait): (Vec<_>, Vec<_>) = std::mem::take(&mut s.ends)
        .into_iter()
        .partition(|(_, at)| cx.now.duration_since(*at) >= END_CONFIRM);
    s.ends = wait;
    for (id, _) in done {
        s.known.insert(id.clone());
        if let Some(c) = s.children.get_mut(&id) {
            c.ended = true;
        }
        if cx.store.remove(folder.key, &id) {
            cx.changed = true;
            cx.logs.push(format!("{}: session {id} ended", folder.name));
        }
    }
}

fn add_new(s: &mut Sessions, folder: &Folder, found: BTreeSet<String>, cx: &mut Cx) {
    for id in found {
        if !s.known.insert(id.clone()) || cx.store.contains(folder.key, &id) {
            continue;
        }
        s.in_server.insert(id.clone());
        cx.store.add(
            folder.key,
            StoredSession {
                worktree: worktree_path(folder.path, &id)
                    .to_string_lossy()
                    .into_owned(),
                branch: branch_name(&id),
                since: unix_now(),
                id: id.clone(),
            },
        );
        cx.changed = true;
        cx.logs.push(format!(
            "{}: Remote Control session {id} started",
            folder.name
        ));
    }
}
