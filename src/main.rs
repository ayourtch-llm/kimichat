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
        let system_content = if chat.current_model == ModelType::GptOss {
            format!(
                "You are an AI assistant with access to file operations and model switching capabilities. \
                You are currently running as {}. You can switch to other models when appropriate:\n\
                - kimi (Kimi-K2-Instruct-0905): Good for general tasks, coding, and quick responses\n\
                - gpt-oss (GPT-OSS-120B): Good for complex reasoning, analysis, and advanced problem-solving\n\n\
                Available tools (use ONLY these exact names):\n\
                - read_file: Read file contents\n\
                - write_file: Write/create a file\n\
                - edit_file: Edit existing file by replacing content\n\
                - list_files: List files (single-level patterns only, no **)\n\
                - switch_model: Switch between models\n\n\
                CRITICAL WARNING: If you attempt to call ANY tool not listed above (such as 'edit', 'repo_browser.search', \
                'repo_browser.open_file', or any other made-up tool name), you will be IMMEDIATELY switched to the Kimi model \
                and your request will be retried. Use ONLY the exact tool names listed above.",
                chat.current_model.display_name()
            )
        } else {
            format!(
                "You are an AI assistant with access to file operations and model switching capabilities. \
                You are currently running as {}. You can switch to other models when appropriate:\n\
                - kimi (Kimi-K2-Instruct-0905): Good for general tasks, coding, and quick responses\n\
                - gpt-oss (GPT-OSS-120B): Good for complex reasoning, analysis, and advanced problem-solving\n\n\
                Available tools (use ONLY these exact names):\n\
                - read_file: Read file contents\n\
                - write_file: Write/create a file\n\
                - edit_file: Edit existing file by replacing content\n\
                - list_files: List files (single-level patterns only, no **)\n\
                - switch_model: Switch between models\n\n\
                IMPORTANT: Only use the exact tool names listed above. Do not make up tool names.",
                chat.current_model.display_name()
            )
        };

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
        // Prevent empty old_content which would cause catastrophic replacement
        if old_content.is_empty() {
            anyhow::bail!(
                "edit_file requires non-empty old_content to find and replace. \
                To add new content, use write_file instead, or provide the actual content to replace."
            );
        }

        let current_content = self.read_file(file_path)?;

        if !current_content.contains(old_content) {
            anyhow::bail!(
                "Old content not found in file. Make sure the old_content exactly matches \
                the text you want to replace (including whitespace and indentation)."
            );
        }

        // Count occurrences to warn about multiple replacements
        let occurrences = current_content.matches(old_content).count();
        if occurrences > 1 {
            eprintln!(
                "{} Warning: Found {} occurrences of old_content. All will be replaced.",
                "⚠️".yellow(),
                occurrences
            );
        }

        let updated_content = current_content.replace(old_content, new_content);
        self.write_file(file_path, &updated_content)?;
        Ok(format!("Successfully edited {} ({} replacement(s))", file_path, occurrences))
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
                                            sys_msg.content = format!(
                                                "You are an AI assistant with access to file operations and model switching capabilities. \
                                                You are currently running as {}. You can switch to other models when appropriate:\n\
                                                - kimi (Kimi-K2-Instruct-0905): Good for general tasks, coding, and quick responses\n\
                                                - gpt-oss (GPT-OSS-120B): Good for complex reasoning, analysis, and advanced problem-solving\n\n\
                                                Available tools (use ONLY these exact names):\n\
                                                - read_file: Read file contents\n\
                                                - write_file: Write/create a file\n\
                                                - edit_file: Edit existing file by replacing content\n\
                                                - list_files: List files (single-level patterns only, no **)\n\
                                                - switch_model: Switch between models\n\n\
                                                IMPORTANT: Only use the exact tool names listed above. Do not make up tool names.",
                                                self.current_model.display_name()
                                            );
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

    async fn call_api(&self, orig_messages: &[Message]) -> Result<(Message, Option<Usage>, ModelType, Vec<Message>)> {
        let mut current_model = self.current_model;
        let mut messages = orig_messages.to_vec().clone();
        

        // Retry logic with exponential backoff
        let mut retry_count = 0;
        loop {
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

                // Check if this is a tool hallucination error from GPT-OSS
                if status == 400 && error_body.contains("tool_use_failed") && error_body.contains("attempted to call tool") {
                    eprintln!("{}", "❌ Tool hallucination detected!".red().bold());
                    eprintln!("{}", error_body.yellow());

                    if self.current_model == ModelType::GptOss {
                        eprintln!("{}", "🔄 GPT-OSS-120B attempted to use non-existent tool. Switching to Kimi and retrying...".bright_cyan());

                        // Switch to Kimi
                        current_model = ModelType::Kimi;

                        // Update system message
                        if let Some(sys_msg) = messages.first_mut() {
                            if sys_msg.role == "system" {
                                sys_msg.content = format!(
                                    "You are an AI assistant with access to file operations and model switching capabilities. \
                                    You are currently running as {}. You can switch to other models when appropriate:\n\
                                    - kimi (Kimi-K2-Instruct-0905): Good for general tasks, coding, and quick responses\n\
                                    - gpt-oss (GPT-OSS-120B): Good for complex reasoning, analysis, and advanced problem-solving\n\n\
                                    Available tools (use ONLY these exact names):\n\
                                    - read_file: Read file contents\n\
                                    - write_file: Write/create a file\n\
                                    - edit_file: Edit existing file by replacing content\n\
                                    - list_files: List files (single-level patterns only, no **)\n\
                                    - switch_model: Switch between models\n\n\
                                    IMPORTANT: Only use the exact tool names listed above. Do not make up tool names.",
                                    self.current_model.display_name()
                                );
                            }
                        }

                        // Retry with Kimi - continue the loop to retry
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

        loop {
            // Summarize and trim history to keep context manageable
            self.summarize_and_trim_history().await?;

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

    // Read kimi.md if it exists to get project context
    let kimi_context = if let Ok(kimi_content) = chat.read_file("kimi.md") {
        println!("{} {}", "📖".bright_cyan(), "Reading project context from kimi.md...".bright_black());
        kimi_content
    } else {
        println!("{} {}", "📖".bright_cyan(), "No kimi.md found. Starting fresh.".bright_black());
        String::new()
    };

    if !kimi_context.is_empty() {
        chat.messages.push(Message {
            role: "system".to_string(),
            content: format!("Project context: {}", kimi_context),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
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
