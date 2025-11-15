//! KimiChat Application Library
//!
//! Main application logic and KimiChat struct.

use anyhow::{Context, Result};
use colored::Colorize;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

// Re-export workspace crates
pub use kimichat_types::{self as types, ModelType, Message, ToolCall, FunctionCall, SwitchModelArgs};
pub use kimichat_models as models;
pub use kimichat_policy::{self as policy, PolicyManager};
pub use kimichat_terminal::{self as terminal, TerminalManager};
pub use kimichat_tools::{self as tools, ToolRegistry, ToolParameters, ToolContext};
pub use kimichat_skills as skills;
pub use kimichat_api as api;
pub use kimichat_agents::{self as agents, PlanningCoordinator, GroqLlmClient, ChatMessage, ExecutionContext};

// Local modules
pub mod cli;
pub mod config;
pub mod app;
pub mod web;
pub mod todo;
pub mod open_file;
pub mod preview;

// Chat-related modules
pub mod chat;
pub mod tools_execution;
pub mod api_client;
pub mod api_streaming;
pub mod conversation_logger;
pub mod request_logger;

// Re-exports from local modules
pub use cli::{Cli, Commands, TerminalCommands};
pub use config::{ClientConfig, normalize_api_url, initialize_tool_registry, initialize_agent_system};
pub use app::{setup_from_cli, run_task_mode, run_repl_mode};
pub use todo::TodoManager;
pub use conversation_logger::ConversationLogger;
pub use request_logger::{log_request, log_response, log_stream_chunk, log_request_to_file};
pub use chat::history::{summarize_and_trim_history, safe_truncate};
pub use chat::session::chat as chat_loop;
pub use chat::state::{save_state, load_state};
pub use tools_execution::parsing::parse_xml_tool_calls;
pub use tools_execution::validation::{validate_and_fix_tool_calls_in_place, repair_tool_call_with_model};
pub use api_client::{call_api, call_api_with_llm_client};
pub use api_streaming::{call_api_streaming, call_api_streaming_with_llm_client};

// Constants
pub const MAX_CONTEXT_TOKENS: usize = kimichat_types::MAX_CONTEXT_TOKENS;
pub const MAX_RETRIES: u32 = kimichat_types::MAX_RETRIES;
pub const GROQ_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
pub const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// Main KimiChat application struct
pub struct KimiChat {
    pub api_key: String,
    pub work_dir: PathBuf,
    pub client: reqwest::Client,
    pub messages: Vec<Message>,
    pub current_model: ModelType,
    pub total_tokens_used: usize,
    pub logger: Option<ConversationLogger>,
    pub tool_registry: ToolRegistry,
    pub agent_coordinator: Option<PlanningCoordinator>,
    pub use_agents: bool,
    pub client_config: ClientConfig,
    pub policy_manager: PolicyManager,
    pub terminal_manager: Arc<Mutex<TerminalManager>>,
    pub skill_registry: Option<Arc<skills::SkillRegistry>>,
    pub non_interactive: bool,
    pub todo_manager: Arc<TodoManager>,
    pub stream_responses: bool,
    pub verbose: bool,
    pub debug_level: u32,
}

impl KimiChat {
    pub fn new(api_key: String, work_dir: PathBuf) -> Self {
        let config = ClientConfig::default();
        let log_dir = work_dir.join("logs/terminals");

        Self {
            api_key: api_key.clone(),
            work_dir: work_dir.clone(),
            client: reqwest::Client::new(),
            messages: Vec::new(),
            current_model: ModelType::BluModel,
            total_tokens_used: 0,
            logger: None,
            tool_registry: ToolRegistry::new(),
            agent_coordinator: None,
            use_agents: false,
            client_config: config,
            policy_manager: PolicyManager::new(),
            terminal_manager: Arc::new(Mutex::new(TerminalManager::new(log_dir))),
            skill_registry: None,
            non_interactive: false,
            todo_manager: Arc::new(TodoManager::new()),
            stream_responses: true,
            verbose: false,
            debug_level: 0,
        }
    }

    pub fn set_debug_level(&mut self, level: u32) {
        self.debug_level = level;
    }

    pub fn get_debug_level(&self) -> u32 {
        self.debug_level
    }

    pub fn should_show_debug(&self, level: u32) -> bool {
        self.debug_level >= level
    }

    pub fn get_tools(&self) -> Vec<models::Tool> {
        self.tool_registry.get_openai_tool_definitions()
            .into_iter()
            .filter_map(|def| serde_json::from_value(def).ok())
            .collect()
    }

