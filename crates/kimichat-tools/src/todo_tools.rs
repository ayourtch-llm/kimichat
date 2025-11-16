use async_trait::async_trait;
use std::collections::HashMap;

use kimichat_toolcore::{param, Tool, ToolParameters, ToolResult, ParameterDefinition};
use kimichat_toolcore::tool_context::ToolContext;
use kimichat_todo::{Task, TaskStatus};

/// Tool for managing the todo list
pub struct TodoWriteTool;

#[async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "todo_write"
    }

    fn description(&self) -> &str {
        "Create and manage a structured task list for tracking progress during complex operations.

Use this tool to:
- Track multi-step tasks and their progress
- Show the user what you're working on
- Update task status as you complete steps

Task Status:
- pending: Not yet started
- in_progress: Currently working (only ONE task should be in_progress at a time)
- completed: Successfully finished

IMPORTANT RULES:
1. Exactly ONE task should be in_progress at a time (not zero, not multiple)
2. Mark tasks as completed IMMEDIATELY after finishing
3. Only mark completed when FULLY accomplished (not if blocked/errored)
4. Use for complex multi-step tasks (3+ steps)
5. Don't use for single straightforward tasks

Task Format:
{
  \"content\": \"Do the thing\",           // Imperative form
  \"status\": \"pending\",                  // pending | in_progress | completed
  \"activeForm\": \"Doing the thing\"       // Present continuous form
}"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("todos", "array", "Array of tasks with content (imperative form), status (pending/in_progress/completed), and activeForm (present continuous form)", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        // Get todo manager from context
        let todo_manager = match context.todo_manager.as_ref() {
            Some(tm) => tm,
            None => return ToolResult::error("Todo manager not available".to_string()),
        };

        // Parse todos array
        let todos_array = match params.data.get("todos").and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => return ToolResult::error("Missing or invalid 'todos' parameter".to_string()),
        };

        // Parse tasks
        let mut tasks = Vec::new();
        for (idx, todo_val) in todos_array.iter().enumerate() {
            let content = match todo_val.get("content").and_then(|v| v.as_str()) {
                Some(c) => c.to_string(),
                None => return ToolResult::error(format!("Task {} missing 'content'", idx + 1)),
            };

            let status_str = match todo_val.get("status").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return ToolResult::error(format!("Task {} missing 'status'", idx + 1)),
            };

            let status = match status_str {
                "pending" => TaskStatus::Pending,
                "in_progress" => TaskStatus::InProgress,
                "completed" => TaskStatus::Completed,
                _ => return ToolResult::error(format!("Task {} has invalid status: {}", idx + 1, status_str)),
            };

            let active_form = match todo_val.get("activeForm").and_then(|v| v.as_str()) {
                Some(af) => af.to_string(),
                None => return ToolResult::error(format!("Task {} missing 'activeForm'", idx + 1)),
            };

            let mut task = Task::new(content, active_form);
            task.status = status;
            tasks.push(task);
        }

        // Validate that exactly one task is in progress (or none)
        let in_progress_count = tasks.iter().filter(|t| t.status == TaskStatus::InProgress).count();
        if in_progress_count > 1 {
            return ToolResult::error(format!(
                "Invalid todo list: {} tasks are in_progress, should be exactly 1 or 0",
                in_progress_count
            ));
        }

        // Update todo manager
        todo_manager.set_tasks(tasks.clone());

        // Display updated todos
        todo_manager.display();

        ToolResult::success(format!(
            "Updated todo list: {} tasks ({} pending, {} in progress, {} completed)",
            tasks.len(),
            tasks.iter().filter(|t| t.status == TaskStatus::Pending).count(),
            tasks.iter().filter(|t| t.status == TaskStatus::InProgress).count(),
            tasks.iter().filter(|t| t.status == TaskStatus::Completed).count()
        ))
    }
}

impl TodoWriteTool {
    pub fn new() -> Self {
        Self
    }
}

/// Tool for viewing the current todo list
pub struct TodoListTool;

#[async_trait]
impl Tool for TodoListTool {
    fn name(&self) -> &str {
        "todo_list"
    }

    fn description(&self) -> &str {
        "View the current todo list and task status"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::new()
    }

    async fn execute(&self, _params: ToolParameters, context: &ToolContext) -> ToolResult {
        let todo_manager = match context.todo_manager.as_ref() {
            Some(tm) => tm,
            None => return ToolResult::error("Todo manager not available".to_string()),
        };

        let tasks = todo_manager.get_tasks();

        if tasks.is_empty() {
            return ToolResult::success("No tasks in todo list".to_string());
        }

        todo_manager.display();

        let pending = tasks.iter().filter(|t| t.status == TaskStatus::Pending).count();
        let in_progress = tasks.iter().filter(|t| t.status == TaskStatus::InProgress).count();
        let completed = tasks.iter().filter(|t| t.status == TaskStatus::Completed).count();

        ToolResult::success(format!(
            "Todo list: {} total tasks ({} pending, {} in progress, {} completed)",
            tasks.len(), pending, in_progress, completed
        ))
    }
}

impl TodoListTool {
    pub fn new() -> Self {
        Self
    }
}
