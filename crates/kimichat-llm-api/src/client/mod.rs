use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};

pub mod anthropic;
pub mod groq;
pub mod llama_cpp;

/// Chat message structure (OpenAI-compatible format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

/// Tool call structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function: FunctionCall,
}

/// Function call structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Streaming chunk for LLM responses
#[derive(Debug, Clone)]
pub struct StreamingChunk {
    pub content: String,
    pub delta: String,
    pub finish_reason: Option<String>,
}

/// LLM client trait - unified interface for all LLM providers
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Chat with tools support (non-streaming)
    async fn chat(&self, messages: Vec<ChatMessage>, tools: Vec<ToolDefinition>) -> Result<LlmResponse>;

    /// Simple chat completion without tools
    async fn chat_completion(&self, messages: &[ChatMessage]) -> Result<String>;

    /// Streaming chat completion - returns a stream of chunks
    async fn chat_streaming(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
    ) -> Result<Box<dyn Stream<Item = Result<StreamingChunk>> + Send + Unpin>> {
        // Default implementation falls back to non-streaming
        Err(anyhow::anyhow!("Streaming not implemented for this client"))
    }
}

/// LLM response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub message: ChatMessage,
    pub usage: Option<TokenUsage>,
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}
