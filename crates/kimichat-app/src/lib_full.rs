use anyhow::{Context, Result};
use colored::Colorize;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use clap::Parser;


mod logging;
mod open_file;
mod preview;
mod core;
mod policy;
mod tools;
mod agents;
mod models;
mod tools_execution;
mod cli;
mod config;
mod chat;
mod api;
mod app;
mod terminal;
mod skills;
mod todo;
mod web;

use logging::ConversationLogger;
use core::{ToolRegistry, ToolParameters};
use core::tool_context::ToolContext;
use policy::PolicyManager;
use tools_execution::validation::{repair_tool_call_with_model, validate_and_fix_tool_calls_in_place};
use cli::{Cli, Commands, TerminalCommands};
use config::{ClientConfig, GROQ_API_URL, normalize_api_url, initialize_tool_registry, initialize_agent_system};
use chat::{save_state, load_state};
use chat::history::summarize_and_trim_history;
use chat::session::chat as chat_session;
use api::{call_api, call_api_streaming, call_api_with_llm_client, call_api_streaming_with_llm_client};
use app::{setup_from_cli, run_task_mode, run_repl_mode};
use agents::{
    PlanningCoordinator, GroqLlmClient,
    ChatMessage, ExecutionContext,
};
use models::{
    ModelType, Message, ToolCall, FunctionCall,
    SwitchModelArgs,
    Tool, FunctionDef,
    ChatResponse, Usage,
};
use terminal::TerminalManager;


pub(crate) const MAX_CONTEXT_TOKENS: usize = 100_000; // Keep conversation under this to avoid rate limits
pub(crate) const MAX_RETRIES: u32 = 3;

pub(crate) struct KimiChat {
    pub(crate) api_key: String,
    pub(crate) work_dir: PathBuf,
    pub(crate) client: reqwest::Client,
    pub(crate) messages: Vec<Message>,
    pub(crate) current_model: ModelType,
    pub(crate) total_tokens_used: usize,
    pub(crate) logger: Option<ConversationLogger>,
    pub(crate) tool_registry: ToolRegistry,
    // Agent system
    pub(crate) agent_coordinator: Option<PlanningCoordinator>,
    pub(crate) use_agents: bool,
    // Client configuration
    pub(crate) client_config: ClientConfig,
    // Policy manager
    pub(crate) policy_manager: PolicyManager,
    // Terminal manager
    pub(crate) terminal_manager: Arc<Mutex<TerminalManager>>,
    // Skill registry
    pub(crate) skill_registry: Option<Arc<skills::SkillRegistry>>,
    // Non-interactive mode (web/API)
    pub(crate) non_interactive: bool,
    // Todo manager for task tracking
    pub(crate) todo_manager: Arc<todo::TodoManager>,
    // Streaming mode
    pub(crate) stream_responses: bool,
    // Verbose debug mode
    pub(crate) verbose: bool,
    // Debug level for controlling debug output (0=off, 1=basic, 2=detailed, etc.)
    pub(crate) debug_level: u32,
}

impl KimiChat {
    fn new(api_key: String, work_dir: PathBuf) -> Self {
        let config = ClientConfig {
            api_key: api_key.clone(),
            backend_blu_model: None,
            backend_grn_model: None,
            backend_red_model: None,
            api_url_blu_model: None,
            api_url_grn_model: None,
            api_url_red_model: None,
            api_key_blu_model: None,
            api_key_grn_model: None,
            api_key_red_model: None,
            model_blu_model_override: None,
            model_grn_model_override: None,
            model_red_model_override: None,
        };
        let policy_manager = PolicyManager::new();
        Self::new_with_config(
            config,
            work_dir,
            false,
            policy_manager,
            false,
            false,
            terminal::TerminalBackendType::Pty,
        )
    }

    fn new_with_agents(api_key: String, work_dir: PathBuf, use_agents: bool) -> Self {
        let config = ClientConfig {
            api_key: api_key.clone(),
            backend_blu_model: None,
            backend_grn_model: None,
            backend_red_model: None,
            api_url_blu_model: None,
            api_url_grn_model: None,
            api_url_red_model: None,
            api_key_blu_model: None,
            api_key_grn_model: None,
            api_key_red_model: None,
            model_blu_model_override: None,
            model_grn_model_override: None,
            model_red_model_override: None,
        };
        let policy_manager = PolicyManager::new();
        Self::new_with_config(
            config,
            work_dir,
            use_agents,
            policy_manager,
            false,
            false,
            terminal::TerminalBackendType::Pty,
        )
    }

