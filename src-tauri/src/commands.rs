//! Commands the web UI calls with `invoke`.

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use umbilical_core::daemon::Request;
use umbilical_core::detect::Attention;
use umbilical_core::{Config, Snapshot};

use crate::{AppState, update};

type CmdResult<T> = Result<T, String>;

#[tauri::command]
pub fn get_status(state: State<AppState>) -> Snapshot {
    state.snapshot()
}

#[tauri::command]
pub fn get_tail(state: State<AppState>, key: String, lines: Option<usize>) -> Vec<String> {
    state
        .daemon
        .call(&Request::Tail {
            key,
            lines: lines.unwrap_or(500),
        })
        .unwrap_or_default()
}

#[tauri::command]
pub fn dir_action(state: State<AppState>, key: String, action: String) -> CmdResult<()> {
    let request = match action.as_str() {
        "restart" => Request::Restart { key },
        "stop" => Request::Stop { key },
        "start" => Request::Start { key },
        other => return Err(format!("unknown action {other}")),
    };
    state.daemon.call::<serde_json::Value>(&request)?;
    Ok(())
}

#[tauri::command]
pub fn answer(state: State<AppState>, key: String, text: String) {
    state.daemon.send(Request::Answer { key, text });
}

#[tauri::command]
pub fn answer_all(state: State<AppState>, attention: Attention, text: String) {
    state.daemon.send(Request::AnswerAll { attention, text });
}

#[tauri::command]
pub fn restart_all(state: State<AppState>) {
    state.daemon.send(Request::RestartAll);
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> CmdResult<Config> {
    Config::load(&state.config_path).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn save_config(app: AppHandle, state: State<AppState>, config: Config) -> CmdResult<()> {
    config
        .save(&state.config_path)
        .map_err(|e| format!("{e:#}"))?;
    for root in &config.roots {
        let _ = std::fs::create_dir_all(umbilical_core::config::expand_home(root));
    }
    state.daemon.send(Request::Reload);
    crate::sync_autostart(&app, config.autostart);
    crate::presence::apply(&app, crate::presence::ShowIn::parse(&config.show_in));
    Ok(())
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> CmdResult<()> {
    if path.starts_with("https://") {
        return app
            .opener()
            .open_url(path, None::<&str>)
            .map_err(|e| e.to_string());
    }
    let p = umbilical_core::config::expand_home(&path);
    app.opener()
        .open_path(p.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> CmdResult<()> {
    app.opener()
        .reveal_item_in_dir(umbilical_core::config::expand_home(&path))
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct AppInfo {
    version: String,
    os: &'static str,
    config_path: String,
    claude_bin: Option<String>,
    repo: &'static str,
}

#[tauri::command]
pub fn app_info(app: AppHandle, state: State<AppState>) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        os: std::env::consts::OS,
        config_path: state.config_path.to_string_lossy().into_owned(),
        claude_bin: state.snapshot().claude_bin,
        repo: "https://github.com/ichi0g0y/umbilical",
    }
}

/// Folders directly inside `path` (what would become sessions).
#[tauri::command]
pub fn list_subdirs(path: String) -> CmdResult<Vec<String>> {
    let dir = umbilical_core::config::expand_home(&path);
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    Ok(names)
}

#[tauri::command]
pub fn get_permissions() -> Vec<crate::permissions::Permission> {
    crate::permissions::list()
}

#[tauri::command]
pub fn request_permission(id: String) -> CmdResult<()> {
    crate::permissions::request(&id)
}

/// First start: write the config with the chosen root folder.
#[tauri::command]
pub fn complete_setup(
    app: AppHandle,
    state: State<AppState>,
    root: String,
    autostart: bool,
) -> CmdResult<()> {
    let root = root.trim().to_string();
    if root.is_empty() {
        return Err("choose a folder".into());
    }
    std::fs::create_dir_all(umbilical_core::config::expand_home(&root))
        .map_err(|e| format!("{root}: {e}"))?;
    let config = Config {
        roots: vec![root],
        autostart,
        ..Config::default()
    };
    config
        .save(&state.config_path)
        .map_err(|e| format!("{e:#}"))?;
    state.daemon.send(Request::Reload);
    crate::sync_autostart(&app, autostart);
    Ok(())
}

#[tauri::command]
pub async fn login_start(app: AppHandle) -> CmdResult<()> {
    crate::login::start(&app)
}

#[tauri::command]
pub fn login_view(app: AppHandle) -> crate::login::LoginView {
    crate::login::view(&app)
}

#[tauri::command]
pub fn login_input(app: AppHandle, text: String) {
    crate::login::input(&app, &text);
}

#[tauri::command]
pub fn login_cancel(app: AppHandle) {
    crate::login::cancel(&app);
}

#[tauri::command]
pub fn get_update(state: State<AppState>) -> serde_json::Value {
    state.update.lock().unwrap().view()
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> CmdResult<Option<update::Available>> {
    update::check(&app).await
}

#[tauri::command]
pub async fn install_update(app: AppHandle, force: Option<bool>) {
    update::install(&app, force.unwrap_or(false)).await;
}
