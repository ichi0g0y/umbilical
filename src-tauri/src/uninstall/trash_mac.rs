//! Move the app bundle to the Trash. macOS only.

use std::path::{Path, PathBuf};

use objc2::msg_send;
use objc2::rc::autoreleasepool;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2_foundation::NSString;

/// The .app that this exe runs from. None for a dev build, or for a copy
/// that macOS runs from a random read-only path (App Translocation).
/// No `canonicalize`: when the .app is a symlink, the link goes to the Trash.
pub fn app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if !crate::update::in_app_bundle(&exe) || exe.to_str()?.contains("/AppTranslocation/") {
        return None;
    }
    // <name>.app/Contents/MacOS/<exe>
    exe.ancestors().nth(3).map(Path::to_path_buf)
}

/// `[[NSFileManager defaultManager] trashItemAtURL:resultingItemURL:error:]`
pub fn trash(path: &Path) -> Result<(), String> {
    let path = path.to_str().ok_or("the path is not UTF-8")?;
    let manager = AnyClass::get(c"NSFileManager").ok_or("no NSFileManager")?;
    let url = AnyClass::get(c"NSURL").ok_or("no NSURL")?;
    let path = NSString::from_str(path);
    autoreleasepool(|_| unsafe {
        let manager: *mut AnyObject = msg_send![manager, defaultManager];
        let url: *mut AnyObject = msg_send![url, fileURLWithPath: &*path];
        let mut error: *mut AnyObject = std::ptr::null_mut();
        let ok: Bool = msg_send![
            manager,
            trashItemAtURL: url,
            resultingItemURL: std::ptr::null_mut::<*mut AnyObject>(),
            error: &mut error as *mut *mut AnyObject
        ];
        if ok.as_bool() {
            return Ok(());
        }
        if error.is_null() {
            return Err("unknown error".into());
        }
        let text: *mut NSString = msg_send![error, localizedDescription];
        Err(text
            .as_ref()
            .map_or_else(|| "unknown error".into(), |t| t.to_string()))
    })
}
