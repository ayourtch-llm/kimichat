use anyhow::Result;
use clap::{Parser, Subcommand};
use clap_complete::Shell;
use std::env;
use std::future::Future;
use std::pin::Pin;

use crate::core::tool::{Tool, ToolParameters};
use crate::core::tool_context::ToolContext;
use crate::policy::PolicyManager;
use crate::tools::file_ops::*;
use crate::tools::system::*;
use crate::tools::search::*;
use crate::terminal::{PtyLaunchTool, PtySendKeysTool, PtyGetScreenTool, PtyListTool, PtyKillTool};

// Note: KimiChat is needed for the Switch command
// It will be imported from the parent module when needed

/// CLI arguments for kimi-chat
#[derive(Parser)]
#[command(name = "kimichat")]
#[command(about = "Kimi Chat - Claude Code-like Experience with Multi-Model AI Support")]
#[command(version = "0.1.0")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Run in interactive mode (default)
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    pub interactive: bool,

    /// Enable multi-agent system for specialized task handling
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub agents: bool,

    /// Generate shell completions
    #[arg(long, value_enum)]
    pub generate: Option<Shell>,

    /// Run in summary mode – give a short description of the task.
    #[arg(long, value_name = "TEXT")]
    pub task: Option<String>,

    /// Pretty‑print the JSON output (only useful with --task)
    #[arg(long)]
    pub pretty: bool,

    /// Use llama.cpp server for all models (e.g., http://localhost:8080)
    /// This is a convenience flag that sets --api-url-blu-model, --api-url-grn-model, and --api-url-red-model
    #[arg(long, value_name = "URL")]
    pub llama_cpp_url: Option<String>,

    /// API URL for the 'blu_model' model (e.g., http://localhost:8080)
    /// If set, uses llama.cpp for blu_model; otherwise uses Groq
    #[arg(long, value_name = "URL")]
    pub api_url_blu_model: Option<String>,

    /// API URL for the 'grn_model' model (e.g., http://localhost:8081)
    /// If set, uses llama.cpp for grn_model; otherwise uses Groq
    #[arg(long, value_name = "URL")]
    pub api_url_grn_model: Option<String>,

    /// API URL for the 'red_model' model (e.g., http://localhost:8082)
    /// If set, uses llama.cpp for red_model; otherwise uses Groq
    #[arg(long, value_name = "URL")]
    pub api_url_red_model: Option<String>,

    /// Override the 'blu_model' model with a custom model name
    #[arg(long, value_name = "MODEL")]
    pub model_blu_model: Option<String>,

    /// Override the 'grn_model' model with a custom model name
    #[arg(long, value_name = "MODEL")]
    pub model_grn_model: Option<String>,

    /// Override the 'red_model' model with a custom model name
    #[arg(long, value_name = "MODEL")]
    pub model_red_model: Option<String>,

    /// Override all models with the same custom model name
    /// This is a convenience flag that sets --model-blu-model, --model-grn-model, and --model-red-model
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,

    /// Backend type for blu_model (groq, anthropic, llama, openai)
    #[arg(long, value_name = "BACKEND")]
    pub blu_backend: Option<String>,

    /// Backend type for grn_model (groq, anthropic, llama, openai)
    #[arg(long, value_name = "BACKEND")]
    pub grn_backend: Option<String>,

    /// Backend type for red_model (groq, anthropic, llama, openai)
    #[arg(long, value_name = "BACKEND")]
    pub red_backend: Option<String>,

    /// API key for blu_model
    #[arg(long, value_name = "KEY")]
    pub blu_key: Option<String>,

    /// API key for grn_model
    #[arg(long, value_name = "KEY")]
    pub grn_key: Option<String>,

    /// API key for red_model
    #[arg(long, value_name = "KEY")]
    pub red_key: Option<String>,

    /// Auto-confirm all actions without asking (auto-pilot mode)
    #[arg(long)]
    pub auto_confirm: bool,

    /// Path to policy file (default: policies.toml in project root)
    #[arg(long, value_name = "PATH")]
    pub policy_file: Option<String>,

    /// Learn from user decisions and save them to policy file
    #[arg(long)]
    pub learn_policies: bool,

    /// Enable streaming mode - show AI responses as they're generated
    #[arg(long)]
    pub stream: bool,

    /// Enable verbose debug output (shows HTTP requests, responses, headers, etc.)
    #[arg(long, short = 'v')]
    pub verbose: bool,

    /// Terminal backend to use for PTY sessions (pty, tmux)
    /// Default: pty. Can also be set via KIMICHAT_TERMINAL_BACKEND env var
    #[arg(long, value_name = "BACKEND")]
    pub terminal_backend: Option<String>,

    /// Enable web server
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub web: bool,

    /// Web server port
    #[arg(long, default_value = "8080", env = "KIMICHAT_WEB_PORT")]
    pub web_port: u16,

    /// Web server bind address
    #[arg(long, default_value = "127.0.0.1", env = "KIMICHAT_WEB_BIND")]
    pub web_bind: String,

    /// Allow TUI session to be attached from web
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub web_attachable: bool,
}

#[derive(Subcommand)]
pub enum Commands {
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
    /// Manage terminal sessions (PTY sessions)
    Terminal {
        #[command(subcommand)]
        command: TerminalCommands,
    },
}

