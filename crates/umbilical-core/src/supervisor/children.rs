//! Start the children, and notice when they exit.

use std::path::Path;
use std::time::{Duration, Instant};

use super::{ALREADY_SERVED_RETRY, Entry, Worker, app_log, unix_now};
use crate::child::{Proc, Spec};
use crate::config::Config;
use crate::env::{find_claude, launcher};

impl Worker {
    pub(super) fn spec_for(&self, key: &str) -> Result<Spec, String> {
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

    pub(super) fn check_children(&mut self) {
        let now = Instant::now();
        let keys: Vec<String> = self.entries.keys().cloned().collect();
        for key in keys {
            let spec = self.spec_for(&key);
            let log_path = self.log_path_for(&key);
            let config = &self.config;
            let e = self.entries.get_mut(&key).unwrap();

            note_attention(e, config);

            // Exited?
            if let Some(msg) = on_exit(e, config, now) {
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

            if let Some(msg) = start(e, spec, &log_path, config, now) {
                self.log(&msg);
            }
        }
    }
}

/// Remember the prompt the child waits for (and log a change in debug mode).
fn note_attention(e: &mut Entry, config: &Config) {
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
}

/// If the child has exited, plan the next start and return a line for the log.
fn on_exit(e: &mut Entry, config: &Config, now: Instant) -> Option<String> {
    let p = e.proc.as_mut()?;
    let exit = p.try_exit()?;
    let uptime = p.started.elapsed();
    e.proc = None;
    e.started_unix = None;
    let (tail, already_served) = {
        let out = e.output.lock().unwrap();
        (out.tail(3), out.already_served)
    };
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
    let wait = if already_served {
        // Not a crash: keep the backoff as it is and try again soon.
        ALREADY_SERVED_RETRY.min(Duration::from_secs(config.backoff_max_secs))
    } else {
        let wait = e.backoff;
        e.backoff = (e.backoff * 2).min(Duration::from_secs(config.backoff_max_secs));
        wait
    };
    e.next_start = Some(now + wait);
    e.next_start_unix = Some(unix_now() + wait.as_secs());
    let why = if already_served {
        "an old registration still holds the folder, "
    } else {
        ""
    };
    Some(format!(
        "{}: {exit} after {}s, {why}restart in {}s",
        e.target.name,
        uptime.as_secs(),
        wait.as_secs()
    ))
}

/// Start the child. If it fails, plan a retry and return a line for the log.
fn start(
    e: &mut Entry,
    spec: Result<Spec, String>,
    log_path: &Path,
    config: &Config,
    now: Instant,
) -> Option<String> {
    match spec.and_then(|spec| {
        Proc::spawn(spec, e.output.clone(), log_path, config.log_max_bytes)
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
            None
        }
        Err(err) => {
            let wait = e.backoff;
            e.backoff = (e.backoff * 2).min(Duration::from_secs(config.backoff_max_secs));
            e.next_start = Some(now + wait);
            e.next_start_unix = Some(unix_now() + wait.as_secs());
            e.last_error = Some(err.clone());
            Some(format!("{}: start failed: {err}", e.target.name))
        }
    }
}
