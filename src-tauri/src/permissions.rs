//! macOS privacy permissions (TCC).
//!
//! The sessions are child processes of Umbilical, so macOS checks Umbilical's
//! permissions for them. The Permissions screen shows each one and asks for it.
//! Other platforms have no such permissions: the list is empty.

use serde::Serialize;

#[derive(Serialize)]
pub struct Permission {
    pub id: &'static str,
    pub name: &'static str,
    pub why: &'static str,
    /// `granted`, `denied`, `not_asked` or `unknown` (macOS gives no way to check).
    pub state: &'static str,
    /// true: "Allow" shows the macOS prompt. false: it opens System Settings.
    pub can_prompt: bool,
    /// Missing important permissions get a red dot.
    pub important: bool,
}

#[cfg(not(target_os = "macos"))]
pub fn list() -> Vec<Permission> {
    Vec::new()
}

#[cfg(not(target_os = "macos"))]
pub fn request(_id: &str) -> Result<(), String> {
    Err("permissions are only on macOS".into())
}

/// Run as `umbilical --permission-states`: print the states and exit.
pub const STATES_FLAG: &str = "--permission-states";

#[cfg(target_os = "macos")]
pub use mac::{list, print_states, request};

#[cfg(not(target_os = "macos"))]
pub fn print_states() {
    println!("{{}}");
}

#[cfg(target_os = "macos")]
mod mac {
    use std::collections::HashMap;
    use std::process::Command;

    use super::{Permission, STATES_FLAG};
    use crate::permissions_ffi as ffi;

    struct Def {
        id: &'static str,
        name: &'static str,
        why: &'static str,
        /// Anchor of the System Settings pane.
        pane: &'static str,
        important: bool,
    }

    const DEFS: &[Def] = &[
        Def {
            id: "accessibility",
            name: "Accessibility",
            why: "Control the mouse, keyboard and other apps.",
            pane: "Privacy_Accessibility",
            important: true,
        },
        Def {
            id: "screen",
            name: "Screen Recording",
            why: "Take screenshots and see the screen.",
            pane: "Privacy_ScreenCapture",
            important: true,
        },
        Def {
            id: "input",
            name: "Input Monitoring",
            why: "Read keyboard and mouse input.",
            pane: "Privacy_ListenEvent",
            important: false,
        },
        Def {
            id: "full_disk",
            name: "Full Disk Access",
            why: "Read and write files in every folder, like Mail and Safari data. macOS has no prompt for this: add Umbilical with the + button.",
            pane: "Privacy_AllFiles",
            important: false,
        },
        Def {
            id: "automation",
            name: "Automation",
            why: "Control other apps with AppleScript. macOS asks once for each app. \"Allow…\" asks for Finder and System Events. The state shown is for Finder.",
            pane: "Privacy_Automation",
            important: false,
        },
        Def {
            id: "camera",
            name: "Camera",
            why: "Use the camera.",
            pane: "Privacy_Camera",
            important: false,
        },
        Def {
            id: "microphone",
            name: "Microphone",
            why: "Use the microphone.",
            pane: "Privacy_Microphone",
            important: false,
        },
    ];

    /// States read in a new process: macOS tells a running process some changes
    /// (Screen Recording) only after a restart.
    fn fresh_states() -> HashMap<String, String> {
        let from_child = std::env::current_exe().ok().and_then(|exe| {
            let out = Command::new(exe).arg(STATES_FLAG).output().ok()?;
            serde_json::from_slice::<HashMap<String, String>>(&out.stdout).ok()
        });
        from_child.unwrap_or_else(|| {
            DEFS.iter()
                .map(|d| (d.id.to_string(), state(d.id).0.to_string()))
                .collect()
        })
    }

    /// For `umbilical --permission-states`: print the states as JSON.
    pub fn print_states() {
        let states: HashMap<&str, &str> = DEFS.iter().map(|d| (d.id, state(d.id).0)).collect();
        println!("{}", serde_json::to_string(&states).unwrap_or_default());
    }

    pub fn list() -> Vec<Permission> {
        let fresh = fresh_states();
        DEFS.iter()
            .map(|d| {
                let (own, can_prompt) = state(d.id);
                let state = fresh.get(d.id).map_or(own, |s| static_state(s));
                Permission {
                    id: d.id,
                    name: d.name,
                    why: d.why,
                    state,
                    can_prompt,
                    important: d.important,
                }
            })
            .collect()
    }

    fn static_state(s: &str) -> &'static str {
        match s {
            "granted" => "granted",
            "denied" => "denied",
            "not_asked" => "not_asked",
            _ => "unknown",
        }
    }

    fn state(id: &str) -> (&'static str, bool) {
        let yes_no = |b: bool| if b { "granted" } else { "denied" };
        match id {
            "accessibility" => (yes_no(ffi::accessibility_trusted()), true),
            "screen" => (yes_no(ffi::screen_capture_allowed()), true),
            "input" => (ffi::input_monitoring(), true),
            "full_disk" => (yes_no(ffi::full_disk_access()), false),
            "camera" => (ffi::av_status(ffi::Media::Video), true),
            "microphone" => (ffi::av_status(ffi::Media::Audio), true),
            // Finder always runs, so it gives a real answer.
            "automation" => (ffi::automation("com.apple.finder"), true),
            _ => ("unknown", false),
        }
    }

    /// Show the macOS prompt when there is one. Open System Settings when the
    /// prompt will not come (already denied, or no prompt for this permission).
    pub fn request(id: &str) -> Result<(), String> {
        let def = DEFS
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| format!("unknown permission {id}"))?;
        let can_prompt = state(id).1;
        let fresh = fresh_states();
        let state = fresh.get(id).map_or("unknown", |s| static_state(s));
        if can_prompt && state != "denied" {
            match id {
                "accessibility" => ffi::accessibility_prompt(),
                "screen" => ffi::screen_capture_request(),
                "input" => ffi::input_monitoring_request(),
                "camera" => ffi::av_request(ffi::Media::Video),
                "microphone" => ffi::av_request(ffi::Media::Audio),
                "automation" => ffi::automation_request(&["Finder", "System Events"]),
                _ => {}
            }
            return Ok(());
        }
        // These two add Umbilical to the list in System Settings, even when denied.
        match id {
            "accessibility" => ffi::accessibility_prompt(),
            "screen" => ffi::screen_capture_request(),
            _ => {}
        }
        open_pane(def.pane)
    }

    fn open_pane(anchor: &str) -> Result<(), String> {
        let url = format!("x-apple.systempreferences:com.apple.preference.security?{anchor}");
        std::process::Command::new("open")
            .arg(url)
            .status()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn list_reads_every_state() {
        let list = super::list();
        assert_eq!(list.len(), 7);
        for p in &list {
            assert!(
                ["granted", "denied", "not_asked", "unknown"].contains(&p.state),
                "{}: {}",
                p.id,
                p.state
            );
        }
    }
}
