use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

use kimichat_agents::{
    PlanningCoordinator, AgentFactory,
};
use kimichat_toolcore::ToolRegistry;
use kimichat_policy::PolicyManager;
use kimichat_tools::*;
use kimichat_models::ModelColor;

pub mod helpers;
pub use helpers::{get_system_prompt, get_api_url, get_api_key, create_model_client, create_client_for_model_type};

// Re-export types from kimichat-llm-api
pub use kimichat_llm_api::{BackendType, GROQ_API_URL, normalize_api_url};

/// Configuration for KimiChat client
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// API key for authentication (Groq default)
    pub api_key: String,

    /// Backend types for each model color [blu, grn, red]
    pub backends: [Option<BackendType>; ModelColor::COUNT],
    
    /// API URLs for each model color [blu, grn, red] - if Some, uses custom backend; if None, uses Groq
    pub api_urls: [Option<String>; ModelColor::COUNT],
    
    /// API keys for each model color [blu, grn, red] - if Some, uses this instead of default api_key
    pub api_keys: [Option<String>; ModelColor::COUNT],
    
    /// Model name overrides for each model color [blu, grn, red]
    pub model_overrides: [Option<String>; ModelColor::COUNT],
}

impl ClientConfig {
    /// Create a new ClientConfig with all fields set to None
    pub fn new() -> Self {
        Self {
            api_key: String::new(),
            backends: [const { None }; ModelColor::COUNT],
            api_urls: [const { None }; ModelColor::COUNT],
            api_keys: [const { None }; ModelColor::COUNT],
            model_overrides: [const { None }; ModelColor::COUNT],
        }
    }
    
    /// Get backend for a specific model color
    pub fn get_backend(&self, color: ModelColor) -> Option<&BackendType> {
        self.backends[color as usize].as_ref()
    }
    
    /// Set backend for a specific model color
    pub fn set_backend(&mut self, color: ModelColor, backend: Option<BackendType>) {
        self.backends[color as usize] = backend;
    }
    
    /// Get API URL for a specific model color
    pub fn get_api_url(&self, color: ModelColor) -> Option<&String> {
        self.api_urls[color as usize].as_ref()
    }
    
    /// Set API URL for a specific model color
    pub fn set_api_url(&mut self, color: ModelColor, url: Option<String>) {
        self.api_urls[color as usize] = url;
    }
    
    /// Get API key for a specific model color
    pub fn get_api_key(&self, color: ModelColor) -> Option<&String> {
        self.api_keys[color as usize].as_ref()
    }
    
    /// Set API key for a specific model color
    pub fn set_api_key(&mut self, color: ModelColor, key: Option<String>) {
        self.api_keys[color as usize] = key;
    }
    
    /// Get model override for a specific model color
    pub fn get_model_override(&self, color: ModelColor) -> Option<&String> {
        self.model_overrides[color as usize].as_ref()
    }
    
    /// Set model override for a specific model color
    pub fn set_model_override(&mut self, color: ModelColor, model: Option<String>) {
        self.model_overrides[color as usize] = model;
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

    // Register skill tools
    registry.register_with_categories(LoadSkillTool, vec!["skills".to_string()]);
    registry.register_with_categories(ListSkillsTool, vec!["skills".to_string()]);
    registry.register_with_categories(FindRelevantSkillsTool, vec!["skills".to_string()]);

    // Register subagent tools
    registry.register_with_categories(LaunchSubagentTool, vec!["agent_control".to_string()]);
    registry.register_with_categories(LaunchSubagentPrettyTool, vec!["agent_control".to_string()]);

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
    let blu_model = ModelColor::BluModel.as_str(
        client_config.get_model_override(ModelColor::BluModel).map(|s| s.as_str()),
        client_config.get_model_override(ModelColor::GrnModel).map(|s| s.as_str()),
        client_config.get_model_override(ModelColor::RedModel).map(|s| s.as_str())
    );
    let grn_model = ModelColor::GrnModel.as_str(
        client_config.get_model_override(ModelColor::BluModel).map(|s| s.as_str()),
        client_config.get_model_override(ModelColor::GrnModel).map(|s| s.as_str()),
        client_config.get_model_override(ModelColor::RedModel).map(|s| s.as_str())
    );
    let red_model = ModelColor::RedModel.as_str(
        client_config.get_model_override(ModelColor::BluModel).map(|s| s.as_str()),
        client_config.get_model_override(ModelColor::GrnModel).map(|s| s.as_str()),
        client_config.get_model_override(ModelColor::RedModel).map(|s| s.as_str())
    );

    // Register LLM clients based on per-model configuration
    // Use the centralized helper function to create clients for all three models

    let blu_model_client = create_model_client(
        "blu",
        client_config.get_backend(ModelColor::BluModel).cloned(),
        client_config.get_api_url(ModelColor::BluModel).cloned(),
        client_config.get_api_key(ModelColor::BluModel).cloned(),
        Some(blu_model.clone()),
        &client_config.api_key,
    );

    let grn_model_client = create_model_client(
        "grn",
        client_config.get_backend(ModelColor::GrnModel).cloned(),
        client_config.get_api_url(ModelColor::GrnModel).cloned(),
        client_config.get_api_key(ModelColor::GrnModel).cloned(),
        Some(grn_model.clone()),
        &client_config.api_key,
    );

    let red_model_client = create_model_client(
        "red",
        client_config.get_backend(ModelColor::RedModel).cloned(),
        client_config.get_api_url(ModelColor::RedModel).cloned(),
        client_config.get_api_key(ModelColor::RedModel).cloned(),
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
