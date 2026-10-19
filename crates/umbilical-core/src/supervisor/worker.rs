//! Worker setup, the main loop and small helpers.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::{Command, STOP_GRACE, Shared, TICK, Worker, app_log, mtime};
use crate::child::{Proc, stop_all};
use crate::config::Config;
use crate::env::child_path;

impl Worker {
    pub(super) fn new(config_path: PathBuf, shared: Arc<Shared>) -> Self {
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
                // Run nothing until the file is fixed (see `Config::inert`).
                Err(e) => (Config::inert(), Some(format!("{e:#}"))),
            }
        };
        let worker = Self {
            config_held: config_error.is_some(),
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

    pub(super) fn run(mut self, rx: Receiver<Command>) {
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

    pub(super) fn tick(&mut self) {
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

    pub(super) fn log(&self, msg: &str) {
        app_log(&self.config.log_dir(), msg);
    }

    /// Only when `debug = true` in the config.
    pub(super) fn debug(&self, msg: &str) {
        if self.config.debug {
            self.log(&format!("[debug] {msg}"));
        }
    }

    pub(super) fn shutdown(&mut self) {
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
