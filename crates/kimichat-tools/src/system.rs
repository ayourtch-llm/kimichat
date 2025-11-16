use kimichat_toolcore::{param, Tool, ToolParameters, ToolResult, ParameterDefinition};
use kimichat_toolcore::tool_context::ToolContext;
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::process::Command as AsyncCommand;
use colored::Colorize;
use std::io::Write;

/// Tool for running shell commands
pub struct RunCommandTool;

#[async_trait]
impl Tool for RunCommandTool {
    fn name(&self) -> &str {
        "run_command"
    }

    fn description(&self) -> &str {
        "Run a shell command and return the output"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("command", "string", "Shell command to execute", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let command = match params.get_required::<String>("command") {
            Ok(command) => command,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        // Basic security checks - prevent dangerous commands
        let dangerous_patterns = [
            "rm -rf /",
            "sudo rm",
            ":(){ :|:& };:",
            "chmod -R 777 /",
            "dd if=",
        ];

        for pattern in &dangerous_patterns {
            if command.contains(pattern) {
                return ToolResult::error(format!("Command blocked for security reasons: contains dangerous pattern '{}'", pattern));
            }
        }

        // Check permission using policy system
        print!("{} {} ", "Run command:".yellow(), command.cyan());
        std::io::stdout().flush().ok();

        let (approved, rejection_reason) = match context.check_permission(
            kimichat_policy::ActionType::CommandExecution,
            &command,
            "Execute? (y/N):"
        ) {
            Ok((approved, reason)) => (approved, reason),
            Err(e) => return ToolResult::error(format!("Permission check failed: {}", e)),
        };

        if !approved {
            let error_msg = if let Some(reason) = rejection_reason {
                format!("Command cancelled by user: {}", reason)
            } else {
                "Command cancelled by user or policy".to_string()
            };
            return ToolResult::error(error_msg);
        }

        println!("{} {}", "Running:".green(), command.cyan());

        // Parse command and arguments
        let orig_command = command.clone();
        let parts: Vec<&str> = command.trim().split_whitespace().collect();
        if parts.is_empty() {
            return ToolResult::error("Empty command".to_string());
        }

        let (cmd, args) = parts.split_first().unwrap();

        // Execute command in work directory
        let output = match AsyncCommand::new("bash")
            .args(["-c", &orig_command])
            .current_dir(&context.work_dir)
            .output()
            .await
        {
            Ok(output) => output,
            Err(e) => {
                return ToolResult::error(format!("Failed to execute command: {}", e));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let result = if !stderr.is_empty() {
            format!(
                "Command: {}\nExit code: {}\nSTDOUT:\n{}\nSTDERR:\n{}",
                command,
                output.status.code().unwrap_or(-1),
                stdout,
                stderr
            )
        } else {
            format!(
                "Command: {}\nExit code: {}\nSTDOUT:\n{}",
                command,
                output.status.code().unwrap_or(-1),
                stdout
            )
        };

        ToolResult::success(result)
    }
}
