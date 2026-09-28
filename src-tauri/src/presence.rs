//! Where the app shows: the menu bar (tray), the Dock (taskbar on Windows), or both.

use std::sync::atomic::{AtomicU8, Ordering};

use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, PartialEq)]
pub enum ShowIn {
    MenuBar,
    Both,
    Dock,
}

impl ShowIn {
    pub fn parse(s: &str) -> Self {
        match s {
            "menu_bar_and_dock" => Self::Both,
            "dock" => Self::Dock,
            _ => Self::MenuBar,
        }
    }

    pub fn tray(self) -> bool {
        self != Self::Dock
    }

    pub fn dock(self) -> bool {
        self != Self::MenuBar
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(ShowIn::MenuBar as u8);

pub fn current() -> ShowIn {
    match CURRENT.load(Ordering::Relaxed) {
        1 => ShowIn::Both,
        2 => ShowIn::Dock,
        _ => ShowIn::MenuBar,
    }
}

/// Show or hide the tray icon and the Dock icon. Safe to call again at any time.
pub fn apply(app: &AppHandle, mode: ShowIn) {
    let before = current();
    CURRENT.store(mode as u8, Ordering::Relaxed);
    if let Some(tray) = app.tray_by_id(crate::tray::TRAY_ID) {
        let _ = tray.set_visible(mode.tray());
    }
    #[cfg(target_os = "macos")]
    {
        let policy = if mode.dock() {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        let _ = app.set_activation_policy(policy);
        // Leaving the Dock hides the app. Keep an open window on screen.
        if before.dock()
            && !mode.dock()
            && let Some(w) = app.get_webview_window("main")
            && w.is_visible().unwrap_or(false)
        {
            crate::show_window(app);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = before;
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_skip_taskbar(!mode.dock());
        }
    }
}

/// The macOS app menu. It is seen only when the app is in the Dock.
/// The items use the tray menu ids, so the tray's menu handler runs them.
#[cfg(target_os = "macos")]
pub fn app_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{MenuBuilder, MenuItem, PredefinedMenuItem, SubmenuBuilder};

    let item =
        |id: &str, text: &str, key: Option<&str>| MenuItem::with_id(app, id, text, true, key);
    let main = SubmenuBuilder::new(app, "Umbilical")
        .item(&PredefinedMenuItem::about(app, None, None)?)
        .separator()
        .item(&item("open", "Open Umbilical…", Some("CmdOrCtrl+O"))?)
        .item(&item("restart_all", "Restart all", None)?)
        .item(&item("logs", "Open logs folder", None)?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, None)?)
        .item(&PredefinedMenuItem::hide_others(app, None)?)
        .item(&PredefinedMenuItem::show_all(app, None)?)
        .separator()
        .item(&item(
            "quit",
            "Quit (sessions keep running)",
            Some("CmdOrCtrl+Q"),
        )?)
        .item(&item("quit_all", "Stop all sessions and quit", None)?)
        .build()?;
    // Copy and paste in the window need the Edit menu.
    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let window = SubmenuBuilder::new(app, "Window")
        .minimize()
        .close_window()
        .build()?;
    MenuBuilder::new(app)
        .items(&[&main, &edit, &window])
        .build()
}
