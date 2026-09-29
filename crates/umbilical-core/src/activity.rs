//! Is a Claude session working right now?
//!
//! Claude Code writes every session to a transcript file:
//! `~/.claude/projects/<folder>/<session>.jsonl`. Sessions in worktrees of the
//! folder (spawn mode `worktree`) use `<folder>--claude-worktrees-<name>`.
//! The last message of a transcript tells if a turn is still running:
//! an answer from Claude that ends the turn means "idle", anything else
//! (a tool call, a tool result, a new prompt) means "working".

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::Value;

/// Transcripts not written for this long are idle, whatever they say.
/// A session that crashed in the middle of a turn must not block forever.
pub const STALE_AFTER: Duration = Duration::from_secs(30 * 60);

/// Only the end of a transcript is read. One line can be big (tool output).
const TAIL_BYTES: u64 = 512 * 1024;

/// `~/.claude/projects`, or `$CLAUDE_CONFIG_DIR/projects`.
pub fn projects_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::config::home().join(".claude"))
        .join("projects")
}

/// Claude Code's folder name for a project: every character that is not a
/// letter or a digit becomes `-`.
pub fn project_dir_name(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// How many sessions of the folder `dir` are working now.
pub fn busy_sessions(projects: &Path, dir: &Path, now: SystemTime) -> usize {
    let base = project_dir_name(dir);
    let worktrees = format!("{base}--claude-worktrees-");
    let Ok(entries) = fs::read_dir(projects) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name == base || name.starts_with(&worktrees)
        })
        .map(|e| busy_in_project(&e.path(), now))
        .sum()
}

fn busy_in_project(project: &Path, now: SystemTime) -> usize {
    let Ok(entries) = fs::read_dir(project) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .filter(|p| is_working(p, now))
        .count()
}

/// Is the session in this transcript in the middle of a turn?
pub fn is_working(transcript: &Path, now: SystemTime) -> bool {
    let Some(age) = fs::metadata(transcript)
        .and_then(|m| m.modified())
        .ok()
        .map(|t| now.duration_since(t).unwrap_or_default())
    else {
        return false;
    };
    if age > STALE_AFTER {
        return false;
    }
    match last_turn_state(transcript) {
        Some(working) => working,
        // Nothing to read yet: only a file that is written right now counts.
        None => age < Duration::from_secs(120),
    }
}

/// `Some(true)`: a turn runs. `Some(false)`: the turn ended. `None`: no message found.
fn last_turn_state(transcript: &Path) -> Option<bool> {
    let text = read_tail(transcript)?;
    text.lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|entry| message_state(&entry))
}

fn message_state(entry: &Value) -> Option<bool> {
    let message = entry.get("message")?;
    match entry.get("type")?.as_str()? {
        "assistant" => {
            let stop = message.get("stop_reason").and_then(Value::as_str);
            // No stop reason: the answer is still streaming.
            Some(matches!(stop, None | Some("tool_use")))
        }
        "user" => Some(!is_interrupt(message)),
        _ => None,
    }
}

/// "[Request interrupted by user]" ends the turn without an answer.
fn is_interrupt(message: &Value) -> bool {
    let content = message.get("content");
    let texts: Vec<&str> = match content {
        Some(Value::String(s)) => vec![s.as_str()],
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect(),
        _ => Vec::new(),
    };
    texts.iter().any(|t| t.starts_with("[Request interrupted"))
}

fn read_tail(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if start > 0 {
        // The first line is cut. Drop it.
        let cut = text.find('\n').map_or(text.len(), |i| i + 1);
        text.drain(..cut);
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, lines.join("\n") + "\n").unwrap();
        p
    }

    const TOOL_USE: &str = r#"{"type":"assistant","message":{"stop_reason":"tool_use","content":[{"type":"tool_use"}]}}"#;
    const TOOL_RESULT: &str =
        r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"ok"}]}}"#;
    const END_TURN: &str = r#"{"type":"assistant","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"done"}]}}"#;
    const INTERRUPT: &str = r#"{"type":"user","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}}"#;
    const META: &str = r#"{"type":"last-prompt"}"#;

    #[test]
    fn folder_names() {
        assert_eq!(
            project_dir_name(Path::new("/Users/a/abyss/ichi0g0y.io")),
            "-Users-a-abyss-ichi0g0y-io"
        );
    }

    #[test]
    fn turn_states() {
        let tmp = tempfile::tempdir().unwrap();
        let now = SystemTime::now();
        let cases = [
            (vec![END_TURN, TOOL_USE], true),
            (vec![TOOL_USE, TOOL_RESULT], true),
            (vec![TOOL_USE, END_TURN, META], false),
            (vec![TOOL_USE, INTERRUPT], false),
        ];
        for (i, (lines, want)) in cases.iter().enumerate() {
            let p = write(tmp.path(), &format!("{i}.jsonl"), lines);
            assert_eq!(is_working(&p, now), *want, "case {i}");
        }
        // An old file is idle, even in the middle of a turn.
        let p = write(tmp.path(), "old.jsonl", &[TOOL_USE]);
        assert!(!is_working(&p, now + STALE_AFTER + Duration::from_secs(1)));
    }

    #[test]
    fn counts_the_folder_and_its_worktrees() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = Path::new("/w/robco");
        for (sub, lines) in [
            ("-w-robco", vec![TOOL_USE]),
            ("-w-robco--claude-worktrees-bridge-cse-1", vec![TOOL_RESULT]),
            ("-w-robco--claude-worktrees-bridge-cse-2", vec![END_TURN]),
            // Another folder whose name starts the same way.
            ("-w-robco-wt", vec![TOOL_USE]),
        ] {
            let d = tmp.path().join(sub);
            fs::create_dir_all(&d).unwrap();
            write(&d, "s.jsonl", &lines);
        }
        assert_eq!(busy_sessions(tmp.path(), dir, SystemTime::now()), 2);
    }
}
