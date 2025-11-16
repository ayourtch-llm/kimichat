// LLM tool implementations for terminal session management

use kimichat_toolcore::{param, {Tool, ToolParameters, ToolResult, ParameterDefinition}};
use kimichat_toolcore::tool_context::ToolContext;
use async_trait::async_trait;
use std::collections::HashMap;
use serde_json::json;

/// Tool for launching a new PTY terminal session
pub struct PtyLaunchTool;

#[async_trait]
impl Tool for PtyLaunchTool {
    fn name(&self) -> &str {
        "pty_launch"
    }

    fn description(&self) -> &str {
        "Launch a new PTY terminal session with optional command, working directory, and size"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("command", "string", "Command to run in the terminal (default: shell)", optional),
            param!("working_dir", "string", "Working directory for the session (default: current)", optional),
            param!("cols", "integer", "Terminal width in columns (default: 80)", optional),
            param!("rows", "integer", "Terminal height in rows (default: 24)", optional),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let command = params.get_optional::<String>("command").unwrap_or(None);
        let working_dir_str = params.get_optional::<String>("working_dir").unwrap_or(None);
        let cols = params.get_optional::<i32>("cols").unwrap_or(None).map(|c| c as u16).unwrap_or(80);
        let rows = params.get_optional::<i32>("rows").unwrap_or(None).map(|r| r as u16).unwrap_or(24);

        // Resolve working directory
        let working_dir = if let Some(dir_str) = &working_dir_str {
            Some(context.work_dir.join(dir_str).display().to_string())
        } else {
            Some(context.work_dir.display().to_string())
        };

        // Generate unique session ID
        let session_id = format!("pty-{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis());

        // Get terminal manager from context
        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        // Create session
        let command_or_shell = command.clone().unwrap_or_else(|| "default shell".to_string());
        let mut manager = terminal_manager.lock().await;
        match manager.create_session(
            session_id.clone(),
            command_or_shell.clone(),
            working_dir.clone(),
            cols,
            rows
        ).await {
            Ok(returned_id) => {
                let result = json!({
                    "session_id": returned_id,
                    "command": command_or_shell,
                    "working_dir": working_dir.unwrap_or_else(|| context.work_dir.display().to_string()),
                    "size": [cols, rows],
                });
                ToolResult::success(format!("PTY session {} launched successfully\n{}", returned_id, serde_json::to_string_pretty(&result).unwrap()))
            }
            Err(e) => ToolResult::error(format!("Failed to launch PTY session: {}", e)),
        }
    }
}

/// Tool for sending keys to a PTY terminal session
pub struct PtySendKeysTool;

#[async_trait]
impl Tool for PtySendKeysTool {
    fn name(&self) -> &str {
        "pty_send_keys"
    }

