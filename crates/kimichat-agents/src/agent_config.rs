use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::agent::Capability;

/// Agent configuration loaded from JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub description: String,
    pub version: String,
    pub model: String,
    pub tools: Vec<String>,
    pub capabilities: Vec<String>,
    pub system_prompt: String,
    pub permissions: AgentPermissions,
    pub task_handlers: HashMap<String, String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPermissions {
    pub file_access: FileAccessLevel,
    pub command_execution: Vec<String>,
    pub network_access: bool,
    pub system_modification: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileAccessLevel {
    None,
    ReadOnly,
    ReadWrite,
    Unrestricted,
}

impl AgentConfig {
    pub fn capabilities(&self) -> Vec<Capability> {
        self.capabilities
            .iter()
            .map(|cap| Capability::from_string(cap))
            .collect()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Agent name cannot be empty".to_string());
        }

        if self.description.is_empty() {
            return Err("Agent description cannot be empty".to_string());
        }

        if self.system_prompt.is_empty() {
            return Err("System prompt cannot be empty".to_string());
        }

        // Allow planner agent to have no tools (it only analyzes and plans)
        if self.tools.is_empty() && self.name != "planner" {
            return Err("Agent must have at least one tool".to_string());
        }

        // Validate model name
        if !["kimi", "gpt_oss", "blu_model", "grn_model", "anthropic"].contains(&self.model.as_str()) {
            return Err(format!("Invalid model: {}. Available models: kimi, gpt_oss, blu_model, grn_model, anthropic", self.model));
        }

        Ok(())
    }

    pub fn can_execute_command(&self, command: &str) -> bool {
        if self.permissions.command_execution.is_empty() {
            return false;
        }

        // Check if command matches any allowed pattern
        for pattern in &self.permissions.command_execution {
            if pattern == "*" {
                return true;
            }
            if command.starts_with(pattern) {
                return true;
            }
        }

        false
    }
}

/// Task decomposition patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPattern {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub subtasks: Vec<SubTaskDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTaskDefinition {
    pub name: String,
    pub description: String,
    pub agent_type: String,
    pub required_tools: Vec<String>,
    pub dependencies: Vec<String>,
}

/// Workflow configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    pub name: String,
    pub description: String,
    pub phases: Vec<WorkflowPhase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPhase {
    pub name: String,
    pub description: String,
    pub agent_type: String,
    pub required_tools: Vec<String>,
    pub validation: Option<ValidationRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    pub rule_type: String,
    pub parameters: HashMap<String, String>,
}