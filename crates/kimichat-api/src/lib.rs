//! LLM API clients and communication for kimichat
//!
//! This crate provides API communication with various LLM backends.

// Re-export LLM client implementations from kimichat-agents
pub use kimichat_agents::{GroqLlmClient, AnthropicLlmClient, LlamaCppClient, LlmClient};
pub use kimichat_agents::{ChatMessage, LlmResponse, ToolCall, ToolDefinition, TokenUsage, StreamingChunk};

// TODO: client.rs and streaming.rs will be implemented in kimichat-chat
// as they depend on the KimiChat struct and logging functions
