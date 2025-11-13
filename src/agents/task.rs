use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Task management utilities and helpers

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTemplate {
    pub name: String,
    pub description_template: String,
    pub required_capabilities: Vec<String>,
    pub suggested_tools: Vec<String>,
    pub task_type: TaskTemplateType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskTemplateType {
    SingleStep,
    MultiStep,
    Parallel,
    Sequential,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDependency {
    pub task_id: String,
    pub dependency_type: DependencyType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencyType {
    /// Must complete before this task can start
    Sequential,
    /// Must complete successfully before this task can start
    SuccessDependent,
    /// Can run in parallel
    Independent,
}

/// Task execution context builder
pub struct TaskContextBuilder {
    workspace_dir: Option<std::path::PathBuf>,
    session_id: Option<String>,
    tool_registry: Option<std::sync::Arc<crate::core::tool_registry::ToolRegistry>>,
    llm_client: Option<std::sync::Arc<dyn crate::agents::agent::LlmClient>>,
    conversation_history: Vec<crate::agents::agent::ChatMessage>,
    terminal_manager: Option<std::sync::Arc<std::sync::Mutex<crate::terminal::TerminalManager>>>,
}

impl TaskContextBuilder {
    pub fn new() -> Self {
        Self {
            workspace_dir: None,
            session_id: None,
            tool_registry: None,
            llm_client: None,
            conversation_history: Vec::new(),
            terminal_manager: None,
        }
    }

    pub fn with_workspace_dir(mut self, dir: std::path::PathBuf) -> Self {
        self.workspace_dir = Some(dir);
        self
    }

    pub fn with_session_id(mut self, id: String) -> Self {
        self.session_id = Some(id);
        self
    }

    pub fn with_tool_registry(mut self, registry: std::sync::Arc<crate::core::tool_registry::ToolRegistry>) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    pub fn with_llm_client(mut self, client: std::sync::Arc<dyn crate::agents::agent::LlmClient>) -> Self {
        self.llm_client = Some(client);
        self
    }

    pub fn with_conversation_history(mut self, history: Vec<crate::agents::agent::ChatMessage>) -> Self {
        self.conversation_history = history;
        self
    }

    pub fn with_terminal_manager(mut self, terminal_manager: std::sync::Arc<std::sync::Mutex<crate::terminal::TerminalManager>>) -> Self {
        self.terminal_manager = Some(terminal_manager);
        self
    }

    pub fn build(self) -> Result<crate::agents::agent::ExecutionContext, String> {
        Ok(crate::agents::agent::ExecutionContext {
            workspace_dir: self.workspace_dir.ok_or("workspace_dir is required")?,
            session_id: self.session_id.ok_or("session_id is required")?,
            tool_registry: self.tool_registry.ok_or("tool_registry is required")?,
            llm_client: self.llm_client.ok_or("llm_client is required")?,
            conversation_history: self.conversation_history,
            terminal_manager: self.terminal_manager,
        })
    }
}

impl Default for TaskContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Task utility functions
pub struct TaskUtils;

impl TaskUtils {
    /// Create a simple task
    pub fn simple_task(id: String, description: String) -> crate::agents::agent::Task {
        crate::agents::agent::Task {
            id,
            description,
            task_type: crate::agents::agent::TaskType::Simple,
            priority: crate::agents::agent::TaskPriority::Medium,
            metadata: HashMap::new(),
        }
    }

    /// Create a high-priority task
    pub fn high_priority_task(id: String, description: String) -> crate::agents::agent::Task {
        crate::agents::agent::Task {
            id,
            description,
            task_type: crate::agents::agent::TaskType::Simple,
            priority: crate::agents::agent::TaskPriority::High,
            metadata: HashMap::new(),
        }
    }

    /// Create a complex task
    pub fn complex_task(id: String, description: String) -> crate::agents::agent::Task {
        crate::agents::agent::Task {
            id,
            description,
            task_type: crate::agents::agent::TaskType::Complex,
            priority: crate::agents::agent::TaskPriority::High,
            metadata: HashMap::new(),
        }
    }

    /// Create a sequential task set
    pub fn sequential_task(id: String, description: String, subtasks: Vec<crate::agents::agent::Task>) -> crate::agents::agent::Task {
        crate::agents::agent::Task {
            id,
            description,
            task_type: crate::agents::agent::TaskType::Sequential(subtasks),
            priority: crate::agents::agent::TaskPriority::High,
            metadata: HashMap::new(),
        }
    }

    /// Create a parallel task set
    pub fn parallel_task(id: String, description: String, subtasks: Vec<crate::agents::agent::Task>) -> crate::agents::agent::Task {
        crate::agents::agent::Task {
            id,
            description,
            task_type: crate::agents::agent::TaskType::Parallel(subtasks),
            priority: crate::agents::agent::TaskPriority::High,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to a task
    pub fn with_metadata(mut task: crate::agents::agent::Task, key: String, value: String) -> crate::agents::agent::Task {
        task.metadata.insert(key, value);
        task
    }
}