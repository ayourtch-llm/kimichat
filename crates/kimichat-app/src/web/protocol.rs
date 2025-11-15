use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::models::Message;

/// Session ID type
pub type SessionId = Uuid;

/// Configuration for creating a new session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub agents_enabled: bool,
    #[serde(default = "default_stream")]
    pub stream_responses: bool,
}

fn default_stream() -> bool {
    true
}

/// Messages sent from client to server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientMessage {
    // Session management
    CreateSession { config: SessionConfig },
    JoinSession { session_id: SessionId },
    LeaveSession,
    ListSessions,

    // Chat interaction
    SendMessage { content: String },
    ConfirmTool { tool_call_id: String, confirmed: bool },
    CancelExecution,

    // Session control
    SwitchModel { model: String, reason: String },
    SaveState { file_path: String },
    LoadState { file_path: String },

    // Skill system
    InvokeSkill { skill_name: String },
}

/// Messages sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerMessage {
    // Session lifecycle
    SessionCreated {
        session_id: SessionId,
        created_at: String,
    },
    SessionJoined {
        session_id: SessionId,
        session_type: String,
        created_at: String,
        current_model: String,
        history: Vec<Message>,
    },
    SessionList {
        sessions: Vec<SessionInfo>,
    },
    SessionError {
        error: String,
    },

    // Chat responses
    AssistantMessage {
        content: String,
        streaming: bool,
    },
    AssistantMessageChunk {
        chunk: String,
    },
    AssistantMessageComplete,

    // Tool interactions
    ToolCallRequest {
        tool_call_id: String,
        name: String,
        arguments: Value,
        requires_confirmation: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        diff: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        iteration: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_iterations: Option<usize>,
    },
    ToolCallResult {
        tool_call_id: String,
        result: String,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        formatted_result: Option<String>,
    },

    // State updates
    ModelSwitched {
        old_model: String,
        new_model: String,
        reason: String,
    },
    TokenUsage {
        prompt_tokens: usize,
        completion_tokens: usize,
        total_tokens: usize,
        session_total: usize,
    },

    // Progress (multi-agent mode)
    TaskProgress {
        task_id: String,
        agent_name: String,
        status: String,
        progress: f32,
        description: String,
    },
    AgentAssigned {
        agent_name: String,
        task_id: String,
        task_description: String,
    },

    // Errors
    Error {
        message: String,
        recoverable: bool,
    },
}

/// Session information for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    #[serde(rename = "type")]
    pub session_type: String,
    pub created_at: String,
    pub last_activity: String,
    pub active_clients: usize,
    pub message_count: usize,
    pub current_model: String,
    pub attachable: bool,
}
