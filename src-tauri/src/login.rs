//! Run `claude auth login` from the app.
//!
//! On macOS the app runs in the desktop session, so the login goes to the
//! Keychain that the remote-control children read. A login over SSH may not.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use umbilical_core::child::{Output, Proc, Spec, stop_all};
use umbilical_core::env::{child_path, find_claude, launcher};

use crate::AppState;

#[derive(Default)]
pub struct LoginState {
    proc: Option<Proc>,
    output: Arc<Mutex<Output>>,
    exit: Option<String>,
    success: bool,
}

#[derive(Serialize)]
pub struct LoginView {
    running: bool,
    lines: Vec<String>,
    exit: Option<String>,
    success: bool,
}

pub fn start(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let config = state.config();
    let path_var = child_path();
    let bin = find_claude(&config.claude_bin, &path_var)
        .ok_or("claude binary not found (set claude_bin in settings)")?;
    let (program, mut args) = launcher(&bin);
    args.extend(["auth".to_string(), "login".to_string()]);
    let mut env = BTreeMap::new();
    env.insert("PATH".to_string(), path_var);
    let spec = Spec {
        program,
        args,
        cwd: umbilical_core::config::home(),
        env,
    };

    let mut login = state.login.lock().unwrap();
    if login.proc.is_some() {
        return Ok(());
    }
    let output = Arc::new(Mutex::new(Output::default()));
    let log = config.log_dir().join("login.log");
    let proc = Proc::spawn(spec, output.clone(), &log, config.log_max_bytes)
        .map_err(|e| format!("{e:#}"))?;
    *login = LoginState {
        proc: Some(proc),
        output,
        exit: None,
        success: false,
    };
    drop(login);

    // Watch for the end. On success, restart every session.
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(300));
            let state = app.state::<AppState>();
            let mut login = state.login.lock().unwrap();
            let Some(p) = login.proc.as_mut() else { return };
            if let Some(exit) = p.try_exit() {
                login.success = exit == "exited with code 0";
                login.exit = Some(exit);
                login.proc = None;
                if login.success {
                    state
                        .daemon
                        .send(umbilical_core::daemon::Request::RestartAll);
                }
                return;
            }
        }
    });
    Ok(())
}

pub fn view(app: &AppHandle) -> LoginView {
    let state = app.state::<AppState>();
    let login = state.login.lock().unwrap();
    LoginView {
        running: login.proc.is_some(),
        lines: login.output.lock().unwrap().tail(200),
        exit: login.exit.clone(),
        success: login.success,
    }
}

pub fn input(app: &AppHandle, text: &str) {
    let state = app.state::<AppState>();
    if let Some(p) = &state.login.lock().unwrap().proc {
        p.write_line(text);
    }
}

pub fn cancel(app: &AppHandle) {
    let state = app.state::<AppState>();
    let proc = state.login.lock().unwrap().proc.take();
    stop_all(proc.into_iter().collect(), Duration::from_secs(2));
}
