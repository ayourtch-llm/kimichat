// LLM tool implementations for terminal session management

use crate::{param, core::tool::{Tool, ToolParameters, ToolResult, ParameterDefinition}};
use crate::core::tool_context::ToolContext;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use serde_json::json;
use tokio::time::{timeout, Duration};

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
        let cols = params.get_optional::<i32>("cols").unwrap_or(None).map(|c| c as u16);
        let rows = params.get_optional::<i32>("rows").unwrap_or(None).map(|r| r as u16);

        // Resolve working directory
        let working_dir = if let Some(dir_str) = working_dir_str {
            Some(context.work_dir.join(dir_str))
        } else {
            None
        };

        // Get terminal manager from context
        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        // Create session
        let mut manager = terminal_manager.lock().unwrap();
        match manager.create_session(command.clone(), working_dir.clone(), cols, rows) {
            Ok(session_id) => {
                let result = json!({
                    "session_id": session_id,
                    "command": command.unwrap_or_else(|| "default shell".to_string()),
                    "working_dir": working_dir.unwrap_or_else(|| context.work_dir.clone()).display().to_string(),
                    "size": [cols.unwrap_or(80), rows.unwrap_or(24)],
                });
                ToolResult::success(format!("PTY session {} launched successfully\n{}", session_id, serde_json::to_string_pretty(&result).unwrap()))
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
        "Send keystrokes to a PTY terminal session. Supports special keys like ^C (Ctrl+C), [UP], [DOWN], [F1], etc."
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("session_id", "integer", "Session ID to send keys to", required),
            param!("keys", "string", "Keys to send to the terminal", required),
            param!("special", "boolean", "Process special key sequences (default: true)", optional),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<i32>("session_id") {
            Ok(id) => id as u32,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let keys = match params.get_required::<String>("keys") {
            Ok(k) => k,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let special = params.get_optional::<bool>("special").unwrap_or(Some(true)).unwrap_or(true);

        // Get terminal manager from context
        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        // Get session - scope the lock to ensure it's dropped before await
        let session_arc = {
            let manager = terminal_manager.lock().unwrap();
            manager.get_session(session_id)
        }; // manager lock is dropped here

        match session_arc {
            Ok(session_arc) => {
                // Clone Arc for async operation
                let session_clone = Arc::clone(&session_arc);

                // Send keys and update screen in a blocking task with timeout
                let task = tokio::task::spawn_blocking(move || -> Result<String, String> {
                    let mut session = session_clone.lock().unwrap();

                    // Send the keys
                    session.send_keys(&keys, special)
                        .map_err(|e| format!("Failed to send keys: {}", e))?;

                    // Wait a short time for output to be available
                    std::thread::sleep(std::time::Duration::from_millis(50));

                    // Update screen buffer with any available output
                    let _ = session.update_screen(); // Ignore errors - buffer may be empty

                    Ok(format!("Keys sent to session {}", session_id))
                });

                // Wrap with timeout to prevent hanging
                match timeout(Duration::from_millis(200), task).await {
                    Ok(Ok(Ok(msg))) => ToolResult::success(msg),
                    Ok(Ok(Err(e))) => ToolResult::error(e),
                    Ok(Err(e)) => ToolResult::error(format!("Task error: {}", e)),
                    Err(_) => ToolResult::error("Operation timed out - PTY may be unresponsive".to_string()),
                }
            }
            Err(e) => ToolResult::error(format!("Failed to get session: {}", e)),
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
            param!("session_id", "integer", "Session ID to get screen from", required),
            param!("include_colors", "boolean", "Include ANSI color codes (default: false)", optional),
            param!("include_cursor", "boolean", "Include cursor position (default: true)", optional),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<i32>("session_id") {
            Ok(id) => id as u32,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let include_colors = params.get_optional::<bool>("include_colors").unwrap_or(Some(false)).unwrap_or(false);
        let include_cursor = params.get_optional::<bool>("include_cursor").unwrap_or(Some(true)).unwrap_or(true);

        // Get terminal manager from context
        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        // Get session and screen contents (just read from buffer, no PTY read)
        let manager = terminal_manager.lock().unwrap();
        match manager.get_session(session_id) {
            Ok(session_arc) => {
                let session = session_arc.lock().unwrap();

                // Just get the current screen buffer state without reading from PTY
                // The buffer is updated when keys are sent or by background tasks
                match session.get_screen(include_colors, include_cursor) {
                    Ok(contents) => {
                        let cursor = session.get_cursor();
                        let metadata = session.metadata();

                        let result = json!({
                            "session_id": session_id,
                            "contents": contents,
                            "cursor_position": [cursor.0, cursor.1],
                            "size": [metadata.size.0, metadata.size.1],
                        });

                        ToolResult::success(serde_json::to_string_pretty(&result).unwrap())
                    }
                    Err(e) => ToolResult::error(format!("Failed to get screen contents: {}", e)),
                }
            }
            Err(e) => ToolResult::error(format!("Failed to get session: {}", e)),
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

        let manager = terminal_manager.lock().unwrap();
        let session_ids = manager.list_sessions();

        let mut sessions_info = Vec::new();
        for session_id in session_ids {
            if let Ok(session_arc) = manager.get_session(session_id) {
                let session = session_arc.lock().unwrap();
                let metadata = session.metadata();

                sessions_info.push(json!({
                    "id": metadata.id,
                    "command": metadata.command,
                    "working_dir": metadata.working_dir.display().to_string(),
                    "status": format!("{:?}", metadata.status),
                    "created_at": metadata.created_at.to_rfc3339(),
                    "size": [metadata.size.0, metadata.size.1],
                }));
            }
        }

        let result = json!({
            "sessions": sessions_info,
            "count": sessions_info.len(),
        });

        ToolResult::success(serde_json::to_string_pretty(&result).unwrap())
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
            param!("session_id", "integer", "Session ID to kill", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<i32>("session_id") {
            Ok(id) => id as u32,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        // Get terminal manager from context
        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        let mut manager = terminal_manager.lock().unwrap();
        match manager.kill_session(session_id) {
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
            param!("session_id", "integer", "Session ID to get cursor from", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<i32>("session_id") {
            Ok(id) => id as u32,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        let manager = terminal_manager.lock().unwrap();
        match manager.get_session(session_id) {
            Ok(session_arc) => {
                let mut session = session_arc.lock().unwrap();

                // Update screen first
                if let Err(e) = session.update_screen() {
                    return ToolResult::error(format!("Failed to update screen: {}", e));
                }

                let cursor = session.get_cursor();
                let result = json!({
                    "session_id": session_id,
                    "position": [cursor.0, cursor.1],
                });

                ToolResult::success(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => ToolResult::error(format!("Failed to get session: {}", e)),
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
            param!("session_id", "integer", "Session ID to resize", required),
            param!("cols", "integer", "New terminal width in columns", required),
            param!("rows", "integer", "New terminal height in rows", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<i32>("session_id") {
            Ok(id) => id as u32,
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

        let manager = terminal_manager.lock().unwrap();
        match manager.get_session(session_id) {
            Ok(session_arc) => {
                let mut session = session_arc.lock().unwrap();
                match session.resize(cols, rows) {
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
            Err(e) => ToolResult::error(format!("Failed to get session: {}", e)),
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
            param!("session_id", "integer", "Session ID to configure", required),
            param!("lines", "integer", "Number of scrollback lines to keep", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<i32>("session_id") {
            Ok(id) => id as u32,
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

        let manager = terminal_manager.lock().unwrap();
        match manager.get_session(session_id) {
            Ok(session_arc) => {
                let mut session = session_arc.lock().unwrap();
                match session.set_scrollback(lines) {
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
            Err(e) => ToolResult::error(format!("Failed to get session: {}", e)),
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
            param!("session_id", "integer", "Session ID to start capturing", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<i32>("session_id") {
            Ok(id) => id as u32,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        let manager = terminal_manager.lock().unwrap();
        match manager.get_session(session_id) {
            Ok(session_arc) => {
                let mut session = session_arc.lock().unwrap();
                match session.start_capture() {
                    Ok(capture_file) => {
                        let result = json!({
                            "session_id": session_id,
                            "capture_file": capture_file.display().to_string(),
                            "status": "capturing",
                        });
                        ToolResult::success(format!("Started capturing session {} to {}\n{}",
                            session_id,
                            capture_file.display(),
                            serde_json::to_string_pretty(&result).unwrap()))
                    }
                    Err(e) => ToolResult::error(format!("Failed to start capture: {}", e)),
                }
            }
            Err(e) => ToolResult::error(format!("Failed to get session: {}", e)),
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
            param!("session_id", "integer", "Session ID to stop capturing", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<i32>("session_id") {
            Ok(id) => id as u32,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let terminal_manager = match &context.terminal_manager {
            Some(tm) => tm,
            None => return ToolResult::error("Terminal manager not available".to_string()),
        };

        let manager = terminal_manager.lock().unwrap();
        match manager.get_session(session_id) {
            Ok(session_arc) => {
                let mut session = session_arc.lock().unwrap();
                match session.stop_capture() {
                    Ok((capture_file, bytes, duration)) => {
                        let result = json!({
                            "session_id": session_id,
                            "capture_file": capture_file.display().to_string(),
                            "bytes_captured": bytes,
                            "duration_seconds": duration,
                        });
                        ToolResult::success(serde_json::to_string_pretty(&result).unwrap())
                    }
                    Err(e) => ToolResult::error(format!("Failed to stop capture: {}", e)),
                }
            }
            Err(e) => ToolResult::error(format!("Failed to get session: {}", e)),
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
            param!("session_id", "integer", "Session ID to hand over to user", required),
            param!("message", "string", "Message to display to the user explaining what's needed", required),
            param!("timeout_seconds", "integer", "Timeout in seconds (default: 300/5 minutes)", optional),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let session_id = match params.get_required::<i32>("session_id") {
            Ok(id) => id as u32,
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

        let manager = terminal_manager.lock().unwrap();
        match manager.get_session(session_id) {
            Ok(session_arc) => {
                let mut session = session_arc.lock().unwrap();

                // Update screen first
                if let Err(e) = session.update_screen() {
                    return ToolResult::error(format!("Failed to update screen: {}", e));
                }

                // Get current screen contents
                let screen_contents = match session.get_screen(false, true) {
                    Ok(contents) => contents,
                    Err(e) => return ToolResult::error(format!("Failed to get screen contents: {}", e)),
                };

                let metadata = session.metadata();

                // For now, we return information about the session state and instructions
                // A full implementation would involve complex async I/O handling to actually
                // attach the terminal to the user's stdin/stdout
                let result = json!({
                    "session_id": session_id,
                    "message": message,
                    "timeout_seconds": timeout_seconds,
                    "current_screen": screen_contents,
                    "working_dir": metadata.working_dir.display().to_string(),
                    "command": metadata.command,
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
            Err(e) => ToolResult::error(format!("Failed to get session: {}", e)),
        }
    }
}
