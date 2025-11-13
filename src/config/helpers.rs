use std::env;

use crate::config::{ClientConfig, GROQ_API_URL, normalize_api_url};
use crate::models::ModelType;

/// Generate system prompt based on current model
pub fn get_system_prompt() -> String {
    "You are an AI assistant with access to file operations and model switching capabilities. \
    The system supports multiple models that can be switched during the conversation:\n\
    - grn_model (GrnModel): **Preferred for cost efficiency** - significantly cheaper than BluModel while providing good performance for most tasks\n\
    - blu_model (BluModel): Use when GrnModel struggles or when you need faster responses\n\n\
    IMPORTANT: You have been provided with a set of tools (functions) that you can use. \
    Only use the tools that are provided to you - do not make up tool names or attempt to use tools that are not available. \
    When making multiple file edits, use plan_edits to create a complete plan, then apply_edit_plan to execute all changes atomically. \
    This prevents issues where you lose track of file state between sequential edits.\n\n\
    Model switches may happen automatically during the conversation based on tool usage and errors. \
    The currently active model will be indicated in system messages as the conversation progresses.".to_string()
}

/// Get the API URL to use based on the current model and client configuration
pub fn get_api_url(client_config: &ClientConfig, model: &ModelType) -> String {
    let url = match model {
        ModelType::BluModel => {
            client_config
                .api_url_blu_model
                .as_ref()
                .map(|s| s.clone())
                .unwrap_or_else(|| GROQ_API_URL.to_string())
        }
        ModelType::GrnModel => {
            client_config
                .api_url_grn_model
                .as_ref()
                .map(|s| s.clone())
                .unwrap_or_else(|| GROQ_API_URL.to_string())
        }
        ModelType::AnthropicModel => {
            // For Anthropic, default to the official API or look for Anthropic-specific URLs
            env::var("ANTHROPIC_BASE_URL")
                .or_else(|_| env::var("ANTHROPIC_BASE_URL_BLU"))
                .or_else(|_| env::var("ANTHROPIC_BASE_URL_GRN"))
                .unwrap_or_else(|_| "https://api.anthropic.com".to_string())
        }
        ModelType::Custom(_) => {
            // For custom models, default to the first available override or Groq
            client_config
                .api_url_blu_model
                .as_ref()
                .or(client_config.api_url_grn_model.as_ref())
                .map(|s| s.clone())
                .unwrap_or_else(|| GROQ_API_URL.to_string())
        }
    };

    // Normalize the URL to ensure it has the correct path
    normalize_api_url(&url)
}

/// Get the appropriate API key for a given model based on configuration
pub fn get_api_key(client_config: &ClientConfig, api_key: &str, model: &ModelType) -> String {
    match model {
        ModelType::BluModel => {
            client_config
                .api_key_blu_model
                .as_ref()
                .map(|s| s.clone())
                .unwrap_or_else(|| api_key.to_string())
        }
        ModelType::GrnModel => {
            client_config
                .api_key_grn_model
                .as_ref()
                .map(|s| s.clone())
                .unwrap_or_else(|| api_key.to_string())
        }
        ModelType::AnthropicModel => {
            // For Anthropic, look for Anthropic-specific keys first
            env::var("ANTHROPIC_API_KEY")
                .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
                .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN_BLU"))
                .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN_GRN"))
                .unwrap_or_else(|_| api_key.to_string())
        }
        ModelType::Custom(_) => {
            // For custom models, default to the first available override or default key
            client_config
                .api_key_blu_model
                .as_ref()
                .or(client_config.api_key_grn_model.as_ref())
                .map(|s| s.clone())
                .unwrap_or_else(|| api_key.to_string())
        }
    }
}
