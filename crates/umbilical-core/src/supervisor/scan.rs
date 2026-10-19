//! Read the config again and find the target directories.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{Entry, STOP_GRACE, Worker, mtime, reset_for_start};
use crate::child::{Output, stop_all};
use crate::config::Config;
use crate::discovery::discover;

impl Worker {
    pub(super) fn reload_config(&mut self) {
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
                self.config_held = false;
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

    pub(super) fn rescan(&mut self) {
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
}
