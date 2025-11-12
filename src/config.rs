use anyhow::Result;
use colored::Colorize;
use std::env;
use std::sync::Arc;

use crate::core::ToolRegistry;
use crate::policy::PolicyManager;
use crate::tools::*;
use crate::agents::{
    PlanningCoordinator, AgentFactory, LlmClient,
    AnthropicLlmClient, GroqLlmClient, LlamaCppClient,
};
use crate::models::ModelType;

// Re-export the Groq API URL constant
pub const GROQ_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";

/// Configuration for KimiChat client
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// API key for authentication (Groq default)
    pub api_key: String,
    /// API URL for 'blu_model' - if Some, uses custom backend; if None, uses Groq
    pub api_url_blu_model: Option<String>,
    /// API URL for 'grn_model' - if Some, uses custom backend; if None, uses Groq
    pub api_url_grn_model: Option<String>,
    /// API key for 'blu_model' - if Some, uses this instead of default api_key
    pub api_key_blu_model: Option<String>,
    /// API key for 'grn_model' - if Some, uses this instead of default api_key
    pub api_key_grn_model: Option<String>,
    /// Override for 'blu_model' model name
    pub model_blu_model_override: Option<String>,
    /// Override for 'grn_model' model name
    pub model_grn_model_override: Option<String>,
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

/// Initialize the tool registry with all available tools
pub fn initialize_tool_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    // Register file operation tools
    registry.register_with_categories(OpenFileTool, vec!["file_ops".to_string()]);
    registry.register_with_categories(ReadFileTool, vec!["file_ops".to_string()]);
    registry.register_with_categories(WriteFileTool, vec!["file_ops".to_string()]);
    registry.register_with_categories(EditFileTool, vec!["file_ops".to_string()]);
    registry.register_with_categories(ListFilesTool, vec!["file_ops".to_string()]);

    // Register search tools
    registry.register_with_categories(SearchFilesTool, vec!["search".to_string()]);

    // Register system tools
    registry.register_with_categories(RunCommandTool, vec!["system".to_string()]);

    // Register model management tools
    registry.register_with_categories(SwitchModelTool::new(), vec!["model_management".to_string()]);
    registry.register_with_categories(PlanEditsTool, vec!["model_management".to_string()]);
    registry.register_with_categories(ApplyEditPlanTool, vec!["model_management".to_string()]);

    // Register iteration control tools
    registry.register_with_categories(RequestMoreIterationsTool, vec!["agent_control".to_string()]);

    registry
}

/// Initialize the agent system with configuration files
pub fn initialize_agent_system(client_config: &ClientConfig, tool_registry: &ToolRegistry, policy_manager: &PolicyManager) -> Result<PlanningCoordinator> {
    println!("{} Initializing agent system...", "🤖".blue());

    // Create agent factory
    let tool_registry_arc = Arc::new((*tool_registry).clone());
    let mut agent_factory = AgentFactory::new(tool_registry_arc, policy_manager.clone());

    // Determine model names with overrides
    let blu_model = client_config.model_blu_model_override.clone()
        .unwrap_or_else(|| ModelType::BluModel.as_str());
    let grn_model = client_config.model_grn_model_override.clone()
        .unwrap_or_else(|| ModelType::GrnModel.as_str());

    // Register LLM clients based on per-model configuration

    // Configure blu_model client
    let blu_model_client: Arc<dyn LlmClient> = if let Some(ref api_url) = client_config.api_url_blu_model {
        if api_url.contains("anthropic") {
            println!("{} Using Anthropic API for 'blu_model' at: {}", "🧠".cyan(), api_url);
            Arc::new(AnthropicLlmClient::new(
                client_config.api_key_blu_model.clone().unwrap_or_default(),
                blu_model,
                api_url.clone(),
                "blu_model".to_string()
            ))
        } else {
            println!("{} Using llama.cpp for 'blu_model' at: {}", "🦙".cyan(), api_url);
            Arc::new(LlamaCppClient::new(
                api_url.clone(),
                blu_model
            ))
        }
    } else if env::var("ANTHROPIC_AUTH_TOKEN_BLU").is_ok() ||
              (env::var("ANTHROPIC_AUTH_TOKEN").is_ok() &&
               (client_config.model_blu_model_override.as_ref()
                .map(|m| m.contains("claude") || m.contains("anthropic"))
                .unwrap_or(false))) {
        println!("{} Using Anthropic API for 'blu_model'", "🧠".cyan());
        let anthropic_key = env::var("ANTHROPIC_AUTH_TOKEN_BLU")
            .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
            .unwrap_or_default();
        Arc::new(AnthropicLlmClient::new(
            anthropic_key,
            blu_model,
            "https://api.anthropic.com".to_string(),
            "blu_model".to_string()
        ))
    } else {
        println!("{} Using Groq API for 'blu_model'", "🚀".cyan());
        Arc::new(GroqLlmClient::new(
            client_config.api_key.clone(),
            blu_model,
            GROQ_API_URL.to_string(),
            "blu_model".to_string()
        ))
    };

    // Configure grn_model client
    let grn_model_client: Arc<dyn LlmClient> = if let Some(ref api_url) = client_config.api_url_grn_model {
        if api_url.contains("anthropic") {
            println!("{} Using Anthropic API for 'grn_model' at: {}", "🧠".cyan(), api_url);
            Arc::new(AnthropicLlmClient::new(
                client_config.api_key_grn_model.clone().unwrap_or_default(),
                grn_model,
                api_url.clone(),
                "grn_model".to_string()
            ))
        } else {
            println!("{} Using llama.cpp for 'grn_model' at: {}", "🦙".cyan(), api_url);
            Arc::new(LlamaCppClient::new(
                api_url.clone(),
                grn_model
            ))
        }
    } else if env::var("ANTHROPIC_AUTH_TOKEN_GRN").is_ok() ||
              (env::var("ANTHROPIC_AUTH_TOKEN").is_ok() &&
               (client_config.model_grn_model_override.as_ref()
                .map(|m| m.contains("claude") || m.contains("anthropic"))
                .unwrap_or(false))) {
        println!("{} Using Anthropic API for 'grn_model'", "🧠".cyan());
        let anthropic_key = env::var("ANTHROPIC_AUTH_TOKEN_GRN")
            .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
            .unwrap_or_default();
        Arc::new(AnthropicLlmClient::new(
            anthropic_key,
            grn_model,
            "https://api.anthropic.com".to_string(),
            "grn_model".to_string()
        ))
    } else {
        println!("{} Using Groq API for 'grn_model'", "🚀".cyan());
        Arc::new(GroqLlmClient::new(
            client_config.api_key.clone(),
            grn_model,
            GROQ_API_URL.to_string(),
            "grn_model".to_string()
        ))
    };

    agent_factory.register_llm_client("blu_model".to_string(), blu_model_client);
    agent_factory.register_llm_client("grn_model".to_string(), grn_model_client);

    // Create coordinator
    let agent_factory_arc = Arc::new(agent_factory);
    let mut coordinator = PlanningCoordinator::new(agent_factory_arc);

    // Load agent configurations
    let config_path = std::path::Path::new("agents/configs");
    if config_path.exists() {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(
                coordinator.load_agent_configs(config_path)
            )
        })?;
        println!("{} Loaded agent configurations from {}", "📁".green(), config_path.display());
    } else {
        println!("{} Agent config directory not found: {}", "⚠️".yellow(), config_path.display());
    }

    println!("{} Agent system initialized successfully!", "✅".green());
    Ok(coordinator)
}
