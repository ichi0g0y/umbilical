//! Stop the sessions when macOS is about to restart, shut down or log out.
//! macOS only.
//!
//! At shutdown macOS sends SIGTERM to each `claude` process, but the network
//! can be gone by then. claude cannot remove its Remote Control registration,
//! so after the restart the server says "already served" for a few minutes.
//! `NSWorkspaceWillPowerOffNotification` comes earlier, while the network
//! still works. We stop the sessions there, and they end cleanly.

use std::time::Duration;

use block2::RcBlock;
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::NSString;
use tauri::{AppHandle, Manager};
use umbilical_core::supervisor::app_log;

use crate::AppState;

/// The user can cancel the restart (or an app can stop it). If we still run
/// after this long, the power off did not happen: start the sessions again.
/// (If the GUI already quit for the logout, the sessions stay stopped until
/// the app starts again.)
const CANCEL_WAIT: Duration = Duration::from_secs(60);

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    static NSWorkspaceWillPowerOffNotification: &'static NSString;
}

/// Watch for the power off notification for the life of the app.
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    observe(move || on_power_off(&app));
}

/// Call `f` on each power off notification.
fn observe(f: impl Fn() + 'static) {
    let Some(center) = workspace_center() else {
        return;
    };
    // No queue: the block runs on the thread that posts it (the main thread).
    // Blocking it while the sessions stop makes macOS wait for them.
    let block = RcBlock::new(move |_note: *mut AnyObject| f());
    unsafe {
        // The center keeps the observer and a copy of the block.
        let _: *mut AnyObject = msg_send![
            center,
            addObserverForName: NSWorkspaceWillPowerOffNotification,
            object: std::ptr::null_mut::<AnyObject>(),
            queue: std::ptr::null_mut::<AnyObject>(),
            usingBlock: &*block
        ];
    }
}

/// `[[NSWorkspace sharedWorkspace] notificationCenter]`
fn workspace_center() -> Option<*mut AnyObject> {
    let cls = AnyClass::get(c"NSWorkspace")?;
    unsafe {
        let workspace: *mut AnyObject = msg_send![cls, sharedWorkspace];
        Some(msg_send![workspace, notificationCenter])
    }
}

fn on_power_off(app: &AppHandle) {
    let state = app.state::<AppState>();
    app_log(
        &state.config().log_dir(),
        "power off: stopping the sessions while the network works",
    );
    // Like an update: the daemon stops every session with Ctrl-C and keeps
    // the list of Remote Control sessions, so they come back after the start.
    let id = state.daemon.shutdown();

    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(CANCEL_WAIT);
        let state = app.state::<AppState>();
        if !state.daemon.resume_after(id) {
            return;
        }
        app_log(
            &state.config().log_dir(),
            "power off did not happen: starting the sessions again",
        );
        let _ = state.daemon.ensure();
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn observer_gets_the_power_off_notification() {
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        observe(move || {
            h.fetch_add(1, Ordering::SeqCst);
        });
        let center = workspace_center().unwrap();
        unsafe {
            let _: () = msg_send![
                center,
                postNotificationName: NSWorkspaceWillPowerOffNotification,
                object: std::ptr::null_mut::<AnyObject>()
            ];
        }
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }
}
