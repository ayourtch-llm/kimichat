use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

use kimichat_toolcore::ToolRegistry;
use kimichat_policy::PolicyManager;
use kimichat_tools::*;
use crate::agents::{
    PlanningCoordinator, AgentFactory,
};
use kimichat_models::ModelType;

pub mod helpers;
pub use helpers::{get_system_prompt, get_api_url, get_api_key, create_model_client, create_client_for_model_type};

// Re-export types from kimichat-llm-api
pub use kimichat_llm_api::{BackendType, GROQ_API_URL, normalize_api_url};

/// Configuration for KimiChat client
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// API key for authentication (Groq default)
    pub api_key: String,

    /// Explicit backend type for blu_model
    pub backend_blu_model: Option<BackendType>,
    /// Explicit backend type for grn_model
    pub backend_grn_model: Option<BackendType>,
    /// Explicit backend type for red_model
    pub backend_red_model: Option<BackendType>,

    /// API URL for 'blu_model' - if Some, uses custom backend; if None, uses Groq
    pub api_url_blu_model: Option<String>,
    /// API URL for 'grn_model' - if Some, uses custom backend; if None, uses Groq
    pub api_url_grn_model: Option<String>,
    /// API URL for 'red_model' - if Some, uses custom backend; if None, uses Groq
    pub api_url_red_model: Option<String>,
    /// API key for 'blu_model' - if Some, uses this instead of default api_key
    pub api_key_blu_model: Option<String>,
    /// API key for 'grn_model' - if Some, uses this instead of default api_key
    pub api_key_grn_model: Option<String>,
    /// API key for 'red_model' - if Some, uses this instead of default api_key
    pub api_key_red_model: Option<String>,
    /// Override for 'blu_model' model name
    pub model_blu_model_override: Option<String>,
    /// Override for 'grn_model' model name
    pub model_grn_model_override: Option<String>,
    /// Override for 'red_model' model name
    pub model_red_model_override: Option<String>,
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

    // Register skill tools
    registry.register_with_categories(LoadSkillTool, vec!["skills".to_string()]);
    registry.register_with_categories(ListSkillsTool, vec!["skills".to_string()]);
    registry.register_with_categories(FindRelevantSkillsTool, vec!["skills".to_string()]);

    // Register todo/task tracking tools
    registry.register_with_categories(TodoWriteTool::new(), vec!["task_tracking".to_string()]);
    registry.register_with_categories(TodoListTool::new(), vec!["task_tracking".to_string()]);

    // Register PTY terminal tools
    registry.register_with_categories(PtyLaunchTool, vec!["terminal".to_string()]);
    registry.register_with_categories(PtySendKeysTool, vec!["terminal".to_string()]);
    registry.register_with_categories(PtyGetScreenTool, vec!["terminal".to_string()]);
    registry.register_with_categories(PtyGetCursorTool, vec!["terminal".to_string()]);
    registry.register_with_categories(PtyResizeTool, vec!["terminal".to_string()]);
    registry.register_with_categories(PtySetScrollbackTool, vec!["terminal".to_string()]);
    registry.register_with_categories(PtyStartCaptureTool, vec!["terminal".to_string()]);
    registry.register_with_categories(PtyStopCaptureTool, vec!["terminal".to_string()]);
    registry.register_with_categories(PtyListTool, vec!["terminal".to_string()]);
    registry.register_with_categories(PtyKillTool, vec!["terminal".to_string()]);
    registry.register_with_categories(PtyRequestUserInputTool, vec!["terminal".to_string()]);

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
    let red_model = client_config.model_red_model_override.clone()
        .unwrap_or_else(|| ModelType::RedModel.as_str());

    // Register LLM clients based on per-model configuration
    // Use the centralized helper function to create clients for all three models

    let blu_model_client = create_model_client(
        "blu",
        client_config.backend_blu_model.clone(),
        client_config.api_url_blu_model.clone(),
        client_config.api_key_blu_model.clone(),
        Some(blu_model.clone()),
        &client_config.api_key,
    );

    let grn_model_client = create_model_client(
        "grn",
        client_config.backend_grn_model.clone(),
        client_config.api_url_grn_model.clone(),
        client_config.api_key_grn_model.clone(),
        Some(grn_model.clone()),
        &client_config.api_key,
    );

    let red_model_client = create_model_client(
        "red",
        client_config.backend_red_model.clone(),
        client_config.api_url_red_model.clone(),
        client_config.api_key_red_model.clone(),
        Some(red_model.clone()),
        &client_config.api_key,
    );

    agent_factory.register_llm_client("blu_model".to_string(), blu_model_client);
    agent_factory.register_llm_client("grn_model".to_string(), grn_model_client);
    agent_factory.register_llm_client("red_model".to_string(), red_model_client);

    // Create coordinator
    let agent_factory_arc = Arc::new(agent_factory);
    let mut coordinator = PlanningCoordinator::new(agent_factory_arc);

    // Load agent configurations (from embedded + optional filesystem)
    let config_path = std::path::Path::new("agents/configs");
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(
            coordinator.load_agent_configs(config_path)
        )
    })?;

    println!("{} Agent system initialized successfully!", "✅".green());
    Ok(coordinator)
}
