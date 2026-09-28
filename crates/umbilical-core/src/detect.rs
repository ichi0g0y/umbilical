//! Read the output of `claude remote-control` and guess its state.
//!
//! The patterns are best-effort. Claude Code can change its text at any time,
//! so a miss only means the GUI shows less detail. It never breaks restarts.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Something that needs the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attention {
    /// The workspace trust dialog is open. Run `claude` once in the directory.
    TrustPrompt,
    /// The bypass permissions warning is open.
    BypassPrompt,
    /// The account is not logged in, or the login expired.
    LoginRequired,
    /// claude asks "Enable Remote Control? (y/n)". The user must say yes once.
    RemoteControlConsent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    Attention(Attention),
    /// A claude.ai/code URL for this session.
    SessionUrl(String),
}

static URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https://claude\.ai/code[^\s\x1b)\]>]*").unwrap());

pub fn scan_line(line: &str) -> Vec<Signal> {
    let mut signals = Vec::new();
    let lower = line.to_lowercase();

    if lower.contains("enable remote control?") {
        signals.push(Signal::Attention(Attention::RemoteControlConsent));
    }
    if lower.contains("do you trust")
        || lower.contains("workspace not trusted")
        || lower.contains("trust the files")
        || lower.contains("trust this folder")
        || lower.contains("workspace trust")
    {
        signals.push(Signal::Attention(Attention::TrustPrompt));
    }
    if lower.contains("bypass permissions mode")
        && (lower.contains("accept") || lower.contains("warning"))
    {
        signals.push(Signal::Attention(Attention::BypassPrompt));
    }
    if lower.contains("not logged in")
        || lower.contains("must be logged in")
        || lower.contains("claude auth login")
        || lower.contains("please run /login")
        || lower.contains("run `claude /login`")
        || lower.contains("please log in")
        || lower.contains("login required")
        || lower.contains("oauth token has expired")
        || lower.contains("requires a subscription")
    {
        signals.push(Signal::Attention(Attention::LoginRequired));
    }
    if let Some(m) = URL.find(line) {
        signals.push(Signal::SessionUrl(
            m.as_str().trim_end_matches(['.', ',']).to_string(),
        ));
    }
    signals
}

/// Terminal queries that need an answer, or the program may wait for one.
/// Returns the bytes to write back.
pub fn terminal_replies(chunk: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    // Cursor position report.
    if contains(chunk, b"\x1b[6n") {
        out.extend_from_slice(b"\x1b[1;1R");
    }
    // Primary device attributes.
    if contains(chunk, b"\x1b[c") || contains(chunk, b"\x1b[0c") {
        out.extend_from_slice(b"\x1b[?62;22c");
    }
    out
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Split raw pty output into clean text lines.
///
/// A TUI redraws with `\r` and escape codes, so we cut on both `\n` and `\r`,
/// remove escape codes and drop empty lines.
#[derive(Default)]
pub struct LineSplitter {
    buf: Vec<u8>,
}

impl LineSplitter {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(chunk);
        let mut lines = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n' || b == b'\r') {
            let raw: Vec<u8> = self.buf.drain(..=pos).collect();
            if let Some(line) = clean(&raw) {
                lines.push(line);
            }
        }
        // A very long line with no break: flush it anyway.
        if self.buf.len() > 16 * 1024 {
            let raw = std::mem::take(&mut self.buf);
            if let Some(line) = clean(&raw) {
                lines.push(line);
            }
        }
        lines
    }

    /// The text after the last line break. Prompts like "(y/n)" wait here
    /// with no line break.
    pub fn pending(&self) -> Option<String> {
        clean(&self.buf)
    }
}

fn clean(raw: &[u8]) -> Option<String> {
    let stripped = strip_ansi_escapes::strip(raw);
    let text = String::from_utf8_lossy(&stripped);
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_url() {
        let s = scan_line("Open https://claude.ai/code/session_abc123 in your browser.");
        assert_eq!(
            s,
            vec![Signal::SessionUrl(
                "https://claude.ai/code/session_abc123".into()
            )]
        );
    }

    #[test]
    fn finds_attention() {
        assert!(
            scan_line("Do you trust the files in this folder?")
                .contains(&Signal::Attention(Attention::TrustPrompt))
        );
        assert!(
            scan_line("Error: Not logged in. Please run /login")
                .contains(&Signal::Attention(Attention::LoginRequired))
        );
        assert!(
            scan_line("Error: You must be logged in to use Remote Control.")
                .contains(&Signal::Attention(Attention::LoginRequired))
        );
        assert!(scan_line("Remote Control is ready").is_empty());
    }

    #[test]
    fn splitter_cleans_ansi_and_cr() {
        let mut s = LineSplitter::default();
        let lines = s.push(b"\x1b[32mhello\x1b[0m\r\n\r\npart");
        assert_eq!(lines, vec!["hello"]);
        let lines = s.push(b"ial\n");
        assert_eq!(lines, vec!["partial"]);
    }

    #[test]
    fn finds_consent_prompt_without_newline() {
        let mut s = LineSplitter::default();
        assert!(
            s.push(
                b"control. Press Ctrl+C to stop.\r\n\x1b[1mEnable Remote Control? (y/n)\x1b[0m "
            )
            .len()
                == 1
        );
        let pending = s.pending().unwrap();
        assert_eq!(pending, "Enable Remote Control? (y/n)");
        assert!(scan_line(&pending).contains(&Signal::Attention(Attention::RemoteControlConsent)));
        assert!(
            scan_line("Error: Workspace not trusted. Please run `claude` in /x first")
                .contains(&Signal::Attention(Attention::TrustPrompt))
        );
    }

    #[test]
    fn replies_to_queries() {
        assert_eq!(terminal_replies(b"abc\x1b[6ndef"), b"\x1b[1;1R");
        assert!(terminal_replies(b"plain").is_empty());
    }
}
