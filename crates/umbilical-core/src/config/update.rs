//! The `[update]` table of the config file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateConfig {
    /// `stable` or `nightly`.
    pub channel: String,
    pub check_on_start: bool,
    /// 0 turns off the periodic check.
    pub check_interval_hours: u64,
    /// true: install without asking. false: only tell the user.
    pub auto_install: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            channel: "stable".into(),
            check_on_start: true,
            check_interval_hours: 6,
            auto_install: false,
        }
    }
}
