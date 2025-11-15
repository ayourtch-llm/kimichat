/// Terminal backend abstraction for PTY and tmux implementations
use anyhow::Result;
use async_trait::async_trait;
use std::time::SystemTime;

/// Terminal session metadata
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub command: String,
    pub created_at: SystemTime,
    pub rows: u16,
    pub cols: u16,
    pub working_dir: Option<String>,
    pub status: String,
}

/// Cursor position in terminal
#[derive(Debug, Clone)]
pub struct CursorPosition {
    pub row: usize,
    pub col: usize,
}

/// Terminal backend trait - abstraction over PTY and tmux
/// All operations must be identical from LLM perspective
#[async_trait]
pub trait TerminalBackend: Send + Sync {
    /// Launch a new terminal session
    /// Returns session ID
    async fn launch_session(
        &mut self,
        id: String,
        command: String,
        rows: u16,
        cols: u16,
        working_dir: Option<String>,
    ) -> Result<String>;

    /// Send input to a session
    /// For commands, caller must append \n explicitly
    async fn send_keys(&mut self, session_id: &str, keys: &str) -> Result<()>;

    /// Get current screen content
    /// Returns visible screen area (rows x cols)
    async fn get_screen(&self, session_id: &str, include_colors: bool, include_cursor: bool) -> Result<String>;

    /// List all active sessions
    async fn list_sessions(&self) -> Result<Vec<SessionInfo>>;

    /// Kill a session
    async fn kill_session(&mut self, session_id: &str) -> Result<()>;

    /// Get cursor position
    async fn get_cursor_position(&self, session_id: &str) -> Result<CursorPosition>;

    /// Resize session
    async fn resize_session(&mut self, session_id: &str, rows: u16, cols: u16) -> Result<()>;

    /// Get scrollback buffer (if supported)
    /// Returns historical output beyond current screen
    async fn get_scrollback(&self, session_id: &str, lines: usize) -> Result<Option<String>>;

    /// Set scrollback buffer size (if supported)
    async fn set_scrollback(&mut self, session_id: &str, lines: usize) -> Result<()>;

    /// Start capturing session output to file
    async fn capture_start(&mut self, session_id: &str, output_file: String) -> Result<()>;

    /// Stop capturing session output
    /// Returns (capture_file_path, bytes_captured, duration_seconds)
    async fn capture_stop(&mut self, session_id: &str) -> Result<(String, usize, f64)>;

    /// Check if session exists
    async fn session_exists(&self, session_id: &str) -> bool;

    /// Get backend name for debugging
    fn backend_name(&self) -> &str;
}

/// Configuration for which backend to use
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalBackendType {
    /// Internal PTY implementation (default)
    Pty,
    /// Tmux-based implementation
    Tmux,
}

impl Default for TerminalBackendType {
    fn default() -> Self {
        Self::Pty
    }
}

impl std::str::FromStr for TerminalBackendType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "pty" | "internal" => Ok(Self::Pty),
            "tmux" => Ok(Self::Tmux),
            _ => Err(anyhow::anyhow!(
                "Invalid terminal backend: '{}'. Valid options: 'pty', 'tmux'",
                s
            )),
        }
    }
}

impl std::fmt::Display for TerminalBackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pty => write!(f, "pty"),
            Self::Tmux => write!(f, "tmux"),
        }
    }
}