    fn description(&self) -> &str {
        "Send keystrokes to a PTY terminal session. IMPORTANT: To execute a command, you MUST end it with \\n (newline) - this is equivalent to pressing Enter. Without \\n, the text just appears on screen but doesn't execute. Examples: 'ls\\n' to run ls, 'cd /tmp\\n' to change directory. Also supports special keys: ^C (Ctrl+C to interrupt), ^D (Ctrl+D for EOF), [UP]/[DOWN] (arrow keys), [TAB] (tab completion), etc."
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("session_id", "string", "Session ID to send keys to", required),
            param!("keys", "string", "Keys to send to the terminal. MUST end with \\n to execute commands (press Enter). Example: 'echo hello\\n'", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<String>("session_id") {
            Ok(id) => id,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let keys = match params.get_required::<String>("keys") {
            Ok(k) => k,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        // Get terminal manager from context
        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        // Send keys
        let mut manager = terminal_manager.lock().await;
        match manager.send_input(&session_id, &keys).await {
            Ok(_) => ToolResult::success(format!("Keys sent to session {}", session_id)),
            Err(e) => ToolResult::error(format!("Failed to send keys: {}", e)),
        }
    }
}

/// Tool for getting the current screen contents of a PTY terminal session
pub struct PtyGetScreenTool;

#[async_trait]
impl Tool for PtyGetScreenTool {
    fn name(&self) -> &str {
        "pty_get_screen"
    }

    fn description(&self) -> &str {
        "Get the current screen contents of a PTY terminal session"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("session_id", "string", "Session ID to get screen from", required),
            param!("include_colors", "boolean", "Include ANSI color codes (default: false)", optional),
            param!("include_cursor", "boolean", "Include cursor position (default: true)", optional),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<String>("session_id") {
            Ok(id) => id,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let include_colors = params.get_optional::<bool>("include_colors").unwrap_or(Some(false)).unwrap_or(false);
        let include_cursor = params.get_optional::<bool>("include_cursor").unwrap_or(Some(true)).unwrap_or(true);

        // Get terminal manager from context
        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        // Get screen contents and cursor position
        let manager = terminal_manager.lock().await;
        match manager.get_screen(&session_id, include_colors, include_cursor).await {
            Ok(contents) => {
                let cursor = manager.get_cursor_position(&session_id).await
                    .unwrap_or((0, 0));

                // Get session info for size
                let size = match manager.list_sessions().await {
                    Ok(sessions) => {
                        sessions.iter()
                            .find(|s| s.id == session_id)
                            .map(|s| [s.cols, s.rows])
                            .unwrap_or([80, 24])
                    }
                    Err(_) => [80, 24],
                };

                let result = json!({
                    "session_id": session_id,
                    "contents": contents,
                    "cursor_position": [cursor.1, cursor.0],
                    "size": size,
                });

                ToolResult::success(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => ToolResult::error(format!("Failed to get screen contents: {}", e)),
        }
    }
}

/// Tool for listing all active PTY terminal sessions
pub struct PtyListTool;

#[async_trait]
impl Tool for PtyListTool {
    fn name(&self) -> &str {
        "pty_list"
    }

    fn description(&self) -> &str {
        "List all active PTY terminal sessions"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::new()
    }

    async fn execute(&self, _params: ToolParameters, context: &ToolContext) -> ToolResult {
        // Get terminal manager from context
        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        let manager = terminal_manager.lock().await;
        match manager.list_sessions().await {
            Ok(sessions) => {
                let sessions_info: Vec<_> = sessions.iter().map(|s| {
                    json!({
                        "id": s.id,
                        "command": s.command,
                        "working_dir": s.working_dir,
                        "status": s.status,
                        "created_at": s.created_at.duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0),
                        "size": [s.cols, s.rows],
                    })
                }).collect();

                let result = json!({
                    "sessions": sessions_info,
                    "count": sessions_info.len(),
                });

                ToolResult::success(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => ToolResult::error(format!("Failed to list sessions: {}", e)),
        }
    }
}

/// Tool for killing a PTY terminal session
pub struct PtyKillTool;

#[async_trait]
impl Tool for PtyKillTool {
    fn name(&self) -> &str {
        "pty_kill"
    }

    fn description(&self) -> &str {
        "Kill a PTY terminal session"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("session_id", "string", "Session ID to kill", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<String>("session_id") {
            Ok(id) => id,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        // Get terminal manager from context
        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        let mut manager = terminal_manager.lock().await;
        match manager.kill_session(&session_id).await {
            Ok(_) => ToolResult::success(format!("Session {} killed successfully", session_id)),
            Err(e) => ToolResult::error(format!("Failed to kill session: {}", e)),
        }
    }
}

/// Tool for getting cursor position
pub struct PtyGetCursorTool;

#[async_trait]
impl Tool for PtyGetCursorTool {
    fn name(&self) -> &str {
        "pty_get_cursor"
    }

    fn description(&self) -> &str {
        "Get the current cursor position in a PTY terminal session"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("session_id", "string", "Session ID to get cursor from", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<String>("session_id") {
            Ok(id) => id,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        let manager = terminal_manager.lock().await;
        match manager.get_cursor_position(&session_id).await {
            Ok((row, col)) => {
                let result = json!({
                    "session_id": session_id,
                    "position": [col, row],
                });

                ToolResult::success(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => ToolResult::error(format!("Failed to get cursor position: {}", e)),
        }
    }
}

/// Tool for resizing a PTY terminal session
pub struct PtyResizeTool;

#[async_trait]
impl Tool for PtyResizeTool {
    fn name(&self) -> &str {
        "pty_resize"
    }

    fn description(&self) -> &str {
        "Resize a PTY terminal session"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("session_id", "string", "Session ID to resize", required),
            param!("cols", "integer", "New terminal width in columns", required),
            param!("rows", "integer", "New terminal height in rows", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<String>("session_id") {
            Ok(id) => id,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let cols = match params.get_required::<i32>("cols") {
            Ok(c) => c as u16,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let rows = match params.get_required::<i32>("rows") {
            Ok(r) => r as u16,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        let mut manager = terminal_manager.lock().await;
        match manager.resize_session(&session_id, rows, cols).await {
            Ok(_) => {
                let result = json!({
                    "session_id": session_id,
                    "size": [cols, rows],
                });
                ToolResult::success(format!("Session {} resized to {}x{}\n{}",
                    session_id, cols, rows,
                    serde_json::to_string_pretty(&result).unwrap()))
            }
            Err(e) => ToolResult::error(format!("Failed to resize session: {}", e)),
        }
    }
}

/// Tool for setting scrollback buffer size
pub struct PtySetScrollbackTool;

#[async_trait]
impl Tool for PtySetScrollbackTool {
    fn name(&self) -> &str {
        "pty_set_scrollback"
    }

    fn description(&self) -> &str {
        "Set the scrollback buffer size for a PTY terminal session"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("session_id", "string", "Session ID to configure", required),
            param!("lines", "integer", "Number of scrollback lines to keep", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<String>("session_id") {
            Ok(id) => id,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let lines = match params.get_required::<i32>("lines") {
            Ok(l) => l as usize,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        let mut manager = terminal_manager.lock().await;
        match manager.set_scrollback(&session_id, lines).await {
            Ok(_) => {
                let result = json!({
                    "session_id": session_id,
                    "scrollback_lines": lines,
                });
                ToolResult::success(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => ToolResult::error(format!("Failed to set scrollback: {}", e)),
        }
    }
}

/// Tool for starting output capture to file
pub struct PtyStartCaptureTool;

#[async_trait]
impl Tool for PtyStartCaptureTool {
    fn name(&self) -> &str {
        "pty_start_capture"
    }

    fn description(&self) -> &str {
        "Start capturing PTY output to a timestamped file"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("session_id", "string", "Session ID to start capturing", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<String>("session_id") {
            Ok(id) => id,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        let mut manager = terminal_manager.lock().await;
        // Backend generates the capture file path
        match manager.capture_start(&session_id, String::new()).await {
            Ok(_) => {
                let result = json!({
                    "session_id": session_id,
                    "status": "capturing",
                });
                ToolResult::success(format!("Started capturing session {}\n{}",
                    session_id,
                    serde_json::to_string_pretty(&result).unwrap()))
            }
            Err(e) => ToolResult::error(format!("Failed to start capture: {}", e)),
        }
    }
}

/// Tool for stopping output capture
pub struct PtyStopCaptureTool;

#[async_trait]
impl Tool for PtyStopCaptureTool {
    fn name(&self) -> &str {
        "pty_stop_capture"
    }

    fn description(&self) -> &str {
        "Stop capturing PTY output and return capture file information"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("session_id", "string", "Session ID to stop capturing", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<String>("session_id") {
            Ok(id) => id,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        let mut manager = terminal_manager.lock().await;
        match manager.capture_stop(&session_id).await {
            Ok((capture_file, bytes, duration)) => {
                let result = json!({
                    "session_id": session_id,
                    "capture_file": capture_file,
                    "bytes_captured": bytes,
                    "duration_seconds": duration,
                });
                ToolResult::success(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => ToolResult::error(format!("Failed to stop capture: {}", e)),
        }
    }
}

/// Tool for requesting user input/interaction with a PTY session
pub struct PtyRequestUserInputTool;

#[async_trait]
impl Tool for PtyRequestUserInputTool {
    fn name(&self) -> &str {
        "pty_request_user_input"
    }

    fn description(&self) -> &str {
        "Request user to interact directly with a PTY terminal session. Displays the current screen and message, then allows user to provide input. Use this when the LLM needs human assistance (e.g., password entry, manual debugging)."
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("session_id", "string", "Session ID to hand over to user", required),
            param!("message", "string", "Message to display to the user explaining what's needed", required),
            param!("timeout_seconds", "integer", "Timeout in seconds (default: 300/5 minutes)", optional),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<String>("session_id") {
            Ok(id) => id,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let message = match params.get_required::<String>("message") {
            Ok(msg) => msg,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let timeout_seconds = params.get_optional::<i32>("timeout_seconds")
            .unwrap_or(Some(300))
            .unwrap_or(300) as u64;

        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        let manager = terminal_manager.lock().await;

        // Get current screen contents
        let screen_contents = match manager.get_screen(&session_id, false, true).await {
            Ok(contents) => contents,
            Err(e) => return ToolResult::error(format!("Failed to get screen contents: {}", e)),
        };

        // Get session info
        let (working_dir, command) = match manager.list_sessions().await {
            Ok(sessions) => {
                sessions.iter()
                    .find(|s| s.id == session_id)
                    .map(|s| (
                        s.working_dir.clone().unwrap_or_else(|| "unknown".to_string()),
                        s.command.clone()
                    ))
                    .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()))
            }
            Err(_) => ("unknown".to_string(), "unknown".to_string()),
        };

        // For now, we return information about the session state and instructions
        // A full implementation would involve complex async I/O handling to actually
        // attach the terminal to the user's stdin/stdout
        let result = json!({
            "session_id": session_id,
            "message": message,
            "timeout_seconds": timeout_seconds,
            "current_screen": screen_contents,
            "working_dir": working_dir,
            "command": command,
            "instructions": format!(
                "User assistance requested for terminal session {}.\n\n\
                Message: {}\n\n\
                Current screen state:\n{}\n\n\
                To interact with this session, use:\n\
                - pty_send_keys to send commands\n\
                - pty_get_screen to see updated output\n\n\
                Session will remain available for {} seconds.",
                session_id, message, screen_contents, timeout_seconds
            ),
        });

        ToolResult::success(serde_json::to_string_pretty(&result).unwrap())
    }
}
