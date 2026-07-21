//! PTY-backed agent pane with VT100 scrollback.

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use vt100::Parser;

const SCROLLBACK_MAX: usize = 800;
const PTY_DEFAULT_ROWS: u16 = 24;
const PTY_DEFAULT_COLS: u16 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneStatus {
    Starting,
    Running,
    WaitingPrompt,
    Done,
    Failed,
    TimedOut,
    Closed,
}

impl PaneStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::WaitingPrompt => "prompt",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Closed => "closed",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Done | Self::Failed | Self::TimedOut | Self::Closed
        )
    }

    pub fn sidebar_mark(self, tick: u64) -> &'static str {
        match self {
            Self::Done => "✓",
            Self::Failed | Self::TimedOut => "✗",
            Self::Closed => "·",
            Self::WaitingPrompt => "?",
            Self::Starting | Self::Running => match tick % 4 {
                0 => "⠋",
                1 => "⠙",
                2 => "⠹",
                _ => "⠸",
            },
        }
    }
}

pub struct PaneJob {
    pub block_key: String,
    pub argv: Vec<String>,
    pub working_dir: PathBuf,
    pub signal_path: PathBuf,
    /// Extra env for the PTY child (e.g. CODEBEACON_MA_*).
    pub env: Vec<(String, String)>,
    pub role: super::conductor::AgentRole,
}

pub struct Pane {
    pub block_key: String,
    pub status: PaneStatus,
    pub via: String,
    pub exit_code: i32,
    pub term: Arc<Mutex<Parser>>,
    pub signal_path: PathBuf,
    pub scroll: usize,
    /// When true, sidebar shows a loader for an in-flight re-prompt.
    pub re_prompt_loading: bool,
    pub role: super::conductor::AgentRole,
    writer: Option<Box<dyn Write + Send>>,
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    master: Option<Box<dyn MasterPty + Send>>,
    stop_reader: Arc<AtomicBool>,
}

impl Pane {
    pub fn inject_line(&self, line: &str) {
        if let Ok(mut p) = self.term.lock() {
            p.process(format!("\r\n{line}\r\n").as_bytes());
        }
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        if let Some(w) = self.writer.as_mut() {
            let _ = w.write_all(bytes);
            let _ = w.flush();
        }
        // User answered — drop waiting mark until the next screen poll.
        if self.status == PaneStatus::WaitingPrompt && !bytes.is_empty() {
            self.status = PaneStatus::Running;
            self.re_prompt_loading = false;
        }
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        let rows = rows.max(2);
        let cols = cols.max(20);
        if let Some(m) = self.master.as_mut() {
            let _ = m.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
        if let Ok(mut p) = self.term.lock() {
            p.set_size(rows, cols);
        }
    }

    pub fn try_wait_child(&mut self) -> Option<i32> {
        let child = self.child.as_mut()?;
        match child.try_wait() {
            Ok(Some(status)) => Some(status.exit_code() as i32),
            _ => None,
        }
    }

    pub fn kill(&mut self) {
        self.stop_reader.store(true, Ordering::SeqCst);
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
        self.writer = None;
        self.master = None;
    }

    pub fn screen_lines(&self, height: u16, width: u16) -> Vec<String> {
        let Ok(p) = self.term.lock() else {
            return vec![];
        };
        let screen = p.screen();
        let rows = height as usize;
        let cols = width as usize;
        let mut out = Vec::with_capacity(rows);
        let total = screen.size().0 as usize;
        let start = total.saturating_sub(rows + self.scroll);
        for r in start..start + rows {
            if r >= total {
                out.push(String::new());
                continue;
            }
            let mut line = String::new();
            for c in 0..cols.min(screen.size().1 as usize) {
                let cell = screen.cell(r as u16, c as u16);
                if let Some(cell) = cell {
                    line.push(cell.contents().chars().next().unwrap_or(' '));
                } else {
                    line.push(' ');
                }
            }
            out.push(line.trim_end().to_string());
        }
        out
    }

    /// Bottom `n` non-empty screen lines joined for await-input detection.
    pub fn screen_text_tail(&self, n_lines: usize) -> String {
        let lines = self.screen_lines(40, 120);
        lines
            .into_iter()
            .filter(|l| !l.trim().is_empty())
            .rev()
            .take(n_lines)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        self.kill();
    }
}

pub fn spawn_pane(job: PaneJob) -> Result<Pane, String> {
    if job.argv.is_empty() {
        return Err(format!("empty argv for {}", job.block_key));
    }
    if job.signal_path.exists() {
        let _ = std::fs::remove_file(&job.signal_path);
    }
    if let Some(parent) = job.signal_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: PTY_DEFAULT_ROWS,
            cols: PTY_DEFAULT_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("openpty {}: {e}", job.block_key))?;

    let mut cmd = CommandBuilder::new(&job.argv[0]);
    for a in &job.argv[1..] {
        cmd.arg(a);
    }
    cmd.cwd(&job.working_dir);
    for (k, v) in &job.env {
        cmd.env(k, v);
    }

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("spawn {}: {e}", job.block_key))?;

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("pty reader {}: {e}", job.block_key))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("pty writer {}: {e}", job.block_key))?;

    let term = Arc::new(Mutex::new(Parser::new(
        PTY_DEFAULT_ROWS,
        PTY_DEFAULT_COLS,
        SCROLLBACK_MAX,
    )));
    let stop_reader = Arc::new(AtomicBool::new(false));
    let term_r = Arc::clone(&term);
    let stop_r = Arc::clone(&stop_reader);
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while !stop_r.load(Ordering::SeqCst) {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if let Ok(mut p) = term_r.lock() {
                        p.process(&buf[..n]);
                    }
                }
                Err(_) => {
                    thread::sleep(Duration::from_millis(20));
                }
            }
        }
    });

    Ok(Pane {
        block_key: job.block_key,
        status: PaneStatus::Running,
        via: String::new(),
        exit_code: 0,
        term,
        signal_path: job.signal_path,
        scroll: 0,
        re_prompt_loading: false,
        role: job.role,
        writer: Some(writer),
        child: Some(child),
        master: Some(pair.master),
        stop_reader,
    })
}

/// Placeholder pane (no PTY) for dry-run / queued display.
pub fn placeholder_pane(block_key: &str, signal_path: PathBuf, status: PaneStatus) -> Pane {
    Pane {
        block_key: block_key.to_string(),
        status,
        via: String::new(),
        exit_code: 0,
        term: Arc::new(Mutex::new(Parser::new(
            PTY_DEFAULT_ROWS,
            PTY_DEFAULT_COLS,
            SCROLLBACK_MAX,
        ))),
        signal_path,
        scroll: 0,
        re_prompt_loading: false,
        role: super::conductor::AgentRole::Ensemble,
        writer: None,
        child: None,
        master: None,
        stop_reader: Arc::new(AtomicBool::new(true)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_statuses() {
        assert!(PaneStatus::Done.is_terminal());
        assert!(!PaneStatus::Running.is_terminal());
        assert_eq!(PaneStatus::Done.sidebar_mark(0), "✓");
        assert_eq!(PaneStatus::WaitingPrompt.sidebar_mark(0), "?");
    }

    #[test]
    fn placeholder_injects() {
        let p = placeholder_pane("auth", PathBuf::from("/tmp/x"), PaneStatus::Starting);
        p.inject_line("hello");
        // Ensure inject does not panic; screen may buffer beyond visible rows.
        let _ = p.screen_lines(24, 80);
        assert_eq!(p.block_key, "auth");
    }
}
