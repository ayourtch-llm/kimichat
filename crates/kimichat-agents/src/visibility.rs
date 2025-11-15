use colored::Colorize;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    pub parent_task_id: Option<String>,
    pub depth: usize,
    pub subtask_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        if let Some(agent) = self.active_agents.get_mut(name) {
            agent.status = status;
            agent.current_task = task_description;

            // Clone agent for display to avoid borrow checker issues
            if self.verbosity_level == VerbosityLevel::Detailed || self.verbosity_level == VerbosityLevel::Debug {
                let agent_clone = agent.clone();
                self.display_agent_update(&agent_clone);
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
        
        print!("{} {} {}", status_color, agent.name.bright_white(), format!("({}%)", agent.progress_percentage).bright_blue());
        
        if let Some(task) = &agent.current_task {
            print!(" - {}", task.cyan());
        }
        println!();
    }
    
    /// Record task start
    pub fn record_task_start(&mut self, task_id: String, agent_name: String, task_description: String) {
        self.record_task_start_with_parent(task_id, agent_name, task_description, None, 0);
    }

    /// Record task start with parent tracking
    pub fn record_task_start_with_parent(
        &mut self,
        task_id: String,
        agent_name: String,
        task_description: String,
        parent_task_id: Option<String>,
        depth: usize,
    ) {
        let event = TaskVisibilityEvent {
            task_id: task_id.clone(),
            agent_name: agent_name.clone(),
            task_description: task_description.clone(),
            start_time: Instant::now(),
            end_time: None,
            status: TaskStatus::Started,
            result_summary: None,
            execution_metrics: ExecutionMetrics {
                execution_time: Duration::from_secs(0),
                tokens_used: None,
                tool_calls: 0,
                api_calls: 0,
                memory_usage: None,
            },
            parent_task_id: parent_task_id.clone(),
            depth,
            subtask_count: 0,
        };

        self.task_history.push(event);

        // Update agent status
        self.update_agent_status(
            &agent_name,
            AgentStatus::Executing,
            Some(task_description.clone())
        );

        // Display with hierarchy
        if self.verbosity_level != VerbosityLevel::Minimal {
            self.display_task_start(&task_id, &agent_name, &task_description, depth);
        }
    }
    
    /// Update task progress
    pub fn update_task_progress(&mut self, task_id: &str, progress: f32, message: Option<String>) {
        if let Some(task) = self.task_history.iter_mut().find(|t| t.task_id == task_id) {
            task.status = TaskStatus::InProgress(progress);
            
            if self.verbosity_level == VerbosityLevel::Detailed || self.verbosity_level == VerbosityLevel::Debug {
                if let Some(msg) = message {
                    println!("{} {} {}", 
                        format!("[{}%]", (progress * 100.0) as u32).bright_blue(),
                        task.agent_name.bright_white(),
                        msg.cyan()
                    );
                }
            }
        }
    }
    
    /// Record task completion
    pub fn record_task_completion(&mut self, task_id: &str, result_summary: Option<String>, success: bool) {
        // First, update the task
        let agent_name = if let Some(task) = self.task_history.iter_mut().find(|t| t.task_id == task_id) {
            task.end_time = Some(Instant::now());
            task.status = if success {
                TaskStatus::Completed
            } else {
                TaskStatus::Failed("Task execution failed".to_string())
            };
            task.result_summary = result_summary;
            Some(task.agent_name.clone())
        } else {
            None
        };

        // Then update metrics and agent status separately to avoid borrow checker issues
        if let Some(ref name) = agent_name {
            // Update performance metrics - clone task to avoid borrow checker issues
            if let Some(task) = self.task_history.iter().find(|t| t.task_id == task_id) {
                let task_clone = task.clone();
                self.update_performance_metrics_from_task(&task_clone, success);
            }

            // Update agent status
            self.update_agent_status(
                name,
                if success { AgentStatus::Completed } else { AgentStatus::Failed("Task failed".to_string()) },
                None
            );
        }
    }
    
    /// Update performance metrics from task
    fn update_performance_metrics_from_task(&mut self, task: &TaskVisibilityEvent, success: bool) {
        self.performance_metrics.total_tasks += 1;
        if success {
            self.performance_metrics.successful_tasks += 1;
        } else {
            self.performance_metrics.failed_tasks += 1;
        }

        // Update agent-specific performance
        let agent_name = task.agent_name.clone();
        let performance = self.performance_metrics.agent_performance
            .entry(agent_name.clone())
            .or_insert_with(|| AgentPerformance {
                tasks_completed: 0,
                success_rate: 0.0,
                average_time: Duration::from_secs(0),
                last_used: None,
            });

        performance.tasks_completed += 1;
        performance.success_rate = self.performance_metrics.successful_tasks as f32 / self.performance_metrics.total_tasks as f32;
        performance.last_used = Some(Instant::now());
    }
    
    /// Display current status summary
    pub fn display_status_summary(&self) {
        println!("{}", "═".repeat(60).bright_blue());
        println!("{}", "📊 SYSTEM STATUS SUMMARY".bright_blue().bold());
        println!("{}", "═".repeat(60).bright_blue());
        
        println!("{} {:?}", "Current Phase:".bright_cyan(), self.current_phase);
        println!("{} {}", "Active Agents:".bright_cyan(), self.active_agents.len().to_string().bright_yellow());
        println!("{} {}", "Total Tasks:".bright_cyan(), self.performance_metrics.total_tasks.to_string().bright_yellow());
        println!("{} {:.1}%", "Success Rate:".bright_cyan(), 
            (self.performance_metrics.successful_tasks as f32 / self.performance_metrics.total_tasks.max(1) as f32 * 100.0)
        );
        
        if self.verbosity_level == VerbosityLevel::Detailed || self.verbosity_level == VerbosityLevel::Debug {
            println!();
            println!("{}", "Active Agents:".bright_cyan().bold());
            for (_, agent) in &self.active_agents {
                self.display_agent_update(agent);
            }
        }
        
        println!("{}", "═".repeat(60).bright_blue());
        println!();
    }
    
    /// Get performance summary
    pub fn get_performance_summary(&self) -> String {
        format!(
            "Total: {}, Success: {} ({}%), Failed: {}",
            self.performance_metrics.total_tasks,
            self.performance_metrics.successful_tasks,
            (self.performance_metrics.successful_tasks as f32 / self.performance_metrics.total_tasks.max(1) as f32 * 100.0) as u32,
            self.performance_metrics.failed_tasks
        )
    }
    
    /// Clear old history (keep last N tasks)
    pub fn cleanup_history(&mut self, keep_last: usize) {
        if self.task_history.len() > keep_last {
            let start_index = self.task_history.len() - keep_last;
            self.task_history = self.task_history.split_off(start_index);
        }
    }

    /// Display task start with hierarchy indentation
    fn display_task_start(&self, _task_id: &str, agent_name: &str, task_description: &str, depth: usize) {
        let prefix = if depth == 0 {
            "▶️".to_string()
        } else {
            format!("{}└─▶", "  ".repeat(depth.saturating_sub(1)))
        };

        println!(
            "{} {} {} {} {}",
            prefix.green(),
            format!("[L{}]", depth).bright_black(),
            agent_name.bright_white(),
            "→".bright_black(),
            task_description.cyan()
        );
    }

    /// Display task hierarchy as a tree
    pub fn display_task_hierarchy(&self) {
        if self.task_history.is_empty() {
            return;
        }

        println!();
        println!("{}", "═".repeat(80).bright_blue());
        println!("{}", "📊 TASK EXECUTION HIERARCHY".bright_blue().bold());
        println!("{}", "═".repeat(80).bright_blue());
        println!();

        // Build parent-child mapping
        let mut children: HashMap<String, Vec<&TaskVisibilityEvent>> = HashMap::new();
        let mut roots = Vec::new();

        for task in &self.task_history {
            if let Some(parent_id) = &task.parent_task_id {
                children.entry(parent_id.clone()).or_insert_with(Vec::new).push(task);
            } else {
                roots.push(task);
            }
        }

        // Display tree recursively
        for root in roots {
            self.display_task_node(root, &children, 0, true);
        }

        println!();
        println!("{}", "═".repeat(80).bright_blue());
        println!();
    }

    /// Display a single task node in the tree
    fn display_task_node(
        &self,
        task: &TaskVisibilityEvent,
        children: &HashMap<String, Vec<&TaskVisibilityEvent>>,
        depth: usize,
        is_last: bool,
    ) {
        // Build tree connectors
        let connector = if depth == 0 {
            "".to_string()
        } else if is_last {
            "└── ".to_string()
        } else {
            "├── ".to_string()
        };

        let indent = "    ".repeat(depth.saturating_sub(1));

        // Status icon
        let status_icon = match &task.status {
            TaskStatus::Started => "⏳",
            TaskStatus::InProgress(_) => "⚙️",
            TaskStatus::Completed => "✅",
            TaskStatus::Failed(_) => "❌",
            TaskStatus::Cancelled => "🚫",
        };

        // Duration
        let duration = if let Some(end_time) = task.end_time {
            format!("{:.2}s", end_time.duration_since(task.start_time).as_secs_f64())
        } else {
            "running".to_string()
        };

        println!(
            "{}{}{} {} {} {} {}",
            indent,
            connector,
            status_icon,
            task.agent_name.bright_white(),
            format!("[{}]", duration).bright_black(),
            "→".bright_black(),
            task.task_description.cyan()
        );

        // Display children
        if let Some(child_tasks) = children.get(&task.task_id) {
            let child_count = child_tasks.len();
            for (i, child) in child_tasks.iter().enumerate() {
                self.display_task_node(child, children, depth + 1, i == child_count - 1);
            }
        }
    }

    /// Display active task stack (breadcrumb trail)
    pub fn display_task_stack(&self) {
        let active_tasks: Vec<&TaskVisibilityEvent> = self
            .task_history
            .iter()
            .filter(|t| matches!(t.status, TaskStatus::Started | TaskStatus::InProgress(_)))
            .collect();

        if active_tasks.is_empty() {
            return;
        }

        println!();
        println!("{}", "📍 ACTIVE TASK STACK".bright_yellow().bold());
        println!("{}", "─".repeat(80).bright_yellow());

        for task in &active_tasks {
            let depth_indicator = "  ".repeat(task.depth);
            let progress = match &task.status {
                TaskStatus::InProgress(p) => format!(" {}%", (p * 100.0) as u32),
                _ => String::new(),
            };

            println!(
                "{}{} {} {} {}{}",
                depth_indicator,
                format!("[L{}]", task.depth).bright_black(),
                task.agent_name.bright_white(),
                "→".bright_black(),
                task.task_description.cyan(),
                progress.bright_yellow()
            );
        }

        println!("{}", "─".repeat(80).bright_yellow());
        println!();
    }

    /// Display task queue status
    pub fn display_queue_status(&self, queue_size: usize, active_count: usize) {
        if queue_size == 0 && active_count == 0 {
            return;
        }

        println!(
            "{} Queue: {} pending | {} active",
            "📋".bright_cyan(),
            queue_size.to_string().bright_yellow(),
            active_count.to_string().bright_green()
        );
    }

    /// Get task history for a specific agent
    pub fn get_agent_tasks(&self, agent_name: &str) -> Vec<&TaskVisibilityEvent> {
        self.task_history
            .iter()
            .filter(|t| t.agent_name == agent_name)
            .collect()
    }

    /// Get current task depth (deepest active task)
    pub fn get_current_depth(&self) -> usize {
        self.task_history
            .iter()
            .filter(|t| matches!(t.status, TaskStatus::Started | TaskStatus::InProgress(_)))
            .map(|t| t.depth)
            .max()
            .unwrap_or(0)
    }

    /// Increment subtask count for a parent task
    pub fn increment_subtask_count(&mut self, parent_task_id: &str) {
        if let Some(task) = self.task_history.iter_mut().find(|t| t.task_id == parent_task_id) {
            task.subtask_count += 1;
        }
    }
}

impl Default for VisibilityManager {
    fn default() -> Self {
        Self::new("default".to_string())
    }
}