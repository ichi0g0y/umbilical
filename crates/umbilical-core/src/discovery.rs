//! Find target directories and work out the settings for each one.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::{Config, DirOverride, expand_home};

/// One directory to watch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Target {
    /// Full path as a string. Used as the ID and as the key in `overrides`.
    pub key: String,
    pub path: PathBuf,
    /// Session name shown in claude.ai/code.
    pub name: String,
    /// Where it came from: a root path, or "manual".
    pub source: String,
    pub enabled: bool,
    pub settings: Settings,
}

/// Final settings for one directory after overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Settings {
    pub claude_bin: String,
    pub permission_mode: String,
    /// The spawn mode from the config.
    pub spawn: String,
    /// The spawn mode that is really used. `worktree` needs git, so it falls back to `same-dir`.
    pub effective_spawn: String,
    pub capacity: Option<u32>,
    pub extra_args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

impl Settings {
    /// Arguments after `claude`.
    pub fn args(&self, name: &str) -> Vec<String> {
        let mut args = vec![
            "remote-control".to_string(),
            "--name".into(),
            name.into(),
            "--permission-mode".into(),
            self.permission_mode.clone(),
            "--spawn".into(),
            self.effective_spawn.clone(),
        ];
        if let Some(capacity) = self.capacity {
            args.push("--capacity".into());
            args.push(capacity.to_string());
        }
        args.extend(self.extra_args.iter().cloned());
        args
    }
}

/// List every target from the config. The result is sorted by name.
pub fn discover(config: &Config) -> Vec<Target> {
    let mut found: Vec<(PathBuf, String)> = Vec::new();

    for root in &config.roots {
        if root.trim().is_empty() {
            continue;
        }
        let root_path = expand_home(root);
        let Ok(entries) = fs::read_dir(&root_path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            // `is_dir` follows symlinks, so links to projects work.
            if hidden || !path.is_dir() {
                continue;
            }
            found.push((path, root.clone()));
        }
    }
    for dir in &config.dirs {
        if dir.trim().is_empty() {
            continue;
        }
        let path = expand_home(dir);
        if path.is_dir() {
            found.push((path, "manual".into()));
        }
    }

    let mut seen = std::collections::HashSet::new();
    found.retain(|(path, _)| !is_excluded(config, path) && seen.insert(key_of(path)));

    let base_names: Vec<String> = found.iter().map(|(p, _)| base_name(p)).collect();
    let mut name_count: HashMap<&str, usize> = HashMap::new();
    for name in &base_names {
        *name_count.entry(name).or_default() += 1;
    }

    let mut targets: Vec<Target> = found
        .iter()
        .zip(&base_names)
        .map(|((path, source), base)| {
            let key = key_of(path);
            let o = config.overrides.get(&key).cloned().unwrap_or_default();
            // Two directories with the same name: add the parent name.
            let default_name = if name_count[base.as_str()] > 1 {
                let parent = path.parent().map(base_name).unwrap_or_else(|| "dir".into());
                format!("{parent}-{base}")
            } else {
                base.clone()
            };
            let name = o
                .name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or(default_name);
            Target {
                key,
                name,
                source: source.clone(),
                enabled: o.enabled.unwrap_or(true),
                settings: resolve(config, &o, path),
                path: path.clone(),
            }
        })
        .collect();
    targets.sort_by(|a, b| a.name.cmp(&b.name).then(a.key.cmp(&b.key)));
    targets
}

fn resolve(config: &Config, o: &DirOverride, path: &Path) -> Settings {
    let spawn = o.spawn.clone().unwrap_or_else(|| config.spawn.clone());
    let effective_spawn = if spawn == "worktree" && !path.join(".git").exists() {
        "same-dir".to_string()
    } else {
        spawn.clone()
    };
    let mut env = config.env.clone();
    env.extend(o.env.clone());
    Settings {
        claude_bin: o
            .claude_bin
            .clone()
            .filter(|b| !b.trim().is_empty())
            .unwrap_or_else(|| config.claude_bin.clone()),
        permission_mode: o
            .permission_mode
            .clone()
            .unwrap_or_else(|| config.permission_mode.clone()),
        spawn,
        effective_spawn,
        capacity: o.capacity.or(config.capacity),
        extra_args: o
            .extra_args
            .clone()
            .unwrap_or_else(|| config.extra_args.clone()),
        env,
    }
}

fn is_excluded(config: &Config, path: &Path) -> bool {
    let name = base_name(path);
    let key = key_of(path);
    config.exclude.iter().any(|e| {
        let e = e.trim();
        !e.is_empty() && (e == name || key_of(&expand_home(e)) == key)
    })
}

pub fn key_of(path: &Path) -> String {
    path.to_string_lossy()
        .trim_end_matches(['/', '\\'])
        .to_string()
}

fn base_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| key_of(path))
}

#[cfg(test)]
mod tests {
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
}