    /// Set the debug level (0=off, 1=basic, 2=detailed, etc.)
    pub(crate) fn set_debug_level(&mut self, level: u32) {
        self.debug_level = level;
    }

    /// Get the current debug level
    pub(crate) fn get_debug_level(&self) -> u32 {
        self.debug_level
    }

    /// Check if debug output should be shown for a given level
    pub(crate) fn should_show_debug(&self, level: u32) -> bool {
        self.debug_level & (1 << (level - 1)) != 0
    }

    fn new_with_config(
        client_config: ClientConfig,
        work_dir: PathBuf,
        use_agents: bool,
        policy_manager: PolicyManager,
        stream_responses: bool,
        verbose: bool,
        backend_type: terminal::TerminalBackendType,
    ) -> Self {
        let tool_registry = initialize_tool_registry();

        // Initialize skill registry
        let skills_dir = work_dir.join("skills");
        let skill_registry = match skills::SkillRegistry::new(skills_dir) {
            Ok(registry) => Some(Arc::new(registry)),
            Err(e) => {
                eprintln!("{} Failed to load skills: {}", "⚠️".yellow(), e);
                eprintln!("{} Skills will not be available", "⚠️".yellow());
                None
            }
        };

        let agent_coordinator = if use_agents {
            match initialize_agent_system(&client_config, &tool_registry, &policy_manager) {
                Ok(coordinator) => Some(coordinator),
                Err(e) => {
                    eprintln!("{} Failed to initialize agent system: {}", "❌".red(), e);
                    eprintln!("{} Falling back to non-agent mode", "⚠️".yellow());
                    None
                }
            }
        } else {
            None
        };

        // Initialize terminal manager with specified backend
        let log_dir = PathBuf::from("logs/terminals");
        let terminal_manager = Arc::new(Mutex::new(
            TerminalManager::with_backend(log_dir, backend_type, terminal::MAX_CONCURRENT_SESSIONS)
        ));

        // Initialize todo manager
        let todo_manager = Arc::new(todo::TodoManager::new());

        // Determine initial model based on overrides or defaults
        // Default to GPT-OSS for cost efficiency - it's significantly cheaper than Kimi
        // while still providing good performance for most tasks
        let initial_model = if let Some(ref override_model) = client_config.model_grn_model_override {
            ModelType::Custom(override_model.clone())
        } else {
            ModelType::GrnModel
        };

        let mut chat = Self {
            api_key: client_config.api_key.clone(),
            work_dir,
            client: reqwest::Client::new(),
            messages: Vec::new(),
            current_model: initial_model,
            total_tokens_used: 0,
            logger: None,
            tool_registry,
            agent_coordinator,
            use_agents,
            client_config,
            policy_manager,
            terminal_manager,
            skill_registry,
            todo_manager,
            stream_responses,
            verbose,
            debug_level: 0, // Default debug level is 0 (off)
            non_interactive: false, // Default to interactive mode
        };

        // Add system message to inform the model about capabilities
        let system_content = config::get_system_prompt();

        chat.messages.push(Message {
            role: "system".to_string(),
            content: system_content,
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning: None,
        });

        // Add initial model notification
        chat.messages.push(Message {
            role: "system".to_string(),
            content: format!("Current model: {}", chat.current_model.display_name()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning: None,
        });

        chat
    }

    pub(crate) fn get_tools(&self) -> Vec<Tool> {
        // Convert new tool registry format to legacy Tool format for backward compatibility
        let registry_tools = self.tool_registry.get_openai_tool_definitions();

        registry_tools.into_iter().map(|tool_def| {
            Tool {
                tool_type: tool_def["type"].as_str().unwrap_or("function").to_string(),
                function: FunctionDef {
                    name: tool_def["function"]["name"].as_str().unwrap_or("").to_string(),
                    description: tool_def["function"]["description"].as_str().unwrap_or("").to_string(),
                    parameters: tool_def["function"]["parameters"].clone(),
                },
            }
        }).collect()
    }

    /// Process user request using the agent system
    async fn process_with_agents(&mut self, user_request: &str, cancellation_token: Option<tokio_util::sync::CancellationToken>) -> Result<String> {
        // Get API URL before mutable borrow
        let api_url = config::get_api_url(&self.client_config, &self.current_model);
        let api_key = config::get_api_key(&self.client_config, &self.api_key, &self.current_model);

        if let Some(coordinator) = &mut self.agent_coordinator {
            // Create execution context for agents
            let tool_registry_arc = std::sync::Arc::new(self.tool_registry.clone());
            let llm_client = std::sync::Arc::new(GroqLlmClient::new(
                api_key,
                self.current_model.as_str().to_string(),
                api_url,
                "process_with_agents".to_string()
            ));

            // Convert message history to agent format
            let conversation_history: Vec<ChatMessage> = self.messages.iter().map(|msg| {
                ChatMessage {
                    role: msg.role.clone(),
                    content: msg.content.clone(),
                    tool_calls: msg.tool_calls.clone().map(|calls| {
                        calls.into_iter().map(|call| crate::agents::agent::ToolCall {
                            id: call.id,
                            function: crate::agents::agent::FunctionCall {
                                name: call.function.name,
                                arguments: call.function.arguments,
                            },
                        }).collect()
                    }),
                    tool_call_id: msg.tool_call_id.clone(),
                    name: msg.name.clone(),
                    reasoning: None,
                }
            }).collect();

            let context = ExecutionContext {
                workspace_dir: self.work_dir.clone(),
                session_id: format!("session_{}", chrono::Utc::now().timestamp()),
                tool_registry: tool_registry_arc,
                llm_client,
                conversation_history,
                terminal_manager: Some(self.terminal_manager.clone()),
                skill_registry: self.skill_registry.clone(),
                todo_manager: Some(self.todo_manager.clone()),
                cancellation_token,
            };

            // Debug: Log current model
            if self.debug_level > 0 {
                eprintln!("[DEBUG] Processing with agents using model: {}", self.current_model.display_name());
            }

            // Process request through coordinator
            let result = coordinator.process_user_request(user_request, &context).await?;

            // Update message history
            self.messages.push(Message {
                role: "user".to_string(),
                content: user_request.to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning: None,
            });

            self.messages.push(Message {
                role: "assistant".to_string(),
                content: result.content.clone(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning: None,
            });

            Ok(result.content)
        } else {
            Err(anyhow::anyhow!("Agent coordinator not initialized"))
        }
    }

    fn read_file(&self, file_path: &str) -> Result<String> {
        let full_path = self.work_dir.join(file_path);
        let content = fs::read_to_string(&full_path)
            .with_context(|| format!("Failed to read file: {}", full_path.display()))?;

        // Return just the content without any metadata
        // This prevents the "[Total: X lines]" from being accidentally included in edits/writes
        Ok(content)
    }

    fn switch_model(&mut self, model_str: &str, reason: &str) -> Result<String> {
        let new_model = match model_str.to_lowercase().as_str() {
            "blu_model" | "blu-model" => ModelType::BluModel,
            "grn_model" | "grn-model" => ModelType::GrnModel,
            "anthropic" | "claude" | "anthropic_model" | "anthropic-model" => ModelType::AnthropicModel,
            _ => anyhow::bail!("Unknown model: {}. Available: 'blu_model', 'grn_model', 'anthropic'", model_str),
        };

        if new_model == self.current_model {
            return Ok(format!(
                "Already using {} model",
                self.current_model.display_name()
            ));
        }

        let old_model = self.current_model.clone();
        self.current_model = new_model.clone();

        // Add message to conversation history about model switch
        self.messages.push(Message {
            role: "system".to_string(),
            content: format!("Model switched to: {} (reason: {})", new_model.display_name(), reason),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning: None,
        });

        Ok(format!(
            "Switched from {} to {} - Reason: {}",
            old_model.display_name(),
            new_model.display_name(),
            reason
        ))
    }

    fn save_state(&self, file_path: &str) -> Result<String> {
        save_state(&self.messages, &self.current_model, self.total_tokens_used, file_path)
    }

    fn load_state(&mut self, file_path: &str) -> Result<String> {
        let (messages, current_model, total_tokens_used, version) = load_state(file_path)?;

        // Restore state
        self.messages = messages;
        self.current_model = current_model;
        self.total_tokens_used = total_tokens_used;

        Ok(format!(
            "Loaded conversation state from {} ({} messages, {} total tokens, version: {})",
            file_path,
            self.messages.len(),
            self.total_tokens_used,
            version
        ))
    }

    async fn execute_tool(&mut self, name: &str, arguments: &str) -> Result<String> {
        // For backward compatibility, handle special tools that need main application state
        match name {
            "switch_model" => {
                let args: SwitchModelArgs = serde_json::from_str(arguments)?;
                self.switch_model(&args.model, &args.reason)
            }
            _ => {
                // Use the tool registry for all tools (including plan_edits and apply_edit_plan)
                let params = ToolParameters::from_json(arguments)
                    .with_context(|| format!("Failed to parse tool arguments for '{}': {}", name, arguments))?;

                let mut context = ToolContext::new(
                    self.work_dir.clone(),
                    format!("session_{}", chrono::Utc::now().timestamp()),
                    self.policy_manager.clone()
                )
                .with_terminal_manager(self.terminal_manager.clone())
                .with_todo_manager(self.todo_manager.clone())
                .with_non_interactive(self.non_interactive);

                // Add skill registry if available
                if let Some(ref registry) = self.skill_registry {
                    context = context.with_skill_registry(Arc::clone(registry));
                }

                let context = context;

                let result = self.tool_registry.execute_tool(name, params, &context).await;

                if result.success {
                    Ok(result.content)
                } else {
                    Err(anyhow::anyhow!("Tool '{}' failed: {}", name, result.error.unwrap_or_else(|| "Unknown error".to_string())))
                }
            }
        }
    }


}

/// Resolve terminal backend type from CLI args and environment variable
/// Priority: CLI arg > ENV var > default (PTY)
pub(crate) fn resolve_terminal_backend(cli: &Cli) -> Result<terminal::TerminalBackendType> {
    use terminal::TerminalBackendType;

    // Get backend string from CLI or env var
    let env_backend = env::var("KIMICHAT_TERMINAL_BACKEND").ok();
    let backend_str = cli.terminal_backend.as_deref()
        .or_else(|| env_backend.as_deref())
        .unwrap_or("pty");

    match backend_str.to_lowercase().as_str() {
        "pty" => Ok(TerminalBackendType::Pty),
        "tmux" => {
            // Check if tmux is available
            if let Ok(output) = std::process::Command::new("tmux").arg("-V").output() {
                if output.status.success() {
                    Ok(TerminalBackendType::Tmux)
                } else {
                    anyhow::bail!(
                        "Tmux backend requested but 'tmux -V' failed. Please ensure tmux is installed and working."
                    )
                }
            } else {
                anyhow::bail!(
                    "Tmux backend requested but tmux command not found. Please install tmux or use --terminal-backend pty"
                )
            }
        }
        other => anyhow::bail!(
            "Invalid terminal backend '{}'. Valid options: pty, tmux", other
        ),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file if it exists
    dotenvy::dotenv().ok();

    // Parse CLI arguments
    let cli = Cli::parse();

    // If a subcommand was provided, execute it and exit
    if let Some(ref command) = cli.command {
        // Special handling for commands that need KimiChat or TerminalManager
        let work_dir = env::current_dir()?;
        let result = match command {
            Commands::Switch { model, reason } => {
                let mut chat = KimiChat::new("".to_string(), work_dir.clone());
                chat.switch_model(model, reason)?
            }
            Commands::Terminal { command: terminal_cmd } => {
                // Initialize TerminalManager for terminal commands
                let log_dir = PathBuf::from("logs/terminals");
                let backend_type = resolve_terminal_backend(&cli)?;
                let terminal_manager = Arc::new(Mutex::new(
                    TerminalManager::with_backend(log_dir, backend_type, terminal::MAX_CONCURRENT_SESSIONS)
                ));
                terminal_cmd.execute(terminal_manager).await?
            }
            _ => command.execute().await?
        };
        println!("{}", result);
        return Ok(());
    }

    // Set up application configuration from CLI
    let app_config = setup_from_cli(&cli)?;

    // Handle task mode if requested
    if let Some(task_text) = cli.task.clone() {
        return run_task_mode(
            &cli,
            task_text,
            app_config.client_config,
            app_config.work_dir,
            app_config.policy_manager,
        )
        .await;
    }

    // Handle web server mode
    if cli.web {
        return app::run_web_server(
            &cli,
            app_config.client_config,
            app_config.work_dir,
            app_config.policy_manager,
        )
        .await;
    }

    // If interactive flag is not set and no subcommand, just exit
    if !cli.interactive {
        println!("No subcommand provided and interactive mode not requested. Exiting.");
        return Ok(());
    }

    // Run REPL mode
    run_repl_mode(
        &cli,
        app_config.client_config,
        app_config.work_dir,
        app_config.policy_manager,
    )
    .await
}
