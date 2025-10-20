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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Started,
    InProgress(f32), // Progress percentage
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub execution_time: Duration,
    pub tokens_used: Option<usize>,
    pub tool_calls: usize,
    pub api_calls: usize,
    pub memory_usage: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_tasks: usize,
    pub successful_tasks: usize,
    pub failed_tasks: usize,
    pub average_execution_time: Duration,
    pub total_tokens_used: usize,
    pub agent_performance: HashMap<String, AgentPerformance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            
            if self.verbosity_level == VerbosityLevel::Detailed || self.verbosity_level == VerbosityLevel::Debug {
                self.display_agent_update(agent);
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
        };
        
        self.task_history.push(event);
        
        // Update agent status
        self.update_agent_status(
            &agent_name,
            AgentStatus::Executing,
            Some(task_description)
        );
        
        if self.verbosity_level != VerbosityLevel::Minimal {
            println!("{} {} started task: {}", 
                "▶️".green(),
                agent_name.bright_white(),
                task_description.cyan()
            );
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
        if let Some(task) = self.task_history.iter_mut().find(|t| t.task_id == task_id) {
            task.end_time = Some(Instant::now());
            task.status = if success {
                TaskStatus::Completed
            } else {
                TaskStatus::Failed("Task execution failed".to_string())
            };
            task.result_summary = result_summary;
            
            // Update performance metrics
            self.update_performance_metrics(task, success);
            
            // Update agent status
            self.update_agent_status(
                &task.agent_name.clone(),
                if success { AgentStatus::Completed } else { AgentStatus::Failed("Task failed".to_string()) },
                None
            );
        }
    }
    
    /// Update performance metrics
    fn update_performance_metrics(&mut self, task: &TaskVisibilityEvent, success: bool) {
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
}

impl Default for VisibilityManager {
    fn default() -> Self {
        Self::new("default".to_string())
    }
}