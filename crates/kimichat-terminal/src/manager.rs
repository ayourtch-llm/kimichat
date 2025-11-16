use std::path::PathBuf;
use anyhow::Result;

use super::backend::{TerminalBackend, TerminalBackendType, SessionInfo};
use super::pty_backend::PtyBackend;
use super::tmux_backend::TmuxBackend;
use super::MAX_CONCURRENT_SESSIONS;

/// Manages all terminal sessions globally using pluggable backends
pub struct TerminalManager {
    backend: Box<dyn TerminalBackend>,
    backend_type: TerminalBackendType,
}

impl std::fmt::Debug for TerminalManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalManager")
            .field("backend", &self.backend.backend_name())
            .field("backend_type", &self.backend_type)
            .finish()
    }
}

impl TerminalManager {
    /// Create a new terminal manager with PTY backend (default)
    pub fn new(log_dir: PathBuf) -> Self {
        Self::with_backend(log_dir, TerminalBackendType::Pty, MAX_CONCURRENT_SESSIONS)
    }

    /// Create a terminal manager with specified backend type
    pub fn with_backend(
        log_dir: PathBuf,
        backend_type: TerminalBackendType,
        max_sessions: usize,
    ) -> Self {
        let backend: Box<dyn TerminalBackend> = match backend_type {
            TerminalBackendType::Pty => {
                Box::new(PtyBackend::new(log_dir.clone(), max_sessions))
            }
            TerminalBackendType::Tmux => {
                match TmuxBackend::new(log_dir.clone(), max_sessions) {
                    Ok(backend) => Box::new(backend),
                    Err(e) => {
                        eprintln!("⚠️  Failed to initialize tmux backend: {}", e);
                        eprintln!("⚠️  Falling back to PTY backend");
                        Box::new(PtyBackend::new(log_dir.clone(), max_sessions))
                    }
                }
            }
        };

        eprintln!("Terminal backend: {}", backend.backend_name());

        Self {
            backend,
            backend_type,
        }
    }

    /// Get the backend type being used
    pub fn backend_type(&self) -> TerminalBackendType {
        self.backend_type
    }

    /// Create a new terminal session
    /// Returns session ID as string
    pub async fn create_session(
        &mut self,
        id: String,
        command: String,
        working_dir: Option<String>,
        cols: u16,
        rows: u16,
    ) -> Result<String> {
        self.backend
            .launch_session(id, command, rows, cols, working_dir)
            .await
    }

    /// Send input to a session
    pub async fn send_input(&mut self, session_id: &str, input: &str) -> Result<()> {
        self.backend.send_keys(session_id, input).await
    }

    /// Get current screen content from a session
    pub async fn get_screen(&self, session_id: &str, include_colors: bool, include_cursor: bool) -> Result<String> {
        self.backend.get_screen(session_id, include_colors, include_cursor).await
    }

    /// List all active sessions
    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        self.backend.list_sessions().await
    }

    /// Kill a session
    pub async fn kill_session(&mut self, session_id: &str) -> Result<()> {
        self.backend.kill_session(session_id).await
    }

    /// Get cursor position in a session
    pub async fn get_cursor_position(
        &self,
        session_id: &str,
    ) -> Result<(usize, usize)> {
        let pos = self.backend.get_cursor_position(session_id).await?;
        Ok((pos.row, pos.col))
    }

    /// Resize a session
    pub async fn resize_session(&mut self, session_id: &str, rows: u16, cols: u16) -> Result<()> {
        self.backend.resize_session(session_id, rows, cols).await
    }

    /// Get scrollback buffer from a session
    pub async fn get_scrollback(&self, session_id: &str, lines: usize) -> Result<Option<String>> {
        self.backend.get_scrollback(session_id, lines).await
    }

    /// Set scrollback buffer size for a session
    pub async fn set_scrollback(&mut self, session_id: &str, lines: usize) -> Result<()> {
        self.backend.set_scrollback(session_id, lines).await
    }

    /// Start capturing session output to file
    pub async fn capture_start(&mut self, session_id: &str, output_file: String) -> Result<()> {
        self.backend
            .capture_start(session_id, output_file)
            .await
    }

    /// Stop capturing session output
    /// Returns (capture_file_path, bytes_captured, duration_seconds)
    pub async fn capture_stop(&mut self, session_id: &str) -> Result<(String, usize, f64)> {
        self.backend.capture_stop(session_id).await
    }

    /// Check if a session exists
    pub async fn session_exists(&self, session_id: &str) -> bool {
        self.backend.session_exists(session_id).await
    }
}
