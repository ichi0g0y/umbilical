//! Umbilical GUI: tray icon, status window and settings.
//!
//! The sessions live in a separate daemon process (`umbilical --daemon`).
//! Quitting the GUI does not stop them.

mod activity;
mod commands;
mod daemon_ctl;
mod login;
mod permissions;
#[cfg(target_os = "macos")]
mod permissions_ffi;
mod presence;
mod tray;
mod update;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use umbilical_core::daemon::Request;
use umbilical_core::supervisor::app_log;
use umbilical_core::{Config, Snapshot};

pub struct AppState {
    pub daemon: daemon_ctl::DaemonCtl,
    pub config_path: PathBuf,
    pub update: Mutex<update::UpdateState>,
    pub login: Mutex<login::LoginState>,
    pub activity: activity::Activity,
}

impl AppState {
    pub fn config(&self) -> Config {
        Config::load(&self.config_path).unwrap_or_default()
    }

    /// Status from the daemon. If it cannot be reached, an empty status with the error.
    pub fn snapshot(&self) -> Snapshot {
        let mut s = self
            .daemon
            .call::<Snapshot>(&Request::Status)
            .unwrap_or_else(|e| Snapshot {
                daemon_error: Some(e),
                ..Snapshot::default()
            });
        self.activity.apply(&mut s);
        s
    }
}

/// `umbilical --permission-states`: print the macOS permission states as JSON.
pub fn print_permission_states() {
    permissions::print_states();
}

pub fn is_permission_states_run() -> bool {
    std::env::args().any(|a| a == permissions::STATES_FLAG)
}

pub fn run() {
    let config_path = Config::default_path();
    // No config file means first start: the window asks for the root folder,
    // and nothing runs until then.
    let first_run = !config_path.exists();
    let config = if first_run {
        None
    } else {
        Some(Config::load(&config_path).unwrap_or_default())
    };

    let app = tauri::Builder::default()
        // Must be first: a second launch only opens the window of the first one.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            daemon: daemon_ctl::DaemonCtl::new(),
            config_path,
            update: Mutex::new(update::UpdateState::default()),
            login: Mutex::new(login::LoginState::default()),
            activity: activity::Activity::default(),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_tail,
            commands::dir_action,
            commands::restart_all,
            commands::answer,
            commands::answer_all,
            commands::get_config,
            commands::save_config,
            commands::pick_folder,
            commands::open_path,
            commands::reveal_path,
            commands::app_info,
            commands::list_subdirs,
            commands::login_start,
            commands::login_view,
            commands::login_input,
            commands::login_cancel,
            commands::complete_setup,
            commands::get_update,
            commands::check_update,
            commands::install_update,
            commands::get_permissions,
            commands::request_permission,
        ])
        .setup(move |app| {
            let show_in =
                presence::ShowIn::parse(config.as_ref().map_or("menu_bar", |c| c.show_in.as_str()));
            // Set this early, so a menu bar only app does not flash in the Dock.
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(if show_in.dock() {
                    tauri::ActivationPolicy::Regular
                } else {
                    tauri::ActivationPolicy::Accessory
                });
                let menu = presence::app_menu(app.handle())?;
                app.set_menu(menu)?;
            }

            if let Some(config) = &config {
                sync_autostart(app.handle(), config.autostart);
            }
            tray::create(app.handle())?;
            presence::apply(app.handle(), show_in);

            // Before the first daemon call: an old daemon is kept while
            // sessions work (see daemon_ctl).
            {
                let state = app.state::<AppState>();
                state.activity.refresh(|| state.config());
                state.daemon.set_hold(state.activity.total() > 0);
            }

            // Push status to the window and the tray. The first call starts
            // the daemon if it does not run yet.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                loop {
                    let state = handle.state::<AppState>();
                    // Know the working sessions before the daemon may be replaced.
                    state.activity.refresh(|| state.config());
                    state.daemon.set_hold(state.activity.total() > 0);
                    if state.daemon.replace_if_outdated() {
                        app_log(
                            &state.config().log_dir(),
                            "daemon replaced: no session works now",
                        );
                    }
                    let snapshot = state.snapshot();
                    let _ = handle.emit("status", &snapshot);
                    tray::refresh(&handle, &snapshot);
                    std::thread::sleep(Duration::from_secs(1));
                }
            });

            update::spawn_checker(app.handle().clone());

            // First run (not started by login): show the window so the user sees it works.
            let autostarted = std::env::args().any(|a| a == "--autostart");
            if !autostarted || first_run {
                show_window(app.handle());
            } else if show_in == presence::ShowIn::Dock && cfg!(not(target_os = "macos")) {
                // No tray: keep a taskbar button to open the window.
                show_window(app.handle());
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.minimize();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window only hides it. The app keeps running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                // Without a tray on Windows, a hidden window cannot be opened
                // again, so keep the taskbar button.
                if presence::current() == presence::ShowIn::Dock && cfg!(not(target_os = "macos")) {
                    let _ = window.minimize();
                } else {
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(move |app, event| match event {
        // No windows left is not a reason to quit.
        RunEvent::ExitRequested {
            api, code: None, ..
        } => api.prevent_exit(),
        // A click on the Dock icon.
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => show_window(app),
        RunEvent::Exit => {
            // The daemon and the sessions keep running.
            let state = app.state::<AppState>();
            app_log(&state.config().log_dir(), "gui exit");
        }
        _ => {}
    });
}

pub fn show_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    #[cfg(target_os = "macos")]
    let _ = app.show();
}

pub fn sync_autostart(app: &tauri::AppHandle, want: bool) {
    // In dev builds, do not register the debug binary for login.
    if cfg!(debug_assertions) {
        return;
    }
    // The LaunchAgent is written here, and the folder may not exist yet.
    #[cfg(target_os = "macos")]
    let _ = std::fs::create_dir_all(
        umbilical_core::config::home()
            .join("Library")
            .join("LaunchAgents"),
    );
    let launcher = app.autolaunch();
    let now = launcher.is_enabled().unwrap_or(false);
    let result = match (want, now) {
        (true, false) => launcher.enable(),
        (false, true) => launcher.disable(),
        _ => Ok(()),
    };
    if let Err(e) = result {
        let state = app.state::<AppState>();
        app_log(&state.config().log_dir(), &format!("autostart: {e}"));
    }
}
