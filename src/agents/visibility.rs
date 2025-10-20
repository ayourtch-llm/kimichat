use colored::Colorize;
use std::time::{Duration, Instant};
use std::collections::HashMap;

/// Enhanced visibility system for agent operations
#[derive(Debug, Clone)]
pub struct VisibilityManager {
    /// Current execution session
    session_id: String,
    /// Active agent tracking
    active_agents: HashMap<String, AgentVisibilityInfo>,
    /// Task execution tracking
    task_history: Vec<TaskVisibilityEvent>,
    /// Performance metrics
    performance_metrics: PerformanceMetrics,
    /// Current execution phase
    current_phase: ExecutionPhase,
    /// User preferences for verbosity
    verbosity_level: VerbosityLevel,
}

#[derive(Debug, Clone)]
pub struct AgentVisibilityInfo {
    pub name: String,
    pub capabilities: Vec<String>,
    pub start_time: Instant,
    pub current_task: Option<String>,
    pub status: AgentStatus,
    pub confidence_score: f32,
    pub estimated_completion: Option<Instant>,
    pub progress_percentage: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Idle,
    Analyzing,
    Executing,
    Waiting,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct TaskVisibilityEvent {
    pub task_id: String,
    pub agent_name: String,
    pub task_description: String,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub status: TaskStatus,
    pub result_summary: Option<String>,
    pub execution_metrics: ExecutionMetrics,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Started,
    InProgress(f32), // Progress percentage
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct ExecutionMetrics {
    pub execution_time: Duration,
    pub tokens_used: Option<usize>,
    pub tool_calls: usize,
    pub api_calls: usize,
    pub memory_usage: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub total_tasks: usize,
    pub successful_tasks: usize,
    pub failed_tasks: usize,
    pub average_execution_time: Duration,
    pub total_tokens_used: usize,
    pub agent_performance: HashMap<String, AgentPerformance>,
}

#[derive(Debug, Clone)]
pub struct AgentPerformance {
    pub tasks_completed: usize,
    pub success_rate: f32,
    pub average_time: Duration,
    pub last_used: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionPhase {
    Planning,
    AgentSelection,
    TaskExecution,
    ResultAggregation,
    Completed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VerbosityLevel {
    Minimal,
    Normal,
    Detailed,
    Debug,
}

impl VisibilityManager {
    /// Create a new visibility manager
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            active_agents: HashMap::new(),
            task_history: Vec::new(),
            performance_metrics: PerformanceMetrics {
                total_tasks: 0,
                successful_tasks: 0,
                failed_tasks: 0,
                average_execution_time: Duration::from_secs(0),
                total_tokens_used: 0,
                agent_performance: HashMap::new(),
            },
            current_phase: ExecutionPhase::Planning,
            verbosity_level: VerbosityLevel::Normal,
        }
    }
    
    /// Set verbosity level
    pub fn set_verbosity_level(&mut self, level: VerbosityLevel) {
        self.verbosity_level = level;
    }
    
    /// Get current execution phase
    pub fn get_current_phase(&self) -> &ExecutionPhase {
        &self.current_phase
    }
    
    /// Set current execution phase
    pub fn set_phase(&mut self, phase: ExecutionPhase) {
        self.current_phase = phase.clone();
        
        if self.verbosity_level != VerbosityLevel::Minimal {
            self.display_phase_change(&phase);
        }
    }
    
    /// Display phase change with appropriate styling
    fn display_phase_change(&self, phase: &ExecutionPhase) {
        let (icon, title, description) = match phase {
            ExecutionPhase::Planning => ("📋", "Planning Phase", "Analyzing request and creating execution plan"),
            ExecutionPhase::AgentSelection => ("🔍", "Agent Selection", "Evaluating and selecting optimal agents"),
            ExecutionPhase::TaskExecution => ("⚡", "Task Execution", "Executing tasks with selected agents"),
            ExecutionPhase::ResultAggregation => ("🧩", "Result Aggregation", "Combining results from all agents"),
            ExecutionPhase::Completed => ("✅", "Completed", "Request processing finished successfully"),
        };
        
        println!();
        println!("{}", format!("{} {}", icon, title).bright_cyan().bold());
        println!("{} {}", "  ↳".bright_cyan(), description.cyan());
        println!();
    }
    
    /// Register a task with the visibility manager
    pub fn register_task(&mut self, 
        _task_id: String, 
        task_description: String,
    ) {
        // This is a simplified version - in a full implementation,
        // we would track task state and integrate with the task queue
        if self.verbosity_level == VerbosityLevel::Debug {
            println!("{} {}", "📋 Registered task:".bright_blue(), task_description.white());
        }
    }
    
    /// Update task progress
    pub fn update_task_progress(&mut self,
        task_id: &str,
        progress_percentage: f32,
        status: &str,
    ) {
        if self.verbosity_level == VerbosityLevel::Detailed || self.verbosity_level == VerbosityLevel::Debug {
            let progress_bar = self.create_progress_bar(progress_percentage);
            println!("{} {} [{}] {}", 
                "🔄".bright_yellow(),
                task_id.bright_white(),
                progress_bar,
                status.bright_cyan()
            );
        }
    }
    
    /// Create a simple progress bar
    fn create_progress_bar(&self, percentage: f32) -> String {
        let filled_blocks = ((percentage / 100.0) * 20.0) as usize;
        let empty_blocks = 20 - filled_blocks;
        
        format!("[{}{}] {:.0}%", 
            "█".repeat(filled_blocks).bright_green(),
            "░".repeat(empty_blocks).bright_black(),
            percentage
        )
    }
    
    /// Get task queue status
    pub fn get_task_queue_status(&self) -> (usize, usize, usize) {
        // In a full implementation, this would return actual counts
        // For now, return dummy data
        (2, 1, 3) // pending, active, completed
    }
    
    /// Get current active agent
    pub fn get_current_agent(&self) -> Option<String> {
        self.active_agents
            .values()
            .find(|agent| matches!(agent.status, AgentStatus::Executing))
            .map(|agent| agent.name.clone())
    }
    
    /// Get performance summary
    pub fn get_performance_summary(&self) -> (f32, usize, usize) {
        // Return average task time, total tokens, success rate
        let avg_time = self.performance_metrics.average_execution_time.as_secs_f32();
        let tokens = self.performance_metrics.total_tokens_used;
        let success_rate = if self.performance_metrics.total_tasks > 0 {
            self.performance_metrics.successful_tasks as f32 / self.performance_metrics.total_tasks as f32
        } else {
            0.0
        };
        
        (avg_time, tokens, (success_rate * 100.0) as usize)
    }
    
    /// Register an agent for visibility tracking
    pub fn register_agent(&mut self, name: String, capabilities: Vec<String>) {
        let info = AgentVisibilityInfo {
            name: name.clone(),
            capabilities,
            start_time: Instant::now(),
            current_task: None,
            status: AgentStatus::Idle,
            confidence_score: 0.0,
            estimated_completion: None,
            progress_percentage: 0.0,
        };
        
        self.active_agents.insert(name, info);
    }
    
    /// Update agent status
    pub fn update_agent_status(&mut self, name: &str, status: AgentStatus, task_description: Option<String>) {
        // Get agent info first to avoid borrow conflicts
        let should_display = self.verbosity_level == VerbosityLevel::Detailed || self.verbosity_level == VerbosityLevel::Debug;
        
        if let Some(agent) = self.active_agents.get_mut(name) {
            agent.status = status;
            agent.current_task = task_description;
            
            if should_display {
                // Create a copy to avoid borrowing issues
                let agent_copy = AgentVisibilityInfo {
                    name: agent.name.clone(),
                    capabilities: agent.capabilities.clone(),
                    start_time: agent.start_time,
                    current_task: agent.current_task.clone(),
                    status: agent.status.clone(),
                    confidence_score: agent.confidence_score,
                    estimated_completion: agent.estimated_completion,
                    progress_percentage: agent.progress_percentage,
                };
                self.display_agent_update(&agent_copy);
            }
        }
    }
    
    /// Display agent status update
    fn display_agent_update(&self, agent: &AgentVisibilityInfo) {
        let status_icon = match agent.status {
            AgentStatus::Idle => "⏸️",
            AgentStatus::Analyzing => "🧠",
            AgentStatus::Executing => "⚡",
            AgentStatus::Waiting => "⏳",
            AgentStatus::Completed => "✅",
            AgentStatus::Failed(_) => "❌",
        };
        
        let status_color = match agent.status {
            AgentStatus::Completed => "✅".green(),
            AgentStatus::Failed(_) => "❌".red(),
            _ => format!("{}", status_icon).yellow(),
        };
        
        print!("{} {} {}", status_color, agent.name.bright_white(), format!("({:.0}%)", agent.progress_percentage).bright_blue());
        
        if let Some(task) = &agent.current_task {
            print!(" - {}", task.cyan());
        }
        println!();
    }
}

impl Default for VisibilityManager {
    fn default() -> Self {
        Self::new("default".to_string())
    }
}