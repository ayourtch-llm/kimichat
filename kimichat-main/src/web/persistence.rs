use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::chat::state::ChatState;
use crate::web::session_manager::SessionId;
use kimichat_models::ModelType;

/// Persistent session data stored on disk
#[derive(Debug, Serialize, Deserialize)]
pub struct PersistentSession {
    pub session_id: SessionId,
    pub title: Option<String>,
    pub chat_state: ChatState,
    pub created_at: String,
    pub last_activity: String,
}

/// Session persistence manager
pub struct SessionPersistence {
    sessions_dir: PathBuf,
}

impl SessionPersistence {
    /// Create a new session persistence manager
    pub fn new<P: AsRef<Path>>(sessions_dir: P) -> Result<Self> {
        let sessions_dir = Self::expand_tilde(sessions_dir.as_ref())?;

        // Create directory if it doesn't exist
        if !sessions_dir.exists() {
            fs::create_dir_all(&sessions_dir)
                .with_context(|| format!("Failed to create sessions directory: {}", sessions_dir.display()))?;
        }

        Ok(Self { sessions_dir })
    }

    /// Expand ~ to home directory
    fn expand_tilde(path: &Path) -> Result<PathBuf> {
        let path_str = path.to_string_lossy();
        if path_str.starts_with("~/") {
            let home = std::env::var("HOME")
                .context("HOME environment variable not set")?;
            Ok(PathBuf::from(home).join(&path_str[2..]))
        } else if path_str == "~" {
            let home = std::env::var("HOME")
                .context("HOME environment variable not set")?;
            Ok(PathBuf::from(home))
        } else {
            Ok(path.to_path_buf())
        }
    }

    /// Get the file path for a session
    fn get_session_path(&self, session_id: &SessionId) -> PathBuf {
        self.sessions_dir.join(format!("{}.json", session_id))
    }

    /// Save a session to disk
    pub fn save_session(&self, persistent_session: &PersistentSession) -> Result<()> {
        let path = self.get_session_path(&persistent_session.session_id);
        let json = serde_json::to_string_pretty(&persistent_session)
            .context("Failed to serialize session")?;

        fs::write(&path, json)
            .with_context(|| format!("Failed to write session to {}", path.display()))?;

        Ok(())
    }

    /// Load a session from disk
    pub fn load_session(&self, session_id: &SessionId) -> Result<PersistentSession> {
        let path = self.get_session_path(session_id);
        let json = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read session from {}", path.display()))?;

        let session: PersistentSession = serde_json::from_str(&json)
            .context("Failed to deserialize session")?;

        Ok(session)
    }

    /// List all saved session IDs
    pub fn list_sessions(&self) -> Result<Vec<SessionId>> {
        let mut sessions = Vec::new();

        if !self.sessions_dir.exists() {
            return Ok(sessions);
        }

        for entry in fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(file_name) = path.file_stem() {
                    if let Ok(session_id) = Uuid::parse_str(&file_name.to_string_lossy()) {
                        sessions.push(session_id);
                    }
                }
            }
        }

        Ok(sessions)
    }

    /// Delete a session from disk
    pub fn delete_session(&self, session_id: &SessionId) -> Result<()> {
        let path = self.get_session_path(session_id);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to delete session file: {}", path.display()))?;
        }
        Ok(())
    }

    /// Check if a session exists on disk
    pub fn session_exists(&self, session_id: &SessionId) -> bool {
        self.get_session_path(session_id).exists()
    }
}
