//! Mark the target directories as trusted for claude.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::Worker;

/// Claude Code may write an old copy of `.claude.json` back and drop the key.
/// Read the file again this often (a read only; it is written only when a key is missing).
const RECHECK: Duration = Duration::from_secs(5 * 60);

impl Worker {
    /// Trust the roots and every enabled target before they start.
    pub(super) fn ensure_trust(&mut self) {
        if !self.config.auto_trust {
            return;
        }
        if self.trusted_at.is_some_and(|t| t.elapsed() >= RECHECK) {
            self.trusted.clear();
            self.trusted_at = None;
        }
        let mut want: Vec<PathBuf> = self
            .config
            .roots
            .iter()
            .filter(|r| !r.trim().is_empty())
            .map(|r| crate::config::expand_home(r))
            .collect();
        want.extend(
            self.entries
                .values()
                .filter(|e| e.target.enabled)
                .map(|e| e.target.path.clone()),
        );
        want.retain(|p| !self.trusted.contains(p));
        if want.is_empty() {
            return;
        }
        let file = if self.config.claude_json.trim().is_empty() {
            crate::trust::claude_json_path()
        } else {
            crate::config::expand_home(&self.config.claude_json)
        };
        match crate::trust::ensure_trusted(&file, &want) {
            Ok(changed) => {
                for key in &changed {
                    self.log(&format!("trusted {key} in {}", file.display()));
                }
                self.trusted.extend(want);
                self.trusted_at.get_or_insert_with(Instant::now);
                self.trust_error = None;
            }
            Err(e) => {
                let msg = format!("could not update trust: {e:#}");
                if self.trust_error.as_deref() != Some(&msg) {
                    self.log(&msg);
                }
                self.trust_error = Some(msg);
            }
        }
    }
}
