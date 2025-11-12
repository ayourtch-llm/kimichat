use serde::{Deserialize, Serialize};
use super::types::Message;

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