#[derive(Subcommand)]
pub enum TerminalCommands {
    /// Launch a new terminal session
    Launch {
        /// Command to execute (defaults to shell)
        #[arg(short = 'c', long)]
        command: Option<String>,
        /// Working directory
        #[arg(short = 'd', long)]
        working_dir: Option<String>,
        /// Terminal columns
        #[arg(long, default_value = "80")]
        cols: u16,
        /// Terminal rows
        #[arg(long, default_value = "24")]
        rows: u16,
    },
    /// View the screen contents of a terminal session
    View {
        /// Session ID to view
        session_id: u32,
    },
    /// List all active terminal sessions
    List,
    /// Kill a terminal session
    Kill {
        /// Session ID to kill
        session_id: u32,
    },
    /// Send keys to a terminal session
    Send {
        /// Session ID to send keys to
        session_id: u32,
        /// Keys to send (supports ^C, [UP], [F1], etc.)
        keys: String,
    },
}

impl Commands {
    pub fn execute(&self) -> Pin<Box<dyn Future<Output = Result<String>> + '_>> {
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
                // This needs KimiChat which we'll handle in main.rs
                let model = model.clone();
                let reason = reason.clone();
                Box::pin(async move {
                    // This will be handled specially in main.rs
                    Ok(format!("Switch to {} for: {}", model, reason))
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
            Commands::Terminal { .. } => {
                // Terminal commands need special handling in main.rs with TerminalManager
                Box::pin(async move {
                    Err(anyhow::anyhow!("Terminal commands require special handling"))
                })
            }
        }
    }
}

impl TerminalCommands {
    pub fn execute(&self, terminal_manager: std::sync::Arc<tokio::sync::Mutex<crate::terminal::TerminalManager>>) -> Pin<Box<dyn Future<Output = Result<String>> + '_>> {
        match self {
            TerminalCommands::Launch { command, working_dir, cols, rows } => {
                let command = command.clone();
                let working_dir = working_dir.clone().map(std::path::PathBuf::from);
                let cols = *cols;
                let rows = *rows;
                Box::pin(async move {
                    let mut params = ToolParameters::new();
                    if let Some(cmd) = command {
                        params.set("command", cmd);
                    }
                    if let Some(wd) = working_dir {
                        params.set("working_dir", wd.to_string_lossy().to_string());
                    }
                    params.set("cols", cols as i64);
                    params.set("rows", rows as i64);

                    let work_dir = env::current_dir().unwrap();
                    let mut context = ToolContext::new(work_dir, "cli_session".to_string(), PolicyManager::new());
                    context = context.with_terminal_manager(terminal_manager);

                    let result = PtyLaunchTool.execute(params, &context).await;
                    if result.success {
                        Ok(result.content)
                    } else {
                        Err(anyhow::anyhow!("{}", result.error.unwrap_or_default()))
                    }
                })
            }
            TerminalCommands::View { session_id } => {
                let session_id = *session_id;
                Box::pin(async move {
                    let mut params = ToolParameters::new();
                    params.set("session_id", session_id as i64);

                    let work_dir = env::current_dir().unwrap();
                    let mut context = ToolContext::new(work_dir, "cli_session".to_string(), PolicyManager::new());
                    context = context.with_terminal_manager(terminal_manager);

                    let result = PtyGetScreenTool.execute(params, &context).await;
                    if result.success {
                        Ok(result.content)
                    } else {
                        Err(anyhow::anyhow!("{}", result.error.unwrap_or_default()))
                    }
                })
            }
            TerminalCommands::List => {
                Box::pin(async move {
                    let params = ToolParameters::new();

                    let work_dir = env::current_dir().unwrap();
                    let mut context = ToolContext::new(work_dir, "cli_session".to_string(), PolicyManager::new());
                    context = context.with_terminal_manager(terminal_manager);

                    let result = PtyListTool.execute(params, &context).await;
                    if result.success {
                        Ok(result.content)
                    } else {
                        Err(anyhow::anyhow!("{}", result.error.unwrap_or_default()))
                    }
                })
            }
            TerminalCommands::Kill { session_id } => {
                let session_id = *session_id;
                Box::pin(async move {
                    let mut params = ToolParameters::new();
                    params.set("session_id", session_id as i64);

                    let work_dir = env::current_dir().unwrap();
                    let mut context = ToolContext::new(work_dir, "cli_session".to_string(), PolicyManager::new());
                    context = context.with_terminal_manager(terminal_manager);

                    let result = PtyKillTool.execute(params, &context).await;
                    if result.success {
                        Ok(result.content)
                    } else {
                        Err(anyhow::anyhow!("{}", result.error.unwrap_or_default()))
                    }
                })
            }
            TerminalCommands::Send { session_id, keys } => {
                let session_id = *session_id;
                let keys = keys.clone();
                Box::pin(async move {
                    let mut params = ToolParameters::new();
                    params.set("session_id", session_id as i64);
                    params.set("keys", keys);
                    params.set("special", true); // Enable special key processing

                    let work_dir = env::current_dir().unwrap();
                    let mut context = ToolContext::new(work_dir, "cli_session".to_string(), PolicyManager::new());
                    context = context.with_terminal_manager(terminal_manager);

                    let result = PtySendKeysTool.execute(params, &context).await;
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
