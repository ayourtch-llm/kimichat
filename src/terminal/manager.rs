use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use anyhow::{Result, bail};

use super::session::{TerminalSession, SessionId};
use super::{MAX_CONCURRENT_SESSIONS};

/// Manages all terminal sessions globally
pub struct TerminalManager {
    sessions: HashMap<SessionId, Arc<Mutex<TerminalSession>>>,
    next_id: u32,
    log_dir: PathBuf,
    max_sessions: usize,
}

impl std::fmt::Debug for TerminalManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalManager")
            .field("next_id", &self.next_id)
            .field("log_dir", &self.log_dir)
            .field("max_sessions", &self.max_sessions)
            .field("session_count", &self.sessions.len())
            .finish()
    }
}

impl TerminalManager {
    /// Create a new terminal manager
    pub fn new(log_dir: PathBuf) -> Self {
        Self {
            sessions: HashMap::new(),
            next_id: 1,
            log_dir,
            max_sessions: MAX_CONCURRENT_SESSIONS,
        }
    }

    /// Get the next available session ID
    fn next_session_id(&mut self) -> SessionId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Create a new terminal session
    pub fn create_session(
        &mut self,
        command: Option<String>,
        working_dir: Option<PathBuf>,
        cols: Option<u16>,
        rows: Option<u16>,
    ) -> Result<SessionId> {
        // Check session limit
        if self.sessions.len() >= self.max_sessions {
            bail!(
                "Maximum concurrent sessions ({}) reached. Please kill some sessions before creating new ones.",
                self.max_sessions
            );
        }

        let session_id = self.next_session_id();
        let session = TerminalSession::new(
            session_id,
            command,
            working_dir,
            cols,
            rows,
            self.log_dir.clone(),
        )?;

        let session_arc = Arc::new(Mutex::new(session));

        // Start background reader thread to continuously update screen buffer
        TerminalSession::start_background_reader(Arc::clone(&session_arc))?;

        self.sessions.insert(session_id, session_arc);
        Ok(session_id)
    }

    /// Get a session by ID
    pub fn get_session(&self, session_id: SessionId) -> Result<Arc<Mutex<TerminalSession>>> {
        self.sessions
            .get(&session_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Session {} not found", session_id))
    }

    /// List all active sessions
    pub fn list_sessions(&self) -> Vec<SessionId> {
        self.sessions.keys().copied().collect()
    }

    /// Kill a session
    pub fn kill_session(&mut self, session_id: SessionId) -> Result<()> {
        if let Some(session) = self.sessions.remove(&session_id) {
            let mut session = session.lock().unwrap();
            session.kill()?;
            Ok(())
        } else {
            bail!("Session {} not found", session_id)
        }
    }

    /// Clean up finished sessions
    pub fn cleanup_finished(&mut self) {
        self.sessions.retain(|_, session| {
            let session = session.lock().unwrap();
            !session.is_finished()
        });
    }
}

impl Drop for TerminalManager {
    fn drop(&mut self) {
        // Kill all sessions on drop
        for (_, session) in self.sessions.drain() {
            let mut session = session.lock().unwrap();
            let _ = session.kill();
        }
    }
}
