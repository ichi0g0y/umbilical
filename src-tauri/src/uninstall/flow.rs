//! The uninstall from the menu: ask, stop, clean up, quit. macOS and Linux.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_dialog::{
    DialogExt, MessageDialogBuilder, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};
use umbilical_core::Config;
use umbilical_core::daemon::Paths;
use umbilical_core::supervisor::app_log;

use super::{STARTED, STOP_WAIT, data, wait_daemon_gone};
use crate::AppState;

const KEEP: &str = "Uninstall";
const DELETE: &str = "Uninstall and Delete Settings";
const CANCEL: &str = "Cancel";

/// Called from the menus.
pub fn start(app: &AppHandle) {
    // A second click while the dialog is open does nothing.
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    // The updater would write a new app while we move this one to the Trash.
    if app.state::<AppState>().update.lock().unwrap().installing {
        STARTED.store(false, Ordering::SeqCst);
        app.dialog()
            .message("An update is being installed. Try again when it is done.")
            .title("Uninstall Umbilical")
            .show(|_| {});
        return;
    }
    let app = app.clone();
    // The dialogs block, and the menu handler runs on the main thread.
    std::thread::spawn(move || {
        if !run(&app) {
            STARTED.store(false, Ordering::SeqCst);
        }
    });
}

/// false: the user cancelled.
fn run(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let config = state.config();
    let config_dir = state
        .config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let log_dir = config.log_dir();
    let Some(delete_data) = confirm(app, &config_dir, &log_dir) else {
        return false;
    };
    app_log(
        &log_dir,
        &format!("uninstall: started (delete settings and logs: {delete_data})"),
    );

    let mut notes = Vec::new();
    // No daemon starts again in this process (not after an update or a
    // cancelled power off either).
    state.daemon.retire();
    // The menus do nothing now.
    if let Some(tray) = app.tray_by_id(crate::tray::TRAY_ID) {
        let _ = tray.set_visible(false);
    }
    let stopped = wait_daemon_gone(&Paths::default(), STOP_WAIT);
    if !stopped {
        notes.push(
            "The daemon did not stop in time, so the settings and logs are kept. \
             Stop the \"umbilical --daemon\" process by hand."
                .into(),
        );
    }
    if let Err(e) = crate::autostart::remove(app) {
        notes.push(format!("Start at login may still be on: {e}"));
    }
    // A daemon that still runs would write its files again.
    let delete = || {
        let own_log_dir = own_log_dir(&config);
        data::delete(&config_dir, &log_dir, own_log_dir, &app_dirs(app))
    };
    if delete_data && stopped {
        notes.extend(delete());
    }
    notes.extend(remove_app());
    done(app, &notes);
    // A log line or the WebView may write files again while the last dialog
    // is open. Deleting twice is safe.
    if delete_data && stopped {
        delete();
    }
    app.exit(0);
    true
}

/// Some(true): also delete the settings and the logs. None: cancel.
fn confirm(app: &AppHandle, config_dir: &Path, log_dir: &Path) -> Option<bool> {
    let trash = if cfg!(target_os = "macos") {
        "moves Umbilical.app to the Trash, "
    } else {
        ""
    };
    let message = format!(
        "This stops every session and the daemon, turns off start at login, {trash}and quits.\n\n\
         \"{DELETE}\" also deletes the settings in {} and the logs in {}. \
         Keep them if you may install Umbilical again.\n\n\
         Your folders, repositories and Remote Control worktrees are not touched.",
        tilde(config_dir),
        tilde(log_dir),
    );
    let result = dialog(app, message)
        .title("Uninstall Umbilical?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::YesNoCancelCustom(
            KEEP.into(),
            DELETE.into(),
            CANCEL.into(),
        ))
        .blocking_show_with_result();
    match result {
        MessageDialogResult::Custom(s) if s == KEEP => Some(false),
        MessageDialogResult::Custom(s) if s == DELETE => Some(true),
        _ => None,
    }
}

/// Only the default log folder is ours alone. A folder from the config
/// can hold other files.
fn own_log_dir(config: &Config) -> bool {
    config.log_dir.trim().is_empty() && std::env::var_os("UMBILICAL_LOG_DIR").is_none()
}

/// The WebView data and caches. Their folders have the app id as the name,
/// so only Umbilical uses them.
fn app_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let id = app.config().identifier.clone();
    let paths = app.path();
    let mut dirs: Vec<PathBuf> = [
        paths.app_data_dir(),
        paths.app_local_data_dir(),
        paths.app_config_dir(),
        paths.app_cache_dir(),
    ]
    .into_iter()
    .flatten()
    .collect();
    #[cfg(target_os = "macos")]
    for parent in ["WebKit", "HTTPStorages"] {
        dirs.push(
            umbilical_core::config::home()
                .join("Library")
                .join(parent)
                .join(&id),
        );
    }
    dirs.retain(|d| d.file_name() == Some(id.as_ref()));
    dirs.sort();
    dirs.dedup();
    dirs
}

/// A note when the app is not removed.
#[cfg(target_os = "macos")]
fn remove_app() -> Option<String> {
    let Some(bundle) = super::trash_mac::app_bundle() else {
        return Some(
            "This copy does not run from an installed Umbilical.app, so no app was moved \
             to the Trash. If you have Umbilical.app, drag it to the Trash."
                .into(),
        );
    };
    super::trash_mac::trash(&bundle).err().map(|e| {
        format!(
            "Could not move {} to the Trash ({e}). Drag it to the Trash.",
            bundle.display()
        )
    })
}

#[cfg(not(target_os = "macos"))]
fn remove_app() -> Option<String> {
    Some("Remove the Umbilical package with your package manager, or delete the AppImage.".into())
}

fn done(app: &AppHandle, notes: &[String]) {
    let mut message = String::from(if notes.is_empty() {
        "Umbilical is uninstalled."
    } else {
        "Umbilical is uninstalled. Please read these notes:"
    });
    for note in notes {
        message.push_str("\n\n");
        message.push_str(note);
    }
    message.push_str("\n\nNot removed:");
    #[cfg(target_os = "macos")]
    message.push_str(&format!(
        "\n• Privacy permissions. Remove Umbilical in System Settings > Privacy & Security, \
         or run this in Terminal: tccutil reset All {}",
        app.config().identifier
    ));
    message.push_str(
        "\n• Worktrees that Remote Control made in your project folders \
         (.claude/worktrees/bridge-*). Remove them with \"git worktree remove PATH\" \
         when you do not need them.",
    );
    dialog(app, message)
        .title("Uninstall finished")
        .buttons(MessageDialogButtons::OkCustom("Quit".into()))
        .blocking_show();
}

/// A message on the main window. Without a window, a menu bar app may show
/// the dialog behind other apps.
fn dialog(app: &AppHandle, message: String) -> MessageDialogBuilder<Wry> {
    crate::show_window(app);
    let builder = app.dialog().message(message);
    match app.get_webview_window("main") {
        Some(window) => builder.parent(&window),
        None => builder,
    }
}

/// `/Users/me/x` -> `~/x`
fn tilde(path: &Path) -> String {
    let home = umbilical_core::config::home();
    match path.strip_prefix(&home) {
        Ok(rest) => PathBuf::from("~").join(rest).display().to_string(),
        Err(_) => path.display().to_string(),
    }
}
