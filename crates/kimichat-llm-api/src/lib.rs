//! # kimichat-llm-api
//!
//! A unified interface for interacting with multiple LLM providers including:
//! - Anthropic (Claude)
//! - Groq
//! - OpenAI
//! - llama.cpp (self-hosted)
//!
//! ## Features
//!
//! - **Unified Interface**: Single `LlmClient` trait for all providers
//! - **Format Translation**: Automatic translation between provider-specific formats
//! - **Streaming Support**: Both streaming and non-streaming APIs
//! - **Flexible Configuration**: Environment variables or programmatic configuration
//! - **Provider Auto-detection**: Automatically detect backend from URL or environment
//!
//! ## Example
//!
//! ```rust,no_run
//! use kimichat_llm_api::{ClientFactory, BackendType};
//! use kimichat_llm_api::client::{LlmClient, ChatMessage};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create a client for Anthropic's Claude
//!     let client = ClientFactory::create(
//!         BackendType::Anthropic,
//!         Some("your-api-key".to_string()),
//!         "claude-3-5-sonnet-20241022".to_string(),
//!         None,
//!         Some("my-agent".to_string()),
//!     );
//!
//!     // Make a chat request
//!     let messages = vec![
//!         ChatMessage {
//!             role: "user".to_string(),
//!             content: "Hello!".to_string(),
//!             tool_calls: None,
//!             tool_call_id: None,
//!             name: None,
//!             reasoning: None,
//!         }
//!     ];
//!
//!     let response = client.chat(messages, vec![]).await?;
//!     println!("Response: {}", response.message.content);
//!
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod config;

#[cfg(test)]
mod tests;

// Re-export commonly used types
pub use client::{
    LlmClient,
    ChatMessage,
    ToolCall,
    FunctionCall,
    LlmResponse,
    TokenUsage,
    ToolDefinition,
    StreamingChunk,
};

pub use config::{
    BackendType,
    ClientFactory,
    GROQ_API_URL,
    ANTHROPIC_API_URL,
    OPENAI_API_URL,
    normalize_api_url,
    get_default_url_for_backend,
};
