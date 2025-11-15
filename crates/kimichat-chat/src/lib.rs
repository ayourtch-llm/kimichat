//! Conversation management for kimichat
//!
//! This crate provides chat history management, message summarization,
//! tool call parsing and validation, and the main chat loop.

pub mod chat;
pub mod tools_execution;

// Logging
mod conversation_logger;
mod request_logger;
pub use conversation_logger::ConversationLogger;
pub use request_logger::{log_request, log_response, log_stream_chunk};

// API calling functions (moved from kimichat-api)
mod api_client;
mod api_streaming;
pub use api_client::{call_api, call_api_with_llm_client};
pub use api_streaming::{call_api_streaming, call_api_streaming_with_llm_client};

// Re-export commonly used types
pub use chat::history::{summarize_and_trim_history, safe_truncate};
pub use chat::session::chat;
pub use chat::state::{save_state, load_state};
pub use tools_execution::parsing::parse_xml_tool_calls;
pub use tools_execution::validation::{validate_and_fix_tool_calls_in_place, repair_tool_call_with_model};
