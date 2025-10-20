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
use logging::ConversationLogger;


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
    
    /// Generate shell completions
    #[arg(long, value_enum)]
    generate: Option<Shell>,
    
    /// Run in summary mode – give a short description of the task.
    #[arg(long, value_name = "TEXT")]
    task: Option<String>,
    
    /// Pretty‑print the JSON output (only useful with --task)
    #[arg(long)]
    pretty: bool,
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
        match self {
            Commands::Read { file_path } => {
                let work_dir = env::current_dir().unwrap();
                let chat = KimiChat::new("".to_string(), work_dir);
                Box::pin(async move {
                    chat.read_file(file_path)
                })
            }
            Commands::Write { file_path, content } => {
                let work_dir = env::current_dir().unwrap();
                let chat = KimiChat::new("".to_string(), work_dir);
                Box::pin(async move {
                    chat.write_file(file_path, content)
                })
            }
            Commands::Edit { file_path, old_content, new_content } => {
                let work_dir = env::current_dir().unwrap();
                let chat = KimiChat::new("".to_string(), work_dir);
                Box::pin(async move {
                    chat.edit_file(file_path, old_content, new_content)
                })
            }
            Commands::List { pattern } => {
                let work_dir = env::current_dir().unwrap();
                let chat = KimiChat::new("".to_string(), work_dir);
                Box::pin(async move {
                    chat.list_files(pattern)
                })
            }
            Commands::Search { query, pattern, regex, case_insensitive, max_results } => {
                let work_dir = env::current_dir().unwrap();
                let chat = KimiChat::new("".to_string(), work_dir);
                Box::pin(async move {
                    chat.search_files(pattern, query, *regex, *case_insensitive, *max_results as usize)
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
                let chat = KimiChat::new("".to_string(), work_dir);
                Box::pin(async move {
                    chat.run_command(command)
                })
            }
            Commands::Open { file_path, start_line, end_line } => {
                let work_dir = env::current_dir().unwrap();
                let chat = KimiChat::new("".to_string(), work_dir);
                Box::pin(async move {
                    if let (Some(start), Some(end)) = (start_line, end_line) {
                        open_file::open_file(Path::new("."), file_path, Some(*start..=*end))
                    } else {
                        open_file::open_file(Path::new("."), file_path, None)
                    }.await
                })
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum ModelType {
    Kimi,
    GptOss,
}

impl ModelType {
    fn as_str(&self) -> &'static str {
        match self {
            ModelType::Kimi => "moonshotai/kimi-k2-instruct-0905",
            ModelType::GptOss => "openai/gpt-oss-120b",
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            ModelType::Kimi => "Kimi-K2-Instruct-0905",
            ModelType::GptOss => "GPT-OSS-120B",
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

struct KimiChat {
    api_key: String,
    work_dir: PathBuf,
    client: reqwest::Client,
    messages: Vec<Message>,
    current_model: ModelType,
    total_tokens_used: usize,
    logger: Option<ConversationLogger>,
}

impl KimiChat {
    /// Generate system prompt based on current model
    fn get_system_prompt(model: &ModelType) -> String {
        let base_prompt = format!(
            "You are an AI assistant with access to file operations and model switching capabilities. \
            You are currently running as {}. You can switch to other models when appropriate:\n\
            - kimi (Kimi-K2-Instruct-0905): Good for general tasks, coding, and quick responses\n\
            - gpt-oss (GPT-OSS-120B): Good for complex reasoning, analysis, and advanced problem-solving\n\n\
            Available tools (use ONLY these exact names):\n\
            - read_file: Read entire file contents (always returns full file)\n\
            - open_file: Read specific line range from a file (use when you only need a section)\n\
            - write_file: Write/create a file\n\
            - edit_file: Edit existing file by replacing content\n\
            - list_files: List files (single-level patterns only, no **)\n\
            - switch_model: Switch between models\n\n",
            model.display_name()
        );

        if *model == ModelType::GptOss {
            format!(
                "{}CRITICAL WARNING: If you attempt to call ANY tool not listed above (such as 'edit', 'repo_browser.search', \
                'repo_browser.open_file', or any other made-up tool name), you will be IMMEDIATELY switched to the Kimi model \
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
        let mut chat = Self {
            api_key,
            work_dir,
            client: reqwest::Client::new(),
            messages: Vec::new(),
            current_model: ModelType::Kimi,
            total_tokens_used: 0,
            logger: None,
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

    fn get_tools() -> Vec<Tool> {
        vec![
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDef {
                    name: "open_file".to_string(),
                    description: "Open a file and display its contents with optional line range".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "file_path": {
                                "type": "string",
                                "description": "Path to the file relative to the work directory"
                            },
                            "start_line": {
                                "type": "integer",
                                "description": "Starting line number (1-based)"
                            },
                            "end_line": {
                                "type": "integer",
                                "description": "Ending line number (1-based)"
                            }
                        },
                        "required": ["file_path"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDef {
                    name: "read_file".to_string(),
                    description: "Read a preview of a file (first 10 lines) with total line count. For reading specific line ranges, use open_file instead.".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "file_path": {
                                "type": "string",
                                "description": "Path to the file relative to the work directory"
                            }
                        },
                        "required": ["file_path"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDef {
                    name: "write_file".to_string(),
                    description: "Write content to a file in the work directory".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "file_path": {
                                "type": "string",
                                "description": "Path to the file relative to the work directory"
                            },
                            "content": {
                                "type": "string",
                                "description": "Content to write to the file"
                            }
                        },
                        "required": ["file_path", "content"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDef {
                    name: "edit_file".to_string(),
                    description: "Edit a file by replacing old_content with new_content. IMPORTANT: old_content must not be empty - provide the exact text to replace. To add new content, use write_file instead.".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "file_path": {
                                "type": "string",
                                "description": "Path to the file relative to the work directory"
                            },
                            "old_content": {
                                "type": "string",
                                "description": "The exact content to be replaced (must not be empty)"
                            },
                            "new_content": {
                                "type": "string",
                                "description": "The new content to replace with"
                            }
                        },
                        "required": ["file_path", "old_content", "new_content"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDef {
                    name: "list_files".to_string(),
                    description: "List files in the work directory matching a single-level glob pattern. Recursive patterns (**) are NOT allowed to prevent massive output. Use patterns like 'src/*', '*.rs', or 'src/*.rs'.".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "pattern": {
                                "type": "string",
                                "description": "Single-level glob pattern (e.g., 'src/*', '*.rs'). Do NOT use ** for recursion.",
                                "default": "*"
                            }
                        }
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDef {
                    name: "search_files".to_string(),
                    description: "Search for a string or regular-expression across files matching a glob pattern. Returns lines with file:line:content format.".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "pattern": {
                                "type": "string",
                                "description": "Single-level glob pattern (e.g., 'src/*.rs'). Defaults to '*' (all files)."
                            },
                            "query": {
                                "type": "string",
                                "description": "Text or regex to search for (required)"
                            },
                            "regex": {
                                "type": "boolean",
                                "description": "Treat 'query' as a Rust regex. Default false."
                            },
                            "case_insensitive": {
                                "type": "boolean",
                                "description": "Plain-text case-insensitive search (ignored when 'regex' is true). Default false."
                            },
                            "max_results": {
                                "type": "integer",
                                "minimum": 1,
                                "description": "Maximum number of matches to return. Default 100."
                            }
                        },
                        "required": ["query"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDef {
                    name: "switch_model".to_string(),
                    description: "Switch to a different AI model. Use this when the current model thinks another model would be better suited for the task. Available models: 'kimi' (Kimi-K2-Instruct-0905 - good for general tasks and coding) and 'gpt-oss' (GPT-OSS-120B - good for complex reasoning and analysis).".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "model": {
                                "type": "string",
                                "enum": ["kimi", "gpt-oss"],
                                "description": "The model to switch to: 'kimi' or 'gpt-oss'"
                            },
                            "reason": {
                                "type": "string",
                                "description": "Brief explanation of why switching to this model"
                            }
                        },
                        "required": ["model", "reason"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDef {
                    name: "run_command".to_string(),
                    description: "Run a shell command interactively - always asks user confirmation before executing".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "command": {
                                "type": "string",
                                "description": "Shell command to run"
                            }
                        },
                        "required": ["command"]
                    }),
                },
            },
        ]
    }

    fn read_file(&self, file_path: &str) -> Result<String> {
        let full_path = self.work_dir.join(file_path);
        let content = fs::read_to_string(&full_path)
            .with_context(|| format!("Failed to read file: {}", full_path.display()))?;

        // Return just the content without any metadata
        // This prevents the "[Total: X lines]" from being accidentally included in edits/writes
        Ok(content)
    }

    fn write_file(&self, file_path: &str, content: &str) -> Result<String> {
        let full_path = self.work_dir.join(file_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&full_path, content)
            .with_context(|| format!("Failed to write file: {}", full_path.display()))?;
        Ok(format!("Successfully wrote to {}", file_path))
    }

    fn show_diff(&self, old: &str, new: &str, context_lines: usize) -> String {
        let diff = TextDiff::from_lines(old, new);
        let mut output = String::new();

        for (idx, group) in diff.grouped_ops(context_lines).iter().enumerate() {
            if idx > 0 {
                output.push_str(&format!("{}\n", "---".bright_black()));
            }
            for op in group {
                for change in diff.iter_inline_changes(op) {
                    let (sign, color_fn): (&str, fn(&str) -> colored::ColoredString) = match change.tag() {
                        ChangeTag::Delete => ("-", |s| s.red()),
                        ChangeTag::Insert => ("+", |s| s.green()),
                        ChangeTag::Equal => (" ", |s| s.normal()),
                    };
                    output.push_str(&format!("{}{}", sign, color_fn(&change.to_string())));
                }
            }
        }
        output
    }

    fn calculate_similarity(&self, s1: &str, s2: &str) -> f64 {
        // Simple similarity calculation based on matching characters
        let len1 = s1.len();
        let len2 = s2.len();

        if len1 == 0 && len2 == 0 {
            return 1.0;
        }

        if len1 == 0 || len2 == 0 {
            return 0.0;
        }

        // Count matching characters
        let chars1: Vec<char> = s1.chars().collect();
        let chars2: Vec<char> = s2.chars().collect();

        let mut matches = 0;
        let min_len = len1.min(len2);

        for i in 0..min_len {
            if chars1.get(i) == chars2.get(i) {
                matches += 1;
            }
        }

        // Normalize by the longer length
        matches as f64 / len1.max(len2) as f64
    }

    fn edit_file(&self, file_path: &str, old_content: &str, new_content: &str) -> Result<String> {
        // Prevent empty old_content which would cause catastrophic replacement
        if old_content.is_empty() {
            anyhow::bail!(
                "edit_file requires non-empty old_content to find and replace. \
                To add new content, use write_file instead, or provide the actual content to replace."
            );
        }

        let current_content = self.read_file(file_path)?;

        // Check if old_content exists in the file
        if !current_content.contains(old_content) {
            // Content not found - show what we were looking for and ask for manual intervention
            println!("{}", "❌ Content not found in file!".red().bold());
            println!("\n{}", "Looking for:".yellow());
            println!("{}", "─".repeat(50).bright_black());
            println!("{}", old_content.bright_white());
            println!("{}", "─".repeat(50).bright_black());

            println!("\n{}", "Proposed replacement:".yellow());
            println!("{}", "─".repeat(50).bright_black());
            println!("{}", new_content.bright_white());
            println!("{}", "─".repeat(50).bright_black());

            // Try fuzzy matching to suggest alternatives
            let lines: Vec<&str> = current_content.lines().collect();
            let search_lines: Vec<&str> = old_content.lines().collect();
            let mut found_candidates = Vec::new();

            if !search_lines.is_empty() {
                let first_search_line = search_lines[0].trim();
                println!("\n{}", "Searching for similar content...".bright_cyan());

                for (i, line) in lines.iter().enumerate() {
                    if line.trim().contains(first_search_line) {
                        // Extract a context block of similar size to old_content
                        let start = i;
                        let end = (i + search_lines.len()).min(lines.len());
                        let candidate = lines[start..end].join("\n");
                        found_candidates.push((i + 1, candidate));
                    }
                }

                // Number the candidates for selection
                for (idx, (line_num, candidate)) in found_candidates.iter().take(3).enumerate() { // Show max 3 candidates
                    println!("\n{} Found similar content #{} at line {}:", "💡".bright_cyan(), idx + 1, line_num);
                    println!("{}", "─".repeat(60).bright_black());

                    // Show diff between what was searched for and what was found
                    let diff = self.show_diff(old_content, candidate, 1);
                    print!("{}", diff);
                    println!("{}", "─".repeat(60).bright_black());

                    // Calculate similarity score
                    let similarity = self.calculate_similarity(old_content, candidate);
                    println!("{} Similarity: {:.0}%", "📊".bright_black(), similarity * 100.0);
                }

                if found_candidates.is_empty() {
                    println!("{}", "No similar content found. The file content might be very different.".yellow());
                }
            }

            // Ask user what to do
            println!("\n{}", "Would you like to:".bright_yellow().bold());

            // Show numbered options for each candidate found
            if !found_candidates.is_empty() {
                for (idx, (line_num, _)) in found_candidates.iter().take(3).enumerate() {
                    println!("  {} - Use similar content #{} (line {})",
                        (idx + 1).to_string().bright_cyan(),
                        idx + 1,
                        line_num);
                }
            }

            let manual_option = if found_candidates.is_empty() { "1".to_string() } else { (found_candidates.len().min(3) + 1).to_string() };
            let cancel_option = if found_candidates.is_empty() { "2".to_string() } else { (found_candidates.len().min(3) + 2).to_string() };

            println!("  {} - Manually provide the correct old_content", manual_option.bright_cyan());
            println!("  {} - Cancel this edit", cancel_option.bright_cyan());

            let options_str = if found_candidates.is_empty() {
                "[1/2]"
            } else {
                &format!("[1-{}]", found_candidates.len().min(3) + 2)
            };
            println!("\n{} {}:", "Choose".bright_green().bold(), options_str.bright_green());

            let mut rl = DefaultEditor::new()?;
            let response = rl.readline(">>> ")?;
            let response = response.trim();

            // Parse response
            if let Ok(choice) = response.parse::<usize>() {
                // Check if it's a candidate selection
                if choice > 0 && choice <= found_candidates.len().min(3) {
                    let (_line_num, candidate_content) = &found_candidates[choice - 1];
                    println!("{} Using similar content #{}", "✓".bright_green(), choice);
                    // Retry with the selected candidate
                    return self.edit_file(file_path, candidate_content, new_content);
                } else if response == manual_option {
                    // Manual editing option
                    println!("{}", "Enter the corrected old_content to find (multiple lines ok, end with empty line):".yellow());
                    let mut manual_old = String::new();
                    loop {
                        match rl.readline("") {
                            Ok(line) if line.is_empty() => break,
                            Ok(line) => {
                                if !manual_old.is_empty() {
                                    manual_old.push('\n');
                                }
                                manual_old.push_str(&line);
                            }
                            Err(_) => break,
                        }
                    }

                    if !manual_old.is_empty() {
                        // Retry with manual input
                        return self.edit_file(file_path, &manual_old, new_content);
                    } else {
                        anyhow::bail!("Edit cancelled - no content provided")
                    }
                } else {
                    anyhow::bail!("Edit cancelled by user")
                }
            } else {
                anyhow::bail!("Edit cancelled by user")
            }
        }

        // Check if this would actually make any changes
        if old_content == new_content {
            return Ok(format!(
                "No changes needed: The content you want to replace and the replacement are identical. \
                The file already contains the desired content. No edit was performed."
            ));
        }

        // Count occurrences
        let occurrences = current_content.matches(old_content).count();

        // Generate the updated content
        let updated_content = current_content.replace(old_content, new_content);

        // Check if the edit would result in no actual file changes
        if current_content == updated_content {
            return Ok(format!(
                "No changes needed: The file content would remain unchanged after this edit. \
                The file already contains the desired state."
            ));
        }

        // Show diff
        println!("\n{}", format!("📝 Proposed changes to {}:", file_path).bright_cyan().bold());
        println!("{}", "═".repeat(60).bright_black());
        let diff_output = self.show_diff(&current_content, &updated_content, 3);
        print!("{}", diff_output);
        println!("{}", "═".repeat(60).bright_black());

        if occurrences > 1 {
            println!("{}", format!("⚠️  Warning: {} occurrences will be replaced", occurrences).yellow());
        }

        // Ask for confirmation
        println!("\n{}", "Apply these changes? [Y/n/e(dit)]".bright_green().bold());

        let mut rl = DefaultEditor::new()?;
        let response = rl.readline(">>> ")?;
        let response = response.trim().to_lowercase();

        match response.as_str() {
            "" | "y" | "yes" => {
                self.write_file(file_path, &updated_content)?;
                Ok(format!("✅ Successfully edited {} ({} replacement(s))", file_path, occurrences))
            }
            "e" | "edit" => {
                // Allow manual editing
                println!("{}", "Enter the corrected old_content (end with Ctrl+D or empty line):".yellow());
                let mut manual_old = String::new();
                loop {
                    match rl.readline("") {
                        Ok(line) if line.is_empty() => break,
                        Ok(line) => {
                            manual_old.push_str(&line);
                            manual_old.push('\n');
                        }
                        Err(_) => break,
                    }
                }

                if !manual_old.is_empty() {
                    // Retry with manual input
                    return self.edit_file(file_path, &manual_old.trim_end(), new_content);
                } else {
                    anyhow::bail!("Edit cancelled - no content provided")
                }
            }
            _ => {
                anyhow::bail!("Edit cancelled by user")
            }
        }
    }

    fn list_files(&self, pattern: &str) -> Result<String> {
        // Disallow recursive patterns to prevent massive output
        if pattern.contains("**") {
            return Ok("Recursive patterns (**) are not allowed. Use single-level patterns like 'src/*' or 'src/*.rs' instead.".to_string());
        }

        let glob_pattern = self.work_dir.join(pattern);
        let mut files = Vec::new();

        for entry in glob::glob(glob_pattern.to_str().unwrap())? {
            if let Ok(path) = entry {
                if let Ok(relative) = path.strip_prefix(&self.work_dir) {
                    files.push(relative.display().to_string());
                }
            }
        }

        if files.is_empty() {
            Ok("No files found matching pattern".to_string())
        } else {
            files.sort();
            Ok(format!("{}\n\nTotal: {} items", files.join("\n"), files.len()))
        }
    }

    fn switch_model(&mut self, model_str: &str, reason: &str) -> Result<String> {
        let new_model = match model_str.to_lowercase().as_str() {
            "kimi" => ModelType::Kimi,
            "gpt-oss" => ModelType::GptOss,
            _ => anyhow::bail!("Unknown model: {}. Available: 'kimi', 'gpt-oss'", model_str),
        };

        if new_model == self.current_model {
            return Ok(format!(
                "Already using {} model",
                self.current_model.display_name()
            ));
        }

        let old_model = self.current_model;
        self.current_model = new_model;

        Ok(format!(
            "Switched from {} to {} - Reason: {}",
            old_model.display_name(),
            new_model.display_name(),
            reason
        ))
    }

    fn run_command(&self, command: &str) -> Result<String> {
        // Ask user for confirmation interactively
        print!(
            "{} {}",
            "Run command:".yellow(),
            command.cyan()
        );
        std::io::stdout().flush()?;
        
        print!(" {} (y/N): ", "Execute?".yellow());
        std::io::stdout().flush()?;
        
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => {
                println!("{} {}", "Running:".green(), command.cyan());
                
                // Execute the command
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .current_dir(&self.work_dir)
                    .output()
                    .with_context(|| format!("Failed to run command: {}", command))?;
                
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                
                let mut result = String::new();
                if !stdout.is_empty() {
                    result.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    result.push_str(&stderr);
                }
                
                Ok(result)
            }
            _ => {
                // Allow user to comment when declining tool usage
                println!("{} {}", "Command cancelled".yellow(), "- you can comment on why you declined".bright_black());
                
                // Ask if user wants to add a comment
                print!("{} (y/N): ", "Add comment?".yellow());
                std::io::stdout().flush()?;
                
                let mut comment_input = String::new();
                std::io::stdin().read_line(&mut comment_input)?;
                
                if comment_input.trim().to_lowercase().as_str() == "y" || comment_input.trim().to_lowercase().as_str() == "yes" {
                    print!("{}: ", "Comment".yellow());
                    std::io::stdout().flush()?;
                    
                    let mut comment = String::new();
                    std::io::stdin().read_line(&mut comment)?;
                    
                    if !comment.trim().is_empty() {
                        Ok(format!("Command cancelled - {}", comment.trim()))
                    } else {
                        Ok("Command cancelled".to_string())
                    }
                } else {
                    Ok("Command cancelled".to_string())
                }
            }
        }
    }

    fn search_files(
        &self,
        pattern: &str,
        query: &str,
        regex: bool,
        case_insensitive: bool,
        max_results: usize,
    ) -> Result<String> {
        // Guard against recursive patterns
        if pattern.contains("**") {
            return Ok("Recursive patterns (**) are not allowed. Use single-level patterns like 'src/*' or 'src/*.rs' instead.".to_string());
        }

        let glob_pattern = self.work_dir.join(pattern);
        let mut results = Vec::new();

        // Compile regex if requested
        let re = if regex {
            Some(regex::Regex::new(query)
                .with_context(|| format!("Invalid regex pattern: {}", query))?)
        } else {
            None
        };

        for entry in glob::glob(glob_pattern.to_str().unwrap())? {
            if let Ok(path) = entry {
                let relative_path = path.strip_prefix(&self.work_dir)?.display().to_string();

                let file = fs::File::open(&path)?;
                let reader = std::io::BufReader::new(file);

                for (idx, line) in reader.lines().enumerate() {
                    let line = line?;
                    let is_match = if let Some(re) = &re {
                        re.is_match(&line)
                    } else if case_insensitive {
                        line.to_lowercase().contains(&query.to_lowercase())
                    } else {
                        line.contains(query)
                    };

                    if is_match {
                        results.push(format!("{}:{}: {}", relative_path, idx + 1, line.trim_end()));
                        if results.len() >= max_results {
                            break;
                        }
                    }
                }

                if results.len() >= max_results {
                    break;
                }
            }
        }

        if results.is_empty() {
            Ok("No matches found.".to_string())
        } else {
            let mut out = results.join("\n");
            if results.len() == max_results {
                out.push_str("\n... (truncated, maximum number of results reached)");
            }
            Ok(out)
        }
    }

    async fn execute_tool(&mut self, name: &str, arguments: &str) -> Result<String> {
        match name {
            "open_file" => {
                let args: OpenFileArgs = serde_json::from_str(arguments)?;
                let line_range = if args.start_line > 0 && args.end_line > 0 {
                    Some(args.start_line..=args.end_line)
                } else {
                    None
                };
                
                // Use the open_file module implementation
                match open_file::open_file(&self.work_dir, &args.file_path, line_range).await {
                    Ok(content) => Ok(content),
                    Err(e) => Err(anyhow::anyhow!("Failed to open file: {}", e))
                }
            }
            "read_file" => {
                let args: ReadFileArgs = serde_json::from_str(arguments)?;
                self.read_file(&args.file_path)
            }
            "write_file" => {
                let args: WriteFileArgs = serde_json::from_str(arguments)?;
                self.write_file(&args.file_path, &args.content)
            }
            "edit_file" => {
                let args: EditFileArgs = serde_json::from_str(arguments)?;
                self.edit_file(&args.file_path, &args.old_content, &args.new_content)
            }
            "list_files" => {
                let args: ListFilesArgs = serde_json::from_str(arguments)?;
                self.list_files(&args.pattern)
            }
            "search_files" => {
                let args: SearchFilesArgs = serde_json::from_str(arguments)?;
                self.search_files(
                    &args.pattern,
                    &args.query,
                    args.regex,
                    args.case_insensitive,
                    args.max_results as usize,
                )
            }
            "switch_model" => {
                let args: SwitchModelArgs = serde_json::from_str(arguments)?;
                self.switch_model(&args.model, &args.reason)
            }
            "run_command" => {
                let args: RunCommandArgs = serde_json::from_str(arguments)?;
                self.run_command(&args.command)
            }
            _ => anyhow::bail!("Unknown tool: {}", name),
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
            ModelType::Kimi => ModelType::GptOss,
            ModelType::GptOss => ModelType::Kimi,
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
            model: ModelType::Kimi.as_str().to_string(),
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

    async fn call_api(&self, orig_messages: &[Message]) -> Result<(Message, Option<Usage>, ModelType, Vec<Message>)> {
        let mut current_model = self.current_model;
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
		tools: Self::get_tools(),
		tool_choice: "auto".to_string(),
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

                    // Check for GPT-OSS hallucinating non-existent tools
                    if error_body.contains("attempted to call tool") && current_model == ModelType::GptOss {
                        eprintln!("{}", "🔄 GPT-OSS-120B attempted to use non-existent tool. Switching to Kimi and retrying...".bright_cyan());

                        // Switch to Kimi
                        current_model = ModelType::Kimi;

                        // Update system message
                        if let Some(sys_msg) = messages.first_mut() {
                            if sys_msg.role == "system" {
                                sys_msg.content = Self::get_system_prompt(&current_model);
                            }
                        }

                        // Retry with Kimi - continue the loop to retry
                        retry_count = 0; // Reset retry count for new model
                        continue;
                    }
                    // Check for Kimi generating malformed tool calls
                    else if (error_body.contains("Failed to call a function") ||
                             error_body.contains("tool call validation failed") ||
                             error_body.contains("parameters for tool") ||
                             error_body.contains("did not match schema")) &&
                            current_model == ModelType::Kimi {
                        eprintln!("{}", "❌ Kimi-K2 generated malformed tool call (invalid JSON/parameters).".red());

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

                        // If repair failed or wasn't possible, switch to GPT-OSS as fallback
                        eprintln!("{}", "🔄 Repair failed. Switching to GPT-OSS and retrying...".bright_cyan());

                        // Switch to GPT-OSS
                        current_model = ModelType::GptOss;

                        // Update system message
                        if let Some(sys_msg) = messages.first_mut() {
                            if sys_msg.role == "system" {
                                sys_msg.content = Self::get_system_prompt(&current_model);
                            }
                        }

                        // Retry with GPT-OSS - continue the loop to retry
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
        const MAX_TOOL_ITERATIONS: usize = 25; // Allow more iterations for complex tasks
        const LOOP_DETECTION_WINDOW: usize = 6; // Check last 6 tool calls

        loop {
            let (response, usage, current_model, messages) = self.call_api(&self.messages).await?;
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

                // Hard limit check (fallback)
                if tool_call_iterations > MAX_TOOL_ITERATIONS {
                    eprintln!(
                        "{} Reached maximum tool call limit ({} iterations).",
                        "⚠️".yellow(),
                        MAX_TOOL_ITERATIONS
                    );
                    self.messages.push(Message {
                        role: "assistant".to_string(),
                        content: format!(
                            "I've made {} tool calls for this request, which is quite a lot. \
                            I may need you to break this down into smaller tasks or provide more specific direction.",
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

                    logger.log_with_tool_calls(
                        "assistant",
                        &response.content,
                        Some(self.current_model.as_str()),
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

                    let result = match self.execute_tool(
                        &tool_call.function.name,
                        &tool_call.function.arguments,
                    ).await {
                        Ok(r) => r,
                        Err(e) => {
                            let error_msg = e.to_string();
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

                    println!("{} {}", "📋 Result:".green(), result.bright_black());

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

    let api_key = env::var("GROQ_API_KEY")
        .context("GROQ_API_KEY environment variable not set")?;

    // Use current directory as work_dir so the AI can see project files
    // NB: do NOT use the 'workspace' subdirectory as work_dir
    let work_dir = env::current_dir()?;

    // Parse CLI arguments
    let cli = Cli::parse();

    // If a subcommand was provided, execute it and exit
    if let Some(command) = cli.command {
        let result = command.execute().await?;
        println!("{}", result);
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
    println!("{}", "Models can switch between Kimi-K2-Instruct-0905 and GPT-OSS-120B automatically".bright_black());
    println!("{}", "Type 'exit' or 'quit' to exit\n".bright_black());

    let mut chat = KimiChat::new(api_key, work_dir);
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

                match chat.chat(line).await {
                    Ok(response) => {
                        // Log assistant response
                        if let Some(logger) = &mut chat.logger {
                            logger.log("assistant", &response, None, false).await;
                        }
                        let model_name = format!("[{}]", chat.current_model.display_name()).bright_magenta();
                        println!("\n{} {} {}\n", model_name, "Assistant:".bright_blue().bold(), response);
                    }
                    Err(e) => {
                        eprintln!("{} {}\n", "Error:".bright_red().bold(), e);
                    }
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
