//! Tray icon (menu bar on macOS).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};
use umbilical_core::daemon::Request;
use umbilical_core::{RunState, Snapshot};

use crate::AppState;

pub const TRAY_ID: &str = "main";

/// `config.tray_count`. Off means the menu bar shows only the icon.
static SHOW_COUNT: AtomicBool = AtomicBool::new(false);

/// Set by the config. The next [`refresh`] draws the new title (about 1s).
pub fn set_show_count(show: bool) {
    SHOW_COUNT.store(show, Ordering::Relaxed);
}

/// What the current menu was built from. We rebuild only when this changes.
#[derive(Default, PartialEq)]
struct MenuShape {
    keys: Vec<String>,
    /// (version, waiting for working sessions)
    update: Option<(String, bool)>,
}

#[derive(Default)]
struct TrayState {
    shape: MenuShape,
    summary: Option<MenuItem<Wry>>,
    dirs: Vec<(String, MenuItem<Wry>)>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    app.manage(Mutex::new(TrayState::default()));
    // macOS draws the black template icon in black or white to match the menu bar.
    // Other trays have no template icons, so they get a white ring.
    #[cfg(target_os = "macos")]
    let bytes = include_bytes!("../icons/tray.png").as_slice();
    #[cfg(not(target_os = "macos"))]
    let bytes = include_bytes!("../icons/tray-white.png").as_slice();
    let icon = Image::from_bytes(bytes)?;
    let menu = Menu::new(app)?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("Umbilical")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu(app, event.id.as_ref()))
        .build(app)?;
    Ok(())
}

fn on_menu(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    match id {
        "open" => crate::show_window(app),
        "about" => open_tab(app, "about"),
        "check_update" => {
            open_tab(app, "about");
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::update::check(&app).await;
            });
        }
        "restart_all" => state.daemon.send(Request::RestartAll),
        "logs" => {
            use tauri_plugin_opener::OpenerExt;
            let dir = state.config().log_dir();
            let _ = std::fs::create_dir_all(&dir);
            let _ = app.opener().open_path(dir.to_string_lossy(), None::<&str>);
        }
        "update" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                crate::update::install(&app, false).await;
            });
        }
        "update_now" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                crate::update::install(&app, true).await;
            });
        }
        // The sessions keep running in the daemon.
        "quit" => app.exit(0),
        "quit_all" => {
            state.daemon.shutdown();
            app.exit(0);
        }
        other => {
            if let Some(key) = other.strip_prefix("dir:") {
                state.daemon.send(Request::Restart {
                    key: key.to_string(),
                });
            }
        }
    }
}

/// Show the window on a tab of the UI.
fn open_tab(app: &AppHandle, tab: &str) {
    crate::show_window(app);
    let _ = app.emit("navigate", tab);
}

/// Update the title, tooltip and menu labels. Called about once a second.
pub fn refresh(app: &AppHandle, snapshot: &Snapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let count = format!("{}/{}", snapshot.running, snapshot.total);
    let _ = tray.set_tooltip(Some(format!("Umbilical: {count} running")));
    // Only macOS draws a title next to the icon. The count is optional, but
    // "setup" and "!" always show, because the user has to do something.
    #[cfg(target_os = "macos")]
    {
        let alert =
            snapshot.dirs.iter().any(|d| d.attention.is_some()) || snapshot.config_error.is_some();
        let mut title = if SHOW_COUNT.load(Ordering::Relaxed) {
            count
        } else {
            String::new()
        };
        if snapshot.needs_setup {
            title = "setup".into();
        } else if alert {
            title = if title.is_empty() {
                "!".into()
            } else {
                format!("{title} !")
            };
        }
        let _ = tray.set_title((!title.is_empty()).then_some(title.as_str()));
    }

    let update = {
        let state = app.state::<AppState>();
        let u = state.update.lock().unwrap();
        u.available.as_ref().map(|a| (a.version.clone(), u.waiting))
    };
    let shape = MenuShape {
        keys: snapshot.dirs.iter().map(|d| d.key.clone()).collect(),
        update,
    };

    let tray_state = app.state::<Mutex<TrayState>>();
    let mut ts = tray_state.lock().unwrap();
    if ts.shape != shape || ts.summary.is_none() {
        match build_menu(app, snapshot, &shape) {
            Ok((menu, summary, dirs)) => {
                let _ = tray.set_menu(Some(menu));
                ts.summary = Some(summary);
                ts.dirs = dirs;
                ts.shape = shape;
            }
            Err(_) => return,
        }
    }
    if let Some(summary) = &ts.summary {
        let _ = summary.set_text(summary_text(snapshot));
    }
    for (key, item) in &ts.dirs {
        if let Some(d) = snapshot.dirs.iter().find(|d| &d.key == key) {
            let _ = item.set_text(dir_text(d));
        }
    }
}

type Built = (Menu<Wry>, MenuItem<Wry>, Vec<(String, MenuItem<Wry>)>);

fn build_menu(app: &AppHandle, snapshot: &Snapshot, shape: &MenuShape) -> tauri::Result<Built> {
    let menu = Menu::new(app)?;
    let summary = MenuItem::with_id(app, "summary", summary_text(snapshot), false, None::<&str>)?;
    menu.append(&summary)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    let mut dirs = Vec::new();
    for d in &snapshot.dirs {
        let item = MenuItem::with_id(
            app,
            format!("dir:{}", d.key),
            dir_text(d),
            true,
            None::<&str>,
        )?;
        menu.append(&item)?;
        dirs.push((d.key.clone(), item));
    }
    if !snapshot.dirs.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }

    menu.append(&MenuItem::with_id(
        app,
        "open",
        "Open Umbilical…",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "about",
        "About Umbilical",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "check_update",
        "Check for Updates…",
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "restart_all",
        "Restart all",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "logs",
        "Open logs folder",
        true,
        None::<&str>,
    )?)?;
    if let Some((v, waiting)) = &shape.update {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        let (id, text) = if *waiting {
            let info = format!("Update {v} waits for working sessions");
            menu.append(&MenuItem::with_id(
                app,
                "update_info",
                info,
                false,
                None::<&str>,
            )?)?;
            (
                "update_now",
                "Install now (stops running turns)".to_string(),
            )
        } else {
            ("update", format!("Install update {v}"))
        };
        menu.append(&MenuItem::with_id(app, id, text, true, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        "Quit (sessions keep running)",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit_all",
        "Stop all sessions and quit",
        true,
        None::<&str>,
    )?)?;
    Ok((menu, summary, dirs))
}

fn summary_text(s: &Snapshot) -> String {
    if let Some(e) = &s.daemon_error {
        return format!("Daemon error: {}", e.lines().next().unwrap_or_default());
    }
    if s.needs_setup {
        return "Setup needed: open Umbilical".into();
    }
    if let Some(e) = &s.config_error {
        return format!("Config error: {}", e.lines().next().unwrap_or_default());
    }
    format!("{} of {} running", s.running, s.total)
}

fn dir_text(d: &umbilical_core::DirStatus) -> String {
    let mark = match (d.state, d.attention.is_some()) {
        (_, true) => "⚠︎",
        (RunState::Running, _) => "●",
        (RunState::Waiting, _) => "◌",
        (RunState::Stopped, _) => "■",
        (RunState::Disabled, _) => "–",
    };
    let state = match d.state {
        RunState::Running => "running",
        RunState::Waiting => "waiting",
        RunState::Stopped => "stopped",
        RunState::Disabled => "disabled",
    };
    format!("{mark} {}  ({state}) — click to restart", d.name)
}
