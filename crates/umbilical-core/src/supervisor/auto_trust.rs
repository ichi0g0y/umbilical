//! Mark the target directories as trusted for claude.

use std::path::PathBuf;

use super::Worker;

impl Worker {
    /// Trust the roots and every enabled target before they start.
    pub(super) fn ensure_trust(&mut self) {
        if !self.config.auto_trust {
            return;
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
