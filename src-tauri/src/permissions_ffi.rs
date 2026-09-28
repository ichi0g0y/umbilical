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
