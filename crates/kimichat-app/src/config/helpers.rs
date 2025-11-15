use std::env;
use std::sync::Arc;

use crate::config::{ClientConfig, GROQ_API_URL, normalize_api_url, BackendType};
use crate::models::ModelType;
use crate::agents::{LlmClient, AnthropicLlmClient, GroqLlmClient, LlamaCppClient};
use colored::Colorize;

/// Generate system prompt based on current model
pub fn get_system_prompt() -> String {
    "You are an AI assistant with access to file operations and model switching capabilities. \
    The system supports multiple models that can be switched during the conversation:\n\
    - grn_model (GrnModel): **Preferred for cost efficiency** - significantly cheaper than BluModel while providing good performance for most tasks\n\
    - blu_model (BluModel): Use when GrnModel struggles or when you need faster responses\n\
    - red_model (RedModel): Use for specialized tasks requiring different capabilities\n\n\
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
        ModelType::RedModel => {
            client_config
                .api_url_red_model
                .as_ref()
                .map(|s| s.clone())
                .unwrap_or_else(|| GROQ_API_URL.to_string())
        }
        ModelType::AnthropicModel => {
            // For Anthropic, default to the official API or look for Anthropic-specific URLs
            env::var("ANTHROPIC_BASE_URL")
                .or_else(|_| env::var("ANTHROPIC_BASE_URL_BLU"))
                .or_else(|_| env::var("ANTHROPIC_BASE_URL_GRN"))
                .or_else(|_| env::var("ANTHROPIC_BASE_URL_RED"))
                .unwrap_or_else(|_| "https://api.anthropic.com".to_string())
        }
        ModelType::Custom(_) => {
            // For custom models, default to the first available override or Groq
            client_config
                .api_url_blu_model
                .as_ref()
                .or(client_config.api_url_grn_model.as_ref())
                .or(client_config.api_url_red_model.as_ref())
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
        ModelType::RedModel => {
            client_config
                .api_key_red_model
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
                .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN_RED"))
                .unwrap_or_else(|_| api_key.to_string())
        }
        ModelType::Custom(_) => {
            // For custom models, default to the first available override or default key
            client_config
                .api_key_blu_model
                .as_ref()
                .or(client_config.api_key_grn_model.as_ref())
                .or(client_config.api_key_red_model.as_ref())
                .map(|s| s.clone())
                .unwrap_or_else(|| api_key.to_string())
        }
    }
}

/// Read model configuration from KIMICHAT_* environment variables
/// Returns (backend, url, key, model)
pub fn get_model_config_from_env(model_name: &str) -> (Option<BackendType>, Option<String>, Option<String>, Option<String>) {
    let prefix = format!("KIMICHAT_{}", model_name.to_uppercase());

    let backend = env::var(format!("{}_BACKEND", prefix))
        .ok()
        .and_then(|s| BackendType::from_str(&s));

    let url = env::var(format!("{}_URL", prefix)).ok();
    let key = env::var(format!("{}_KEY", prefix)).ok();
    let model = env::var(format!("{}_MODEL", prefix)).ok();

    (backend, url, key, model)
}

/// Create an LLM client for a specific model based on configuration
/// This centralizes the logic for creating clients across all three models (blu, grn, red)
pub fn create_model_client(
    model_name: &str,              // "blu", "grn", or "red"
    backend: Option<BackendType>,
    api_url: Option<String>,
    api_key: Option<String>,
    model_override: Option<String>,
    default_api_key: &str,
) -> Arc<dyn LlmClient> {
    let model_name_upper = model_name.to_uppercase();

    // Get the model string - either from override or default
    let model_str = if let Some(ref override_str) = model_override {
        override_str.clone()
    } else {
        match model_name {
            "blu" => ModelType::BluModel.as_str(),
            "grn" => ModelType::GrnModel.as_str(),
            "red" => ModelType::RedModel.as_str(),
            _ => ModelType::GrnModel.as_str(),
        }
    };

    // Determine backend: explicit > URL detection > env var detection > default (Groq)
    let detected_backend = backend.unwrap_or_else(|| {
        if let Some(ref url) = api_url {
            if url.contains("anthropic") {
                BackendType::Anthropic
            } else {
                BackendType::Llama
            }
        } else if env::var(format!("ANTHROPIC_AUTH_TOKEN_{}", model_name_upper)).is_ok() ||
                  (env::var("ANTHROPIC_AUTH_TOKEN").is_ok() &&
                   model_override.as_ref()
                    .map(|m| m.contains("claude") || m.contains("anthropic"))
                    .unwrap_or(false)) {
            BackendType::Anthropic
        } else {
            BackendType::Groq
        }
    });

    match detected_backend {
        BackendType::Anthropic => {
            let url = api_url.unwrap_or_else(|| "https://api.anthropic.com".to_string());
            let key = api_key
                .or_else(|| env::var(format!("ANTHROPIC_AUTH_TOKEN_{}", model_name_upper)).ok())
                .or_else(|| env::var("ANTHROPIC_AUTH_TOKEN").ok())
                .unwrap_or_default();
            println!("{} Using Anthropic API for '{}_model' at: {}", "🧠".cyan(), model_name, url);
            Arc::new(AnthropicLlmClient::new(
                key,
                model_str,
                url,
                format!("{}_model", model_name)
            ))
        }
        BackendType::Llama => {
            let url = api_url.expect(&format!("llama.cpp backend requires api_url_{}_model", model_name));
            println!("{} Using llama.cpp for '{}_model' at: {}", "🦙".cyan(), model_name, url);
            Arc::new(LlamaCppClient::new(
                url,
                model_str
            ))
        }
        BackendType::Groq => {
            println!("{} Using Groq API for '{}_model'", "🚀".cyan(), model_name);
            Arc::new(GroqLlmClient::new(
                default_api_key.to_string(),
                model_str,
                GROQ_API_URL.to_string(),
                format!("{}_model", model_name)
            ))
        }
        BackendType::OpenAI => {
            let url = api_url.unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string());
            let key = api_key.unwrap_or_else(|| default_api_key.to_string());
            println!("{} Using OpenAI API for '{}_model' at: {}", "🤖".cyan(), model_name, url);
            // Use GroqLlmClient as it's OpenAI-compatible
            Arc::new(GroqLlmClient::new(
                key,
                model_str,
                url,
                format!("{}_model", model_name)
            ))
        }
    }
}
