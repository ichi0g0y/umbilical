//! Calls into macOS frameworks for the permission checks. macOS only.

use std::ffi::c_void;

use block2::RcBlock;
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};
use objc2::msg_send;
use objc2::runtime::{AnyClass, Bool};
use objc2_foundation::NSString;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOHIDCheckAccess(request: u32) -> u32;
    fn IOHIDRequestAccess(request: u32) -> bool;
}

#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {
    static AVMediaTypeVideo: &'static NSString;
    static AVMediaTypeAudio: &'static NSString;
}

const HID_LISTEN_EVENT: u32 = 1;

pub fn accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Shows the macOS dialog and adds Umbilical to the Accessibility list.
pub fn accessibility_prompt() {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as *const c_void);
    }
}

/// macOS may keep the old answer until Umbilical restarts.
pub fn screen_capture_allowed() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

pub fn screen_capture_request() {
    unsafe {
        CGRequestScreenCaptureAccess();
    }
}

pub fn input_monitoring() -> &'static str {
    match unsafe { IOHIDCheckAccess(HID_LISTEN_EVENT) } {
        0 => "granted",
        1 => "denied",
        _ => "not_asked",
    }
}

pub fn input_monitoring_request() {
    unsafe {
        IOHIDRequestAccess(HID_LISTEN_EVENT);
    }
}

/// There is no API for this. If the privacy database can be read, access is on.
pub fn full_disk_access() -> bool {
    let db =
        umbilical_core::config::home().join("Library/Application Support/com.apple.TCC/TCC.db");
    std::fs::File::open(db).is_ok()
}

#[derive(Clone, Copy)]
pub enum Media {
    Video,
    Audio,
}

fn media_type(m: Media) -> &'static NSString {
    unsafe {
        match m {
            Media::Video => AVMediaTypeVideo,
            Media::Audio => AVMediaTypeAudio,
        }
    }
}

fn capture_device() -> Option<&'static AnyClass> {
    AnyClass::get(c"AVCaptureDevice")
}

pub fn av_status(m: Media) -> &'static str {
    let Some(cls) = capture_device() else {
        return "unknown";
    };
    let status: isize = unsafe { msg_send![cls, authorizationStatusForMediaType: media_type(m)] };
    match status {
        0 => "not_asked",
        3 => "granted",
        _ => "denied",
    }
}

pub fn av_request(m: Media) {
    let Some(cls) = capture_device() else {
        return;
    };
    let done = RcBlock::new(|_granted: Bool| {});
    unsafe {
        let _: () = msg_send![
            cls,
            requestAccessForMediaType: media_type(m),
            completionHandler: &*done
        ];
    }
}

#[repr(C)]
struct AEDesc {
    descriptor_type: u32,
    data_handle: *mut c_void,
}

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn AECreateDesc(type_code: u32, data: *const c_void, size: isize, result: *mut AEDesc) -> i16;
    fn AEDisposeDesc(desc: *mut AEDesc) -> i16;
    fn AEDeterminePermissionToAutomateTarget(
        target: *const AEDesc,
        event_class: u32,
        event_id: u32,
        ask_user_if_needed: u8,
    ) -> i32;
}

const TYPE_APPLICATION_BUNDLE_ID: u32 = u32::from_be_bytes(*b"bund");
const TYPE_WILD_CARD: u32 = u32::from_be_bytes(*b"****");

/// May Umbilical send Apple Events to the app with this bundle ID?
/// Never asks the user. The app must be running for a real answer.
pub fn automation(bundle_id: &str) -> &'static str {
    let mut desc = AEDesc {
        descriptor_type: 0,
        data_handle: std::ptr::null_mut(),
    };
    let bytes = bundle_id.as_bytes();
    unsafe {
        if AECreateDesc(
            TYPE_APPLICATION_BUNDLE_ID,
            bytes.as_ptr().cast(),
            bytes.len() as isize,
            &mut desc,
        ) != 0
        {
            return "unknown";
        }
        let status =
            AEDeterminePermissionToAutomateTarget(&desc, TYPE_WILD_CARD, TYPE_WILD_CARD, 0);
        AEDisposeDesc(&mut desc);
        match status {
            0 => "granted",
            -1743 => "denied",
            -1744 => "not_asked",
            // -600: the app does not run.
            _ => "unknown",
        }
    }
}

/// Send a harmless AppleScript command to each app. macOS then asks the user,
/// and Umbilical shows up in the Automation list (it has no + button).
/// The command must really send an event: `get name` of an app is answered by
/// AppleScript itself, and macOS asks nothing.
pub fn automation_request(scripts: &[&str]) {
    let scripts: Vec<String> = scripts.iter().map(|s| s.to_string()).collect();
    std::thread::spawn(move || {
        for script in scripts {
            let _ = std::process::Command::new("osascript")
                .args(["-e", &script])
                .output();
        }
    });
}
