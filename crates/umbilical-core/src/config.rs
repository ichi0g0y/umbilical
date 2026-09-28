//! Config file (`~/.umbilical/config.toml`).
//!
//! The file is the source of truth. The GUI reads it, changes it and writes it back.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// `--spawn` modes of `claude remote-control`.
pub const SPAWN_MODES: &[&str] = &["same-dir", "worktree", "session"];

/// `--permission-mode` values of `claude remote-control`.
pub const PERMISSION_MODES: &[&str] = &[
    "acceptEdits",
    "auto",
    "bypassPermissions",
    "default",
    "dontAsk",
    "plan",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Every directory directly under these roots is a target.
    pub roots: Vec<String>,
    /// Single directories added by hand.
    pub dirs: Vec<String>,
    /// Directory names or paths to skip.
    pub exclude: Vec<String>,
    pub scan_interval_secs: u64,

    /// Path to the claude binary. Empty means "find it".
    pub claude_bin: String,
    pub permission_mode: String,
    pub spawn: String,
    /// `--capacity`. `None` means the claude default.
    pub capacity: Option<u32>,
    pub extra_args: Vec<String>,
    pub env: BTreeMap<String, String>,

    /// Mark the roots and every target as trusted in Claude Code before start.
    pub auto_trust: bool,
    /// Claude Code's `.claude.json`. Empty means the default place.
    pub claude_json: String,

    pub backoff_min_secs: u64,
    pub backoff_max_secs: u64,
    /// If a child runs this long, the backoff goes back to the minimum.
    pub stable_reset_secs: u64,

    /// Empty means the platform default.
    pub log_dir: String,
    pub log_max_bytes: u64,
    /// More detail in umbilical.log, and claude's own `--debug-file` per folder.
    pub debug: bool,

    pub autostart: bool,
    pub update: UpdateConfig,

    /// Per-directory settings. The key is the full path of the directory.
    pub overrides: BTreeMap<String, DirOverride>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            roots: vec!["~/.umbilical/repos".into()],
            dirs: Vec::new(),
            exclude: Vec::new(),
            scan_interval_secs: 5,
            claude_bin: String::new(),
            permission_mode: "bypassPermissions".into(),
            spawn: "worktree".into(),
            capacity: None,
            extra_args: Vec::new(),
            env: BTreeMap::new(),
            auto_trust: true,
            claude_json: String::new(),
            backoff_min_secs: 5,
            backoff_max_secs: 300,
            stable_reset_secs: 600,
            log_dir: String::new(),
            log_max_bytes: 5 * 1024 * 1024,
            debug: false,
            autostart: true,
            update: UpdateConfig::default(),
            overrides: BTreeMap::new(),
        }
    }
}

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

/// Settings for one directory. `None` means "use the global value".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DirOverride {
    pub enabled: Option<bool>,
    pub name: Option<String>,
    pub claude_bin: Option<String>,
    pub permission_mode: Option<String>,
    pub spawn: Option<String>,
    pub capacity: Option<u32>,
    pub extra_args: Option<Vec<String>>,
    /// Added on top of the global env.
    pub env: BTreeMap<String, String>,
}

impl DirOverride {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl Config {
    /// `~/.umbilical/config.toml`
    pub fn default_path() -> PathBuf {
        home().join(".umbilical").join("config.toml")
    }

    /// Load the file. If it does not exist, write the default config and return it.
    pub fn load_or_create(path: &Path) -> Result<Self> {
        if !path.exists() {
            let config = Self::default();
            config.save(path)?;
            return Ok(config);
        }
        Self::load(path)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let config: Self =
            toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut clean = self.clone();
        clean.overrides.retain(|_, o| !o.is_empty());
        let text = toml::to_string_pretty(&clean)?;
        // Write to a temp file first, so a crash never leaves a half file.
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        check_one_of("spawn", &self.spawn, SPAWN_MODES)?;
        check_one_of("permission_mode", &self.permission_mode, PERMISSION_MODES)?;
        if !matches!(self.update.channel.as_str(), "stable" | "nightly") {
            anyhow::bail!(
                "update.channel must be \"stable\" or \"nightly\", got {:?}",
                self.update.channel
            );
        }
        if self.backoff_min_secs == 0 || self.backoff_max_secs < self.backoff_min_secs {
            anyhow::bail!("backoff_min_secs must be > 0 and <= backoff_max_secs");
        }
        for (path, o) in &self.overrides {
            if let Some(spawn) = &o.spawn {
                check_one_of(&format!("overrides.{path:?}.spawn"), spawn, SPAWN_MODES)?;
            }
            if let Some(mode) = &o.permission_mode {
                check_one_of(
                    &format!("overrides.{path:?}.permission_mode"),
                    mode,
                    PERMISSION_MODES,
                )?;
            }
        }
        Ok(())
    }

    pub fn log_dir(&self) -> PathBuf {
        if self.log_dir.trim().is_empty() {
            default_log_dir()
        } else {
            expand_home(&self.log_dir)
        }
    }
}

fn check_one_of(field: &str, value: &str, allowed: &[&str]) -> Result<()> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        anyhow::bail!("{field} must be one of {allowed:?}, got {value:?}")
    }
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Turn `~` or `~/x` into a full path.
pub fn expand_home(path: &str) -> PathBuf {
    let path = path.trim();
    if path == "~" {
        return home();
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        return home().join(rest);
    }
    PathBuf::from(path)
}

pub fn default_log_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("UMBILICAL_LOG_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    if cfg!(target_os = "macos") {
        home().join("Library").join("Logs").join("Umbilical")
    } else if cfg!(windows) {
        dirs::data_local_dir()
            .unwrap_or_else(home)
            .join("Umbilical")
            .join("logs")
    } else {
        dirs::state_dir()
            .or_else(dirs::data_local_dir)
            .unwrap_or_else(home)
            .join("umbilical")
            .join("logs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut config = Config::default();
        config.update.channel = "nightly".into();
        config.overrides.insert(
            "/tmp/a".into(),
            DirOverride {
                spawn: Some("same-dir".into()),
                ..Default::default()
            },
        );
        config
            .overrides
            .insert("/tmp/empty".into(), DirOverride::default());
        config.save(&path).unwrap();

        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.update.channel, "nightly");
        assert_eq!(
            loaded.overrides["/tmp/a"].spawn.as_deref(),
            Some("same-dir")
        );
        // Empty overrides are not written.
        assert!(!loaded.overrides.contains_key("/tmp/empty"));
    }

    #[test]
    fn bad_values_are_rejected() {
        let config: Config = toml::from_str("spawn = \"nope\"").unwrap();
        assert!(config.validate().is_err());
        let config: Config = toml::from_str("[update]\nchannel = \"beta\"").unwrap();
        assert!(config.validate().is_err());
        let config: Config =
            toml::from_str("[overrides.\"/x\"]\npermission_mode = \"yolo\"").unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn load_or_create_writes_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("config.toml");
        let config = Config::load_or_create(&path).unwrap();
        assert_eq!(config, Config::default());
        assert!(path.exists());
    }

    #[test]
    fn expand_home_works() {
        assert_eq!(expand_home("~"), home());
        assert_eq!(expand_home("~/a/b"), home().join("a/b"));
        assert_eq!(expand_home("/abs"), PathBuf::from("/abs"));
    }
}
