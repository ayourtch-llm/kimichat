use anyhow::{Context, Result};
use colored::Colorize;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

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

use logging::{ConversationLogger, log_request, log_request_to_file, log_response, log_stream_chunk};
use core::{ToolRegistry, ToolParameters};
use core::tool_context::ToolContext;
use policy::PolicyManager;
use tools_execution::parse_xml_tool_calls;
use tools_execution::validation::{repair_tool_call_with_model, validate_and_fix_tool_calls_in_place};
use cli::{Cli, Commands};
use config::{ClientConfig, GROQ_API_URL, normalize_api_url, initialize_tool_registry, initialize_agent_system};
use chat::{save_state, load_state};
use chat::history::summarize_and_trim_history;
use chat::session::chat as chat_session;
use agents::{
    PlanningCoordinator, GroqLlmClient,
    ChatMessage, ToolDefinition, ExecutionContext,
};
use models::{
    ModelType, Message, ToolCall, FunctionCall,
    SwitchModelArgs,
    ChatRequest, Tool, FunctionDef,
    ChatResponse, Usage,
    StreamChunk,
};


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
    // Streaming mode
    pub(crate) stream_responses: bool,
    // Verbose debug mode
    pub(crate) verbose: bool,
    // Debug level for controlling debug output (0=off, 1=basic, 2=detailed, etc.)
    pub(crate) debug_level: u32,
}

impl KimiChat {
    /// Normalize API URL by ensuring it has the correct path for OpenAI-compatible endpoints
    pub(crate) fn normalize_api_url(url: &str) -> String {
        normalize_api_url(url)
    }

    /// Generate system prompt based on current model
    pub(crate) fn get_system_prompt() -> String {
        "You are an AI assistant with access to file operations and model switching capabilities. \
        The system supports multiple models that can be switched during the conversation:\n\
        - grn_model (GrnModel): **Preferred for cost efficiency** - significantly cheaper than BluModel while providing good performance for most tasks\n\
        - blu_model (BluModel): Use when GrnModel struggles or when you need faster responses\n\n\
        IMPORTANT: You have been provided with a set of tools (functions) that you can use. \
        Only use the tools that are provided to you - do not make up tool names or attempt to use tools that are not available. \
        When making multiple file edits, use plan_edits to create a complete plan, then apply_edit_plan to execute all changes atomically. \
        This prevents issues where you lose track of file state between sequential edits.\n\n\
        Model switches may happen automatically during the conversation based on tool usage and errors. \
        The currently active model will be indicated in system messages as the conversation progresses.".to_string()
    }

    /// Get the API URL to use based on the current model and client configuration
    pub(crate) fn get_api_url(&self, model: &ModelType) -> String {
        let url = match model {
            ModelType::BluModel => {
                self.client_config.api_url_blu_model
                    .as_ref()
                    .map(|s| s.clone())
                    .unwrap_or_else(|| GROQ_API_URL.to_string())
            }
            ModelType::GrnModel => {
                self.client_config.api_url_grn_model
                    .as_ref()
                    .map(|s| s.clone())
                    .unwrap_or_else(|| GROQ_API_URL.to_string())
            }
            ModelType::AnthropicModel => {
                // For Anthropic, default to the official API or look for Anthropic-specific URLs
                env::var("ANTHROPIC_BASE_URL")
                    .or_else(|_| env::var("ANTHROPIC_BASE_URL_BLU"))
                    .or_else(|_| env::var("ANTHROPIC_BASE_URL_GRN"))
                    .unwrap_or_else(|_| "https://api.anthropic.com".to_string())
            }
            ModelType::Custom(_) => {
                // For custom models, default to the first available override or Groq
                self.client_config.api_url_blu_model
                    .as_ref()
                    .or(self.client_config.api_url_grn_model.as_ref())
                    .map(|s| s.clone())
                    .unwrap_or_else(|| GROQ_API_URL.to_string())
            }
        };

        // Normalize the URL to ensure it has the correct path
        Self::normalize_api_url(&url)
    }

    /// Get the appropriate API key for a given model based on configuration
    pub(crate) fn get_api_key(&self, model: &ModelType) -> String {
        match model {
            ModelType::BluModel => {
                self.client_config.api_key_blu_model
                    .as_ref()
                    .map(|s| s.clone())
                    .unwrap_or_else(|| self.api_key.clone())
            }
            ModelType::GrnModel => {
                self.client_config.api_key_grn_model
                    .as_ref()
                    .map(|s| s.clone())
                    .unwrap_or_else(|| self.api_key.clone())
            }
            ModelType::AnthropicModel => {
                // For Anthropic, look for Anthropic-specific keys first
                env::var("ANTHROPIC_API_KEY")
                    .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
                    .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN_BLU"))
                    .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN_GRN"))
                    .unwrap_or_else(|_| self.api_key.clone())
            }
            ModelType::Custom(_) => {
                // For custom models, default to the first available override or default key
                self.client_config.api_key_blu_model
                    .as_ref()
                    .or(self.client_config.api_key_grn_model.as_ref())
                    .map(|s| s.clone())
                    .unwrap_or_else(|| self.api_key.clone())
            }
        }
    }

