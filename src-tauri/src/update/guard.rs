//! Updates are off in dev builds. The updater replaces the folder around the
//! exe as if it was the app bundle. For `target/debug/umbilical` that folder
//! is the cargo build output, so an install deletes it.

use std::path::Path;

/// Why updates are off in this build, or None when they are on.
///
/// Only the release and nightly workflows set `UMBILICAL_RELEASE`, so a
/// local `task build` (a real .app with version 0.1.0) never replaces itself
/// with a download either.
pub fn off_reason() -> Option<&'static str> {
    if cfg!(debug_assertions) {
        return Some("Updates are off in dev builds");
    }
    if option_env!("UMBILICAL_RELEASE").is_none() {
        return Some("Updates are off in local builds");
    }
    let Ok(exe) = std::env::current_exe() else {
        return Some("Updates are off: the app path is unknown");
    };
    if cfg!(target_os = "macos") && !in_app_bundle(&exe) {
        return Some("Updates are off outside an .app bundle");
    }
    // On Linux the updater can replace only an AppImage (a .deb is updated
    // with the package manager).
    if cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_none() {
        return Some("Updates work only in the AppImage");
    }
    None
}

/// Is `exe` at `*.app/Contents/MacOS/<exe>`?
pub fn in_app_bundle(exe: &Path) -> bool {
    let macos = exe.parent();
    let contents = macos.and_then(Path::parent);
    let app = contents.and_then(Path::parent);
    macos.and_then(Path::file_name) == Some("MacOS".as_ref())
        && contents.and_then(Path::file_name) == Some("Contents".as_ref())
        && app.and_then(Path::extension) == Some("app".as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_paths() {
        assert!(in_app_bundle(Path::new(
            "/Applications/Umbilical.app/Contents/MacOS/umbilical"
        )));
        assert!(!in_app_bundle(Path::new("/repo/target/debug/umbilical")));
        assert!(!in_app_bundle(Path::new("/repo/target/release/umbilical")));
        assert!(!in_app_bundle(Path::new("/Contents/MacOS/umbilical")));
    }

    #[test]
    #[cfg(debug_assertions)]
    fn off_in_dev() {
        assert!(off_reason().is_some());
    }
}
