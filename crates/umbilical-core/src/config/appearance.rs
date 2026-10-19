//! The `[appearance]` table of the config file. Only the GUI reads it.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Color themes of the GUI. Each one has a light and a dark version.
/// `umbilical` is the app's own purple.
pub const THEMES: &[&str] = &[
    "umbilical",
    "catppuccin",
    "dracula",
    "nord",
    "solarized",
    "gruvbox",
    "tokyo-night",
    "rose-pine",
    "one",
    "github",
];

/// `system` follows the OS light / dark setting.
pub const APPEARANCE_MODES: &[&str] = &["system", "light", "dark"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    /// One of [`THEMES`].
    pub theme: String,
    /// One of [`APPEARANCE_MODES`].
    pub mode: String,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: "umbilical".into(),
            mode: "system".into(),
        }
    }
}

impl AppearanceConfig {
    pub fn validate(&self) -> Result<()> {
        super::check_one_of("appearance.theme", &self.theme, THEMES)?;
        super::check_one_of("appearance.mode", &self.mode, APPEARANCE_MODES)
    }
}
