use kimichat_toolcore::tool_registry::ToolRegistry;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use async_trait::async_trait;
use futures::Stream;

// Re-export types from kimichat-llm-api
pub use kimichat_llm_api::{
    LlmClient, ChatMessage, ToolCall, FunctionCall, LlmResponse,
    TokenUsage, ToolDefinition, StreamingChunk,
};

/// Agent capabilities
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Capability {
    CodeAnalysis,
    FileOperations,
    Search,
    SystemOperations,
    ModelManagement,
    ArchitectureDesign,
    CodeReview,
    Refactoring,
    Testing,
    GitOperations,
    SecurityAnalysis,
    PerformanceAnalysis,
}

impl Capability {
    pub fn from_string(s: &str) -> Self {
        match s {
            "code_analysis" => Capability::CodeAnalysis,
            "file_operations" => Capability::FileOperations,
            "search" => Capability::Search,
            "system_operations" => Capability::SystemOperations,
            "model_management" => Capability::ModelManagement,
            "architecture_design" => Capability::ArchitectureDesign,
            "code_review" => Capability::CodeReview,
            "refactoring" => Capability::Refactoring,
            "testing" => Capability::Testing,
            "git_operations" => Capability::GitOperations,
            "security_analysis" => Capability::SecurityAnalysis,
            "performance_analysis" => Capability::PerformanceAnalysis,
            _ => Capability::CodeAnalysis, // Default fallback
        }
    }
}

/// Task definition for agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub task_type: TaskType,
    pub priority: TaskPriority,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    Simple,
    Complex,
    Parallel(Vec<Task>),
    Sequential(Vec<Task>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

/// Agent execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub success: bool,
    pub content: String,
    pub task_id: String,
    pub agent_name: String,
    pub execution_time: u64, // milliseconds
    pub metadata: HashMap<String, String>,
    pub next_tasks: Option<Vec<Task>>,
}

impl AgentResult {
    pub fn success(content: String, task_id: String, agent_name: String) -> Self {
        Self {
            success: true,
            content,
            task_id,
            agent_name,
            execution_time: 0,
            metadata: HashMap::new(),
            next_tasks: None,
        }
    }

    pub fn error(content: String, task_id: String, agent_name: String) -> Self {
        Self {
            success: false,
            content,
            task_id,
            agent_name,
            execution_time: 0,
            metadata: HashMap::new(),
            next_tasks: None,
        }
    }

    pub fn with_execution_time(mut self, time: u64) -> Self {
        self.execution_time = time;
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn with_next_tasks(mut self, tasks: Vec<Task>) -> Self {
        self.next_tasks = Some(tasks);
        self
    }
}

/// Execution context for agents
#[derive(Clone)]
pub struct ExecutionContext {
    pub workspace_dir: std::path::PathBuf,
    pub session_id: String,
    pub tool_registry: std::sync::Arc<ToolRegistry>,
    pub llm_client: std::sync::Arc<dyn LlmClient>,
    pub conversation_history: Vec<ChatMessage>,
    pub terminal_manager: Option<std::sync::Arc<tokio::sync::Mutex<crate::terminal::TerminalManager>>>,
    pub skill_registry: Option<std::sync::Arc<kimichat_skills::SkillRegistry>>,
    pub todo_manager: Option<std::sync::Arc<kimichat_todo::TodoManager>>,
    pub cancellation_token: Option<tokio_util::sync::CancellationToken>,
}


/// Core agent trait
#[async_trait]
pub trait Agent: Send + Sync {
    /// Agent name (must be unique)
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// Agent capabilities
    fn capabilities(&self) -> Vec<Capability>;

    /// Can this agent handle the given task?
    fn can_handle(&self, task: &Task) -> bool {
        // Default implementation - check if task complexity matches capabilities
        match task.task_type {
            TaskType::Simple => true, // Most agents can handle simple tasks
            TaskType::Complex => self.capabilities().len() > 2, // Complex tasks need multiple capabilities
            TaskType::Parallel(_) | TaskType::Sequential(_) => {
                // Multi-step tasks need sophisticated agents
                self.capabilities().contains(&Capability::ArchitectureDesign) ||
                self.capabilities().contains(&Capability::CodeReview)
            }
        }
    }

    /// Execute a task
    async fn execute(&self, task: Task, context: &ExecutionContext) -> AgentResult;

    /// Get the preferred model for this agent
    // Default to GPT-OSS for cost efficiency - significantly cheaper than Kimi
    fn preferred_model(&self) -> &str { "gpt_oss" }

    /// Get the system prompt for this agent
    fn system_prompt(&self) -> &str;

    /// Get the tools this agent needs
    fn required_tools(&self) -> Vec<String>;
}