    /// Read a file from the work directory
    pub fn read_file(&self, path: &str) -> Result<String> {
        let file_path = self.work_dir.join(path);
        std::fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))
    }

    /// Save conversation state to a file
    pub fn save_state(&self, file_path: &str) -> Result<String> {
        chat::state::save_state(
            &self.messages,
            &self.current_model,
            self.total_tokens_used,
            file_path,
        )
    }

    /// Load conversation state from a file
    pub fn load_state(&mut self, file_path: &str) -> Result<String> {
        let (messages, current_model, total_tokens_used, version) =
            chat::state::load_state(file_path)?;

        self.messages = messages;
        self.current_model = current_model;
        self.total_tokens_used = total_tokens_used;

        Ok(format!(
            "Loaded conversation state from {} (version {}, {} messages, {} total tokens)",
            file_path,
            version,
            self.messages.len(),
            self.total_tokens_used
        ))
    }

    pub fn resolve_terminal_backend(cli: &Cli) -> Result<terminal::backend::TerminalBackendType> {
        use terminal::backend::TerminalBackendType;

        // Check CLI argument first, then environment variable
        let env_backend = env::var("KIMICHAT_TERMINAL_BACKEND").ok();
        let backend_str = cli.terminal_backend.as_deref()
            .or_else(|| env_backend.as_deref());

        match backend_str {
            Some("tmux") => Ok(TerminalBackendType::Tmux),
            Some("pty") | None => Ok(TerminalBackendType::Pty),
            Some(other) => anyhow::bail!("Unknown terminal backend: {}", other),
        }
    }

    pub fn new_with_config(
        client_config: ClientConfig,
        work_dir: PathBuf,
        use_agents: bool,
        policy_manager: PolicyManager,
        stream_responses: bool,
        verbose: bool,
        backend_type: terminal::backend::TerminalBackendType,
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
        let todo_manager = Arc::new(TodoManager::new());

        // Determine initial model based on overrides or defaults
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
            debug_level: 0,
            non_interactive: false,
        };

        // Add system message
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

    pub async fn process_with_agents(&mut self, user_request: &str, cancellation_token: Option<tokio_util::sync::CancellationToken>) -> Result<String> {
        let api_url = config::get_api_url(&self.client_config, &self.current_model);
        let api_key = config::get_api_key(&self.client_config, &self.api_key, &self.current_model);

        if let Some(coordinator) = &mut self.agent_coordinator {
            let tool_registry_arc = std::sync::Arc::new(self.tool_registry.clone());
            let llm_client = std::sync::Arc::new(GroqLlmClient::new(
                api_key,
                self.current_model.as_str().to_string(),
                api_url,
                "process_with_agents".to_string()
            ));

            let conversation_history: Vec<ChatMessage> = self.messages.iter().map(|msg| {
                ChatMessage {
                    role: msg.role.clone(),
                    content: msg.content.clone(),
                    tool_calls: msg.tool_calls.clone().map(|calls| {
                        calls.into_iter().map(|call| kimichat_agents::ToolCall {
                            id: call.id,
                            function: kimichat_agents::FunctionCall {
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
                // todo_manager: Some(self.todo_manager.clone()), // TODO: Re-enable when ExecutionContext supports todo_manager
                cancellation_token,
            };

            if self.debug_level > 0 {
                eprintln!("[DEBUG] Processing with agents using model: {}", self.current_model.display_name());
            }

            let result = coordinator.process_user_request(user_request, &context).await?;

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

    pub fn switch_model(&mut self, model_str: &str, reason: &str) -> Result<String> {
        let new_model = match model_str.to_lowercase().as_str() {
            "blu_model" | "blu-model" => ModelType::BluModel,
            "grn_model" | "grn-model" => ModelType::GrnModel,
            "anthropic" | "claude" | "anthropic_model" | "anthropic-model" => ModelType::AnthropicModel,
            _ => anyhow::bail!("Unknown model: {}. Available: 'blu_model', 'grn_model', 'anthropic'", model_str),
        };

        if new_model == self.current_model {
            return Ok(format!("Already using {} model", self.current_model.display_name()));
        }

        let old_model = self.current_model.clone();
        self.current_model = new_model.clone();

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

    pub async fn execute_tool(&mut self, name: &str, arguments: &str) -> Result<String> {
        match name {
            "switch_model" => {
                let args: SwitchModelArgs = serde_json::from_str(arguments)?;
                self.switch_model(&args.model, &args.reason)
            }
            _ => {
                let params = ToolParameters::from_json(arguments)
                    .with_context(|| format!("Failed to parse tool arguments for '{}': {}", name, arguments))?;

                let mut context = ToolContext::new(
                    self.work_dir.clone(),
                    format!("session_{}", chrono::Utc::now().timestamp()),
                    self.policy_manager.clone()
                )
                .with_terminal_manager(self.terminal_manager.clone())
                // .with_todo_manager(self.todo_manager.clone()) // TODO: Re-enable when ToolContext supports todo_manager
                .with_non_interactive(self.non_interactive);

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

/// Standalone helper function for resolving terminal backend type
pub fn resolve_terminal_backend(cli: &Cli) -> Result<terminal::backend::TerminalBackendType> {
    KimiChat::resolve_terminal_backend(cli)
}
