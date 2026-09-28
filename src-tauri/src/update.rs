//! Auto update from GitHub Releases, with a stable and a nightly channel.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Url};
use tauri_plugin_updater::{Update, UpdaterExt};
use umbilical_core::supervisor::app_log;

use crate::AppState;

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
    /// Unix seconds.
    pub last_checked: Option<u64>,
    pub available: Option<Available>,
    pub error: Option<String>,
    #[serde(skip)]
    pub pending: Option<Update>,
}

impl UpdateState {
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
pub async fn install(app: &AppHandle) {
    let state = app.state::<AppState>();
    let log_dir = state.config().log_dir();
    let pending = {
        let mut u = state.update.lock().unwrap();
        if u.installing {
            return;
        }
        u.installing = true;
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
        app_log(&log_dir, &format!("update: install failed: {e}"));
        // The supervisor is gone, so restart to bring the children back.
    } else {
        app_log(&log_dir, "update: installed, restarting");
    }
    app.restart();
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
                if let Ok(Some(_)) = check(&app).await
                    && config.auto_install
                {
                    install(&app).await;
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
mod tests {
    use super::*;
    use semver::Version;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn stable_only_goes_up() {
        assert!(is_update("stable", &v("0.1.0"), &v("0.2.0")));
        assert!(!is_update("stable", &v("0.2.0"), &v("0.1.0")));
        assert!(!is_update("stable", &v("0.2.0"), &v("0.2.0")));
    }

    #[test]
    fn nightly_to_stable_can_go_down() {
        assert!(is_update(
            "stable",
            &v("0.2.1-nightly.20260928"),
            &v("0.2.0")
        ));
        assert!(!is_update("stable", &v("0.2.0"), &v("0.2.0")));
    }

    #[test]
    fn nightly_ordering() {
        assert!(is_update(
            "nightly",
            &v("0.2.1-nightly.20260928"),
            &v("0.2.1-nightly.20260929")
        ));
        assert!(is_update(
            "nightly",
            &v("0.2.0"),
            &v("0.2.1-nightly.20260929")
        ));
        assert!(!is_update(
            "nightly",
            &v("0.2.1-nightly.20260929"),
            &v("0.2.1-nightly.20260928")
        ));
    }
}
