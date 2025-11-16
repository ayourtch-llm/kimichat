// Logging module - conversation and request logging
pub mod conversation_logger;
pub mod request_logger;

// Re-export ConversationLogger for backward compatibility
pub use conversation_logger::ConversationLogger;

// Re-export request logging functions
pub use request_logger::{
    log_request,
    log_request_to_file,
    log_response,
    log_stream_chunk,
};

/// Safely truncate a string to a maximum number of characters
pub fn safe_truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect()
    }
}
