// Logging module - conversation and request logging
pub mod conversation_logger;
pub mod request_logger;

use std::path::PathBuf;
use anyhow::{Result, Context};

// Re-export ConversationLogger for backward compatibility
pub use conversation_logger::ConversationLogger;

// Re-export request logging functions
pub use request_logger::{
    log_request,
    log_request_to_file,
    log_response,
    log_response_to_file,
    log_raw_response_to_file,
    log_stream_chunk,
};

/// Safely truncate a string to a maximum number of characters
pub fn safe_truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        // Reserve space for "..." suffix
        let trunc_chars = if max_chars >= 3 { max_chars - 3 } else { 0 };
        format!("{}...", s.chars().take(trunc_chars).collect::<String>())
    }
}

/// Get or create the base okaychat directory (~/.okaychat)
/// This is shared between logging and model caching
pub fn get_okaychat_dir() -> Result<PathBuf> {
    let home_dir = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("Failed to get home directory")?;

    let okaychat_dir = PathBuf::from(home_dir)
        .join(".okaychat");

    // Create directory if it doesn't exist
    if !okaychat_dir.exists() {
        std::fs::create_dir_all(&okaychat_dir)
            .context("Failed to create okaychat directory")?;
    }

    Ok(okaychat_dir)
}

/// Get or create the logs directory (~/.okaychat/logs)
pub fn get_logs_dir() -> Result<PathBuf> {
    let logs_dir = get_okaychat_dir()?.join("logs");
    
    // Create directory if it doesn't exist
    if !logs_dir.exists() {
        std::fs::create_dir_all(&logs_dir)
            .context("Failed to create logs directory")?;
    }

    Ok(logs_dir)
}
