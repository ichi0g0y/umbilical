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
    let config: Config = toml::from_str("[overrides.\"/x\"]\npermission_mode = \"yolo\"").unwrap();
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
