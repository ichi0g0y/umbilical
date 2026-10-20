//! Start at login.
//!
//! On macOS the plugin writes `~/Library/LaunchAgents/<name>.plist` with the
//! path of the exe. It only checks that the file is there, not which exe it
//! starts. So a LaunchAgent from an old local build keeps starting that old
//! build after each restart. We check the path and write the file again.

use tauri::Manager;
use tauri_plugin_autostart::ManagerExt;
use umbilical_core::supervisor::app_log;

use crate::AppState;

pub fn sync(app: &tauri::AppHandle, want: bool) {
    // Only release and nightly builds register for login. A dev build or a
    // local `task build` in `target/` must not replace the installed app.
    if cfg!(debug_assertions) || option_env!("UMBILICAL_RELEASE").is_none() {
        return;
    }
    // The LaunchAgent is written here, and the folder may not exist yet.
    #[cfg(target_os = "macos")]
    let _ = std::fs::create_dir_all(launch_agents_dir());
    let launcher = app.autolaunch();
    let now = launcher.is_enabled().unwrap_or(false);
    let result = match (want, now) {
        (true, false) => launcher.enable(),
        (true, true) if starts_other_exe(app) => {
            log(
                app,
                "autostart: the LaunchAgent starts another copy, writing it again",
            );
            // `enable` writes the whole file again.
            launcher.enable()
        }
        (false, true) => launcher.disable(),
        _ => Ok(()),
    };
    if let Err(e) = result {
        log(app, &format!("autostart: {e}"));
    }
}

fn log(app: &tauri::AppHandle, msg: &str) {
    let state = app.state::<AppState>();
    app_log(&state.config().log_dir(), msg);
}

#[cfg(target_os = "macos")]
fn launch_agents_dir() -> std::path::PathBuf {
    umbilical_core::config::home()
        .join("Library")
        .join("LaunchAgents")
}

/// Does the LaunchAgent start an exe other than this one?
#[cfg(target_os = "macos")]
fn starts_other_exe(app: &tauri::AppHandle) -> bool {
    let plist = launch_agents_dir().join(format!("{}.plist", app.package_info().name));
    let Ok(text) = std::fs::read_to_string(plist) else {
        return false;
    };
    // The plugin writes the canonical exe path as the first program argument.
    let Some(exe) = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
    else {
        return false;
    };
    // Only a copy in an .app takes the login item over. Never a quarantined
    // copy at a random read-only path, or one that runs from a disk image.
    if !crate::update::in_app_bundle(std::path::Path::new(&exe))
        || exe.contains("/AppTranslocation/")
        || exe.starts_with("/Volumes/")
    {
        return false;
    }
    !text.contains(&format!("<string>{exe}</string>"))
}

#[cfg(not(target_os = "macos"))]
fn starts_other_exe(_app: &tauri::AppHandle) -> bool {
    false
}
