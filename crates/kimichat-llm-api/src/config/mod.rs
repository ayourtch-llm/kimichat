use std::env;
use std::sync::Arc;

use crate::client::{LlmClient, anthropic::AnthropicLlmClient, groq::GroqLlmClient, llama_cpp::LlamaCppClient};

pub mod factory;
pub use factory::ClientFactory;

/// Backend type for LLM models
#[derive(Debug, Clone, PartialEq)]
pub enum BackendType {
    Groq,
    Anthropic,
    Llama,
    OpenAI,
}

impl BackendType {
    /// Parse backend type from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "groq" => Some(Self::Groq),
            "anthropic" | "claude" => Some(Self::Anthropic),
            "llama" | "llamacpp" | "llama.cpp" | "llama-cpp" => Some(Self::Llama),
            "openai" => Some(Self::OpenAI),
            _ => None,
        }
    }

    /// Get string representation
    pub fn as_str(&self) -> &str {
        match self {
            Self::Groq => "groq",
            Self::Anthropic => "anthropic",
            Self::Llama => "llama",
            Self::OpenAI => "openai",
        }
    }
}

/// Default Groq API URL
pub const GROQ_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";

/// Default Anthropic API URL
pub const ANTHROPIC_API_URL: &str = "https://api.anthropic.com";

/// Default OpenAI API URL
pub const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";

/// Normalize API URL by ensuring it has the correct path for OpenAI-compatible endpoints
pub fn normalize_api_url(url: &str) -> String {
    // If URL already contains a path with "completions", use it as-is
    if url.contains("/completions") || url.contains("/chat") {
        return url.to_string();
    }

    // If URL ends with a slash, append path without leading slash
    if url.ends_with('/') {
        format!("{}v1/chat/completions", url)
    } else {
        // Append the standard OpenAI-compatible path
        format!("{}/v1/chat/completions", url)
    }
}
