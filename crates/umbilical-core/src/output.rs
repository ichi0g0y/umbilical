//! What one child printed: the last lines and what we found in them.

use std::collections::VecDeque;
use std::time::Instant;

use crate::detect::{Attention, Signal};
use crate::resume::{SessionLine, scan_session_line};

/// How many output lines we keep in memory for each directory.
pub const TAIL_LINES: usize = 1000;

/// Output of one directory. It lives across restarts.
#[derive(Debug, Default)]
pub struct Output {
    pub lines: VecDeque<String>,
    pub attention: Option<Attention>,
    pub session_url: Option<String>,
    /// The last start failed because an old registration still holds the folder.
    pub already_served: bool,
    /// Umbilical sent Ctrl-C to stop the process.
    pub stopping: bool,
    /// The process printed `Shutting down N active session(s)`.
    pub shutdown_seen: bool,
    /// Web sessions that ended while the process ran normally. The
    /// supervisor takes them out. Ends during a stop are not added, because
    /// a stop ends every session and they must come back after it.
    pub session_ends: Vec<(String, Instant)>,
}

impl Output {
    pub(crate) fn push(&mut self, line: String) {
        if self.lines.len() >= TAIL_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    /// The last lines, newest last.
    pub fn tail(&self, n: usize) -> Vec<String> {
        let skip = self.lines.len().saturating_sub(n);
        self.lines.iter().skip(skip).cloned().collect()
    }

    /// Forget the state of the last run. `first` is the first line to show.
    pub(crate) fn start(&mut self, first: String) {
        self.attention = None;
        self.session_url = None;
        self.already_served = false;
        self.stopping = false;
        self.shutdown_seen = false;
        self.session_ends.clear();
        self.push(first);
    }

    pub(crate) fn apply(&mut self, signals: Vec<Signal>) {
        for signal in signals {
            match signal {
                Signal::Attention(a) => self.attention = Some(a),
                Signal::SessionUrl(u) => self.session_url = Some(u),
                Signal::AlreadyServed => self.already_served = true,
            }
        }
    }

    /// Note a web session that ended, or the start of a shutdown.
    pub(crate) fn note_session_line(&mut self, line: &str) {
        match scan_session_line(line) {
            Some(SessionLine::ShuttingDown) => self.shutdown_seen = true,
            Some(SessionLine::Ended(id)) if !self.stopping && !self.shutdown_seen => {
                self.session_ends.push((id, Instant::now()));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ends_only_while_running_normally() {
        let mut out = Output::default();
        out.note_session_line("Session completed (3m 2s) cse_A1");
        assert_eq!(out.session_ends.len(), 1);
        assert_eq!(out.session_ends[0].0, "cse_A1");

        out.stopping = true;
        out.note_session_line("Session completed (1s) cse_B2");
        assert_eq!(out.session_ends.len(), 1);

        out.start("start".into());
        assert!(out.session_ends.is_empty());
        out.note_session_line("Shutting down 2 active session(s)");
        out.note_session_line("Session failed: Process exited with error cse_C3");
        assert!(out.shutdown_seen);
        assert!(out.session_ends.is_empty());
    }
}
