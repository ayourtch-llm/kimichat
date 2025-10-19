use anyhow::{Context, Result};
use colored::Colorize;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

const GROQ_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const MAX_CONTEXT_TOKENS: usize = 100_000; // Keep conversation under this to avoid rate limits
const MAX_RETRIES: u32 = 3;

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
    "**/*".to_string()
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

struct KimiChat {
    api_key: String,
    work_dir: PathBuf,
    client: reqwest::Client,
    messages: Vec<Message>,
    current_model: ModelType,
    total_tokens_used: usize,
}

impl KimiChat {
    fn new(api_key: String, work_dir: PathBuf) -> Self {
        let mut chat = Self {
            api_key,
            work_dir,
            client: reqwest::Client::new(),
            messages: Vec::new(),
            current_model: ModelType::Kimi,
            total_tokens_used: 0,
        };

        // Add system message to inform the model about capabilities
        chat.messages.push(Message {
            role: "system".to_string(),
            content: format!(
                "You are an AI assistant with access to file operations and model switching capabilities. \
                You are currently running as {}. You can switch to other models when appropriate:\n\
                - kimi (Kimi-K2-Instruct-0905): Good for general tasks, coding, and quick responses\n\
                - gpt-oss (GPT-OSS-120B): Good for complex reasoning, analysis, and advanced problem-solving\n\n\
                Use the switch_model tool when you believe another model would be better suited for the user's request. \
                The conversation history will be preserved when switching models.",
                chat.current_model.display_name()
            ),
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
                    name: "read_file".to_string(),
                    description: "Read the contents of a file from the work directory".to_string(),
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
                    description: "Edit a file by replacing old_content with new_content".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "file_path": {
                                "type": "string",
                                "description": "Path to the file relative to the work directory"
                            },
                            "old_content": {
                                "type": "string",
                                "description": "The content to be replaced"
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
                    description: "List files in the work directory matching a glob pattern".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "pattern": {
                                "type": "string",
                                "description": "Glob pattern to match files (e.g., '**/*.rs' for all Rust files)",
                                "default": "**/*"
                            }
                        }
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
        ]
    }

    fn read_file(&self, file_path: &str) -> Result<String> {
        let full_path = self.work_dir.join(file_path);
        fs::read_to_string(&full_path)
            .with_context(|| format!("Failed to read file: {}", full_path.display()))
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

    fn edit_file(&self, file_path: &str, old_content: &str, new_content: &str) -> Result<String> {
        let current_content = self.read_file(file_path)?;

        if !current_content.contains(old_content) {
            anyhow::bail!("Old content not found in file");
        }

        let updated_content = current_content.replace(old_content, new_content);
        self.write_file(file_path, &updated_content)?;
        Ok(format!("Successfully edited {}", file_path))
    }

    fn list_files(&self, pattern: &str) -> Result<String> {
        let glob_pattern = self.work_dir.join(pattern);
        let mut files = Vec::new();

        for entry in glob::glob(glob_pattern.to_str().unwrap())? {
            if let Ok(path) = entry {
                if path.is_file() {
                    if let Ok(relative) = path.strip_prefix(&self.work_dir) {
                        files.push(relative.display().to_string());
                    }
                }
            }
        }

        if files.is_empty() {
            Ok("No files found matching pattern".to_string())
        } else {
            Ok(files.join("\n"))
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

    fn execute_tool(&mut self, name: &str, arguments: &str) -> Result<String> {
        match name {
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
            "switch_model" => {
                let args: SwitchModelArgs = serde_json::from_str(arguments)?;
                self.switch_model(&args.model, &args.reason)
            }
            _ => anyhow::bail!("Unknown tool: {}", name),
        }
    }

    fn trim_history_if_needed(&mut self) {
        // Rough estimate: average 4 characters per token
        let estimated_tokens: usize = self.messages.iter()
            .map(|m| m.content.len() / 4)
            .sum();

        if estimated_tokens > MAX_CONTEXT_TOKENS {
            println!(
                "{} Context is getting large (~{} tokens). Trimming older messages...",
                "✂️".yellow(),
                estimated_tokens
            );

            // Always keep the system message (first message)
            let system_message = self.messages.first().cloned();

            // Keep the last 50% of messages to stay well under the limit
            let keep_count = (self.messages.len() / 2).max(10);
            let messages_to_keep: Vec<Message> = self.messages
                .iter()
                .rev()
                .take(keep_count)
                .rev()
                .cloned()
                .collect();

            self.messages.clear();
            if let Some(sys_msg) = system_message {
                self.messages.push(sys_msg);
            }
            self.messages.extend(messages_to_keep);

            let new_estimated: usize = self.messages.iter()
                .map(|m| m.content.len() / 4)
                .sum();
            println!(
                "{} Trimmed to {} messages (~{} tokens)",
                "✅".green(),
                self.messages.len(),
                new_estimated
            );
        }
    }

    async fn call_api(&self, messages: &[Message]) -> Result<(Message, Option<Usage>)> {
        let request = ChatRequest {
            model: self.current_model.as_str().to_string(),
            messages: messages.to_vec(),
            tools: Self::get_tools(),
            tool_choice: "auto".to_string(),
        };

        // Retry logic with exponential backoff
        let mut retry_count = 0;
        loop {
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

            let response = response.error_for_status()?;
            let response_text = response.text().await?;
            let chat_response: ChatResponse = serde_json::from_str(&response_text)
                .with_context(|| format!("Failed to parse API response: {}", response_text))?;

            let message = chat_response
                .choices
                .into_iter()
                .next()
                .map(|c| c.message)
                .context("No response from API")?;

            return Ok((message, chat_response.usage));
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

        loop {
            // Trim history if needed before making API call
            self.trim_history_if_needed();

            let (response, usage) = self.call_api(&self.messages).await?;

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
                self.messages.push(response.clone());

                for tool_call in tool_calls {
                    println!(
                        "{} {} with args: {}",
                        "🔧 Calling tool:".yellow(),
                        tool_call.function.name.cyan(),
                        tool_call.function.arguments.bright_black()
                    );

                    let result = match self.execute_tool(
                        &tool_call.function.name,
                        &tool_call.function.arguments,
                    ) {
                        Ok(r) => r,
                        Err(e) => format!("Error: {}", e),
                    };

                    println!("{} {}", "📋 Result:".green(), result.bright_black());

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
    let work_dir = env::current_dir()?;

    println!("{}", "🤖 Kimi Chat - Claude Code-like Experience".bright_cyan().bold());
    println!("{}", format!("Working directory: {}", work_dir.display()).bright_black());
    println!("{}", "Models can switch between Kimi-K2-Instruct-0905 and GPT-OSS-120B automatically".bright_black());
    println!("{}", "Type 'exit' or 'quit' to exit\n".bright_black());

    let mut chat = KimiChat::new(api_key, work_dir);
    let mut rl = DefaultEditor::new()?;

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

                match chat.chat(line).await {
                    Ok(response) => {
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

    Ok(())
}
