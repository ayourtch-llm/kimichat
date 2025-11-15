use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use uuid::Uuid;

use crate::config::ClientConfig;
use crate::models::Message;
use crate::policy::PolicyManager;
use crate::web::protocol::{ServerMessage, SessionConfig, SessionInfo};
use crate::KimiChat;

/// Pending tool confirmation
pub struct PendingConfirmation {
    pub tool_name: String,
    pub tool_args: String,
    pub responder: oneshot::Sender<bool>,
}

pub type SessionId = Uuid;

/// Type of session
#[derive(Debug, Clone, PartialEq)]
pub enum SessionType {
    Web,           // Standalone web session
    Tui,           // TUI session (can be attached to)
    Shared,        // Multi-client session
}

impl SessionType {
    pub fn as_str(&self) -> &str {
        match self {
            SessionType::Web => "Web",
            SessionType::Tui => "Tui",
            SessionType::Shared => "Shared",
        }
    }
}

/// A client connection to a session
#[derive(Debug)]
pub struct ClientConnection {
    pub client_id: Uuid,
    pub ws_sender: mpsc::UnboundedSender<ServerMessage>,
    pub joined_at: DateTime<Utc>,
}

/// A chat session
pub struct Session {
    pub id: SessionId,
    pub session_type: SessionType,
    pub kimichat: Arc<tokio::sync::Mutex<KimiChat>>,
    pub clients: Arc<RwLock<Vec<ClientConnection>>>,
    pub created_at: DateTime<Utc>,
    pub last_activity: Arc<tokio::sync::Mutex<DateTime<Utc>>>,
    pub pending_confirmations: Arc<RwLock<HashMap<String, PendingConfirmation>>>,
}

impl Session {
    pub fn new(
        id: SessionId,
        session_type: SessionType,
        kimichat: KimiChat,
    ) -> Self {
        Self {
            id,
            session_type,
            kimichat: Arc::new(tokio::sync::Mutex::new(kimichat)),
            clients: Arc::new(RwLock::new(Vec::new())),
            created_at: Utc::now(),
            last_activity: Arc::new(tokio::sync::Mutex::new(Utc::now())),
            pending_confirmations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a pending confirmation and return a receiver to wait on
    pub async fn register_confirmation(
        &self,
        tool_call_id: String,
        tool_name: String,
        tool_args: String,
    ) -> oneshot::Receiver<bool> {
        let (tx, rx) = oneshot::channel();
        let confirmation = PendingConfirmation {
            tool_name,
            tool_args,
            responder: tx,
        };
        self.pending_confirmations.write().await.insert(tool_call_id, confirmation);
        rx
    }

    /// Respond to a pending confirmation
    pub async fn respond_to_confirmation(&self, tool_call_id: &str, confirmed: bool) -> bool {
        if let Some(pending) = self.pending_confirmations.write().await.remove(tool_call_id) {
            // Send response (ignore error if receiver dropped)
            let _ = pending.responder.send(confirmed);
            true
        } else {
            false
        }
    }

    pub async fn add_client(&self, client_id: Uuid, ws_sender: mpsc::UnboundedSender<ServerMessage>) {
        let conn = ClientConnection {
            client_id,
            ws_sender,
            joined_at: Utc::now(),
        };
        self.clients.write().await.push(conn);
        self.update_activity().await;
    }

    pub async fn remove_client(&self, client_id: Uuid) {
        self.clients.write().await.retain(|c| c.client_id != client_id);
        self.update_activity().await;
    }

    pub async fn broadcast(&self, message: ServerMessage) {
        let clients = self.clients.read().await;
        for client in clients.iter() {
            let _ = client.ws_sender.send(message.clone());
        }
    }

    pub async fn send_to_client(&self, client_id: Uuid, message: ServerMessage) {
        let clients = self.clients.read().await;
        if let Some(client) = clients.iter().find(|c| c.client_id == client_id) {
            let _ = client.ws_sender.send(message);
        }
    }

    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }

    pub async fn update_activity(&self) {
        *self.last_activity.lock().await = Utc::now();
    }

    pub async fn get_info(&self) -> SessionInfo {
        let kimichat = self.kimichat.lock().await;
        let clients = self.clients.read().await;
        let last_activity = *self.last_activity.lock().await;

        SessionInfo {
            id: self.id,
            session_type: self.session_type.as_str().to_string(),
            created_at: self.created_at.to_rfc3339(),
            last_activity: last_activity.to_rfc3339(),
            active_clients: clients.len(),
            message_count: kimichat.messages.len(),
            current_model: kimichat.current_model.display_name(),
            attachable: self.session_type == SessionType::Tui || self.session_type == SessionType::Shared,
        }
    }
}

/// Manages all active sessions
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<SessionId, Arc<Session>>>>,
    work_dir: PathBuf,
    client_config: ClientConfig,
    policy_manager: PolicyManager,
}

impl SessionManager {
    pub fn new(
        work_dir: PathBuf,
        client_config: ClientConfig,
        policy_manager: PolicyManager,
    ) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            work_dir,
            client_config,
            policy_manager,
        }
    }

    /// Create a new web session
    pub async fn create_session(&self, config: SessionConfig) -> Result<SessionId> {
        let session_id = Uuid::new_v4();

        // Determine which model to use
        let model_str = config.model.as_deref().unwrap_or("grn_model");
        let model = crate::models::ModelType::from_str(model_str);

        // Create KimiChat instance
        let mut kimichat = KimiChat::new_with_config(
            self.client_config.clone(),
            self.work_dir.clone(),
            config.agents_enabled,
            self.policy_manager.clone(),
            config.stream_responses,
            false, // verbose
            crate::terminal::TerminalBackendType::Pty,
        );

        kimichat.current_model = model;
        kimichat.non_interactive = true; // Web sessions should not prompt for input

        // Create session
        let session = Arc::new(Session::new(
            session_id,
            SessionType::Web,
            kimichat,
        ));

        // Store session
        self.sessions.write().await.insert(session_id, session);

        Ok(session_id)
    }

    /// Get a session by ID
    pub async fn get_session(&self, session_id: &SessionId) -> Option<Arc<Session>> {
        self.sessions.read().await.get(session_id).cloned()
    }

    /// List all active sessions
    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.read().await;
        let mut infos = Vec::new();

        for session in sessions.values() {
            infos.push(session.get_info().await);
        }

        // Sort by last activity (most recent first)
        infos.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));

        infos
    }

    /// Remove a session
    pub async fn remove_session(&self, session_id: &SessionId) -> Result<()> {
        self.sessions.write().await.remove(session_id);
        Ok(())
    }

    /// Register a TUI session (for attachment)
    pub async fn register_tui_session(
        &self,
        session_id: SessionId,
        kimichat: KimiChat,
    ) -> Result<Arc<Session>> {
        let session = Arc::new(Session::new(
            session_id,
            SessionType::Tui,
            kimichat,
        ));

        self.sessions.write().await.insert(session_id, session.clone());

        Ok(session)
    }

    /// Clean up inactive sessions (future enhancement)
    pub async fn cleanup_inactive(&self, _timeout_seconds: i64) -> usize {
        // TODO: Implement session timeout and cleanup
        0
    }
}
