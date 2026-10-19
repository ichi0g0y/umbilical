//! The `[appearance]` table of the config file. Only the GUI reads it.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Color themes of the GUI. `system` is the Umbilical purple, light or dark
/// like the OS. Every other theme is light or dark by itself.
/// Keep the names in sync with THEMES in ui/src/lib/themes.ts.
pub const THEMES: &[&str] = &[
    "system",
    "umbilical-light",
    "umbilical-dark",
    "catppuccin-latte",
    "catppuccin-frappe",
    "catppuccin-macchiato",
    "catppuccin-mocha",
    "dracula-light",
    "dracula-dark",
    "nord-light",
    "nord-dark",
    "solarized-light",
    "solarized-dark",
    "gruvbox-light",
    "gruvbox-dark",
    "tokyo-night-light",
    "tokyo-night-dark",
    "rose-pine-light",
    "rose-pine-dark",
    "one-light",
    "one-dark",
    "github-light",
    "github-dark",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "RawAppearance")]
pub struct AppearanceConfig {
    /// One of [`THEMES`].
    pub theme: String,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: "system".into(),
        }
    }
}

impl AppearanceConfig {
    pub fn validate(&self) -> Result<()> {
        super::check_one_of("appearance.theme", &self.theme, THEMES)
    }
}

/// The table as the file has it. Old files have a theme family (like
/// `catppuccin`) and `mode` (`system`, `light` or `dark`). They still load:
/// the two values become one theme.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawAppearance {
    theme: Option<String>,
    mode: Option<String>,
}

impl From<RawAppearance> for AppearanceConfig {
    fn from(raw: RawAppearance) -> Self {
        let theme = match raw.theme {
            Some(theme) if THEMES.contains(&theme.as_str()) => theme,
            // An old family becomes a theme. An unknown name stays, so
            // `validate` reports it.
            theme => {
                let family = theme.unwrap_or_else(|| "umbilical".into());
                old_theme(&family, raw.mode.as_deref()).unwrap_or(family)
            }
        };
        Self { theme }
    }
}

/// The theme for an old family and mode, or `None` for an unknown family or
/// mode.
fn old_theme(family: &str, mode: Option<&str>) -> Option<String> {
    if !matches!(mode, None | Some("system" | "light" | "dark")) {
        return None;
    }
    let version = match (family, mode) {
        ("umbilical", Some("light")) => "light",
        ("umbilical", Some("dark")) => "dark",
        ("umbilical", _) => return Some("system".into()),
        ("catppuccin", Some("light")) => "latte",
        ("catppuccin", _) => "mocha",
        (_, Some("light")) => "light",
        // Only the Umbilical colors follow the OS now. Other families pick
        // their dark version for "system".
        _ => "dark",
    };
    let theme = format!("{family}-{version}");
    THEMES.contains(&theme.as_str()).then_some(theme)
}
