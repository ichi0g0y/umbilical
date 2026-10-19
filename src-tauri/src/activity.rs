//! Working sessions per folder. Updates and daemon changes wait for them,
//! because a restart cuts the turn that is running.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use umbilical_core::activity::{busy_sessions, projects_dir};
use umbilical_core::discovery::discover;
use umbilical_core::{Config, RunState, Snapshot};

const REFRESH_EVERY: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Activity {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    at: Option<Instant>,
    /// Folder key -> working sessions.
    busy: BTreeMap<String, usize>,
    /// Folders whose remote-control runs, from the last status. A folder that
    /// is stopped has no session for a restart to cut. None: no status yet.
    running: Option<BTreeSet<String>>,
}

impl Activity {
    /// Read the transcripts again, at most every few seconds.
    pub fn refresh(&self, config: impl FnOnce() -> Config) {
        if self
            .inner
            .lock()
            .unwrap()
            .at
            .is_some_and(|t| t.elapsed() < REFRESH_EVERY)
        {
            return;
        }
        let projects = projects_dir();
        let now = SystemTime::now();
        let busy = discover(&config())
            .into_iter()
            .filter(|t| t.enabled)
            .map(|t| (t.key, busy_sessions(&projects, &t.path, now)))
            .filter(|(_, n)| *n > 0)
            .collect();
        let mut inner = self.inner.lock().unwrap();
        inner.at = Some(Instant::now());
        inner.busy = busy;
    }

    /// Working sessions in all folders.
    pub fn total(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner
            .busy
            .iter()
            .filter(|(key, _)| inner.running.as_ref().is_none_or(|r| r.contains(*key)))
            .map(|(_, n)| n)
            .sum()
    }

    pub fn apply(&self, snapshot: &mut Snapshot) {
        let mut inner = self.inner.lock().unwrap();
        // An empty status (daemon not reached) says nothing about the folders.
        if snapshot.daemon_error.is_none() {
            inner.running = Some(
                snapshot
                    .dirs
                    .iter()
                    .filter(|d| d.state == RunState::Running)
                    .map(|d| d.key.clone())
                    .collect(),
            );
        }
        for d in &mut snapshot.dirs {
            d.busy = inner.busy.get(&d.key).copied().unwrap_or(0);
        }
    }
}
