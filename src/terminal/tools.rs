// LLM tool implementations for terminal session management

use crate::{param, core::tool::{Tool, ToolParameters, ToolResult, ParameterDefinition}};
use crate::core::tool_context::ToolContext;
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

        // Get session and send keys
        let manager = terminal_manager.lock().unwrap();
        match manager.get_session(session_id) {
            Ok(session_arc) => {
                let mut session = session_arc.lock().unwrap();
                match session.send_keys(&keys, special) {
                    Ok(_) => ToolResult::success(format!("Keys sent to session {}", session_id)),
                    Err(e) => ToolResult::error(format!("Failed to send keys: {}", e)),
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

        // Get session and screen contents
        let manager = terminal_manager.lock().unwrap();
        match manager.get_session(session_id) {
            Ok(session_arc) => {
                let mut session = session_arc.lock().unwrap();

                // First update the screen with any new output
                if let Err(e) = session.update_screen() {
                    return ToolResult::error(format!("Failed to update screen: {}", e));
                }

                // Get screen contents
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
