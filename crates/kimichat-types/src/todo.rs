use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Task status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::InProgress => write!(f, "in_progress"),
            TaskStatus::Completed => write!(f, "completed"),
        }
    }
}

/// A single task in the todo list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Task description (imperative form: "Do the thing")
    pub content: String,
    /// Current status
    pub status: TaskStatus,
    /// Active form for display when in progress ("Doing the thing")
    pub active_form: String,
}

impl Task {
    pub fn new(content: String, active_form: String) -> Self {
        Self {
            content,
            status: TaskStatus::Pending,
            active_form,
        }
    }

    pub fn icon(&self) -> &str {
        match self.status {
            TaskStatus::Pending => "⏸️",
            TaskStatus::InProgress => "▶️",
            TaskStatus::Completed => "✅",
        }
    }
}

/// Manages the todo list for the current session
#[derive(Debug, Clone)]
pub struct TodoManager {
    tasks: Arc<Mutex<Vec<Task>>>,
}

impl TodoManager {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Set the complete todo list (replacing existing)
    pub fn set_tasks(&self, tasks: Vec<Task>) {
        let mut guard = self.tasks.lock().unwrap();
        *guard = tasks;
    }

    /// Get all tasks
    pub fn get_tasks(&self) -> Vec<Task> {
        self.tasks.lock().unwrap().clone()
    }

    /// Get tasks by status
    pub fn get_tasks_by_status(&self, status: TaskStatus) -> Vec<Task> {
        self.tasks
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.status == status)
            .cloned()
            .collect()
    }

    /// Count tasks by status
    pub fn count_by_status(&self, status: TaskStatus) -> usize {
        self.tasks
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.status == status)
            .count()
    }

    /// Get all tasks
    pub fn display(&self) -> String {
        let tasks = self.tasks.lock().unwrap();

        if tasks.is_empty() {
            return String::new();
        }

        let pending = tasks.iter().filter(|t| t.status == TaskStatus::Pending).count();
        let in_progress = tasks.iter().filter(|t| t.status == TaskStatus::InProgress).count();
        let completed = tasks.iter().filter(|t| t.status == TaskStatus::Completed).count();

        let mut output = String::new();
        output.push_str("\n═══════════════════════════════════════════════════════════\n");
        output.push_str(&format!("📋 Tasks: {} pending, {} in progress, {} completed\n", pending, in_progress, completed));
        output.push_str("═══════════════════════════════════════════════════════════\n");

        for (idx, task) in tasks.iter().enumerate() {
            let task_num = format!("{}.", idx + 1);
            let content = match task.status {
                TaskStatus::Pending => task.content.clone(),
                TaskStatus::InProgress => task.active_form.clone(),
                TaskStatus::Completed => task.content.clone(),
            };

            output.push_str(&format!("  {} {} {}\n", task_num, task.icon(), content));
        }

        output.push_str("═══════════════════════════════════════════════════════════\n");
        output
    }

    /// Display a compact summary (one line)
    pub fn display_compact(&self) -> String {
        let tasks = self.tasks.lock().unwrap();

        if tasks.is_empty() {
            return String::new();
        }

        let pending = tasks.iter().filter(|t| t.status == TaskStatus::Pending).count();
        let in_progress = tasks.iter().filter(|t| t.status == TaskStatus::InProgress).count();
        let completed = tasks.iter().filter(|t| t.status == TaskStatus::Completed).count();
        let total = tasks.len();

        // Show current in-progress task if any
        if let Some(current) = tasks.iter().find(|t| t.status == TaskStatus::InProgress) {
            format!("▶️ {} ({}/{} tasks complete)", current.active_form, completed, total)
        } else if completed == total {
            format!("🎉 All tasks completed! ({}/{})", completed, total)
        } else {
            String::new()
        }
    }

    /// Validate that exactly one task is in progress (or none)
    pub fn validate_in_progress_count(&self) -> Result<(), String> {
        let count = self.count_by_status(TaskStatus::InProgress);
        if count > 1 {
            Err(format!(
                "Invalid todo state: {} tasks in progress (should be exactly 1 or 0)",
                count
            ))
        } else {
            Ok(())
        }
    }
}

impl Default for TodoManager {
    fn default() -> Self {
        Self::new()
    }
}
