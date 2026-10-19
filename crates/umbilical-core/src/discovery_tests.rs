//! Tests for `discovery.rs`.
use super::*;
use std::fs;

fn setup() -> (tempfile::TempDir, Config) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repos");
    for d in ["alpha", "beta", ".hidden"] {
        fs::create_dir_all(root.join(d)).unwrap();
    }
    fs::create_dir_all(root.join("alpha").join(".git")).unwrap();
    fs::write(root.join("file.txt"), "x").unwrap();
    let config = Config {
        roots: vec![key_of(&root)],
        ..Default::default()
    };
    (tmp, config)
}

#[test]
fn finds_dirs_and_skips_hidden_and_files() {
    let (_tmp, config) = setup();
    let names: Vec<_> = discover(&config).into_iter().map(|t| t.name).collect();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[test]
fn worktree_falls_back_without_git() {
    let (_tmp, config) = setup();
    let targets = discover(&config);
    assert_eq!(targets[0].settings.effective_spawn, "worktree");
    assert_eq!(targets[1].settings.spawn, "worktree");
    assert_eq!(targets[1].settings.effective_spawn, "same-dir");
}

#[test]
fn exclude_by_name_and_path() {
    let (tmp, mut config) = setup();
    config.exclude = vec!["alpha".into()];
    assert_eq!(discover(&config).len(), 1);
    config.exclude = vec![key_of(&tmp.path().join("repos").join("beta"))];
    let names: Vec<_> = discover(&config).into_iter().map(|t| t.name).collect();
    assert_eq!(names, vec!["alpha"]);
}

#[test]
fn overrides_apply() {
    let (_tmp, mut config) = setup();
    config.env.insert("A".into(), "1".into());
    let beta = discover(&config)[1].key.clone();
    config.overrides.insert(
        beta,
        DirOverride {
            enabled: Some(false),
            name: Some("b".into()),
            permission_mode: Some("plan".into()),
            capacity: Some(4),
            env: [("B".to_string(), "2".to_string())].into(),
            ..Default::default()
        },
    );
    let t = discover(&config)
        .into_iter()
        .find(|t| t.name == "b")
        .unwrap();
    assert!(!t.enabled);
    assert_eq!(t.settings.permission_mode, "plan");
    assert_eq!(t.settings.env.len(), 2);
    let args = t.settings.args(&t.name);
    assert!(args.windows(2).any(|w| w == ["--capacity", "4"]));
    assert!(args.windows(2).any(|w| w == ["--name", "b"]));
}

#[test]
fn skip_first_session() {
    const FLAG: &str = "--no-create-session-in-dir";
    let (_tmp, mut config) = setup();
    let has_flag = |config: &Config| {
        let t = &discover(config)[0];
        t.settings.args(&t.name).iter().any(|a| a == FLAG)
    };
    // On by default.
    assert!(has_flag(&config));
    // Session mode needs the first session.
    config.spawn = "session".into();
    assert!(!has_flag(&config));
    config.spawn = "worktree".into();
    let alpha = discover(&config)[0].key.clone();
    config.overrides.insert(
        alpha,
        DirOverride {
            skip_first_session: Some(false),
            ..Default::default()
        },
    );
    assert!(!has_flag(&config));
}

#[test]
fn manual_dirs_and_duplicate_names() {
    let (tmp, mut config) = setup();
    let other = tmp.path().join("other").join("alpha");
    fs::create_dir_all(&other).unwrap();
    config.dirs = vec![key_of(&other), key_of(&other)];
    let targets = discover(&config);
    assert_eq!(targets.len(), 3);
    let names: Vec<_> = targets.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"repos-alpha"));
    assert!(names.contains(&"other-alpha"));
    assert!(targets.iter().any(|t| t.source == "manual"));
}

#[cfg(unix)]
#[test]
fn follows_symlinks() {
    let (tmp, config) = setup();
    let real = tmp.path().join("real-project");
    fs::create_dir_all(&real).unwrap();
    std::os::unix::fs::symlink(&real, tmp.path().join("repos").join("linked")).unwrap();
    assert!(discover(&config).iter().any(|t| t.name == "linked"));
}
