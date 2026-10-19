//! Tests for `config.rs`.
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
    config.appearance.theme = "catppuccin-frappe".into();
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
    assert_eq!(loaded.appearance.theme, "catppuccin-frappe");
    // The old `mode` key is not written any more.
    let written: toml::Table = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert!(written["appearance"].get("mode").is_none());
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
    let config: Config = toml::from_str("[overrides.\"/x\"]\npermission_mode = \"yolo\"").unwrap();
    assert!(config.validate().is_err());
    let config: Config = toml::from_str("[appearance]\ntheme = \"neon\"").unwrap();
    assert!(config.validate().is_err());
    let config: Config = toml::from_str("[appearance]\ntheme = \"neon\"\nmode = \"dark\"").unwrap();
    assert!(config.validate().is_err());
    let config: Config = toml::from_str("[appearance]\ntheme = \"nord\"\nmode = \"dim\"").unwrap();
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

#[test]
fn every_theme_is_valid() {
    for theme in THEMES {
        let mut config = Config::default();
        config.appearance.theme = (*theme).into();
        assert!(config.validate().is_ok(), "{theme}");
    }
}

#[test]
fn old_theme_and_mode_load_as_one_theme() {
    let cases = [
        ("umbilical", "system", "system"),
        ("umbilical", "light", "umbilical-light"),
        ("umbilical", "dark", "umbilical-dark"),
        ("catppuccin", "light", "catppuccin-latte"),
        ("catppuccin", "dark", "catppuccin-mocha"),
        ("catppuccin", "system", "catppuccin-mocha"),
        ("solarized", "light", "solarized-light"),
        ("tokyo-night", "dark", "tokyo-night-dark"),
        ("github", "system", "github-dark"),
        // A new name wins over an old mode.
        ("nord-light", "dark", "nord-light"),
    ];
    for (theme, mode, want) in cases {
        let table = format!("[appearance]\ntheme = {theme:?}\nmode = {mode:?}");
        let config: Config = toml::from_str(&table).unwrap();
        assert_eq!(config.appearance.theme, want, "{table}");
        assert!(config.validate().is_ok(), "{table}");
    }
    // Missing values use the old defaults: umbilical and system.
    let config: Config = toml::from_str("[appearance]\ntheme = \"umbilical\"").unwrap();
    assert_eq!(config.appearance.theme, "system");
    let config: Config = toml::from_str("[appearance]\nmode = \"dark\"").unwrap();
    assert_eq!(config.appearance.theme, "umbilical-dark");
}

#[test]
fn old_appearance_file_loads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let old = "[appearance]\ntheme = \"rose-pine\"\nmode = \"light\"\n";
    fs::write(&path, old).unwrap();
    let config = Config::load(&path).unwrap();
    assert_eq!(config.appearance.theme, "rose-pine-light");
}

/// The GUI has the same theme names, and a color block for each one.
#[test]
fn ui_knows_every_theme() {
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src");
    let read = |p: &str| fs::read_to_string(ui.join(p)).unwrap();
    let (list, types) = (read("lib/themes.ts"), read("lib/api.ts"));
    let css: String = fs::read_dir(ui.join("themes"))
        .unwrap()
        .map(|e| fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    for theme in THEMES {
        let quoted = format!("\"{theme}\"");
        assert!(list.contains(&quoted), "{theme} in lib/themes.ts");
        assert!(types.contains(&quoted), "{theme} in lib/api.ts");
        let umbilical = *theme == "system" || theme.starts_with("umbilical-");
        let block = format!(":root[data-theme={quoted}] {{");
        assert!(umbilical || css.contains(&block), "{theme} in themes/");
    }
    let count = list.matches("[\"").count() + list.matches("value: \"").count();
    assert_eq!(count, THEMES.len(), "lib/themes.ts has other names");
}
