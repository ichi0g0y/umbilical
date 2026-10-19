//! The version whose install failed. Auto-install does not try it again, or a
//! broken install would stop the sessions on every check.

use std::fs;
use std::path::PathBuf;

fn path() -> PathBuf {
    umbilical_core::config::home()
        .join(".umbilical")
        .join("update-failed")
}

pub fn remember(version: &str) {
    let _ = fs::write(path(), version);
}

pub fn failed_before(version: &str) -> bool {
    fs::read_to_string(path()).is_ok_and(|v| v.trim() == version)
}
