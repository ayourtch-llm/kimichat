use crate::agents::agent::{Agent, ExecutionContext, LlmClient};
use crate::agents::agent_config::AgentConfig;
use crate::core::tool_registry::ToolRegistry;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use colored::Colorize;

/// Factory for creating agents from configuration
pub struct AgentFactory {
    tool_registry: Arc<ToolRegistry>,
    llm_clients: HashMap<String, Arc<dyn LlmClient>>,
    policy_manager: crate::policy::PolicyManager,
}

impl AgentFactory {
    pub fn new(tool_registry: Arc<ToolRegistry>, policy_manager: crate::policy::PolicyManager) -> Self {
        Self {
            tool_registry,
            llm_clients: HashMap::new(),
            policy_manager,
        }
    }

    pub fn register_llm_client(&mut self, model: String, client: Arc<dyn LlmClient>) {
        self.llm_clients.insert(model, client);
    }

    pub fn create_agent(&self, config: &AgentConfig) -> Result<Box<dyn Agent>> {
        // Validate configuration
        config.validate()
            .map_err(|e| anyhow::anyhow!("Invalid agent config: {}", e))?;

        // Debug: Show what agent we're creating and what tools it should have
        eprintln!("[DEBUG] Creating agent '{}' with {} tools: {:?}",
                 config.name, config.tools.len(), config.tools);

        // Get LLM client for this agent
        let llm_client = self.llm_clients.get(&config.model)
            .ok_or_else(|| anyhow::anyhow!("No LLM client available for model: {}", config.model))?
            .clone();

        // Create configurable agent
        let agent = ConfigurableAgent::new(
            config.clone(),
            Arc::clone(&self.tool_registry),
            llm_client,
            self.policy_manager.clone(),
        )?;

        Ok(Box::new(agent))
    }
}

/// Configurable agent implementation
pub struct ConfigurableAgent {
    config: AgentConfig,
    tool_registry: Arc<ToolRegistry>,
    llm_client: Arc<dyn LlmClient>,
    policy_manager: crate::policy::PolicyManager,
}

impl ConfigurableAgent {
    pub fn new(
        config: AgentConfig,
        tool_registry: Arc<ToolRegistry>,
        llm_client: Arc<dyn LlmClient>,
        policy_manager: crate::policy::PolicyManager,
    ) -> Result<Self> {
        // Validate that all required tools are available
        for tool_name in &config.tools {
            if !tool_registry.has_tool(tool_name) {
                return Err(anyhow::anyhow!("Required tool '{}' not found in registry", tool_name));
            }
        }

        Ok(Self {
            config,
            tool_registry,
            llm_client,
            policy_manager,
        })
    }

