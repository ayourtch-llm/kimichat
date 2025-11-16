use std::env;
use std::sync::Arc;

use crate::client::{LlmClient, anthropic::AnthropicLlmClient, groq::GroqLlmClient, llama_cpp::LlamaCppClient};
use crate::config::{BackendType, GROQ_API_URL, ANTHROPIC_API_URL, OPENAI_API_URL};

/// Client factory for creating LLM clients
pub struct ClientFactory;

impl ClientFactory {
    /// Create an LLM client based on the specified backend type
    ///
    /// # Arguments
    /// * `backend` - The backend type to use (Groq, Anthropic, Llama, OpenAI)
    /// * `api_key` - API key for authentication (optional for some backends like llama.cpp)
    /// * `model` - Model name to use
    /// * `api_url` - Optional custom API URL (uses default if None)
    /// * `agent_name` - Optional agent name for logging purposes (defaults to "default")
    ///
    /// # Returns
    /// Arc-wrapped LLM client implementing the LlmClient trait
    pub fn create(
        backend: BackendType,
        api_key: Option<String>,
        model: String,
        api_url: Option<String>,
        agent_name: Option<String>,
    ) -> Arc<dyn LlmClient> {
        let agent_name = agent_name.unwrap_or_else(|| "default".to_string());

        match backend {
            BackendType::Anthropic => {
                let url = api_url.unwrap_or_else(|| ANTHROPIC_API_URL.to_string());
                let key = api_key
                    .or_else(|| env::var("ANTHROPIC_API_KEY").ok())
                    .or_else(|| env::var("ANTHROPIC_AUTH_TOKEN").ok())
                    .unwrap_or_default();

                Arc::new(AnthropicLlmClient::new(key, model, url, agent_name))
            }
            BackendType::Llama => {
                let url = api_url.expect("llama.cpp backend requires api_url to be specified");
                Arc::new(LlamaCppClient::new(url, model))
            }
            BackendType::Groq => {
                let url = api_url.unwrap_or_else(|| GROQ_API_URL.to_string());
                let key = api_key
                    .or_else(|| env::var("GROQ_API_KEY").ok())
                    .unwrap_or_default();

                Arc::new(GroqLlmClient::new(key, model, url, agent_name))
            }
            BackendType::OpenAI => {
                let url = api_url.unwrap_or_else(|| OPENAI_API_URL.to_string());
                let key = api_key
                    .or_else(|| env::var("OPENAI_API_KEY").ok())
                    .unwrap_or_default();

                // OpenAI uses the same client as Groq (OpenAI-compatible)
                Arc::new(GroqLlmClient::new(key, model, url, agent_name))
            }
        }
    }

    /// Create an LLM client with automatic backend detection
    ///
    /// Detects the backend based on:
    /// 1. URL patterns (if URL contains "anthropic", uses Anthropic)
    /// 2. Environment variables (if ANTHROPIC_AUTH_TOKEN is set, uses Anthropic)
    /// 3. Falls back to Groq if no specific backend is detected
    ///
    /// # Arguments
    /// * `api_key` - API key for authentication
    /// * `model` - Model name to use
    /// * `api_url` - Optional custom API URL
    /// * `agent_name` - Optional agent name for logging
    ///
    /// # Returns
    /// Arc-wrapped LLM client implementing the LlmClient trait
    pub fn create_with_auto_detect(
        api_key: Option<String>,
        model: String,
        api_url: Option<String>,
        agent_name: Option<String>,
    ) -> Arc<dyn LlmClient> {
        let backend = if let Some(ref url) = api_url {
            if url.contains("anthropic") {
                BackendType::Anthropic
            } else if url.contains("openai") {
                BackendType::OpenAI
            } else {
                BackendType::Llama
            }
        } else if env::var("ANTHROPIC_AUTH_TOKEN").is_ok() ||
                  env::var("ANTHROPIC_API_KEY").is_ok() {
            BackendType::Anthropic
        } else {
            BackendType::Groq
        };

        Self::create(backend, api_key, model, api_url, agent_name)
    }
}
