use crate::agents::agent::{Agent, ExecutionContext, LlmClient};
use crate::agents::agent_config::AgentConfig;
use crate::core::tool_registry::ToolRegistry;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

/// Factory for creating agents from configuration
pub struct AgentFactory {
    tool_registry: Arc<ToolRegistry>,
    llm_clients: HashMap<String, Arc<dyn LlmClient>>,
}

impl AgentFactory {
    pub fn new(tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            tool_registry,
            llm_clients: HashMap::new(),
        }
    }

    pub fn register_llm_client(&mut self, model: String, client: Arc<dyn LlmClient>) {
        self.llm_clients.insert(model, client);
    }

    pub fn create_agent(&self, config: &AgentConfig) -> Result<Box<dyn Agent>> {
        // Validate configuration
        config.validate()
            .map_err(|e| anyhow::anyhow!("Invalid agent config: {}", e))?;

        // Get LLM client for this agent
        let llm_client = self.llm_clients.get(&config.model)
            .ok_or_else(|| anyhow::anyhow!("No LLM client available for model: {}", config.model))?
            .clone();

        // Create configurable agent
        let agent = ConfigurableAgent::new(
            config.clone(),
            Arc::clone(&self.tool_registry),
            llm_client,
        )?;

        Ok(Box::new(agent))
    }
}

/// Configurable agent implementation
pub struct ConfigurableAgent {
    config: AgentConfig,
    tool_registry: Arc<ToolRegistry>,
    llm_client: Arc<dyn LlmClient>,
}

impl ConfigurableAgent {
    pub fn new(
        config: AgentConfig,
        tool_registry: Arc<ToolRegistry>,
        llm_client: Arc<dyn LlmClient>,
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
        })
    }

    async fn execute_with_tools(
        &self,
        task: &crate::agents::agent::Task,
        context: &ExecutionContext,
    ) -> crate::agents::agent::AgentResult {
        let start_time = std::time::Instant::now();

        // Prepare tools for this agent
        let available_tools: Vec<_> = self.config.tools
            .iter()
            .filter_map(|tool_name| self.tool_registry.get_tool(tool_name))
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

        // Prepare conversation context
        let mut messages = vec![
            crate::agents::agent::ChatMessage {
                role: "system".to_string(),
                content: self.config.system_prompt.clone(),
                tool_calls: None,
            }
        ];

        // Add recent conversation history
        messages.extend(context.conversation_history.iter().cloned());

        // Add task description
        messages.push(crate::agents::agent::ChatMessage {
            role: "user".to_string(),
            content: format!("Task: {}\n\nPlease execute this task using your available tools.", task.description),
            tool_calls: None,
        });

        // Execute with LLM and tool calling
        match self.llm_client.chat(messages, available_tools).await {
            Ok(response) => {
                let execution_time = start_time.elapsed().as_millis() as u64;

                crate::agents::agent::AgentResult::success(
                    response.message.content,
                    task.id.clone(),
                    self.name().to_string(),
                )
                .with_execution_time(execution_time)
            }
            Err(e) => {
                let execution_time = start_time.elapsed().as_millis() as u64;

                crate::agents::agent::AgentResult::error(
                    format!("LLM execution failed: {}", e),
                    task.id.clone(),
                    self.name().to_string(),
                )
                .with_execution_time(execution_time)
            }
        }
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