    fn new(api_key: String, work_dir: PathBuf) -> Self {
        let config = ClientConfig {
            api_key: api_key.clone(),
            api_url_blu_model: None,
            api_url_grn_model: None,
            api_key_blu_model: None,
            api_key_grn_model: None,
            model_blu_model_override: None,
            model_grn_model_override: None,
        };
        let policy_manager = PolicyManager::new();
        Self::new_with_config(config, work_dir, false, policy_manager, false, false)
    }

    fn new_with_agents(api_key: String, work_dir: PathBuf, use_agents: bool) -> Self {
        let config = ClientConfig {
            api_key: api_key.clone(),
            api_url_blu_model: None,
            api_url_grn_model: None,
            api_key_blu_model: None,
            api_key_grn_model: None,
            model_blu_model_override: None,
            model_grn_model_override: None,
        };
        let policy_manager = PolicyManager::new();
        Self::new_with_config(config, work_dir, use_agents, policy_manager, false, false)
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

    fn new_with_config(client_config: ClientConfig, work_dir: PathBuf, use_agents: bool, policy_manager: PolicyManager, stream_responses: bool, verbose: bool) -> Self {
        let tool_registry = initialize_tool_registry();
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
            stream_responses,
            verbose,
            debug_level: 0, // Default debug level is 0 (off)
        };

        // Add system message to inform the model about capabilities
        let system_content = Self::get_system_prompt();

        chat.messages.push(Message {
            role: "system".to_string(),
            content: system_content,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });

        // Add initial model notification
        chat.messages.push(Message {
            role: "system".to_string(),
            content: format!("Current model: {}", chat.current_model.display_name()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
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
    async fn process_with_agents(&mut self, user_request: &str) -> Result<String> {
        // Get API URL before mutable borrow
        let api_url = self.get_api_url(&self.current_model);
        let api_key = self.get_api_key(&self.current_model);

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
        summarize_and_trim_history(self).await
    }

    /// Attempt to repair malformed tool calls using a separate API call to a model
    async fn repair_tool_call_with_model(&self, tool_call: &ToolCall, error_msg: &str) -> Result<ToolCall> {
        repair_tool_call_with_model(self, tool_call, error_msg).await
    }

    fn validate_and_fix_tool_calls_in_place(&mut self) -> Result<bool> {
        validate_and_fix_tool_calls_in_place(self)
    }

    /// Handle streaming API response, displaying chunks as they arrive
    async fn call_api_streaming(&self, orig_messages: &[Message]) -> Result<(Message, Option<Usage>, ModelType)> {
        use std::io::{self, Write};
        use futures_util::StreamExt;

        let current_model = self.current_model.clone();

        let request = ChatRequest {
            model: current_model.as_str().to_string(),
            messages: orig_messages.to_vec(),
            tools: self.get_tools(),
            tool_choice: "auto".to_string(),
            stream: Some(true),
        };

        // Get the appropriate API URL based on the current model
        let api_url = self.get_api_url(&current_model);

        // Log request details in verbose mode
        log_request(&api_url, &request, &self.api_key, self.verbose);

        // Log request to file for persistent debugging
        let _ = log_request_to_file(&api_url, &request, &current_model, &self.api_key);

        let api_key = self.get_api_key(&current_model);
        let response = self
            .client
            .post(&api_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        let headers = response.headers().clone();

        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_else(|_| "Unable to read error body".to_string());

            // Log error response
            log_response(&status, &headers, &error_body, self.verbose);

            return Err(anyhow::anyhow!("API request failed with status {}: {}", status, error_body));
        }

        if self.verbose {
            println!("\n{}", "📡 Starting streaming response...".bright_cyan());
            println!("{}", "═".repeat(80).bright_cyan());
        }

        // Process streaming response
        let mut accumulated_content = String::new();
        let mut accumulated_reasoning = String::new();
        let mut accumulated_tool_calls: Vec<ToolCall> = Vec::new();
        let mut role = String::new();
        let mut usage: Option<Usage> = None;
        let mut buffer = String::new();

        // Show thinking indicator
        print!("🤔 Thinking...");
        io::stdout().flush().unwrap();
        let mut first_chunk = true;
        let mut first_reasoning = true;

        // Read the response as a stream of bytes
        let mut stream = response.bytes_stream();
        let mut chunk_counter = 0;

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

                        // Log stream chunk in verbose mode
                        chunk_counter += 1;
                        log_stream_chunk(chunk_counter, data, self.verbose);

                        // Check for stream end marker
                        if data.trim() == "[DONE]" {
                            if self.verbose {
                                println!("{}", "✓ Stream completed".bright_green());
                                println!("{}", "═".repeat(80).bright_green());
                            }
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

                                // Accumulate and display reasoning content (thinking process)
                                if let Some(reasoning) = &delta.reasoning_content {
                                    if first_chunk {
                                        // Clear thinking indicator
                                        print!("\r\x1B[K");
                                        io::stdout().flush().unwrap();
                                        first_chunk = false;
                                    }

                                    if first_reasoning {
                                        // Show reasoning header
                                        print!("{}", "💭 ".bright_black());
                                        first_reasoning = false;
                                    }

                                    accumulated_reasoning.push_str(reasoning);
                                    // Display reasoning in dim color to distinguish from actual response
                                    print!("{}", reasoning.bright_black());
                                    io::stdout().flush().unwrap();
                                }

                                // Accumulate content and display it
                                if let Some(content) = &delta.content {
                                    if first_chunk {
                                        // Clear thinking indicator
                                        print!("\r\x1B[K");
                                        io::stdout().flush().unwrap();
                                        first_chunk = false;
                                    }

                                    // If we just finished reasoning, add separator
                                    if !first_reasoning && accumulated_content.is_empty() {
                                        println!(); // New line after reasoning
                                    }

                                    accumulated_content.push_str(content);
                                    print!("{}", content);
                                    io::stdout().flush().unwrap();
                                }

                                // Accumulate tool calls if present (streaming deltas)
                                if let Some(tool_call_deltas) = &delta.tool_calls {
                                    if first_chunk {
                                        // Clear thinking indicator
                                        print!("\r\x1B[K");
                                        print!("🔧 Tool calls...");
                                        io::stdout().flush().unwrap();
                                        first_chunk = false;
                                    }

                                    for delta_call in tool_call_deltas {
                                        // Ensure we have enough slots in the accumulated array
                                        while accumulated_tool_calls.len() <= delta_call.index {
                                            accumulated_tool_calls.push(ToolCall {
                                                id: String::new(),
                                                tool_type: "function".to_string(),
                                                function: FunctionCall {
                                                    name: String::new(),
                                                    arguments: String::new(),
                                                },
                                            });
                                        }

                                        let tool_call = &mut accumulated_tool_calls[delta_call.index];

                                        // Merge the delta into the accumulated tool call
                                        if let Some(id) = &delta_call.id {
                                            tool_call.id = id.clone();
                                        }
                                        if let Some(tool_type) = &delta_call.tool_type {
                                            tool_call.tool_type = tool_type.clone();
                                        }
                                        if let Some(function_delta) = &delta_call.function {
                                            if let Some(name) = &function_delta.name {
                                                tool_call.function.name = name.clone();
                                            }
                                            if let Some(args) = &function_delta.arguments {
                                                tool_call.function.arguments.push_str(args);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => return Err(anyhow::anyhow!("Error reading stream: {}", e)),
            }
        }

        // Clear thinking indicator if it was never cleared (no content received)
        if first_chunk {
            print!("\r\x1B[K");
            io::stdout().flush().unwrap();
        }

        println!(); // New line after streaming complete

        // Build the final message
        let mut message = Message {
            role: if role.is_empty() { "assistant".to_string() } else { role },
            content: accumulated_content.clone(),
            tool_calls: if accumulated_tool_calls.is_empty() { None } else { Some(accumulated_tool_calls) },
            tool_call_id: None,
            name: None,
        };

        // If no structured tool calls were received, check for XML format in content
        if message.tool_calls.is_none() {
            if let Some(parsed_calls) = parse_xml_tool_calls(&accumulated_content) {
                eprintln!("{} Detected XML-format tool calls, parsing {} call(s)", "🔧".bright_yellow(), parsed_calls.len());
                message.tool_calls = Some(parsed_calls);
                // Clear the XML from content to avoid displaying it
                message.content = String::new();
            }
        }

        Ok((message, usage, current_model))
    }

    async fn call_api(&self, orig_messages: &[Message]) -> Result<(Message, Option<Usage>, ModelType)> {
        let mut current_model = self.current_model.clone();
        // Clone messages for potential retry logic with model switching
        let mut messages = orig_messages.to_vec();

        // Check if we need to use the new LlmClient system for Anthropic
        let is_custom_claude = if let ModelType::Custom(ref name) = current_model {
            name.contains("claude")
        } else {
            false
        };

        let should_use_anthropic = matches!(current_model, ModelType::AnthropicModel) ||
            is_custom_claude ||
            (self.client_config.api_url_blu_model.as_ref().map(|u| u.contains("anthropic")).unwrap_or(false)) ||
            (self.client_config.api_url_grn_model.as_ref().map(|u| u.contains("anthropic")).unwrap_or(false));

        if self.should_show_debug(1) {
            println!("🔧 DEBUG: current_model = {:?}", current_model);
            println!("🔧 DEBUG: should_use_anthropic = {}", should_use_anthropic);
        }
        if should_use_anthropic {
            if self.should_show_debug(1) {
                println!("🔧 DEBUG: Using call_api_with_llm_client for Anthropic");
            }
            return self.call_api_with_llm_client(&messages, &current_model).await;
        } else {
            if self.should_show_debug(1) {
                println!("🔧 DEBUG: Using regular OpenAI-style call_api");
            }
        }

        // Retry logic with exponential backoff
        let mut retry_count = 0;
        loop {
	    let request = ChatRequest {
		model: current_model.as_str().to_string(),
		messages: messages.clone(),
		tools: self.get_tools(),
		tool_choice: "auto".to_string(),
		stream: None,
	    };

            // Get the appropriate API URL based on the current model
            let api_url = self.get_api_url(&current_model);

            // Log request details in verbose mode
            log_request(&api_url, &request, &self.api_key, self.verbose);

            // Log request to file for persistent debugging
            let _ = log_request_to_file(&api_url, &request, &current_model, &self.api_key);

            let api_key = self.get_api_key(&current_model);
            let response = self
                .client
                .post(&api_url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await?;

            let status = response.status();
            let headers = response.headers().clone();

            // Handle rate limiting with exponential backoff
            if status == 429 {
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
            if !status.is_success() {
                let error_body = response.text().await.unwrap_or_else(|_| "Unable to read error body".to_string());

                // Log error response in verbose mode
                log_response(&status, &headers, &error_body, self.verbose);

                // Check if this is a tool-related error
                if status == 400 && error_body.contains("tool_use_failed") {
                    eprintln!("{}", "❌ Tool calling error detected!".red().bold());
                    eprintln!("{}", error_body.yellow());

                    // Check for GrnModel hallucinating non-existent tools
                    if error_body.contains("attempted to call tool") && current_model == ModelType::GrnModel {
                        eprintln!("{}", "🔄 GrnModel attempted to use non-existent tool. Switching to BluModel and retrying...".bright_cyan());

                        // Switch to BluModel
                        current_model = ModelType::BluModel;

                        // Add message to conversation history about model switch
                        messages.push(Message {
                            role: "system".to_string(),
                            content: format!("Model switched to: {} (reason: invalid tool usage)", current_model.display_name()),
                            tool_calls: None,
                            tool_call_id: None,
                            name: None,
                        });

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

                        // Add message to conversation history about model switch
                        messages.push(Message {
                            role: "system".to_string(),
                            content: format!("Model switched to: {} (reason: tool call repair failed)", current_model.display_name()),
                            tool_calls: None,
                            tool_call_id: None,
                            name: None,
                        });

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

            // Log successful response in verbose mode
            log_response(&status, &headers, &response_text, self.verbose);

            let chat_response: ChatResponse = serde_json::from_str(&response_text)
                .with_context(|| format!("Failed to parse API response: {}", response_text))?;

            let mut message = chat_response
                .choices
                .into_iter()
                .next()
                .map(|c| c.message)
                .context("No response from API")?;

            // If no structured tool calls were received, check for XML format in content
            if message.tool_calls.is_none() {
                if let Some(parsed_calls) = parse_xml_tool_calls(&message.content) {
                    eprintln!("{} Detected XML-format tool calls, parsing {} call(s)", "🔧".bright_yellow(), parsed_calls.len());
                    message.tool_calls = Some(parsed_calls);
                    // Clear the XML from content to avoid displaying it
                    message.content = String::new();
                }
            }

            return Ok((message, chat_response.usage, current_model));
        }
    }

    /// Call API using the new LlmClient system (for Anthropic and future backends)
    async fn call_api_with_llm_client(&self, messages: &[Message], model: &ModelType) -> Result<(Message, Option<Usage>, ModelType)> {
        if self.should_show_debug(1) {
            println!("🔧 DEBUG: call_api_with_llm_client called with model: {:?}", model);
        }
        if self.should_show_debug(2) {
            println!("🔧 DEBUG: client_config.api_url_blu_model: {:?}", self.client_config.api_url_blu_model);
            println!("🔧 DEBUG: client_config.api_url_grn_model: {:?}", self.client_config.api_url_grn_model);
        }

        // Convert old Message format to new ChatMessage format
        let chat_messages: Vec<ChatMessage> = messages.iter().map(|msg| {
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

        // Convert tools to the new format
        let tools: Vec<ToolDefinition> = self.get_tools().into_iter().map(|tool| {
            ToolDefinition {
                name: tool.function.name,
                description: tool.function.description,
                parameters: tool.function.parameters,
            }
        }).collect();

        // Create the appropriate LlmClient using the same logic as agent mode
        let llm_client: std::sync::Arc<dyn crate::agents::agent::LlmClient> =
            if matches!(model, ModelType::BluModel) {
                // Blu model logic (same as agent mode)
                if let Some(ref api_url) = self.client_config.api_url_blu_model {
                    if api_url.contains("anthropic") {
                        println!("{} Using Anthropic API for 'blu_model' at: {}", "🧠".cyan(), api_url);
                        if self.should_show_debug(2) {
                            println!("🔧 DEBUG: BluModel Anthropic URL: '{}', API Key present: {}", api_url, self.client_config.api_key_blu_model.is_some());
                        }
                        std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                            self.client_config.api_key_blu_model.clone().unwrap_or_default(),
                            model.as_str(),
                            api_url.clone(),
                            "blu_model".to_string()
                        ))
                    } else {
                        println!("{} Using llama.cpp for 'blu_model' at: {}", "🦙".cyan(), api_url);
                        std::sync::Arc::new(crate::agents::llama_cpp_client::LlamaCppClient::new(
                            api_url.clone(),
                            model.as_str()
                        ))
                    }
                } else if env::var("ANTHROPIC_AUTH_TOKEN_BLU").is_ok() ||
                          (env::var("ANTHROPIC_AUTH_TOKEN").is_ok() &&
                           (self.client_config.model_blu_model_override.as_ref()
                            .map(|m| m.contains("claude") || m.contains("anthropic"))
                            .unwrap_or(false))) {
                    println!("{} Using Anthropic API for 'blu_model'", "🧠".cyan());
                    let anthropic_key = env::var("ANTHROPIC_AUTH_TOKEN_BLU")
                        .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
                        .unwrap_or_default();
                    std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                        anthropic_key,
                        model.as_str(),
                        "https://api.anthropic.com".to_string(),
                        "blu_model".to_string()
                    ))
                } else {
                    println!("{} Using Groq API for 'blu_model'", "🚀".cyan());
                    std::sync::Arc::new(crate::agents::groq_client::GroqLlmClient::new(
                        self.client_config.api_key.clone(),
                        model.as_str(),
                        crate::GROQ_API_URL.to_string(),
                        "blu_model".to_string()
                    ))
                }
            } else if matches!(model, ModelType::AnthropicModel) {
                // Anthropic model logic - use raw URL
                let api_url = env::var("ANTHROPIC_BASE_URL")
                    .or_else(|_| env::var("ANTHROPIC_BASE_URL_BLU"))
                    .or_else(|_| env::var("ANTHROPIC_BASE_URL_GRN"))
                    .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
                println!("{} Using Anthropic API for 'anthropic' at: {}", "🧠".cyan(), api_url);
                if self.should_show_debug(2) {
                    println!("🔧 DEBUG: AnthropicModel URL: '{}'", api_url);
                }
                let api_key = env::var("ANTHROPIC_API_KEY")
                    .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
                    .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN_BLU"))
                    .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN_GRN"))
                    .unwrap_or_else(|_| self.client_config.api_key.clone());
                std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                    api_key,
                    model.as_str(),
                    api_url,
                    "anthropic".to_string()
                ))
            } else {
                // Grn model logic (same as agent mode)
                if let Some(ref api_url) = self.client_config.api_url_grn_model {
                    if api_url.contains("anthropic") {
                        println!("{} Using Anthropic API for 'grn_model' at: {}", "🧠".cyan(), api_url);
                        if self.should_show_debug(2) {
                            println!("🔧 DEBUG: GrnModel Anthropic URL: '{}', API Key present: {}", api_url, self.client_config.api_key_grn_model.is_some());
                        }
                        std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                            self.client_config.api_key_grn_model.clone().unwrap_or_default(),
                            model.as_str(),
                            api_url.clone(),
                            "grn_model".to_string()
                        ))
                    } else {
                        println!("{} Using llama.cpp for 'grn_model' at: {}", "🦙".cyan(), api_url);
                        std::sync::Arc::new(crate::agents::llama_cpp_client::LlamaCppClient::new(
                            api_url.clone(),
                            model.as_str()
                        ))
                    }
                } else if env::var("ANTHROPIC_AUTH_TOKEN_GRN").is_ok() ||
                          (env::var("ANTHROPIC_AUTH_TOKEN").is_ok() &&
                           (self.client_config.model_grn_model_override.as_ref()
                            .map(|m| m.contains("claude") || m.contains("anthropic"))
                            .unwrap_or(false))) {
                    println!("{} Using Anthropic API for 'grn_model'", "🧠".cyan());
                    let anthropic_key = env::var("ANTHROPIC_AUTH_TOKEN_GRN")
                        .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
                        .unwrap_or_default();
                    std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                        anthropic_key,
                        model.as_str(),
                        "https://api.anthropic.com".to_string(),
                        "grn_model".to_string()
                    ))
                } else {
                    println!("{} Using Groq API for 'grn_model'", "🚀".cyan());
                    std::sync::Arc::new(crate::agents::groq_client::GroqLlmClient::new(
                        self.client_config.api_key.clone(),
                        model.as_str(),
                        crate::GROQ_API_URL.to_string(),
                        "grn_model".to_string()
                    ))
                }
            };

        // Make the API call
        let response = llm_client.chat(chat_messages, tools).await?;

        // Convert the response back to the old format
        let message = Message {
            role: response.message.role,
            content: response.message.content,
            tool_calls: response.message.tool_calls.map(|calls| {
                calls.into_iter().map(|call| crate::ToolCall {
                    id: call.id,
                    tool_type: "function".to_string(),
                    function: crate::FunctionCall {
                        name: call.function.name,
                        arguments: call.function.arguments,
                    },
                }).collect()
            }),
            tool_call_id: response.message.tool_call_id,
            name: response.message.name,
        };

        let usage = response.usage.map(|u| Usage {
            prompt_tokens: u.prompt_tokens as usize,
            completion_tokens: u.completion_tokens as usize,
            total_tokens: u.total_tokens as usize,
        });

        Ok((message, usage, model.clone()))
    }

    /// Streaming API call using the new LlmClient system
    async fn call_api_streaming_with_llm_client(&self, messages: &[Message], model: &ModelType) -> Result<(Message, Option<Usage>, ModelType)> {
        if self.should_show_debug(1) {
            println!("🔧 DEBUG: call_api_streaming_with_llm_client called with model: {:?}", model);
        }

        // Convert old Message format to new ChatMessage format
        let chat_messages: Vec<ChatMessage> = messages.iter().map(|msg| {
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

        // Convert tools to the new format
        let tools: Vec<ToolDefinition> = self.get_tools().into_iter().map(|tool| {
            ToolDefinition {
                name: tool.function.name,
                description: tool.function.description,
                parameters: tool.function.parameters,
            }
        }).collect();

        // Create the appropriate LlmClient using the same logic as call_api_with_llm_client
        let llm_client: std::sync::Arc<dyn crate::agents::agent::LlmClient> =
            if matches!(model, ModelType::BluModel) {
                // Blu model logic (same as agent mode)
                if let Some(ref api_url) = self.client_config.api_url_blu_model {
                    if api_url.contains("anthropic") {
                        println!("{} Using Anthropic streaming API for 'blu_model' at: {}", "🧠".cyan(), api_url);
                        std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                            self.client_config.api_key_blu_model.clone().unwrap_or_default(),
                            model.as_str(),
                            api_url.clone(),
                            "blu_model".to_string()
                        ))
                    } else {
                        println!("{} Using llama.cpp for 'blu_model' at: {}", "🦙".cyan(), api_url);
                        std::sync::Arc::new(crate::agents::llama_cpp_client::LlamaCppClient::new(
                            api_url.clone(),
                            model.as_str()
                        ))
                    }
                } else if env::var("ANTHROPIC_AUTH_TOKEN_BLU").is_ok() ||
                          (env::var("ANTHROPIC_AUTH_TOKEN").is_ok() &&
                           (self.client_config.model_blu_model_override.as_ref()
                            .map(|m| m.contains("claude") || m.contains("anthropic"))
                            .unwrap_or(false))) {
                    println!("{} Using Anthropic streaming API for 'blu_model'", "🧠".cyan());
                    let anthropic_key = env::var("ANTHROPIC_AUTH_TOKEN_BLU")
                        .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
                        .unwrap_or_default();
                    std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                        anthropic_key,
                        model.as_str(),
                        "https://api.anthropic.com".to_string(),
                        "blu_model".to_string()
                    ))
                } else {
                    println!("{} Using Groq API for 'blu_model'", "🚀".cyan());
                    std::sync::Arc::new(crate::agents::groq_client::GroqLlmClient::new(
                        self.client_config.api_key.clone(),
                        model.as_str(),
                        crate::GROQ_API_URL.to_string(),
                        "blu_model".to_string()
                    ))
                }
            } else if matches!(model, ModelType::AnthropicModel) {
                // Anthropic model logic - use raw URL
                let api_url = env::var("ANTHROPIC_BASE_URL")
                    .or_else(|_| env::var("ANTHROPIC_BASE_URL_BLU"))
                    .or_else(|_| env::var("ANTHROPIC_BASE_URL_GRN"))
                    .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
                println!("{} Using Anthropic streaming API for 'anthropic' at: {}", "🧠".cyan(), api_url);
                let api_key = env::var("ANTHROPIC_API_KEY")
                    .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
                    .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN_BLU"))
                    .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN_GRN"))
                    .unwrap_or_else(|_| self.client_config.api_key.clone());
                std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                    api_key,
                    model.as_str(),
                    api_url,
                    "anthropic".to_string()
                ))
            } else {
                // Grn model logic (same as agent mode)
                if let Some(ref api_url) = self.client_config.api_url_grn_model {
                    if api_url.contains("anthropic") {
                        println!("{} Using Anthropic streaming API for 'grn_model' at: {}", "🧠".cyan(), api_url);
                        std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                            self.client_config.api_key_grn_model.clone().unwrap_or_default(),
                            model.as_str(),
                            api_url.clone(),
                            "grn_model".to_string()
                        ))
                    } else {
                        println!("{} Using llama.cpp for 'grn_model' at: {}", "🦙".cyan(), api_url);
                        std::sync::Arc::new(crate::agents::llama_cpp_client::LlamaCppClient::new(
                            api_url.clone(),
                            model.as_str()
                        ))
                    }
                } else if env::var("ANTHROPIC_AUTH_TOKEN_GRN").is_ok() ||
                          (env::var("ANTHROPIC_AUTH_TOKEN").is_ok() &&
                           (self.client_config.model_grn_model_override.as_ref()
                            .map(|m| m.contains("claude") || m.contains("anthropic"))
                            .unwrap_or(false))) {
                    println!("{} Using Anthropic streaming API for 'grn_model'", "🧠".cyan());
                    let anthropic_key = env::var("ANTHROPIC_AUTH_TOKEN_GRN")
                        .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
                        .unwrap_or_default();
                    std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                        anthropic_key,
                        model.as_str(),
                        "https://api.anthropic.com".to_string(),
                        "grn_model".to_string()
                    ))
                } else {
                    println!("{} Using Groq API for 'grn_model'", "🚀".cyan());
                    std::sync::Arc::new(crate::agents::groq_client::GroqLlmClient::new(
                        self.client_config.api_key.clone(),
                        model.as_str(),
                        crate::GROQ_API_URL.to_string(),
                        "grn_model".to_string()
                    ))
                }
            };

        println!("\n{}", "📡 Starting Anthropic streaming response...".bright_cyan());

        // Initialize response accumulation
        let mut accumulated_content = String::new();

        // Get the streaming response
        let mut stream = llm_client.chat_streaming(chat_messages.clone(), tools.clone()).await?;

        // Process the stream with minimal buffering
        use futures::StreamExt;
        use std::io::{self, Write};
        
        // Ensure stdout is flushed immediately for each chunk
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    // Print the delta immediately without any buffering
                    if !chunk.delta.is_empty() {
                        // Use direct write and flush for minimal latency
                        io::stdout().write_all(chunk.delta.as_bytes()).unwrap();
                        io::stdout().flush().unwrap();
                        accumulated_content.push_str(&chunk.delta);
                    }

                    // Check if we're done
                    if let Some(ref reason) = chunk.finish_reason {
                        if reason == "stop" {
                            break;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{} Streaming error: {}", "❌".red(), e);
                    break;
                }
            }
        }

        println!(); // New line after streaming complete

        // For now, we'll make a non-streaming call to get the complete response with tool calls
        // This is a limitation of the current format translation approach
        let response = llm_client.chat(chat_messages, tools).await?;

        // Convert the response back to the old format
        let message = Message {
            role: response.message.role,
            content: response.message.content,
            tool_calls: response.message.tool_calls.map(|calls| {
                calls.into_iter().map(|call| crate::ToolCall {
                    id: call.id,
                    tool_type: "function".to_string(),
                    function: crate::FunctionCall {
                        name: call.function.name,
                        arguments: call.function.arguments,
                    },
                }).collect()
            }),
            tool_call_id: response.message.tool_call_id,
            name: response.message.name,
        };

        let usage = response.usage.map(|u| Usage {
            prompt_tokens: u.prompt_tokens as usize,
            completion_tokens: u.completion_tokens as usize,
            total_tokens: u.total_tokens as usize,
        });

        Ok((message, usage, model.clone()))
    }
    async fn chat(&mut self, user_message: &str) -> Result<String> {
        chat_session(self, user_message).await
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
    // Also check for Anthropic environment variables
    let api_url_blu_model = cli.api_url_blu_model
        .or_else(|| cli.llama_cpp_url.clone())
        .or_else(|| env::var("ANTHROPIC_BASE_URL_BLU").ok())
        .or_else(|| env::var("ANTHROPIC_BASE_URL").ok());

    let api_url_grn_model = cli.api_url_grn_model
        .or_else(|| cli.llama_cpp_url.clone())
        .or_else(|| env::var("ANTHROPIC_BASE_URL_GRN").ok())
        .or_else(|| env::var("ANTHROPIC_BASE_URL").ok());

    // Check for per-model API keys (for Anthropic or other services)
    let api_key_blu_model = env::var("ANTHROPIC_AUTH_TOKEN_BLU").ok()
        .or_else(|| env::var("ANTHROPIC_AUTH_TOKEN").ok());

    let api_key_grn_model = env::var("ANTHROPIC_AUTH_TOKEN_GRN").ok()
        .or_else(|| env::var("ANTHROPIC_AUTH_TOKEN").ok());

    // Auto-detect Anthropic and set appropriate model names if not overridden
    let is_anthropic_blu = api_url_blu_model.as_ref()
        .map(|url| url.contains("anthropic"))
        .unwrap_or(false);
    let is_anthropic_grn = api_url_grn_model.as_ref()
        .map(|url| url.contains("anthropic"))
        .unwrap_or(false);

    let model_blu_override = cli.model_blu_model.clone()
        .or_else(|| cli.model.clone())
        .or_else(|| {
            if is_anthropic_blu {
                env::var("ANTHROPIC_MODEL_BLU").ok()
                    .or_else(|| env::var("ANTHROPIC_MODEL").ok())
                    .or(Some("claude-3-5-sonnet-20241022".to_string()))
            } else {
                None
            }
        });

    let model_grn_override = cli.model_grn_model.clone()
        .or_else(|| cli.model.clone())
        .or_else(|| {
            if is_anthropic_grn {
                env::var("ANTHROPIC_MODEL_GRN").ok()
                    .or_else(|| env::var("ANTHROPIC_MODEL").ok())
                    .or(Some("claude-3-5-sonnet-20241022".to_string()))
            } else {
                None
            }
        });

    // API key is only required if at least one model uses Groq (no API URL specified and no per-model key)
    let needs_groq_key = (api_url_blu_model.is_none() && api_key_blu_model.is_none())
                      || (api_url_grn_model.is_none() && api_key_grn_model.is_none());

    let api_key = if needs_groq_key {
        env::var("GROQ_API_KEY")
            .context("GROQ_API_KEY environment variable not set. Use --api-url-blu-model and/or --api-url-grn-model with ANTHROPIC_AUTH_TOKEN to use other backends.")?
    } else {
        // Using custom backends with per-model keys, no Groq key needed
        String::new()
    };

    // Use current directory as work_dir so the AI can see project files
    // NB: do NOT use the 'workspace' subdirectory as work_dir
    let work_dir = env::current_dir()?;

    // If a subcommand was provided, execute it and exit
    if let Some(command) = cli.command {
        // Special handling for Switch command which needs KimiChat
        let result = match &command {
            Commands::Switch { model, reason } => {
                let mut chat = KimiChat::new("".to_string(), work_dir.clone());
                chat.switch_model(model, reason)?
            }
            _ => command.execute().await?
        };
        println!("{}", result);
        return Ok(());
    }

    // Create client configuration from CLI arguments
    // Priority: specific flags override general --model flag, with auto-detection for Anthropic
    let client_config = ClientConfig {
        api_key: api_key.clone(),
        api_url_blu_model: api_url_blu_model.clone(),
        api_url_grn_model: api_url_grn_model.clone(),
        api_key_blu_model,
        api_key_grn_model,
        model_blu_model_override: model_blu_override.clone(),
        model_grn_model_override: model_grn_override.clone(),
    };

    // Inform user about auto-detected Anthropic configuration
    if is_anthropic_blu {
        let model_name = model_blu_override.as_ref().unwrap();
        eprintln!("{} Anthropic detected for blu_model: using model '{}'", "🤖".cyan(), model_name);
    }
    if is_anthropic_grn {
        let model_name = model_grn_override.as_ref().unwrap();
        eprintln!("{} Anthropic detected for grn_model: using model '{}'", "🤖".cyan(), model_name);
    }

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

        let mut chat = KimiChat::new_with_config(client_config.clone(), work_dir.clone(), cli.agents, policy_manager.clone(), cli.stream, cli.verbose);

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

    if cli.agents {
        println!("{}", "🚀 Multi-Agent System ENABLED - Specialized agents will handle your tasks".green().bold());
    }

    println!("{}", "Type 'exit' or 'quit' to exit\n".bright_black());

    let mut chat = KimiChat::new_with_config(client_config, work_dir, cli.agents, policy_manager, cli.stream, cli.verbose);

    // Show the actual current model configuration
    let current_model_display = match chat.current_model {
        ModelType::BluModel => format!("BluModel/{} (auto-switched from default)", chat.current_model.display_name()),
        ModelType::GrnModel => format!("GrnModel/{} (default)", chat.current_model.display_name()),
        ModelType::AnthropicModel => format!("Anthropic/{}", chat.current_model.display_name()),
        ModelType::Custom(ref name) => format!("Custom/{}", name),
    };

    // Show what backends are being used
    let blu_backend = if chat.client_config.api_url_blu_model.as_ref().map(|u| u.contains("anthropic")).unwrap_or(false) ||
                       env::var("ANTHROPIC_AUTH_TOKEN_BLU").is_ok() {
        "Anthropic API 🧠"
    } else if chat.client_config.api_url_blu_model.is_some() {
        "llama.cpp 🦙"
    } else {
        "Groq API 🚀"
    };

    let grn_backend = if chat.client_config.api_url_grn_model.as_ref().map(|u| u.contains("anthropic")).unwrap_or(false) ||
                       env::var("ANTHROPIC_AUTH_TOKEN_GRN").is_ok() {
        "Anthropic API 🧠"
    } else if chat.client_config.api_url_grn_model.is_some() {
        "llama.cpp 🦙"
    } else {
        "Groq API 🚀"
    };

    println!("{}", format!("Default model: {} • BluModel uses {}, GrnModel uses {}",
        current_model_display, blu_backend, grn_backend).bright_black());

    // Debug info (shown at debug level 1+)
    if chat.should_show_debug(1) {
        println!("{}", format!("🔧 DEBUG: blu_model URL: {:?}", chat.client_config.api_url_blu_model).bright_black());
        println!("{}", format!("🔧 DEBUG: grn_model URL: {:?}", chat.client_config.api_url_grn_model).bright_black());
        println!("{}", format!("🔧 DEBUG: Current model: {:?}", chat.current_model).bright_black());
    }

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

                // Handle /save and /load commands
                if line.starts_with("/save ") {
                    let file_path = line[6..].trim();
                    match chat.save_state(file_path) {
                        Ok(msg) => println!("{} {}", "💾".bright_green(), msg),
                        Err(e) => eprintln!("{} Failed to save: {}", "❌".bright_red(), e),
                    }
                    continue;
                }

                if line.starts_with("/load ") {
                    let file_path = line[6..].trim();
                    match chat.load_state(file_path) {
                        Ok(msg) => println!("{} {}", "📂".bright_green(), msg),
                        Err(e) => eprintln!("{} Failed to load: {}", "❌".bright_red(), e),
                    }
                    continue;
                }

                // Handle /debug command
                if line == "/debug" {
                    println!("{} Debug level: {} (binary: {:b})", "🔧".bright_cyan(), chat.get_debug_level(), chat.get_debug_level());
                    println!("{} Usage: /debug <level>", "💡".bright_yellow());
                    println!("  0 = off");
                    println!("  1 = basic (bit 0)");
                    println!("  2 = detailed (bit 1)");
                    println!("  4 = verbose (bit 2)");
                    println!("  Example: /debug 3 (enables basic + detailed)");
                    continue;
                }

                if line.starts_with("/debug ") {
                    let level_str = line[7..].trim();
                    match level_str.parse::<u32>() {
                        Ok(level) => {
                            chat.set_debug_level(level);
                            println!("{} Debug level set to {} (binary: {:b})", "🔧".bright_green(), level, level);
                        }
                        Err(_) => {
                            eprintln!("{} Invalid debug level: '{}'. Use a number like 0, 1, 3, 7, etc.", "❌".bright_red(), level_str);
                        }
                    }
                    continue;
                }

                rl.add_history_entry(line)?;

                // Log the user message before sending
                if let Some(logger) = &mut chat.logger {
                    logger.log("user", line, None, false).await;
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
                } else {
                    // Add extra newline after streaming to separate from next prompt
                    println!();
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
