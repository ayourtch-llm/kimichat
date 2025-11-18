use kimichat_toolcore::{param, Tool, ToolParameters, ToolResult, ParameterDefinition};
use kimichat_toolcore::tool_context::ToolContext;
use async_trait::async_trait;
use std::collections::HashMap;
use std::process::Command;
use serde_json;

/// Tool for launching a subagent to execute a task independently
pub struct LaunchSubagentTool;

#[async_trait]
impl Tool for LaunchSubagentTool {
    fn name(&self) -> &str {
        "launch_subagent"
    }

    fn description(&self) -> &str {
        "Launch a subagent to execute a task independently and return structured JSON results. Use this when you need to delegate a self-contained task that should be completed without interaction. The subagent runs in non-interactive mode and returns clean JSON output with task results, files modified, and execution metadata."
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("task", "string", "The task description for the subagent to execute", required),
            param!("working_directory", "string", "Optional working directory for the subagent (defaults to current directory)", optional),
            param!("timeout_seconds", "integer", "Optional timeout in seconds (defaults to 300)", optional),
            param!("auto_confirm", "boolean", "Whether to auto-confirm all actions without prompting (defaults to true)", optional),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let task = match params.get_required::<String>("task") {
            Ok(task) => task,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let working_directory = params.get_optional::<String>("working_directory")
            .unwrap_or_else(|_| Some(context.work_dir.to_string_lossy().to_string()))
            .unwrap_or_else(|| context.work_dir.to_string_lossy().to_string());

        let timeout_seconds = params.get_optional::<i64>("timeout_seconds")
            .unwrap_or_else(|_| Some(300))
            .unwrap_or(300);
        let auto_confirm = params.get_optional::<bool>("auto_confirm")
            .unwrap_or_else(|_| Some(true))
            .unwrap_or(true);

        // Build the kimichat command
        let mut cmd = Command::new("kimichat");
        cmd.arg("--task")
           .arg(&task)
           .arg("--auto-confirm"); // Always use auto-confirm for subagent calls

        if working_directory != context.work_dir.to_string_lossy() {
            cmd.current_dir(&working_directory);
        }

        // Set environment variables for timeout if needed
        if timeout_seconds != 300 {
            cmd.env("KIMICHAT_TIMEOUT", timeout_seconds.to_string());
        }

        // Execute the command
        let output = match tokio::task::spawn_blocking(move || {
            cmd.output()
        }).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => return ToolResult::error(format!("Failed to execute subagent: {}", e)),
            Err(e) => return ToolResult::error(format!("Task join error: {}", e)),
        };

        // Parse the JSON output
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            
            // Try to parse as JSON
            match serde_json::from_str::<serde_json::Value>(&stdout) {
                Ok(json_result) => {
                    // Format the result nicely
                    let formatted_result = format!(
                        "✅ **Subagent Task Completed Successfully**\n\n**Task:** {}\n\n**Result:** {}",
                        task,
                        serde_json::to_string_pretty(&json_result).unwrap_or_else(|_| stdout.to_string())
                    );
                    ToolResult::success(formatted_result)
                }
                Err(_) => {
                    // If not valid JSON, return as plain text
                    let result = format!(
                        "✅ **Subagent Task Completed**\n\n**Task:** {}\n\n**Output:**\n{}",
                        task,
                        stdout
                    );
                    ToolResult::success(result)
                }
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            
            let error_msg = format!(
                "❌ **Subagent Task Failed**\n\n**Task:** {}\n\n**Error:**\n{}\n\n**Output:**\n{}",
                task,
                stderr,
                stdout
            );
            
            ToolResult::error(error_msg)
        }
    }
}

/// Tool for running subagent with pretty JSON output
pub struct LaunchSubagentPrettyTool;

#[async_trait]
impl Tool for LaunchSubagentPrettyTool {
    fn name(&self) -> &str {
        "launch_subagent_pretty"
    }

    fn description(&self) -> &str {
        "Launch a subagent with pretty-printed JSON output. Same as launch_subagent but formats the JSON result for better readability."
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("task", "string", "The task description for the subagent to execute", required),
            param!("working_directory", "string", "Optional working directory for the subagent (defaults to current directory)", optional),
            param!("timeout_seconds", "integer", "Optional timeout in seconds (defaults to 300)", optional),
            param!("auto_confirm", "boolean", "Whether to auto-confirm all actions without prompting (defaults to true)", optional),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let task = match params.get_required::<String>("task") {
            Ok(task) => task,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let working_directory = params.get_optional::<String>("working_directory")
            .unwrap_or_else(|_| Some(context.work_dir.to_string_lossy().to_string()))
            .unwrap_or_else(|| context.work_dir.to_string_lossy().to_string());

        let timeout_seconds = params.get_optional::<i64>("timeout_seconds")
            .unwrap_or_else(|_| Some(300))
            .unwrap_or(300);
        let auto_confirm = params.get_optional::<bool>("auto_confirm")
            .unwrap_or_else(|_| Some(true))
            .unwrap_or(true);

        // Build the kimichat command with --pretty flag
        let mut cmd = Command::new("kimichat");
        cmd.arg("--task")
           .arg(&task)
           .arg("--pretty")
           .arg("--auto-confirm");

        if working_directory != context.work_dir.to_string_lossy() {
            cmd.current_dir(&working_directory);
        }

        // Set environment variables for timeout if needed
        if timeout_seconds != 300 {
            cmd.env("KIMICHAT_TIMEOUT", timeout_seconds.to_string());
        }

        // Execute the command
        let output = match tokio::task::spawn_blocking(move || {
            cmd.output()
        }).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => return ToolResult::error(format!("Failed to execute subagent: {}", e)),
            Err(e) => return ToolResult::error(format!("Task join error: {}", e)),
        };

        // Parse the JSON output
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            
            // Try to parse as JSON
            match serde_json::from_str::<serde_json::Value>(&stdout) {
                Ok(json_result) => {
                    // Format the result nicely
                    let formatted_result = format!(
                        "✅ **Subagent Task Completed Successfully**\n\n**Task:** {}\n\n**Pretty JSON Result:**\n```json\n{}\n```",
                        task,
                        serde_json::to_string_pretty(&json_result).unwrap_or_else(|_| stdout.to_string())
                    );
                    ToolResult::success(formatted_result)
                }
                Err(_) => {
                    // If not valid JSON, return as plain text
                    let result = format!(
                        "✅ **Subagent Task Completed**\n\n**Task:** {}\n\n**Output:**\n```\n{}\n```",
                        task,
                        stdout
                    );
                    ToolResult::success(result)
                }
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            
            let error_msg = format!(
                "❌ **Subagent Task Failed**\n\n**Task:** {}\n\n**Error:**\n```\n{}\n```\n\n**Output:**\n```\n{}\n```",
                task,
                stderr,
                stdout
            );
            
            ToolResult::error(error_msg)
        }
    }
}