use anyhow::{Context, Result};
use std::path::Path;
use colored::Colorize;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;
use std::ops::RangeInclusive;
use std::io::BufReader;
use std::fs::File;
use std::io::prelude::*;
use std::io::Write;
use similar::{ChangeTag, TextDiff};
use regex::Regex;

use clap::{Parser, Subcommand};
use clap_complete::Shell;
use std::future::Future;
use std::pin::Pin;
use crate::preview::two_word_preview;


mod logging;
mod open_file;
mod preview;
mod core;
mod policy;
mod tools;
mod agents;
use logging::ConversationLogger;
use core::{ToolRegistry, ToolParameters};
use policy::{PolicyManager, ActionType, Decision};
use core::tool_context::ToolContext;
use tools::*;
use agents::*;


const GROQ_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const MAX_CONTEXT_TOKENS: usize = 100_000; // Keep conversation under this to avoid rate limits
const MAX_RETRIES: u32 = 3;

/// CLI arguments for kimi-chat
#[derive(Parser)]
#[command(name = "kimichat")]
#[command(about = "Kimi Chat - Claude Code-like Experience with Multi-Model AI Support")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    
    /// Run in interactive mode (default)
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    interactive: bool,

    /// Enable multi-agent system for specialized task handling
    #[arg(long, action = clap::ArgAction::SetTrue)]
    agents: bool,

    /// Generate shell completions
    #[arg(long, value_enum)]
    generate: Option<Shell>,
    
    /// Run in summary mode – give a short description of the task.
    #[arg(long, value_name = "TEXT")]
    task: Option<String>,

    /// Pretty‑print the JSON output (only useful with --task)
    #[arg(long)]
    pretty: bool,

    /// Use llama.cpp server for both models (e.g., http://localhost:8080)
    /// This is a convenience flag that sets both --api-url-blu-model and --api-url-grn-model
    #[arg(long, value_name = "URL")]
    llama_cpp_url: Option<String>,

    /// API URL for the 'blu_model' model (e.g., http://localhost:8080)
    /// If set, uses llama.cpp for blu_model; otherwise uses Groq
    #[arg(long, value_name = "URL")]
    api_url_blu_model: Option<String>,

    /// API URL for the 'grn_model' model (e.g., http://localhost:8081)
    /// If set, uses llama.cpp for grn_model; otherwise uses Groq
    #[arg(long, value_name = "URL")]
    api_url_grn_model: Option<String>,

    /// Override the 'blu_model' model with a custom model name
    #[arg(long, value_name = "MODEL")]
    model_blu_model: Option<String>,

    /// Override the 'grn_model' model with a custom model name
    #[arg(long, value_name = "MODEL")]
    model_grn_model: Option<String>,

    /// Override both models with the same custom model name
    /// This is a convenience flag that sets both --model-blu-model and --model-grn-model
    #[arg(long, value_name = "MODEL")]
    model: Option<String>,

    /// Auto-confirm all actions without asking (auto-pilot mode)
    #[arg(long)]
    auto_confirm: bool,

    /// Path to policy file (default: policies.toml in project root)
    #[arg(long, value_name = "PATH")]
    policy_file: Option<String>,

    /// Learn from user decisions and save them to policy file
    #[arg(long)]
    learn_policies: bool,

    /// Enable streaming mode - show AI responses as they're generated
    #[arg(long)]
    stream: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Read file contents (shows first 10 lines with total count)
    Read {
        /// Path to the file to read
        file_path: String,
    },
    /// Write content to a file
    Write {
        /// Path to the file to write
        file_path: String,
        /// Content to write to the file
        content: String,
    },
    /// Edit a file by replacing old content with new content
    Edit {
        /// Path to the file to edit
        file_path: String,
        /// Old content to find and replace (must not be empty)
        #[arg(short = 'o', long)]
        old_content: String,
        /// New content to replace with
        #[arg(short = 'n', long)]
        new_content: String,
    },
    /// List files matching a glob pattern (no recursive ** allowed)
    List {
        /// Glob pattern (e.g., 'src/*.rs'). Defaults to '*'
        #[arg(default_value = "*")]
        pattern: String,
    },
    /// Search for text across files
    Search {
        /// Text or pattern to search for
        query: String,
        /// File pattern to search in (e.g., 'src/*.rs'). Defaults to '*.rs'
        #[arg(short = 'p', long, default_value = "*.rs")]
        pattern: String,
        /// Treat query as regular expression
        #[arg(short = 'r', long)]
        regex: bool,
        /// Case-insensitive search
        #[arg(short = 'i', long)]
        case_insensitive: bool,
        /// Maximum number of results to return
        #[arg(short = 'm', long, default_value = "100")]
        max_results: u32,
    },
    /// Switch to a different AI model
    Switch {
        /// Model to switch to ('kimi' or 'gpt-oss')
        model: String,
        /// Reason for switching
        reason: String,
    },
    /// Run a shell command
    Run {
        /// Command to execute
        command: String,
    },
    /// Open and display file contents with optional line range
    Open {
        /// Path to the file to open
        file_path: String,
        /// Starting line number (1-based)
        #[arg(short = 's', long)]
        start_line: Option<usize>,
        /// Ending line number (1-based)
        #[arg(short = 'e', long)]
        end_line: Option<usize>,
    },
}

impl Commands {
    fn execute(&self) -> Pin<Box<dyn Future<Output = Result<String>> + '_>> {
        use crate::core::tool::{Tool, ToolParameters};
        use crate::core::tool_context::ToolContext;
        use crate::tools::file_ops::*;
        use crate::tools::system::*;
        use crate::tools::search::*;

