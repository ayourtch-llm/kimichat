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
    tool_registry: Option<std::sync::Arc<kimichat_tools::core::tool_registry::ToolRegistry>>,
    llm_client: Option<std::sync::Arc<dyn crate::agent::LlmClient>>,
    conversation_history: Vec<crate::agent::ChatMessage>,
    terminal_manager: Option<std::sync::Arc<tokio::sync::Mutex<kimichat_terminal::TerminalManager>>>,
    skill_registry: Option<std::sync::Arc<kimichat_skills::SkillRegistry>>,
    //     todo_manager: Option<std::sync::Arc<// TODO: crate::todo::TodoManager>>,
    cancellation_token: Option<tokio_util::sync::CancellationToken>,
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
            skill_registry: None,
            //             todo_manager: None,
            cancellation_token: None,
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

    pub fn with_tool_registry(mut self, registry: std::sync::Arc<kimichat_tools::core::tool_registry::ToolRegistry>) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    pub fn with_llm_client(mut self, client: std::sync::Arc<dyn crate::agent::LlmClient>) -> Self {
        self.llm_client = Some(client);
        self
    }

    pub fn with_conversation_history(mut self, history: Vec<crate::agent::ChatMessage>) -> Self {
        self.conversation_history = history;
        self
    }

    pub fn with_terminal_manager(mut self, terminal_manager: std::sync::Arc<tokio::sync::Mutex<kimichat_terminal::TerminalManager>>) -> Self {
        self.terminal_manager = Some(terminal_manager);
        self
    }

    pub fn with_skill_registry(mut self, skill_registry: std::sync::Arc<kimichat_skills::SkillRegistry>) -> Self {
        self.skill_registry = Some(skill_registry);
        self
    }

    //     pub fn with_todo_manager(mut self, todo_manager: std::sync::Arc<// TODO: crate::todo::TodoManager>) -> Self {
    //         self.todo_manager = Some(todo_manager);
    //         self
    //     }
    // 
pub fn build(self) -> Result<crate::agent::ExecutionContext, String> {
        Ok(crate::agent::ExecutionContext {
            workspace_dir: self.workspace_dir.ok_or("workspace_dir is required")?,
            session_id: self.session_id.ok_or("session_id is required")?,
            tool_registry: self.tool_registry.ok_or("tool_registry is required")?,
            llm_client: self.llm_client.ok_or("llm_client is required")?,
            conversation_history: self.conversation_history,
            terminal_manager: self.terminal_manager,
            skill_registry: self.skill_registry,
            //             todo_manager: self.todo_manager,
            cancellation_token: self.cancellation_token,
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
    pub fn simple_task(id: String, description: String) -> crate::agent::Task {
        crate::agent::Task {
            id,
            description,
            task_type: crate::agent::TaskType::Simple,
            priority: crate::agent::TaskPriority::Medium,
            metadata: HashMap::new(),
        }
    }

    /// Create a high-priority task
    pub fn high_priority_task(id: String, description: String) -> crate::agent::Task {
        crate::agent::Task {
            id,
            description,
            task_type: crate::agent::TaskType::Simple,
            priority: crate::agent::TaskPriority::High,
            metadata: HashMap::new(),
        }
    }

    /// Create a complex task
    pub fn complex_task(id: String, description: String) -> crate::agent::Task {
        crate::agent::Task {
            id,
            description,
            task_type: crate::agent::TaskType::Complex,
            priority: crate::agent::TaskPriority::High,
            metadata: HashMap::new(),
        }
    }

    /// Create a sequential task set
    pub fn sequential_task(id: String, description: String, subtasks: Vec<crate::agent::Task>) -> crate::agent::Task {
        crate::agent::Task {
            id,
            description,
            task_type: crate::agent::TaskType::Sequential(subtasks),
            priority: crate::agent::TaskPriority::High,
            metadata: HashMap::new(),
        }
    }

    /// Create a parallel task set
    pub fn parallel_task(id: String, description: String, subtasks: Vec<crate::agent::Task>) -> crate::agent::Task {
        crate::agent::Task {
            id,
            description,
            task_type: crate::agent::TaskType::Parallel(subtasks),
            priority: crate::agent::TaskPriority::High,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to a task
    pub fn with_metadata(mut task: crate::agent::Task, key: String, value: String) -> crate::agent::Task {
        task.metadata.insert(key, value);
        task
    }
}