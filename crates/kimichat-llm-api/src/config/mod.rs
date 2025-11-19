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

/// Get the default URL for a given backend type
pub fn get_default_url_for_backend(backend: &BackendType) -> Option<String> {
    match backend {
        BackendType::Anthropic => Some(ANTHROPIC_API_URL.to_string()),
        BackendType::Groq => Some(GROQ_API_URL.to_string()),
        BackendType::OpenAI => Some(OPENAI_API_URL.to_string()),
        BackendType::Llama => None, // Llama.cpp doesn't have a default URL
    }
}

/// Default model names for each color
pub const DEFAULT_BLU_MODEL: &str = "moonshotai/kimi-k2-instruct-0905";
pub const DEFAULT_GRN_MODEL: &str = "openai/gpt-oss-120b";
pub const DEFAULT_RED_MODEL: &str = "meta-llama/llama-3.1-70b-versatile";

/// Default backends for each color
pub const DEFAULT_BLU_BACKEND: BackendType = BackendType::Groq;
pub const DEFAULT_GRN_BACKEND: BackendType = BackendType::Groq;
pub const DEFAULT_RED_BACKEND: BackendType = BackendType::Groq;

/// Default API URLs for each color
pub const DEFAULT_BLU_API_URL: &str = GROQ_API_URL;
pub const DEFAULT_GRN_API_URL: &str = GROQ_API_URL;
pub const DEFAULT_RED_API_URL: &str = GROQ_API_URL;

/// Parse model configuration string in format "model@backend(api_url)" or "model@backend" or "model"
/// Returns (model_name, backend, api_url)
pub fn parse_model_attings(atts: &str) -> (String, Option<BackendType>, Option<String>) {
    // Split by @ to get model and backend part
    let parts: Vec<&str> = atts.split('@').collect();
    let model = parts.first().copied().unwrap_or("");
    let mut backend = None;
    let mut api_url = None;
    if parts.len() > 1 {
        let backend_part = parts[1];
        // Check if backend part contains parentheses for URL
        if let Some(pos) = backend_part.find('(') {
            // Format: backend(url)
            let backend_name = &backend_part[..pos];
            let url = &backend_part[pos + 1..backend_part.len() - 1]; // Remove parentheses
            backend = BackendType::from_str(backend_name);
            api_url = Some(url.to_string());
        } else {
            // Format: backend
            backend = BackendType::from_str(backend_part);
            // For default URLs, we'll determine them based on backend type
        }
    }
    // If no backend specified, use default backend for the color
    if backend.is_none() {
        // In this case, we just return the model and leave backend and URL as None
        // This allows us to handle "model" format where only model name is specified
        // We'll need to determine backend and URL based on context in calling function
    }
    (model.to_string(), backend, api_url)
}

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
