//! The tray menu items.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Wry};
use umbilical_core::{RunState, Snapshot};

use super::MenuShape;

pub(super) type Built = (Menu<Wry>, MenuItem<Wry>, Vec<(String, MenuItem<Wry>)>);

pub(super) fn build_menu(
    app: &AppHandle,
    snapshot: &Snapshot,
    shape: &MenuShape,
) -> tauri::Result<Built> {
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
    // A dev build shows why updates are off, greyed out.
    let off = crate::update::off_reason();
    menu.append(&MenuItem::with_id(
        app,
        "check_update",
        off.unwrap_or("Check for Updates…"),
        off.is_none(),
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
    // Windows has its own uninstaller (Settings > Apps).
    #[cfg(not(windows))]
    {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&MenuItem::with_id(
            app,
            "uninstall",
            "Uninstall Umbilical…",
            true,
            None::<&str>,
        )?)?;
    }
    Ok((menu, summary, dirs))
}

pub(super) fn summary_text(s: &Snapshot) -> String {
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

pub(super) fn dir_text(d: &umbilical_core::DirStatus) -> String {
    let mark = match (d.state, d.attention.is_some()) {
        (_, true) => "⚠︎",
        (RunState::Running, _) => "●",
        (RunState::Waiting, _) => "◌",
        (RunState::Stopped, _) => "■",
        (RunState::Disabled, _) => "–",
    };
    let state = match d.state {
        RunState::Running => "running",
        RunState::Waiting if d.already_served => "waiting for the old registration to end",
        RunState::Waiting => "waiting",
        RunState::Stopped => "stopped",
        RunState::Disabled => "disabled",
    };
    format!("{mark} {}  ({state}) — click to restart", d.name)
}
