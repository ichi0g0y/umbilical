//! Auto update from GitHub Releases, with a stable and a nightly channel.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Url};
use tauri_plugin_updater::{Update, UpdaterExt};
use umbilical_core::supervisor::app_log;

use crate::AppState;

mod failed;
mod guard;
#[cfg(target_os = "macos")]
pub use guard::in_app_bundle;
pub use guard::off_reason;

const REPO: &str = "https://github.com/ichi0g0y/umbilical";

#[derive(Debug, Clone, Serialize)]
pub struct Available {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Default, Serialize)]
pub struct UpdateState {
    pub channel: String,
    pub checking: bool,
    pub installing: bool,
    /// The install waits until no session is working.
    pub waiting: bool,
    /// Unix seconds.
    pub last_checked: Option<u64>,
    pub available: Option<Available>,
    pub error: Option<String>,
    /// Set when this build never checks or installs (a dev build).
    pub off: Option<String>,
    #[serde(skip)]
    pub pending: Option<Update>,
}

impl UpdateState {
    pub fn new() -> Self {
        Self {
            off: off_reason().map(String::from),
            ..Self::default()
        }
    }

    pub fn view(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

pub fn endpoint(channel: &str) -> String {
    match channel {
        "nightly" => format!("{REPO}/releases/download/nightly/latest.json"),
        _ => format!("{REPO}/releases/latest/download/latest.json"),
    }
}

/// Is `remote` an update for `current` on this channel?
pub fn is_update(channel: &str, current: &semver::Version, remote: &semver::Version) -> bool {
    if channel == "stable" && !current.pre.is_empty() {
        // Moving from nightly back to stable: accept the stable build even
        // if its number is lower.
        return remote != current;
    }
    remote > current
}

pub async fn check(app: &AppHandle) -> Result<Option<Available>, String> {
    if off_reason().is_some() {
        return Ok(None);
    }
    let state = app.state::<AppState>();
    let channel = state.config().update.channel;
    {
        let mut u = state.update.lock().unwrap();
        if u.checking || u.installing {
            return Ok(u.available.clone());
        }
        u.checking = true;
        u.channel = channel.clone();
    }
    let result = do_check(app, &channel).await;

    let mut u = state.update.lock().unwrap();
    u.checking = false;
    u.last_checked = Some(unix_now());
    match result {
        Ok(update) => {
            u.error = None;
            u.available = update.as_ref().map(|up| Available {
                version: up.version.clone(),
                notes: up.body.clone(),
                date: up.date.map(|d| d.to_string()),
            });
            u.pending = update;
            let view = u.view();
            let available = u.available.clone();
            drop(u);
            let _ = app.emit("update", view);
            Ok(available)
        }
        Err(e) => {
            u.error = Some(e.clone());
            let view = u.view();
            drop(u);
            let _ = app.emit("update", view);
            Err(e)
        }
    }
}

async fn do_check(app: &AppHandle, channel: &str) -> Result<Option<Update>, String> {
    let url = Url::parse(&endpoint(channel)).map_err(|e| e.to_string())?;
    let channel = channel.to_string();
    let updater = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(|e| e.to_string())?
        .version_comparator(move |current, remote| is_update(&channel, &current, &remote.version))
        .build()
        .map_err(|e| e.to_string())?;
    updater.check().await.map_err(|e| e.to_string())
}

/// Download, stop all children, install, restart the app.
/// Without `force`, first wait until no session is working: the restart cuts
/// the turn that is running.
pub async fn install(app: &AppHandle, force: bool) {
    if off_reason().is_some() {
        return;
    }
    if !force && !wait_until_idle(app).await {
        return;
    }
    let state = app.state::<AppState>();
    let log_dir = state.config().log_dir();
    let pending = {
        let mut u = state.update.lock().unwrap();
        if u.installing {
            return;
        }
        u.installing = true;
        u.waiting = false;
        u.pending.clone()
    };
    let Some(update) = pending else {
        state.update.lock().unwrap().installing = false;
        return;
    };
    let _ = app.emit("update", state.update.lock().unwrap().view());
    app_log(&log_dir, &format!("update: downloading {}", update.version));

    let bytes = match update.download(|_, _| {}, || {}).await {
        Ok(b) => b,
        Err(e) => {
            fail(app, format!("download failed: {e}"));
            return;
        }
    };
    // Stop the children before the installer replaces the app. On Windows the
    // installer may end this process right away.
    // The daemon runs the same binary. Stop it (and the sessions) so the
    // installer can replace the file. The new GUI starts a new daemon.
    app_log(&log_dir, "update: stopping the daemon");
    state.daemon.shutdown();
    if let Err(e) = update.install(bytes) {
        // No restart: the same install would fail again after it (a loop
        // with auto_install). Bring the sessions back and show the error.
        failed::remember(&update.version);
        state.daemon.resume();
        let _ = state.daemon.ensure();
        fail(
            app,
            format!(
                "install of {} failed: {e}. The sessions run again.",
                update.version
            ),
        );
        return;
    }
    app_log(&log_dir, "update: installed, restarting");
    app.restart();
}

/// false: another call waits already, it was cancelled, or a forced install started.
async fn wait_until_idle(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let mut first = true;
    loop {
        let busy = state.activity.total();
        let (done, announce) = {
            let mut u = state.update.lock().unwrap();
            if u.installing || (first && u.waiting) || (!first && !u.waiting) {
                (Some(false), None)
            } else if busy == 0 {
                u.waiting = false;
                (Some(true), None)
            } else if first {
                u.waiting = true;
                (None, Some(u.view()))
            } else {
                (None, None)
            }
        };
        if let Some(done) = done {
            return done;
        }
        if let Some(view) = announce {
            let msg = format!("update: waiting, {busy} session(s) working");
            app_log(&state.config().log_dir(), &msg);
            let _ = app.emit("update", view);
        }
        first = false;
        tokio_sleep(Duration::from_secs(15)).await;
    }
}

fn fail(app: &AppHandle, msg: String) {
    let state = app.state::<AppState>();
    app_log(&state.config().log_dir(), &format!("update: {msg}"));
    let mut u = state.update.lock().unwrap();
    u.installing = false;
    u.error = Some(msg);
    let view = u.view();
    drop(u);
    let _ = app.emit("update", view);
}

/// Background loop: check on start and every N hours, as the config says.
pub fn spawn_checker(app: AppHandle) {
    if let Some(reason) = off_reason() {
        let log_dir = app.state::<AppState>().config().log_dir();
        app_log(&log_dir, &format!("update: {reason}"));
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut last: Option<Instant> = None;
        let mut last_channel = String::new();
        loop {
            let config = app.state::<AppState>().config().update;
            let channel_changed = !last_channel.is_empty() && last_channel != config.channel;
            let due = match last {
                None => config.check_on_start,
                Some(t) => {
                    config.check_interval_hours > 0
                        && t.elapsed() >= Duration::from_secs(config.check_interval_hours * 3600)
                }
            };
            if due || channel_changed {
                // A version that failed to install is not tried by itself again.
                if let Ok(Some(a)) = check(&app).await
                    && config.auto_install
                    && !failed::failed_before(&a.version)
                {
                    install(&app, false).await;
                }
                last = Some(Instant::now());
            } else if last.is_none() {
                last = Some(Instant::now());
            }
            last_channel = config.channel;
            tokio_sleep(Duration::from_secs(60)).await;
        }
    });
}

async fn tokio_sleep(d: Duration) {
    // tauri's async runtime is tokio, but we avoid a direct tokio dependency.
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