        match self {
            Commands::Read { file_path } => {
                let work_dir = env::current_dir().unwrap();
                let file_path = file_path.clone();
                Box::pin(async move {
                    let mut params = ToolParameters::new();
                    params.set("file_path", file_path);
                    let context = ToolContext::new(work_dir, "cli_session".to_string(), PolicyManager::new());
                    let result = ReadFileTool.execute(params, &context).await;
                    if result.success {
                        Ok(result.content)
                    } else {
                        Err(anyhow::anyhow!("{}", result.error.unwrap_or_default()))
                    }
                })
            }
            Commands::Write { file_path, content } => {
                let work_dir = env::current_dir().unwrap();
                let file_path = file_path.clone();
                let content = content.clone();
                Box::pin(async move {
                    let mut params = ToolParameters::new();
                    params.set("file_path", file_path);
                    params.set("content", content);
                    let context = ToolContext::new(work_dir, "cli_session".to_string(), PolicyManager::new());
                    let result = WriteFileTool.execute(params, &context).await;
                    if result.success {
                        Ok(result.content)
                    } else {
                        Err(anyhow::anyhow!("{}", result.error.unwrap_or_default()))
                    }
                })
            }
            Commands::Edit { file_path, old_content, new_content } => {
                let work_dir = env::current_dir().unwrap();
                let file_path = file_path.clone();
                let old_content = old_content.clone();
                let new_content = new_content.clone();
                Box::pin(async move {
                    let mut params = ToolParameters::new();
                    params.set("file_path", file_path);
                    params.set("old_content", old_content);
                    params.set("new_content", new_content);
                    let context = ToolContext::new(work_dir, "cli_session".to_string(), PolicyManager::new());
                    let result = EditFileTool.execute(params, &context).await;
                    if result.success {
                        Ok(result.content)
                    } else {
                        Err(anyhow::anyhow!("{}", result.error.unwrap_or_default()))
                    }
                })
            }
            Commands::List { pattern } => {
                let work_dir = env::current_dir().unwrap();
                let pattern = pattern.clone();
                Box::pin(async move {
                    let mut params = ToolParameters::new();
                    params.set("pattern", pattern);
                    let context = ToolContext::new(work_dir, "cli_session".to_string(), PolicyManager::new());
                    let result = ListFilesTool.execute(params, &context).await;
                    if result.success {
                        Ok(result.content)
                    } else {
                        Err(anyhow::anyhow!("{}", result.error.unwrap_or_default()))
                    }
                })
            }
            Commands::Search { query, pattern, regex, case_insensitive, max_results } => {
                let work_dir = env::current_dir().unwrap();
                let query = query.clone();
                let pattern = pattern.clone();
                let regex = *regex;
                let case_insensitive = *case_insensitive;
                let max_results = *max_results;
                Box::pin(async move {
                    let mut params = ToolParameters::new();
                    params.set("query", query);
                    params.set("pattern", pattern);
                    params.set("regex", regex);
                    params.set("case_insensitive", case_insensitive);
                    params.set("max_results", max_results as i64);
                    let context = ToolContext::new(work_dir, "cli_session".to_string(), PolicyManager::new());
                    let result = SearchFilesTool.execute(params, &context).await;
                    if result.success {
                        Ok(result.content)
                    } else {
                        Err(anyhow::anyhow!("{}", result.error.unwrap_or_default()))
                    }
                })
            }
            Commands::Switch { model, reason } => {
                let work_dir = env::current_dir().unwrap();
                let mut chat = KimiChat::new("".to_string(), work_dir);
                Box::pin(async move {
                    chat.switch_model(model, reason)
                })
            }
            Commands::Run { command } => {
                let work_dir = env::current_dir().unwrap();
                let command = command.clone();
                Box::pin(async move {
                    let mut params = ToolParameters::new();
                    params.set("command", command);
                    let context = ToolContext::new(work_dir, "cli_session".to_string(), PolicyManager::new());
                    let result = RunCommandTool.execute(params, &context).await;
                    if result.success {
                        Ok(result.content)
                    } else {
                        Err(anyhow::anyhow!("{}", result.error.unwrap_or_default()))
                    }
                })
            }
            Commands::Open { file_path, start_line, end_line } => {
                let work_dir = env::current_dir().unwrap();
                let file_path = file_path.clone();
                let start_line = *start_line;
                let end_line = *end_line;
                Box::pin(async move {
                    let mut params = ToolParameters::new();
                    params.set("file_path", file_path);
                    if let Some(start) = start_line {
                        params.set("start_line", start as i64);
                    }
                    if let Some(end) = end_line {
                        params.set("end_line", end as i64);
                    }
                    let context = ToolContext::new(work_dir, "cli_session".to_string(), PolicyManager::new());
                    let result = OpenFileTool.execute(params, &context).await;
                    if result.success {
                        Ok(result.content)
                    } else {
                        Err(anyhow::anyhow!("{}", result.error.unwrap_or_default()))
                    }
                })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum ModelType {
    BluModel,
    GrnModel,
    Custom(String),
}

impl ModelType {
    fn as_str(&self) -> String {
        match self {
            ModelType::BluModel => "moonshotai/kimi-k2-instruct-0905".to_string(),
            ModelType::GrnModel => "openai/gpt-oss-120b".to_string(),
            ModelType::Custom(name) => name.clone(),
        }
    }

    fn display_name(&self) -> String {
        match self {
            ModelType::BluModel => "Kimi-K2-Instruct-0905".to_string(),
            ModelType::GrnModel => "GPT-OSS-120B".to_string(),
            ModelType::Custom(name) => name.clone(),
        }
    }

    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "blu_model" | "blu-model" | "blumodel" => ModelType::BluModel,
            "grn_model" | "grn-model" | "grnmodel" => ModelType::GrnModel,
            _ => ModelType::Custom(s.to_string()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Message {
    role: String,
    #[serde(default)]
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ToolCall {
    id: String,
    #[serde(rename = "type")]
    tool_type: String,
    function: FunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Tool {
    #[serde(rename = "type")]
    tool_type: String,
    function: FunctionDef,
}

#[derive(Debug, Serialize, Deserialize)]
struct FunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    tools: Vec<Tool>,
    tool_choice: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    object: Option<String>,
    #[serde(default)]
    created: Option<i64>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
    #[serde(default)]
    index: Option<i32>,
    #[serde(default)]
    finish_reason: Option<String>,
    #[serde(default)]
    logprobs: Option<serde_json::Value>,
}

// Structures for streaming responses
#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    object: Option<String>,
    #[serde(default)]
    created: Option<i64>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    #[serde(default)]
    index: Option<i32>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Deserialize)]
struct ReadFileArgs {
    file_path: String,
}

#[derive(Debug, Deserialize)]
struct WriteFileArgs {
    file_path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ListFilesArgs {
    #[serde(default = "default_pattern")]
    pattern: String,
}

fn default_pattern() -> String {
    "*".to_string()
}

#[derive(Debug, Deserialize)]
struct EditFileArgs {
    file_path: String,
    old_content: String,
    new_content: String,
}

#[derive(Debug, Deserialize)]
struct SwitchModelArgs {
    model: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct RunCommandArgs {
    command: String,
}

#[derive(Debug, Deserialize)]
struct SearchFilesArgs {
    #[serde(default)]
    query: String,
    #[serde(default = "default_pattern")]
    pattern: String,
    #[serde(default)]
    regex: bool,
    #[serde(default)]
    case_insensitive: bool,
    #[serde(default)]
    max_results: u32,
}

#[derive(Debug, Deserialize)]
struct OpenFileArgs {
    file_path: String,
    #[serde(default)]
    start_line: usize,
    #[serde(default)]
    end_line: usize,
}

fn default_max_results() -> u32 { 100 }


/// Configuration for KimiChat client
#[derive(Debug, Clone)]
struct ClientConfig {
    /// API key for authentication (not used for llama.cpp)
    api_key: String,
    /// API URL for 'blu_model' - if Some, uses llama.cpp; if None, uses Groq
    api_url_blu_model: Option<String>,
    /// API URL for 'grn_model' - if Some, uses llama.cpp; if None, uses Groq
    api_url_grn_model: Option<String>,
    /// Override for 'blu_model' model name
    model_blu_model_override: Option<String>,
    /// Override for 'grn_model' model name
    model_grn_model_override: Option<String>,
}

struct KimiChat {
    api_key: String,
    work_dir: PathBuf,
    client: reqwest::Client,
    messages: Vec<Message>,
    current_model: ModelType,
    total_tokens_used: usize,
    logger: Option<ConversationLogger>,
    tool_registry: ToolRegistry,
    // Agent system
    agent_coordinator: Option<PlanningCoordinator>,
    use_agents: bool,
    // Client configuration
    client_config: ClientConfig,
    // Policy manager
    policy_manager: PolicyManager,
    // Streaming mode
    stream_responses: bool,
}

impl KimiChat {
    /// Generate system prompt based on current model
    fn get_system_prompt(model: &ModelType) -> String {
        let base_prompt = format!(
            "You are an AI assistant with access to file operations and model switching capabilities. \
            You are currently running as {}. You can switch to other models when appropriate:\n\
            - grn_model (GrnModel): **Preferred for cost efficiency** - significantly cheaper than BluModel while providing good performance for most tasks\n\
            - blu_model (BluModel): Use when GrnModel struggles or when you need faster responses\n\n\
            Available tools (use ONLY these exact names):\n\
            - read_file: Read entire file contents (always returns full file)\n\
            - open_file: Read specific line range from a file (use when you only need a section)\n\
            - write_file: Write/create a file\n\
            - edit_file: Edit existing file by replacing content (for single edits)\n\
            - plan_edits: Plan multiple file edits to apply atomically (RECOMMENDED for multiple related changes)\n\
            - apply_edit_plan: Apply the previously created edit plan\n\
            - list_files: List files (single-level patterns only, no **)\n\
            - switch_model: Switch between models\n\n\
            IMPORTANT WORKFLOW for multiple edits:\n\
            1. When making multiple changes to files, use plan_edits to create a complete plan\n\
            2. Review the plan validation output\n\
            3. Use apply_edit_plan to execute all changes atomically\n\
            This prevents issues where you lose track of file state between sequential edits.\n\n",
            model.display_name()
        );

        if *model == ModelType::GrnModel {
            format!(
                "{}CRITICAL WARNING: If you attempt to call ANY tool not listed above (such as 'edit', 'repo_browser.search', \
                'repo_browser.open_file', or any other made-up tool name), you will be IMMEDIATELY switched to the BluModel model \
                and your request will be retried. Use ONLY the exact tool names listed above.",
                base_prompt
            )
        } else {
            format!(
                "{}IMPORTANT: Only use the exact tool names listed above. Do not make up tool names.",
                base_prompt
            )
        }
    }

    fn new(api_key: String, work_dir: PathBuf) -> Self {
        let config = ClientConfig {
            api_key: api_key.clone(),
            api_url_blu_model: None,
            api_url_grn_model: None,
            model_blu_model_override: None,
            model_grn_model_override: None,
        };
        let policy_manager = PolicyManager::new();
        Self::new_with_config(config, work_dir, false, policy_manager, false)
    }

    fn new_with_agents(api_key: String, work_dir: PathBuf, use_agents: bool) -> Self {
        let config = ClientConfig {
            api_key: api_key.clone(),
            api_url_blu_model: None,
            api_url_grn_model: None,
            model_blu_model_override: None,
            model_grn_model_override: None,
        };
        let policy_manager = PolicyManager::new();
        Self::new_with_config(config, work_dir, use_agents, policy_manager, false)
    }

    fn new_with_config(client_config: ClientConfig, work_dir: PathBuf, use_agents: bool, policy_manager: PolicyManager, stream_responses: bool) -> Self {
        let tool_registry = Self::initialize_tool_registry();
        let agent_coordinator = if use_agents {
            match Self::initialize_agent_system(&client_config, &tool_registry, &policy_manager) {
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

        let mut chat = Self {
            api_key: client_config.api_key.clone(),
            work_dir,
            client: reqwest::Client::new(),
            messages: Vec::new(),
            // Default to GPT-OSS for cost efficiency - it's significantly cheaper than Kimi
            // while still providing good performance for most tasks
            current_model: ModelType::GrnModel,
            total_tokens_used: 0,
            logger: None,
            tool_registry,
            agent_coordinator,
            use_agents,
            client_config,
            policy_manager,
            stream_responses,
        };

        // Add system message to inform the model about capabilities
        let system_content = Self::get_system_prompt(&chat.current_model);

        chat.messages.push(Message {
            role: "system".to_string(),
            content: system_content,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });

        chat
    }

    /// Initialize the tool registry with all available tools
    fn initialize_tool_registry() -> ToolRegistry {
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
    fn initialize_agent_system(client_config: &ClientConfig, tool_registry: &ToolRegistry, policy_manager: &PolicyManager) -> Result<PlanningCoordinator> {
        println!("{} Initializing agent system...", "🤖".blue());

        // Create agent factory
        let tool_registry_arc = std::sync::Arc::new((*tool_registry).clone());
        let mut agent_factory = AgentFactory::new(tool_registry_arc, policy_manager.clone());

        // Determine model names with overrides
        let blu_model = client_config.model_blu_model_override.clone()
            .unwrap_or_else(|| ModelType::BluModel.as_str());
        let grn_model = client_config.model_grn_model_override.clone()
            .unwrap_or_else(|| ModelType::GrnModel.as_str());

        // Register LLM clients based on per-model configuration

        // Configure blu_model client
        let blu_model_client: std::sync::Arc<dyn LlmClient> = if let Some(ref api_url) = client_config.api_url_blu_model {
            println!("{} Using llama.cpp for 'blu_model' at: {}", "🦙".cyan(), api_url);
            std::sync::Arc::new(LlamaCppClient::new(
                api_url.clone(),
                blu_model
            ))
        } else {
            println!("{} Using Groq API for 'blu_model'", "🚀".cyan());
            std::sync::Arc::new(GroqLlmClient::new(
                client_config.api_key.clone(),
                blu_model
            ))
        };

        // Configure grn_model client
        let grn_model_client: std::sync::Arc<dyn LlmClient> = if let Some(ref api_url) = client_config.api_url_grn_model {
            println!("{} Using llama.cpp for 'grn_model' at: {}", "🦙".cyan(), api_url);
            std::sync::Arc::new(LlamaCppClient::new(
                api_url.clone(),
                grn_model
            ))
        } else {
            println!("{} Using Groq API for 'grn_model'", "🚀".cyan());
            std::sync::Arc::new(GroqLlmClient::new(
                client_config.api_key.clone(),
                grn_model
            ))
        };

        agent_factory.register_llm_client("blu_model".to_string(), blu_model_client);
        agent_factory.register_llm_client("grn_model".to_string(), grn_model_client);

        // Create coordinator
        let agent_factory_arc = std::sync::Arc::new(agent_factory);
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

    fn get_tools(&self) -> Vec<Tool> {
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
    async fn process_with_agents(&mut self, user_request: &str) -> Result<String> {
        if let Some(coordinator) = &mut self.agent_coordinator {
            // Create execution context for agents
            let tool_registry_arc = std::sync::Arc::new(self.tool_registry.clone());
            let llm_client = std::sync::Arc::new(GroqLlmClient::new(
                self.api_key.clone(),
                self.current_model.as_str().to_string()
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
                }
            }).collect();

            let context = ExecutionContext {
                workspace_dir: self.work_dir.clone(),
                session_id: format!("session_{}", chrono::Utc::now().timestamp()),
                tool_registry: tool_registry_arc,
                llm_client,
                conversation_history,
            };

            // Process request through coordinator
            let result = coordinator.process_user_request(user_request, &context).await?;

            // Update message history
            self.messages.push(Message {
                role: "user".to_string(),
                content: user_request.to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });

            self.messages.push(Message {
                role: "assistant".to_string(),
                content: result.content.clone(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
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
            _ => anyhow::bail!("Unknown model: {}. Available: 'blu_model', 'grn_model'", model_str),
        };

        if new_model == self.current_model {
            return Ok(format!(
                "Already using {} model",
                self.current_model.display_name()
            ));
        }

        let old_model = self.current_model.clone();
        self.current_model = new_model.clone();

        Ok(format!(
            "Switched from {} to {} - Reason: {}",
            old_model.display_name(),
            new_model.display_name(),
            reason
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

                let context = ToolContext::new(
                    self.work_dir.clone(),
                    format!("session_{}", chrono::Utc::now().timestamp()),
                    self.policy_manager.clone()
                );

                let result = self.tool_registry.execute_tool(name, params, &context).await;

                if result.success {
                    Ok(result.content)
                } else {
                    Err(anyhow::anyhow!("Tool '{}' failed: {}", name, result.error.unwrap_or_else(|| "Unknown error".to_string())))
                }
            }
        }
    }

    async fn summarize_and_trim_history(&mut self) -> Result<()> {
        const MAX_MESSAGES_BEFORE_SUMMARY: usize = 20;
        const KEEP_RECENT_MESSAGES: usize = 5;

        // Only summarize if we have enough messages
        if self.messages.len() <= MAX_MESSAGES_BEFORE_SUMMARY {
            return Ok(());
        }

        // Use the "other" model for summarization
        let summary_model = match self.current_model {
            ModelType::BluModel => ModelType::GrnModel,
            ModelType::GrnModel => ModelType::BluModel,
            ModelType::Custom(_) => ModelType::BluModel, // Default to BluModel for custom models
        };

        println!(
            "{} History getting long ({} messages). Asking {} to summarize...",
            "📝".yellow(),
            self.messages.len(),
            summary_model.display_name()
        );

        // Keep system message and recent messages
        let system_message = self.messages.first().cloned();
        let recent_messages: Vec<Message> = self.messages
            .iter()
            .rev()
            .take(KEEP_RECENT_MESSAGES)
            .rev()
            .cloned()
            .collect();

        // Get messages to summarize (everything except system and recent)
        let to_summarize: Vec<Message> = self.messages
            .iter()
            .skip(1) // Skip system
            .take(self.messages.len() - KEEP_RECENT_MESSAGES - 1)
            .cloned()
            .collect();

        // Build summary request
        let mut summary_history = vec![Message {
            role: "system".to_string(),
            content: format!(
                "You are {}. You are being asked to summarize a conversation that was handled by {}. \
                After summarizing, you may recommend switching to yourself if you believe you would be \
                better suited for the ongoing work based on the context.",
                summary_model.display_name(),
                self.current_model.display_name()
            ),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }];

        // Format the conversation to summarize
        let conversation_text = to_summarize.iter()
            .map(|m| {
                let role = &m.role;
                let content = if m.content.len() > 500 {
                    format!("{}... [truncated]", &m.content[..500])
                } else {
                    m.content.clone()
                };
                format!("{}: {}", role, content)
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        summary_history.push(Message {
            role: "user".to_string(),
            content: format!(
                "Summarize this conversation history in 2-3 concise sentences, focusing on key context, decisions, and file changes:\n\n{}\n\n\
                Then, based on the recent context and what seems to be the ongoing work, add a separate line starting with 'RECOMMENDATION: ' \
                followed by either 'STAY' (keep current model) or 'SWITCH' (switch to you) and briefly explain why in one sentence.",
                conversation_text
            ),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });

        // Call API to get summary using the OTHER model
        let request = ChatRequest {
            model: summary_model.as_str().to_string(),
            messages: summary_history,
            tools: vec![],
            tool_choice: "none".to_string(),
            stream: None,
        };

        let response = self.client
            .post(GROQ_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            // If summarization fails, just trim without summarizing
            println!("{} Summarization failed, doing simple trim", "⚠️".yellow());
            self.messages = vec![system_message.unwrap()];
            self.messages.extend(recent_messages);
            return Ok(());
        }

        let response_text = response.text().await?;
        let chat_response: ChatResponse = serde_json::from_str(&response_text)?;

        if let Some(summary_msg) = chat_response.choices.into_iter().next().map(|c| c.message) {
            let full_response = summary_msg.content;

            // Parse recommendation
            let (summary, recommendation_text) = if let Some(rec_pos) = full_response.find("RECOMMENDATION:") {
                let summary = full_response[..rec_pos].trim().to_string();
                let recommendation = full_response[rec_pos..].trim().to_string();

                println!("{} {}", "💡".bright_cyan(), recommendation);
                (summary, Some(recommendation))
            } else {
                (full_response, None)
            };

            // Rebuild history with summary
            let mut new_history = vec![];

            if let Some(sys_msg) = system_message {
                new_history.push(sys_msg);
            }

            // Add summary as a system-level context message
            new_history.push(Message {
                role: "system".to_string(),
                content: format!("Previous conversation summary: {}", summary),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });

            // Add recent messages
            new_history.extend(recent_messages);

            self.messages = new_history;

            println!(
                "{} History summarized and trimmed to {} messages",
                "✅".green(),
                self.messages.len()
            );

            // If there's a SWITCH recommendation, ask the current model to decide
            if let Some(rec_text) = recommendation_text {
                if rec_text.contains("SWITCH") {
                    println!(
                        "{} {} suggests switching. Asking {} to decide...",
                        "🤔".yellow(),
                        summary_model.display_name(),
                        self.current_model.display_name()
                    );

                    // Ask current model to decide
                    let decision_prompt = vec![
                        Message {
                            role: "system".to_string(),
                            content: format!(
                                "You are {}. You have been handling this conversation.",
                                self.current_model.display_name()
                            ),
                            tool_calls: None,
                            tool_call_id: None,
                            name: None,
                        },
                        Message {
                            role: "user".to_string(),
                            content: format!(
                                "{} has reviewed the conversation history and made the following recommendation:\n\n{}\n\n\
                                Based on this recommendation and your understanding of the current context, do you agree to switch to {}? \
                                Respond with only 'AGREE' or 'DECLINE' followed by a brief one-sentence explanation.",
                                summary_model.display_name(),
                                rec_text,
                                summary_model.display_name()
                            ),
                            tool_calls: None,
                            tool_call_id: None,
                            name: None,
                        },
                    ];

                    let decision_request = ChatRequest {
                        model: self.current_model.as_str().to_string(),
                        messages: decision_prompt,
                        tools: vec![],
                        tool_choice: "none".to_string(),
                        stream: None,
                    };

                    let decision_response = self.client
                        .post(GROQ_API_URL)
                        .header("Authorization", format!("Bearer {}", self.api_key))
                        .header("Content-Type", "application/json")
                        .json(&decision_request)
                        .send()
                        .await?;

                    if decision_response.status().is_success() {
                        let decision_text = decision_response.text().await?;
                        if let Ok(decision_chat) = serde_json::from_str::<ChatResponse>(&decision_text) {
                            if let Some(decision_msg) = decision_chat.choices.into_iter().next().map(|c| c.message) {
                                let decision = decision_msg.content;
                                println!("{} {} says: {}", "💬".bright_green(), self.current_model.display_name(), decision);

                                if decision.to_uppercase().contains("AGREE") {
                                    println!(
                                        "{} Switching to {} by mutual agreement",
                                        "🔄".bright_cyan(),
                                        summary_model.display_name()
                                    );
                                    self.current_model = summary_model;

                                    // Update system message
                                    if let Some(sys_msg) = self.messages.first_mut() {
                                        if sys_msg.role == "system" {
                                            sys_msg.content = Self::get_system_prompt(&self.current_model);
                                        }
                                    }
                                } else {
                                    println!(
                                        "{} Staying with {}",
                                        "✋".yellow(),
                                        self.current_model.display_name()
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Attempt to repair malformed tool calls using a separate API call to a model
    async fn repair_tool_call_with_model(&self, tool_call: &ToolCall, error_msg: &str) -> Result<ToolCall> {
        eprintln!("{} Attempting to repair tool call '{}' using AI...", "🔧".bright_yellow(), tool_call.function.name);

        let repair_prompt = format!(
            "A tool call failed with a validation error. Please fix the JSON arguments.\n\n\
            Tool name: {}\n\
            Original arguments (malformed): {}\n\
            Error: {}\n\n\
            Requirements:\n\
            - Return ONLY the corrected JSON arguments as a valid JSON object\n\
            - Do not include any explanation, markdown formatting, or extra text\n\
            - Ensure all field types match the schema (integers as numbers, not strings)\n\
            - Common issues: trailing quotes after numbers, string instead of integer values\n\n\
            Corrected JSON arguments:",
            tool_call.function.name,
            tool_call.function.arguments,
            error_msg
        );

        // Create a simple repair request using Kimi (fast and good at structured output)
        let repair_request = ChatRequest {
            model: ModelType::BluModel.as_str().to_string(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: "You are a JSON repair assistant. Return only valid JSON, no explanations.".to_string(),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
                Message {
                    role: "user".to_string(),
                    content: repair_prompt,
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
            ],
            tools: vec![], // No tools for repair request
            tool_choice: "none".to_string(),
            stream: None,
        };

        // Make API call
        let response = self.client
            .post(GROQ_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&repair_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("Repair API call failed: {}", error_text);
        }

        let api_response: ChatResponse = response.json().await?;

        if let Some(choice) = api_response.choices.first() {
            let repaired_json = choice.message.content.trim();

            // Validate the repaired JSON
            if let Ok(_) = serde_json::from_str::<serde_json::Value>(repaired_json) {
                eprintln!("{} Successfully repaired tool call arguments", "✓".bright_green());

                // Return repaired tool call
                Ok(ToolCall {
                    id: tool_call.id.clone(),
                    tool_type: tool_call.tool_type.clone(),
                    function: FunctionCall {
                        name: tool_call.function.name.clone(),
                        arguments: repaired_json.to_string(),
                    },
                })
            } else {
                anyhow::bail!("Repaired JSON is still invalid: {}", repaired_json)
            }
        } else {
            anyhow::bail!("No response from repair API call")
        }
    }

    fn validate_and_fix_tool_calls(&self, messages: &mut Vec<Message>) -> Result<bool> {
        let mut fixed_any = false;

        for message in messages.iter_mut() {
            if let Some(tool_calls) = &mut message.tool_calls {
                for tool_call in tool_calls.iter_mut() {
                    let original_args = tool_call.function.arguments.clone();

                    // Try to parse the arguments as JSON
                    match serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments) {
                        Ok(mut json_args) => {
                            // Validate and fix based on tool name
                            let mut needs_fix = false;

                            match tool_call.function.name.as_str() {
                                "open_file" | "read_file" => {
                                    // Check if start_line or end_line are strings instead of integers
                                    if let Some(obj) = json_args.as_object_mut() {
                                        // Check start_line
                                        let start_fix = obj.get("start_line")
                                            .and_then(|v| v.as_str())
                                            .and_then(|s| s.parse::<i64>().ok());

                                        if let Some(num) = start_fix {
                                            obj.insert("start_line".to_string(), serde_json::json!(num));
                                            needs_fix = true;
                                            eprintln!("{} Fixed start_line: string → integer {}", "🔧".yellow(), num);
                                        }

                                        // Check end_line
                                        let end_fix = obj.get("end_line")
                                            .and_then(|v| v.as_str())
                                            .and_then(|s| s.parse::<i64>().ok());

                                        if let Some(num) = end_fix {
                                            obj.insert("end_line".to_string(), serde_json::json!(num));
                                            needs_fix = true;
                                            eprintln!("{} Fixed end_line: string → integer {}", "🔧".yellow(), num);
                                        }
                                    }
                                }
                                "search_files" => {
                                    // Check if max_results is a string
                                    if let Some(obj) = json_args.as_object_mut() {
                                        let max_fix = obj.get("max_results")
                                            .and_then(|v| v.as_str())
                                            .and_then(|s| s.parse::<i64>().ok());

                                        if let Some(num) = max_fix {
                                            obj.insert("max_results".to_string(), serde_json::json!(num));
                                            needs_fix = true;
                                            eprintln!("{} Fixed max_results: string → integer {}", "🔧".yellow(), num);
                                        }
                                    }
                                }
                                _ => {}
                            }

                            if needs_fix {
                                tool_call.function.arguments = serde_json::to_string(&json_args)?;
                                fixed_any = true;
                            }
                        }
                        Err(e) => {
                            // JSON parsing failed - try to fix common issues
                            let mut fixed_args = original_args.clone();

                            // Common issue: trailing quote after number (e.g., "end_line": 60")
                            // Pattern: number followed by quote and closing brace
                            let re = regex::Regex::new(r#":\s*(\d+)"\s*([,}])"#)?;
                            if re.is_match(&fixed_args) {
                                fixed_args = re.replace_all(&fixed_args, ": $1$2").to_string();
                                eprintln!("{} Fixed malformed JSON: removed trailing quotes after numbers", "🔧".yellow());

                                // Verify the fix worked
                                if serde_json::from_str::<serde_json::Value>(&fixed_args).is_ok() {
                                    tool_call.function.arguments = fixed_args;
                                    fixed_any = true;
                                } else {
                                    eprintln!("{} Failed to fix malformed JSON for tool {}: {}", "⚠️".red(), tool_call.function.name, e);
                                }
                            } else {
                                eprintln!("{} Malformed JSON for tool {}: {}", "⚠️".red(), tool_call.function.name, e);
                            }
                        }
                    }
                }
            }
        }

        Ok(fixed_any)
    }

    /// Handle streaming API response, displaying chunks as they arrive
    async fn call_api_streaming(&self, orig_messages: &[Message]) -> Result<(Message, Option<Usage>, ModelType, Vec<Message>)> {
        use std::io::{self, Write};
        use futures_util::StreamExt;

        let mut current_model = self.current_model.clone();
        let mut messages = orig_messages.to_vec().clone();

        // Validate and fix tool calls before sending
        if let Ok(fixed) = self.validate_and_fix_tool_calls(&mut messages) {
            if fixed {
                eprintln!("{} Tool calls were automatically fixed before sending to API", "✅".green());
            }
        }

        let request = ChatRequest {
            model: current_model.as_str().to_string(),
            messages: messages.clone(),
            tools: self.get_tools(),
            tool_choice: "auto".to_string(),
            stream: Some(true),
        };

        let response = self
            .client
            .post(GROQ_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "Unable to read error body".to_string());
            return Err(anyhow::anyhow!("API request failed with status {}: {}", status, error_body));
        }

        // Process streaming response
        let mut accumulated_content = String::new();
        let mut accumulated_tool_calls: Option<Vec<ToolCall>> = None;
        let mut role = String::new();
        let mut usage: Option<Usage> = None;
        let mut buffer = String::new();

        // Show thinking indicator
        print!("🤔 Thinking...");
        io::stdout().flush().unwrap();
        let mut first_chunk = true;

        // Read the response as a stream of bytes
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(bytes) => {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));

                    // Process complete lines (SSE format: "data: {json}\n\n")
                    while let Some(line_end) = buffer.find("\n\n") {
                        let line = buffer[..line_end].to_string();
                        buffer = buffer[line_end + 2..].to_string();

                        // Skip empty lines and non-data lines
                        if line.trim().is_empty() || !line.starts_with("data: ") {
                            continue;
                        }

                        let data = &line[6..]; // Skip "data: " prefix

                        // Check for stream end marker
                        if data.trim() == "[DONE]" {
                            break;
                        }

                        // Parse the JSON chunk
                        if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                            if let Some(usage_data) = chunk.usage {
                                usage = Some(usage_data);
                            }

                            if let Some(choice) = chunk.choices.first() {
                                let delta = &choice.delta;

                                // Update role if present
                                if let Some(r) = &delta.role {
                                    role = r.clone();
                                }

                                // Accumulate content and display it
                                if let Some(content) = &delta.content {
                                    if first_chunk {
                                        // Clear thinking indicator
                                        print!("\r\x1B[K");
                                        io::stdout().flush().unwrap();
                                        first_chunk = false;
                                    }

                                    accumulated_content.push_str(content);
                                    print!("{}", content);
                                    io::stdout().flush().unwrap();
                                }

                                // Accumulate tool calls if present
                                if let Some(tool_calls) = &delta.tool_calls {
                                    accumulated_tool_calls = Some(tool_calls.clone());
                                }
                            }
                        }
                    }
                }
                Err(e) => return Err(anyhow::anyhow!("Error reading stream: {}", e)),
            }
        }

        println!(); // New line after streaming complete

        // Build the final message
        let message = Message {
            role: if role.is_empty() { "assistant".to_string() } else { role },
            content: accumulated_content,
            tool_calls: accumulated_tool_calls,
            tool_call_id: None,
            name: None,
        };

        Ok((message, usage, current_model, messages))
    }

    async fn call_api(&self, orig_messages: &[Message]) -> Result<(Message, Option<Usage>, ModelType, Vec<Message>)> {
        let mut current_model = self.current_model.clone();
        let mut messages = orig_messages.to_vec().clone();


        // Retry logic with exponential backoff
        let mut retry_count = 0;
        loop {
            // Validate and fix tool calls before sending
            if let Ok(fixed) = self.validate_and_fix_tool_calls(&mut messages) {
                if fixed {
                    eprintln!("{} Tool calls were automatically fixed before sending to API", "✅".green());
                }
            }

	    let request = ChatRequest {
		model: current_model.as_str().to_string(),
		messages: messages.clone(),
		tools: self.get_tools(),
		tool_choice: "auto".to_string(),
		stream: None,
	    };
            let response = self
                .client
                .post(GROQ_API_URL)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await?;

            // Handle rate limiting with exponential backoff
            if response.status() == 429 {
                if retry_count >= MAX_RETRIES {
                    anyhow::bail!("Rate limit exceeded after {} retries", MAX_RETRIES);
                }

                let wait_time = Duration::from_secs(2u64.pow(retry_count));
                println!(
                    "{} Rate limited. Waiting {} seconds before retry {}/{}...",
                    "⏳".yellow(),
                    wait_time.as_secs(),
                    retry_count + 1,
                    MAX_RETRIES
                );
                sleep(wait_time).await;
                retry_count += 1;
                continue;
            }

            // Check for errors and provide detailed debugging
            if !response.status().is_success() {
                let status = response.status();
                let error_body = response.text().await.unwrap_or_else(|_| "Unable to read error body".to_string());

                // Check if this is a tool-related error
                if status == 400 && error_body.contains("tool_use_failed") {
                    eprintln!("{}", "❌ Tool calling error detected!".red().bold());
                    eprintln!("{}", error_body.yellow());

                    // Check for GrnModel hallucinating non-existent tools
                    if error_body.contains("attempted to call tool") && current_model == ModelType::GrnModel {
                        eprintln!("{}", "🔄 GrnModel attempted to use non-existent tool. Switching to BluModel and retrying...".bright_cyan());

                        // Switch to BluModel
                        current_model = ModelType::BluModel;

                        // Update system message
                        if let Some(sys_msg) = messages.first_mut() {
                            if sys_msg.role == "system" {
                                sys_msg.content = Self::get_system_prompt(&current_model);
                            }
                        }

                        // Retry with BluModel - continue the loop to retry
                        retry_count = 0; // Reset retry count for new model
                        continue;
                    }
                    // Check for BluModel generating malformed tool calls
                    else if (error_body.contains("Failed to call a function") ||
                             error_body.contains("tool call validation failed") ||
                             error_body.contains("parameters for tool") ||
                             error_body.contains("did not match schema")) &&
                            current_model == ModelType::BluModel {
                        eprintln!("{}", "❌ BluModel generated malformed tool call (invalid JSON/parameters).".red());

                        // First, try to repair the tool call using AI
                        let mut repaired = false;

                        // Find the last assistant message with tool calls
                        if let Some(last_assistant_msg) = messages.iter_mut().rev().find(|m| m.role == "assistant" && m.tool_calls.is_some()) {
                            if let Some(tool_calls) = &last_assistant_msg.tool_calls {
                                eprintln!("{} Attempting AI-powered repair before switching models...", "🔧".bright_yellow());

                                // Try to repair each tool call
                                let mut repaired_calls = Vec::new();
                                for tc in tool_calls {
                                    match self.repair_tool_call_with_model(tc, &error_body).await {
                                        Ok(repaired_tc) => {
                                            repaired_calls.push(repaired_tc);
                                        }
                                        Err(e) => {
                                            eprintln!("{} Failed to repair tool call '{}': {}", "⚠️".yellow(), tc.function.name, e);
                                            // If repair fails, keep original
                                            repaired_calls.push(tc.clone());
                                        }
                                    }
                                }

                                // Update the message with repaired tool calls
                                last_assistant_msg.tool_calls = Some(repaired_calls);
                                repaired = true;
                                eprintln!("{} Retrying with repaired tool calls...", "🔄".bright_cyan());
                            }
                        }

                        if repaired {
                            // Retry with repaired tool calls
                            retry_count = 0;
                            continue;
                        }

                        // If repair failed or wasn't possible, switch to GrnModel as fallback
                        eprintln!("{}", "🔄 Repair failed. Switching to GrnModel and retrying...".bright_cyan());

                        // Switch to GrnModel
                        current_model = ModelType::GrnModel;

                        // Update system message
                        if let Some(sys_msg) = messages.first_mut() {
                            if sys_msg.role == "system" {
                                sys_msg.content = Self::get_system_prompt(&current_model);
                            }
                        }

                        // Retry with GrnModel - continue the loop to retry
                        retry_count = 0; // Reset retry count for new model
                        continue;
                    }
                }

                eprintln!("{}", "=== API Error Details ===".red());
                eprintln!("Status: {}", status);
                eprintln!("Error body: {}", error_body);

                // Try to show the request that caused the error
                eprintln!("\n{}", "Request details:".yellow());
                eprintln!("Messages count: {}", messages.len());
                if let Ok(req_json) = serde_json::to_string_pretty(&request) {
                    // Truncate very long requests
                    if req_json.len() > 2000 {
                        eprintln!("Request (truncated): {}...", &req_json[..2000]);
                    } else {
                        eprintln!("Request: {}", req_json);
                    }
                }
                eprintln!("{}", "======================".red());

                return Err(anyhow::anyhow!("API request failed with status {}: {}", status, error_body));
            }

            let response_text = response.text().await?;
            let chat_response: ChatResponse = serde_json::from_str(&response_text)
                .with_context(|| format!("Failed to parse API response: {}", response_text))?;

            let message = chat_response
                .choices
                .into_iter()
                .next()
                .map(|c| c.message)
                .context("No response from API")?;

            return Ok((message, chat_response.usage, current_model, messages));
        }
    }

    async fn chat(&mut self, user_message: &str) -> Result<String> {
        self.messages.push(Message {
            role: "user".to_string(),
            content: user_message.to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });

        // Summarize ONCE before starting the tool-calling loop, not during it
        // This prevents discarding recent tool results mid-conversation
        self.summarize_and_trim_history().await?;

        let mut tool_call_iterations = 0;
        let mut recent_tool_calls: Vec<String> = Vec::new(); // Track recent tool calls
        const MAX_TOOL_ITERATIONS: usize = 100; // Increased limit with intelligent evaluation
        const LOOP_DETECTION_WINDOW: usize = 6; // Check last 6 tool calls
        const PROGRESS_EVAL_INTERVAL: u32 = 15; // Evaluate progress every 15 tool calls

        // Initialize progress evaluator for all operations
        let mut progress_evaluator = Some(crate::agents::progress_evaluator::ProgressEvaluator::new(
            std::sync::Arc::new(crate::agents::groq_client::GroqLlmClient::new(
                self.api_key.clone(),
                "kimi".to_string()
            )),
            0.6, // Minimum confidence threshold
            PROGRESS_EVAL_INTERVAL,
        ));

        // Track tool calls for progress evaluation
        let mut tool_call_history: Vec<crate::agents::progress_evaluator::ToolCallInfo> = Vec::new();
        let mut files_changed: std::collections::HashSet<String> = std::collections::HashSet::new();
        let start_time = std::time::Instant::now();
        let mut errors_encountered: Vec<String> = Vec::new();

        loop {
            let (response, usage, current_model, messages) = if self.stream_responses {
                self.call_api_streaming(&self.messages).await?
            } else {
                self.call_api(&self.messages).await?
            };
            self.messages = messages;
            if self.current_model != current_model {
                println!("Forced model switch: {:?} -> {:?}", &self.current_model, &current_model);
                self.current_model = current_model;
            }

            // Display token usage
            if let Some(usage) = &usage {
                self.total_tokens_used += usage.total_tokens;
                println!(
                    "{} Prompt: {} | Completion: {} | Total: {} | Session: {}",
                    "📊".bright_black(),
                    usage.prompt_tokens.to_string().bright_black(),
                    usage.completion_tokens.to_string().bright_black(),
                    usage.total_tokens.to_string().bright_black(),
                    self.total_tokens_used.to_string().cyan()
                );
            }

            if let Some(tool_calls) = &response.tool_calls {
                tool_call_iterations += 1;

                // Check for repeated identical tool calls (actual loop detection)
                let tool_signature = tool_calls.iter()
                    .map(|tc| format!("{}:{}", tc.function.name, tc.function.arguments))
                    .collect::<Vec<_>>()
                    .join("|");

                recent_tool_calls.push(tool_signature.clone());

                // Keep only recent tool calls
                if recent_tool_calls.len() > LOOP_DETECTION_WINDOW {
                    recent_tool_calls.remove(0);
                }

                // Detect if the same tool call appears too many times in the recent window
                let repetition_count = recent_tool_calls.iter()
                    .filter(|&sig| sig == &tool_signature)
                    .count();

                if repetition_count >= 3 {
                    eprintln!(
                        "{} Detected repeated tool call pattern (same call {} times in recent history). Likely stuck in a loop.",
                        "⚠️".red().bold(),
                        repetition_count
                    );
                    self.messages.push(Message {
                        role: "assistant".to_string(),
                        content: format!(
                            "I apologize, but I'm calling the same tool repeatedly without making progress. \
                            The tool call pattern is repeating. Please try breaking down your request into smaller, \
                            more specific steps, or provide additional guidance."
                        ),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                    return Ok("Repeated tool call pattern detected. Please refine your request.".to_string());
                }

                // Intelligent progress evaluation (replaces hard limit)
                if let Some(ref mut evaluator) = progress_evaluator {
                    // Debug: Show evaluation check
                    if tool_call_iterations % PROGRESS_EVAL_INTERVAL as usize == 0 && tool_call_iterations > 0 {
                        eprintln!("[DEBUG] Checking if evaluation should trigger at iteration {} (interval: {})",
                                 tool_call_iterations, PROGRESS_EVAL_INTERVAL);
                    }

                    if evaluator.should_evaluate(tool_call_iterations as u32) {
                        println!("{}", format!("🧠 Evaluating progress after {} tool calls...", tool_call_iterations).bright_blue());
                        eprintln!("[DEBUG] Progress evaluation triggered at iteration {}", tool_call_iterations);

                        // Create tool call summary
                        let mut tool_usage = std::collections::HashMap::new();
                        for call in &tool_call_history {
                            *tool_usage.entry(call.tool_name.clone()).or_insert(0) += 1;
                        }

                        let summary = crate::agents::progress_evaluator::ToolCallSummary {
                            total_calls: tool_call_iterations as u32,
                            tool_usage,
                            recent_calls: tool_call_history.iter().rev().take(10).cloned().collect(),
                            current_task: "Executing user request with tools".to_string(),
                            original_request: user_message.to_string(),
                            elapsed_seconds: start_time.elapsed().as_secs(),
                            errors: errors_encountered.clone(),
                            files_changed: files_changed.iter().cloned().collect(),
                        };

                        match evaluator.evaluate_progress(&summary).await {
                            Ok(evaluation) => {
                                println!("{}", format!("🎯 Progress Evaluation: {:.0}% complete", evaluation.progress_percentage * 100.0).bright_green());
                                println!("{}", format!("📊 Confidence: {:.0}%", evaluation.confidence * 100.0).bright_black());

                                if !evaluation.recommendations.is_empty() {
                                    println!("{}", "💡 Recommendations:".bright_cyan());
                                    for rec in &evaluation.recommendations {
                                        println!("  • {}", rec);
                                    }
                                }

                                if !evaluation.should_continue {
                                    println!("{}", "🛑 Agent evaluation suggests stopping or changing strategy".yellow());
                                    self.messages.push(Message {
                                        role: "assistant".to_string(),
                                        content: format!(
                                            "Based on progress evaluation: {}\n\nRecommendations:\n{}\n\nReasoning: {}",
                                            if evaluation.change_strategy {
                                                "I should change my approach."
                                            } else {
                                                "I should stop and ask for guidance."
                                            },
                                            evaluation.recommendations.join("\n"),
                                            evaluation.reasoning
                                        ),
                                        tool_calls: None,
                                        tool_call_id: None,
                                        name: None,
                                    });
                                    return Ok("Intelligent progress evaluation suggested stopping this approach.".to_string());
                                }

                                if evaluation.change_strategy {
                                    println!("{}", "🔄 Agent evaluation suggests changing strategy".bright_yellow());
                                    self.messages.push(Message {
                                        role: "system".to_string(),
                                        content: format!(
                                            "Progress evaluation suggests changing approach. Reasoning: {}\nRecommendations:\n{}",
                                            evaluation.reasoning,
                                            evaluation.recommendations.join("\n")
                                        ),
                                        tool_calls: None,
                                        tool_call_id: None,
                                        name: None,
                                    });
                                }
                            }
                            Err(e) => {
                                eprintln!("{} Progress evaluation failed: {}", "⚠️".yellow(), e);
                                // Continue with conservative fallback
                            }
                        }
                    }
                }

                // Conservative hard limit as final fallback
                if tool_call_iterations > MAX_TOOL_ITERATIONS {
                    eprintln!(
                        "{} Reached maximum tool call limit ({} iterations).",
                        "⚠️".yellow(),
                        MAX_TOOL_ITERATIONS
                    );
                    self.messages.push(Message {
                        role: "assistant".to_string(),
                        content: format!(
                            "I've made {} tool calls for this request. Despite intelligent progress evaluation, \
                            I've reached the safety limit. Please break this down into smaller tasks or provide more specific direction.",
                            tool_call_iterations
                        ),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                    return Ok(format!(
                        "Reached maximum tool call limit ({} iterations). Please simplify your request.",
                        tool_call_iterations
                    ));
                }

                self.messages.push(response.clone());

                // Log assistant message with tool calls
                if let Some(logger) = &mut self.logger {
                    let tool_call_info: Vec<(String, String, String)> = tool_calls
                        .iter()
                        .map(|tc| (
                            tc.id.clone(),
                            tc.function.name.clone(),
                            tc.function.arguments.clone()
                        ))
                        .collect();

                    if std::env::var("DEBUG_LOG").is_ok() {
                        eprintln!("[DEBUG] Logging {} tool calls", tool_call_info.len());
                    }

                    let model_name = self.current_model.as_str();
                    logger.log_with_tool_calls(
                        "assistant",
                        &response.content,
                        Some(&model_name),
                        tool_call_info,
                    ).await;
                }

                for tool_call in tool_calls {
                    println!(
                        "{} {} with args: {} (iteration {}/{})",
                        "🔧 Calling tool:".yellow(),
                        tool_call.function.name.cyan(),
                        tool_call.function.arguments.bright_black(),
                        tool_call_iterations,
                        MAX_TOOL_ITERATIONS
                    );

                    let tool_start_time = std::time::Instant::now();
                    let result = match self.execute_tool(
                        &tool_call.function.name,
                        &tool_call.function.arguments,
                    ).await {
                        Ok(r) => r,
                        Err(e) => {
                            let error_msg = e.to_string();

                            // Track error for progress evaluation
                            errors_encountered.push(format!("{}: {}", tool_call.function.name, error_msg));
                            // Make cancellation errors very explicit to the model
                            if error_msg.contains("cancelled by user") ||
                               error_msg.contains("Edit cancelled") ||
                               error_msg.contains("Command cancelled") {
                                // Extract user's comment if present
                                let user_feedback = if error_msg.contains(" - ") {
                                    error_msg.split(" - ").skip(1).collect::<Vec<_>>().join(" - ")
                                } else {
                                    String::new()
                                };

                                let feedback_section = if !user_feedback.is_empty() {
                                    format!("\n\nUSER'S FEEDBACK: {}\nThis feedback explains why the operation was cancelled. Address this concern in your next approach.", user_feedback)
                                } else {
                                    String::new()
                                };

                                format!(
                                    "OPERATION CANCELLED BY USER. The user explicitly cancelled this operation. \
                                    DO NOT retry this same approach. Please acknowledge the cancellation and either:\n\
                                    1. Ask the user what they would like to do instead\n\
                                    2. Try a completely different approach that addresses the user's concerns\n\
                                    3. Stop if this was the only viable option\
                                    {}\n\
                                    \nOriginal message: {}",
                                    feedback_section,
                                    error_msg
                                )
                            } else {
                                format!("Error: {}", error_msg)
                            }
                        }
                    };

                    // Display result to user (truncate for file reading tools)
                    let display_result = if tool_call.function.name == "open_file" || tool_call.function.name == "read_file" {
                        let lines: Vec<&str> = result.lines().collect();
                        if lines.len() > 10 {
                            let first_10 = lines[..10].join("\n");
                            let remaining = lines.len() - 10;
                            format!("{}\n\n...and {} more lines", first_10, remaining)
                        } else {
                            result.clone()
                        }
                    } else {
                        result.clone()
                    };

                    println!("{} {}", "📋 Result:".green(), display_result.bright_black());

                    // Log tool result
                    if let Some(logger) = &mut self.logger {
                        if std::env::var("DEBUG_LOG").is_ok() {
                            eprintln!("[DEBUG] Logging tool result for {}", tool_call.function.name);
                        }
                        logger.log_tool_result(
                            &result,
                            &tool_call.id,
                            &tool_call.function.name,
                        ).await;
                    }

                    // Track tool call for progress evaluation
                    let duration = tool_start_time.elapsed();
                    let result_summary = if result.len() > 200 {
                        format!("{} (truncated)", &result[..200])
                    } else {
                        result.clone()
                    };

                    // Track files that were changed
                    if tool_call.function.name.contains("write_file") ||
                       tool_call.function.name.contains("edit_file") {
                        if let Ok(args) = serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments) {
                            if let Some(file_path) = args.get("file_path").and_then(|v| v.as_str()) {
                                files_changed.insert(file_path.to_string());
                            }
                        }
                    }

                    let call_info = crate::agents::progress_evaluator::ToolCallInfo {
                        tool_name: tool_call.function.name.clone(),
                        parameters: tool_call.function.arguments.clone(),
                        success: !result.contains("failed") && !result.contains("cancelled"),
                        duration_ms: duration.as_millis() as u64,
                        result_summary: Some(result_summary),
                    };
                    tool_call_history.push(call_info);

                    self.messages.push(Message {
                        role: "tool".to_string(),
                        content: result,
                        tool_calls: None,
                        tool_call_id: Some(tool_call.id.clone()),
                        name: Some(tool_call.function.name.clone()),
                    });
                }
            } else {
                self.messages.push(response.clone());
                return Ok(response.content);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file if it exists
    dotenvy::dotenv().ok();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Determine API URLs for each model
    // Priority: specific flags (--api-url-blu-model, --api-url-grn-model) override general flag (--llama-cpp-url)
    let api_url_blu_model = cli.api_url_blu_model.or_else(|| cli.llama_cpp_url.clone());
    let api_url_grn_model = cli.api_url_grn_model.or_else(|| cli.llama_cpp_url.clone());

    // API key is only required if at least one model uses Groq (no API URL specified)
    let using_groq = api_url_blu_model.is_none() || api_url_grn_model.is_none();
    let api_key = if using_groq {
        env::var("GROQ_API_KEY")
            .context("GROQ_API_KEY environment variable not set. Use --api-url-blu-model and/or --api-url-grn-model to use llama.cpp instead of Groq.")?
    } else {
        // Both models use llama.cpp, no API key needed
        String::new()
    };

    // Use current directory as work_dir so the AI can see project files
    // NB: do NOT use the 'workspace' subdirectory as work_dir
    let work_dir = env::current_dir()?;

    // If a subcommand was provided, execute it and exit
    if let Some(command) = cli.command {
        let result = command.execute().await?;
        println!("{}", result);
        return Ok(());
    }

    // Create client configuration from CLI arguments
    // Priority: specific flags override general --model flag
    let client_config = ClientConfig {
        api_key: api_key.clone(),
        api_url_blu_model,
        api_url_grn_model,
        model_blu_model_override: cli.model_blu_model.clone().or_else(|| cli.model.clone()),
        model_grn_model_override: cli.model_grn_model.clone().or_else(|| cli.model.clone()),
    };

    // Create policy manager based on CLI arguments
    let policy_manager = if cli.auto_confirm {
        eprintln!("{} Auto-confirm mode enabled - all actions will be approved automatically", "🚀".green());
        PolicyManager::allow_all()
    } else if cli.policy_file.is_some() || cli.learn_policies {
        let policy_file = cli.policy_file.unwrap_or_else(|| "policies.toml".to_string());
        let policy_path = work_dir.join(&policy_file);
        match PolicyManager::from_file(&policy_path, cli.learn_policies) {
            Ok(pm) => {
                eprintln!("{} Loaded policy file: {}", "📋".cyan(), policy_path.display());
                if cli.learn_policies {
                    eprintln!("{} Policy learning enabled - user decisions will be saved to policy file", "📚".cyan());
                }
                pm
            }
            Err(e) => {
                eprintln!("{} Failed to load policy file: {}", "⚠️".yellow(), e);
                eprintln!("{} Using default policy (ask for confirmation)", "📋".cyan());
                PolicyManager::new()
            }
        }
    } else {
        PolicyManager::new()
    };

    // Handle task mode if requested
    if let Some(task_text) = cli.task {
        println!("{}", "🤖 Kimi Chat - Task Mode".bright_cyan().bold());
        println!("{}", format!("Working directory: {}", work_dir.display()).bright_black());

        if cli.agents {
            println!("{}", "🚀 Multi-Agent System ENABLED".green().bold());
        }

        println!("{}", format!("Task: {}", task_text).bright_yellow());
        println!();

        let mut chat = KimiChat::new_with_config(client_config.clone(), work_dir.clone(), cli.agents, policy_manager.clone(), cli.stream);

        // Initialize logger for task mode
        chat.logger = match ConversationLogger::new_task_mode(&chat.work_dir).await {
            Ok(l) => Some(l),
            Err(e) => {
                eprintln!("Task logging disabled: {}", e);
                None
            }
        };

        let response = if chat.use_agents && chat.agent_coordinator.is_some() {
            // Use agent system
            match chat.process_with_agents(&task_text).await {
                Ok(response) => response,
                Err(e) => {
                    eprintln!("{} {}\n", "Agent Error:".bright_red().bold(), e);
                    // Fallback to regular chat
                    match chat.chat(&task_text).await {
                        Ok(response) => response,
                        Err(e) => {
                            eprintln!("{} {}\n", "Error:".bright_red().bold(), e);
                            return Ok(());
                        }
                    }
                }
            }
        } else {
            // Use regular chat
            match chat.chat(&task_text).await {
                Ok(response) => response,
                Err(e) => {
                    eprintln!("{} {}\n", "Error:".bright_red().bold(), e);
                    return Ok(());
                }
            }
        };

        if cli.pretty {
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "response": response,
                "agents_used": chat.use_agents
            })).unwrap_or_else(|_| response.to_string()));
        } else {
            println!("{}", response);
        }

        return Ok(());
    }

    // If interactive flag is set (or default), proceed to REPL
    if !cli.interactive {
        // If not interactive and no subcommand, just exit
        println!("No subcommand provided and interactive mode not requested. Exiting.");
        return Ok(());
    }

    println!("{}", "🤖 Kimi Chat - Claude Code-like Experience".bright_cyan().bold());
    println!("{}", format!("Working directory: {}", work_dir.display()).bright_black());
    println!("{}", "Default model: GrnModel/GPT-OSS-120B (cost-efficient) • Auto-switches to BluModel/Kimi-K2-Instruct-0905 when needed".bright_black());

    if cli.agents {
        println!("{}", "🚀 Multi-Agent System ENABLED - Specialized agents will handle your tasks".green().bold());
    }

    println!("{}", "Type 'exit' or 'quit' to exit\n".bright_black());

    let mut chat = KimiChat::new_with_config(client_config, work_dir, cli.agents, policy_manager, cli.stream);
    // Initialize logger (async) – logs go into the workspace directory
    chat.logger = match ConversationLogger::new(&chat.work_dir).await {
        Ok(l) => Some(l),
        Err(e) => {
            eprintln!("Logging disabled: {}", e);
            None
        }
    };

    // If logger was created, log the initial system message that KimiChat::new added
    if let Some(logger) = &mut chat.logger {
        // The first message in chat.messages is the system prompt
        if let Some(sys_msg) = chat.messages.first() {
            logger
                .log(
                    "system",
                    &sys_msg.content,
                    None,
                    false,
                )
                .await;
        }
    }

    let mut rl = DefaultEditor::new()?;

    // Read kimi.md if it exists to get project context
    let kimi_context = if let Ok(kimi_content) = chat.read_file("kimi.md") {
        println!("{} {}", "📖".bright_cyan(), "Reading project context from kimi.md...".bright_black());
        kimi_content
    } else {
        println!("{} {}", "📖".bright_cyan(), "No kimi.md found. Starting fresh.".bright_black());
        String::new()
    };

    if !kimi_context.is_empty() {
        let sys_msg = Message {
            role: "system".to_string(),
            content: format!("Project context: {}", kimi_context),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        };
        // Log this system addition
        if let Some(logger) = &mut chat.logger {
            logger
                .log("system", &sys_msg.content, None, false)
                .await;
        }
        chat.messages.push(sys_msg);
    }

    loop {
        let model_indicator = format!("[{}]", chat.current_model.display_name()).bright_magenta();
        let readline = rl.readline(&format!("{} {} ", model_indicator, "You:".bright_green().bold()));

        match readline {
            Ok(line) => {
                let line = line.trim();

                if line.is_empty() {
                    continue;
                }

                if line == "exit" || line == "quit" {
                    println!("{}", "Goodbye!".bright_cyan());
                    break;
                }

                rl.add_history_entry(line)?;

                // Log the user message before sending
                if let Some(logger) = &mut chat.logger {
                    logger.log("user", line, None, false).await;
                }

                // Show thinking indicator when streaming is enabled
                if chat.stream_responses {
                    use std::io::{self, Write};
                    print!("{} ", "🤔 Thinking...".bright_black());
                    io::stdout().flush().unwrap();
                }

                let response = if chat.use_agents && chat.agent_coordinator.is_some() {
                    // Use agent system
                    match chat.process_with_agents(line).await {
                        Ok(response) => response,
                        Err(e) => {
                            eprintln!("{} {}\n", "Agent Error:".bright_red().bold(), e);
                            // Fallback to regular chat
                            match chat.chat(line).await {
                                Ok(response) => response,
                                Err(e) => {
                                    eprintln!("{} {}\n", "Error:".bright_red().bold(), e);
                                    continue;
                                }
                            }
                        }
                    }
                } else {
                    // Use regular chat
                    match chat.chat(line).await {
                        Ok(response) => response,
                        Err(e) => {
                            eprintln!("{} {}\n", "Error:".bright_red().bold(), e);
                            continue;
                        }
                    }
                };

                // Log assistant response
                if let Some(logger) = &mut chat.logger {
                    logger.log("assistant", &response, None, false).await;
                }

                // Display response if not streaming (streaming already displayed it)
                if !chat.stream_responses {
                    let model_label = format!("[{}]", chat.current_model.display_name()).bright_magenta();
                    let assistant_label = "Assistant:".bright_blue().bold();
                    println!("\n{} {} {}\n", model_label, assistant_label, response);
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("{}", "^C".bright_black());
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!("{}", "Goodbye!".bright_cyan());
                break;
            }
            Err(err) => {
                eprintln!("{} {}", "Error:".bright_red().bold(), err);
                break;
            }
        }
    }

    // Graceful shutdown of logger (flush & close)
    if let Some(logger) = &mut chat.logger {
        logger.shutdown().await;
    }

    Ok(())
}
