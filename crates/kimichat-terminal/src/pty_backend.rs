/// PTY-based terminal backend (internal implementation)
use super::backend::{TerminalBackend, SessionInfo, CursorPosition};
use super::session::{TerminalSession, SessionId, SessionStatus};
use anyhow::{Result, bail};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// PTY backend using internal PTY implementation
pub struct PtyBackend {
    sessions: HashMap<String, Arc<Mutex<TerminalSession>>>,
    session_id_map: HashMap<String, SessionId>,  // String ID -> numeric ID mapping
    capture_files: HashMap<String, PathBuf>,  // Track capture file paths per session
    next_session_id: SessionId,
    log_dir: PathBuf,
    max_sessions: usize,
}

impl PtyBackend {
    /// Create a new PTY backend
    pub fn new(log_dir: PathBuf, max_sessions: usize) -> Self {
        Self {
            sessions: HashMap::new(),
            session_id_map: HashMap::new(),
            capture_files: HashMap::new(),
            next_session_id: 1,
            log_dir,
            max_sessions,
        }
    }

    /// Get the next available session ID
    fn next_id(&mut self) -> SessionId {
        let id = self.next_session_id;
        self.next_session_id += 1;
        id
    }

    /// Get session by string ID
    fn get_session(&self, session_id: &str) -> Result<Arc<Mutex<TerminalSession>>> {
        self.sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))
    }
}

#[async_trait]
impl TerminalBackend for PtyBackend {
    async fn launch_session(
        &mut self,
        id: String,
        command: String,
        rows: u16,
        cols: u16,
        working_dir: Option<String>,
    ) -> Result<String> {
        // Check session limit
        if self.sessions.len() >= self.max_sessions {
            bail!(
                "Maximum concurrent sessions ({}) reached. Please kill some sessions before creating new ones.",
                self.max_sessions
            );
        }

        let numeric_id = self.next_id();
        let working_dir_path = working_dir.map(PathBuf::from);

        // Create the session using existing TerminalSession logic
        let session = TerminalSession::new(
            numeric_id,
            Some(command),
            working_dir_path,
            Some(cols),
            Some(rows),
            self.log_dir.clone(),
        )?;

        let session_arc = Arc::new(Mutex::new(session));

        // Start background reader thread
        TerminalSession::start_background_reader(Arc::clone(&session_arc))?;

        // Store session with string ID
        self.sessions.insert(id.clone(), session_arc);
        self.session_id_map.insert(id.clone(), numeric_id);

        Ok(id)
    }

    async fn send_keys(&mut self, session_id: &str, keys: &str) -> Result<()> {
        let session = self.get_session(session_id)?;
        let mut session = session.lock().unwrap();
        // Send keys without special key handling (caller handles \n explicitly)
        session.send_keys(keys, false)
    }

    async fn get_screen(&self, session_id: &str, include_colors: bool, include_cursor: bool) -> Result<String> {
        let session = self.get_session(session_id)?;
        let session = session.lock().unwrap();
        session.get_screen(include_colors, include_cursor)
    }

    async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let mut sessions = Vec::new();
        for (string_id, session) in &self.sessions {
            let session = session.lock().unwrap();
            let metadata = session.metadata();
            sessions.push(SessionInfo {
                id: string_id.clone(),
                command: metadata.command.clone(),
                created_at: metadata.created_at.into(),
                rows: metadata.size.1,
                cols: metadata.size.0,
                working_dir: Some(metadata.working_dir.display().to_string()),
                status: format!("{:?}", metadata.status),
            });
        }
        Ok(sessions)
    }

    async fn kill_session(&mut self, session_id: &str) -> Result<()> {
        if let Some(session) = self.sessions.remove(session_id) {
            self.session_id_map.remove(session_id);
            let mut session = session.lock().unwrap();
            session.kill()?;
            Ok(())
        } else {
            bail!("Session '{}' not found", session_id)
        }
    }

    async fn get_cursor_position(&self, session_id: &str) -> Result<CursorPosition> {
        let session = self.get_session(session_id)?;
        let session = session.lock().unwrap();
        let (col, row) = session.get_cursor();
        Ok(CursorPosition {
            row: row as usize,
            col: col as usize,
        })
    }

    async fn resize_session(&mut self, session_id: &str, rows: u16, cols: u16) -> Result<()> {
        let session = self.get_session(session_id)?;
        let mut session = session.lock().unwrap();
        session.resize(cols, rows)
    }

    async fn get_scrollback(&self, session_id: &str, _lines: usize) -> Result<Option<String>> {
        // For now, scrollback is included in screen contents
        // TODO: Implement separate scrollback retrieval if needed
        let session = self.get_session(session_id)?;
        let session = session.lock().unwrap();
        Ok(Some(session.get_screen(false, false)?))
    }

    async fn set_scrollback(&mut self, session_id: &str, lines: usize) -> Result<()> {
        let session = self.get_session(session_id)?;
        let mut session = session.lock().unwrap();
        session.set_scrollback(lines)
    }

    async fn capture_start(&mut self, session_id: &str, output_file: String) -> Result<()> {
        let session = self.get_session(session_id)?;
        let mut session = session.lock().unwrap();
        // Note: The existing API doesn't take a file path, it generates one
        // For backend API compatibility, we ignore the provided path and use the generated one
        let capture_path = session.start_capture()?;
        self.capture_files.insert(session_id.to_string(), capture_path);
        Ok(())
    }

    async fn capture_stop(&mut self, session_id: &str) -> Result<(String, usize, f64)> {
        let session = self.get_session(session_id)?;
        let mut session = session.lock().unwrap();
        let (path, bytes, duration) = session.stop_capture()?;
        self.capture_files.remove(session_id);
        Ok((path.display().to_string(), bytes as usize, duration))
    }

    async fn session_exists(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    fn backend_name(&self) -> &str {
        "pty"
    }
}

impl Drop for PtyBackend {
    fn drop(&mut self) {
        // Kill all sessions on drop (matching existing behavior)
        for (_, session) in self.sessions.drain() {
            let mut session = session.lock().unwrap();
            let _ = session.kill();
        }
    }
}
