use anyhow::{Context, Result};
use colored::Colorize;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use clap::Parser;

// Import workspace crates
use kimichat_types::{ModelType, Message, ToolCall, FunctionCall, SwitchModelArgs};
use kimichat_models::{Tool, FunctionDef, ChatResponse, Usage};
use kimichat_policy::PolicyManager;
use kimichat_tools::{ToolRegistry, ToolParameters, ToolContext};
use kimichat_terminal::{self as terminal, TerminalManager};
use kimichat_skills as skills;
use kimichat::{ConversationLogger, save_state, load_state, summarize_and_trim_history, chat_loop, resolve_terminal_backend};
use kimichat::chat as chat_session;
use kimichat::{call_api, call_api_streaming, call_api_with_llm_client, call_api_streaming_with_llm_client};
use kimichat::tools_execution::validation::{repair_tool_call_with_model, validate_and_fix_tool_calls_in_place};
use kimichat_agents::{PlanningCoordinator, GroqLlmClient, ChatMessage, ExecutionContext, ToolCall as AgentToolCall, FunctionCall as AgentFunctionCall};

// Local modules
mod cli;
mod config;
mod app;
mod web;
mod todo;
mod open_file;
mod preview;

use kimichat::{Cli, Commands, TerminalCommands, ClientConfig, normalize_api_url, initialize_tool_registry, initialize_agent_system};
use kimichat::app::{setup_from_cli, run_task_mode, run_repl_mode};

const MAX_CONTEXT_TOKENS: usize = kimichat_types::MAX_CONTEXT_TOKENS;
const MAX_RETRIES: u32 = kimichat_types::MAX_RETRIES;

use kimichat::KimiChat;


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