    async fn execute_with_tools(
        &self,
        task: &crate::agents::agent::Task,
        context: &ExecutionContext,
    ) -> crate::agents::agent::AgentResult {
        let start_time = std::time::Instant::now();

        // Prepare tools for this agent
        eprintln!("[DEBUG] Agent '{}' preparing tools from config.tools: {:?}",
                 self.config.name, self.config.tools);

        let available_tools: Vec<_> = self.config.tools
            .iter()
            .filter_map(|tool_name| {
                let tool = self.tool_registry.get_tool(tool_name);
                if tool.is_none() {
                    eprintln!("[DEBUG] Tool '{}' not found in registry!", tool_name);
                }
                tool
            })
            .map(|tool| {
                let openai_def = tool.to_openai_definition();
                // Extract just the parameters schema from the full definition
                let parameters = openai_def["function"]["parameters"].clone();

                crate::agents::agent::ToolDefinition {
                    name: tool.name().to_string(),
                    description: tool.description().to_string(),
                    parameters,
                }
            })
            .collect();

        eprintln!("[DEBUG] Agent '{}' has {} available tools: {:?}",
                 self.config.name,
                 available_tools.len(),
                 available_tools.iter().map(|t| &t.name).collect::<Vec<_>>());

        // Prepare conversation context with iteration management instructions
        let enhanced_system_prompt = format!(
            "{}\n\n\
            ═══════════════════════════════════════════════════════════════\n\
            📊 ITERATION MANAGEMENT (CRITICAL)\n\
            ═══════════════════════════════════════════════════════════════\n\
            You have a DEFAULT LIMIT of 50 iterations (tool call rounds).\n\n\
            RULES:\n\
            1. Use tools efficiently - each tool call consumes one iteration\n\
            2. After 20-30 tool calls, you should be ready to provide your answer\n\
            3. When you receive a warning about remaining iterations, you MUST:\n\
               - STOP calling tools immediately\n\
               - Provide your final answer based on information gathered\n\
            4. If you genuinely need more iterations for complex tasks:\n\
               - Use 'request_more_iterations' tool (if available)\n\
               - Provide strong justification and progress summary\n\
            5. Running out of iterations WITHOUT answering = FAILURE\n\n\
            REMEMBER: Your goal is to ANSWER the user's question, not to \n\
            explore endlessly. Be efficient and decisive.\n\
            ═══════════════════════════════════════════════════════════════",
            self.config.system_prompt
        );

        eprintln!("[DEBUG] Agent '{}' system prompt first 200 chars: {}...",
                 self.config.name,
                 &self.config.system_prompt.chars().take(200).collect::<String>());

        let mut messages = vec![
            crate::agents::agent::ChatMessage {
                role: "system".to_string(),
                content: enhanced_system_prompt,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }
        ];

        // Add recent conversation history
        messages.extend(context.conversation_history.iter().cloned());

        // Add task description
        messages.push(crate::agents::agent::ChatMessage {
            role: "user".to_string(),
            content: format!("Task: {}\n\nPlease execute this task using your available tools.", task.description),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });

        // Execute with LLM and tool calling loop
        let mut max_iterations = 50;
        let mut iteration = 0;
        loop {
            if iteration >= max_iterations {
                break;
            }

            iteration += 1; // Increment at the start of each iteration
            println!("{} Iteration {}/{}", "🔄".cyan(), iteration, max_iterations);

            // Warn the model when approaching iteration limit
            let mut current_messages = messages.clone();
            if iteration >= max_iterations - 3 {
                let remaining = max_iterations - iteration;
                let urgency = if remaining <= 2 { "🚨 CRITICAL" } else { "⚠️ WARNING" };
                current_messages.push(crate::agents::agent::ChatMessage {
                    role: "system".to_string(),
                    content: format!(
                        "{}: Only {} iteration(s) remain before maximum limit!\n\n\
                        YOU MUST STOP CALLING TOOLS NOW.\n\
                        Provide your final text response based on information already gathered.\n\
                        If you call another tool, you may run out of iterations before providing an answer.\n\n\
                        Summarize what you've found and answer the user's question NOW.",
                        urgency, remaining
                    ),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
                println!("{} Injected iteration limit warning to model", "⚠️".yellow());
            }

            match self.llm_client.chat(current_messages.clone(), available_tools.clone()).await {
                Ok(response) => {
                    // Check if LLM wants to call tools
                    if let Some(tool_calls) = &response.message.tool_calls {
                        println!("{} LLM requested {} tool call(s)", "🔧".yellow(), tool_calls.len());

                        // Add assistant message with tool calls
                        messages.push(response.message.clone());

                        // Execute each tool call
                        for tool_call in tool_calls {
                            let tool_name = &tool_call.function.name;
                            let tool_args = &tool_call.function.arguments;

                            println!("  {} Calling tool: {} with args: {}", "▶️".blue(), tool_name,
                                if tool_args.len() > 100 { format!("{}...", &tool_args[..100]) } else { tool_args.clone() });

                            // Execute tool using registry
                            let tool_result = if let Some(tool) = self.tool_registry.get_tool(tool_name) {
                                // Parse arguments and execute
                                match crate::core::ToolParameters::from_json(tool_args) {
                                    Ok(params) => {
                                        let tool_context = crate::core::tool_context::ToolContext::new(
                                            context.workspace_dir.clone(),
                                            context.session_id.clone(),
                                            self.policy_manager.clone(),
                                        );
                                        tool.execute(params, &tool_context).await
                                    }
                                    Err(e) => {
                                        crate::core::tool::ToolResult::error(format!("Failed to parse tool arguments: {}", e))
                                    }
                                }
                            } else {
                                crate::core::tool::ToolResult::error(format!("Tool '{}' not found", tool_name))
                            };

                            let result_preview = if tool_result.success {
                                if tool_result.content.len() > 200 {
                                    format!("{}...", &tool_result.content[..200])
                                } else {
                                    tool_result.content.clone()
                                }
                            } else {
                                tool_result.error.clone().unwrap_or_else(|| "Unknown error".to_string())
                            };
                            println!("  {} Tool result: {}", if tool_result.success { "✅" } else { "❌" }, result_preview);

                            // Check if this is a request_more_iterations tool call
                            if tool_name == "request_more_iterations" && tool_result.success {
                                // Parse the approval from the result
                                if tool_result.content.contains("✅ APPROVED") {
                                    // Extract the granted iterations from the result
                                    if let Some(granted_str) = tool_result.content.split("APPROVED: ").nth(1) {
                                        if let Some(num_str) = granted_str.split(" additional").next() {
                                            if let Ok(additional) = num_str.trim().parse::<usize>() {
                                                max_iterations += additional;
                                                println!("{} Granted {} additional iterations. New limit: {}",
                                                    "🎁".green(), additional, max_iterations);
                                            }
                                        }
                                    }
                                }
                            }

                            // Add tool result to conversation
                            let tool_result_content = if tool_result.success {
                                tool_result.content
                            } else {
                                tool_result.error.unwrap_or_else(|| "Unknown error".to_string())
                            };

                            messages.push(crate::agents::agent::ChatMessage {
                                role: "tool".to_string(),
                                content: tool_result_content,
                                tool_calls: None,
                                tool_call_id: Some(tool_call.id.clone()),
                                name: Some(tool_name.clone()),
                            });
                        }

                        // Continue loop to get next LLM response
                        continue;
                    } else {
                        // No tool calls - return final response
                        println!("{} LLM returned final response (length: {})", "✅".green(), response.message.content.len());
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return crate::agents::agent::AgentResult::success(
                            response.message.content,
                            task.id.clone(),
                            self.name().to_string(),
                        )
                        .with_execution_time(execution_time);
                    }
                }
                Err(e) => {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return crate::agents::agent::AgentResult::error(
                        format!("LLM execution failed: {}", e),
                        task.id.clone(),
                        self.name().to_string(),
                    )
                    .with_execution_time(execution_time);
                }
            }
        }

        // Max iterations reached
        let execution_time = start_time.elapsed().as_millis() as u64;
        crate::agents::agent::AgentResult::error(
            format!("Max iterations ({}) reached without final response", max_iterations),
            task.id.clone(),
            self.name().to_string(),
        )
        .with_execution_time(execution_time)
    }
}

#[async_trait::async_trait]
impl Agent for ConfigurableAgent {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn description(&self) -> &str {
        &self.config.description
    }

    fn capabilities(&self) -> Vec<crate::agents::agent::Capability> {
        self.config.capabilities()
    }

    fn can_handle(&self, task: &crate::agents::agent::Task) -> bool {
        // Check if we have the required tools for this task
        // For now, use a simple heuristic based on task description
        let task_desc = task.description.to_lowercase();

        // Check specific capabilities based on task content
        if task_desc.contains("read") || task_desc.contains("write") || task_desc.contains("file") {
            return self.config.tools.iter().any(|t| t.contains("file"));
        }

        if task_desc.contains("search") || task_desc.contains("find") {
            return self.config.tools.iter().any(|t| t.contains("search"));
        }

        if task_desc.contains("command") || task_desc.contains("run") {
            return self.config.tools.iter().any(|t| t.contains("command"));
        }

        if task_desc.contains("code") || task_desc.contains("analyze") {
            return self.config.capabilities.contains(&"code_analysis".to_string());
        }

        true // Default to can handle
    }

    async fn execute(&self, task: crate::agents::agent::Task, context: &ExecutionContext) -> crate::agents::agent::AgentResult {
        self.execute_with_tools(&task, context).await
    }

    fn preferred_model(&self) -> &str {
        &self.config.model
    }

    fn system_prompt(&self) -> &str {
        &self.config.system_prompt
    }

    fn required_tools(&self) -> Vec<String> {
        self.config.tools.clone()
    }
}