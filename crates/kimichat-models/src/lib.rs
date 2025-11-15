//! API request and response models for kimichat
//!
//! This crate provides structures for LLM API requests and responses.

use kimichat_types::Message;
use serde::{Deserialize, Serialize};

// ============================================================================
// Request Structures
// ============================================================================

/// Tool definition for chat API
#[derive(Debug, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDef,
}

/// Function definition within a tool
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Chat API request structure
#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    pub tool_choice: String,
    pub tools: Vec<Tool>,
    pub messages: Vec<Message>,
}

// ============================================================================
// Response Structures
// ============================================================================

/// Token usage information from API response
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// Chat API response structure
#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub object: Option<String>,
    #[serde(default)]
    pub created: Option<i64>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// Choice structure within chat response
#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: Message,
    #[serde(default)]
    pub index: Option<i32>,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub logprobs: Option<serde_json::Value>,
}

// ============================================================================
// Streaming Response Structures
// ============================================================================

/// Streaming chunk from chat API
#[derive(Debug, Deserialize)]
pub struct StreamChunk {
    pub choices: Vec<StreamChoice>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub object: Option<String>,
    #[serde(default)]
    pub created: Option<i64>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// Choice structure within streaming chunk
#[derive(Debug, Deserialize)]
pub struct StreamChoice {
    pub delta: StreamDelta,
    #[serde(default)]
    pub index: Option<i32>,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// Delta structure within streaming choice
#[derive(Debug, Deserialize)]
pub struct StreamDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<StreamToolCallDelta>>,
}

/// Tool call delta in streaming response
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StreamToolCallDelta {
    pub index: usize,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "type", default)]
    pub tool_type: Option<String>,
    #[serde(default)]
    pub function: Option<StreamFunctionDelta>,
}

/// Function delta in streaming tool call
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StreamFunctionDelta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}
