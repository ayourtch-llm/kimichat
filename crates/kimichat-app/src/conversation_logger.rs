use chrono::Local;
use serde::Serialize;
use serde_json;
use anyhow::Result;
use std::path::{Path, PathBuf};
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;

#[derive(Serialize)]
struct ToolCallInfo {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct LogEntry {
    timestamp: String, // ISO‑8601 Local time
    role: String,
    content: String,
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_binary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCallInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_depth: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_name: Option<String>,
}

pub struct ConversationLogger {
    file_path: PathBuf,
    file: Option<tokio::fs::File>,
}

impl ConversationLogger {
    /// Create a new logger; generates the file name based on the current local time.
    pub async fn new(workspace: &Path) -> Result<Self> {
        // Ensure workspace exists
        fs::create_dir_all(workspace).await?;

        // Create logs subdirectory if it doesn't exist
        let logs_dir = workspace.join("logs");
        fs::create_dir_all(&logs_dir).await?;

        let now_local = Local::now();
        let filename = format!(
            "kchat-{}.jsonl",
            now_local.format("%Y-%m-%d-%H%M%S")
        );
        let file_path = logs_dir.join(filename);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .await?;
        Ok(Self { file_path, file: Some(file) })
    }

    /// Create a new logger for task mode; generates the file name with "-task" suffix.
    pub async fn new_task_mode(workspace: &Path) -> Result<Self> {
        // Ensure workspace exists
        fs::create_dir_all(workspace).await?;

        // Create logs subdirectory if it doesn't exist
        let logs_dir = workspace.join("logs");
        fs::create_dir_all(&logs_dir).await?;

        let now_local = Local::now();
        let filename = format!(
            "kchat-{}-task.jsonl",
            now_local.format("%Y-%m-%d-%H%M%S")
        );
        let file_path = logs_dir.join(filename);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .await?;
        Ok(Self { file_path, file: Some(file) })
    }

    /// Append a single log entry.
    pub async fn log(&mut self, role: &str, content: &str, model: Option<&str>, is_binary: bool) {
        self.log_with_task_context(role, content, model, is_binary, None, None, None, None).await;
    }

    /// Append a log entry with task context
    pub async fn log_with_task_context(
        &mut self,
        role: &str,
        content: &str,
        model: Option<&str>,
        is_binary: bool,
        task_id: Option<&str>,
        parent_task_id: Option<&str>,
        task_depth: Option<usize>,
        agent_name: Option<&str>,
    ) {
        let entry = LogEntry {
            timestamp: Local::now().to_rfc3339(),
            role: role.to_string(),
            content: content.to_string(),
            model: model.map(|s| s.to_string()),
            is_binary: if is_binary { Some(true) } else { None },
            tool_calls: None,
            tool_call_id: None,
            name: None,
            task_id: task_id.map(|s| s.to_string()),
            parent_task_id: parent_task_id.map(|s| s.to_string()),
            task_depth,
            agent_name: agent_name.map(|s| s.to_string()),
        };
        if let Some(file) = &mut self.file {
            if let Ok(json) = serde_json::to_string(&entry) {
                // Write the JSON line
                if let Err(e) = file.write_all(json.as_bytes()).await {
                    eprintln!("[Logging error] {}", e);
                } else if let Err(e) = file.write_all(b"\n").await {
                    eprintln!("[Logging error] {}", e);
                }
            }
        }
    }

    /// Log an assistant message with tool calls
    pub async fn log_with_tool_calls(
        &mut self,
        role: &str,
        content: &str,
        model: Option<&str>,
        tool_calls: Vec<(String, String, String)>, // (id, name, arguments)
    ) {
        self.log_with_tool_calls_and_task(role, content, model, tool_calls, None, None, None, None).await;
    }

    /// Log an assistant message with tool calls and task context
    pub async fn log_with_tool_calls_and_task(
        &mut self,
        role: &str,
        content: &str,
        model: Option<&str>,
        tool_calls: Vec<(String, String, String)>, // (id, name, arguments)
        task_id: Option<&str>,
        parent_task_id: Option<&str>,
        task_depth: Option<usize>,
        agent_name: Option<&str>,
    ) {
        let tool_call_info: Vec<ToolCallInfo> = tool_calls
            .into_iter()
            .map(|(id, name, arguments)| ToolCallInfo { id, name, arguments })
            .collect();

        let entry = LogEntry {
            timestamp: Local::now().to_rfc3339(),
            role: role.to_string(),
            content: content.to_string(),
            model: model.map(|s| s.to_string()),
            is_binary: None,
            tool_calls: Some(tool_call_info),
            tool_call_id: None,
            name: None,
            task_id: task_id.map(|s| s.to_string()),
            parent_task_id: parent_task_id.map(|s| s.to_string()),
            task_depth,
            agent_name: agent_name.map(|s| s.to_string()),
        };
        if let Some(file) = &mut self.file {
            if let Ok(json) = serde_json::to_string(&entry) {
                if std::env::var("DEBUG_LOG").is_ok() {
                    eprintln!("[DEBUG] Writing tool_calls log entry: {}", &json[..json.len().min(100)]);
                }
                if let Err(e) = file.write_all(json.as_bytes()).await {
                    eprintln!("[Logging error] {}", e);
                } else if let Err(e) = file.write_all(b"\n").await {
                    eprintln!("[Logging error] {}", e);
                } else {
                    // Flush to ensure it's written
                    let _ = file.flush().await;
                }
            }
        }
    }

    /// Log a tool result
    pub async fn log_tool_result(
        &mut self,
        content: &str,
        tool_call_id: &str,
        tool_name: &str,
    ) {
        self.log_tool_result_with_task(content, tool_call_id, tool_name, None, None, None, None).await;
    }

    /// Log a tool result with task context
    pub async fn log_tool_result_with_task(
        &mut self,
        content: &str,
        tool_call_id: &str,
        tool_name: &str,
        task_id: Option<&str>,
        parent_task_id: Option<&str>,
        task_depth: Option<usize>,
        agent_name: Option<&str>,
    ) {
        let entry = LogEntry {
            timestamp: Local::now().to_rfc3339(),
            role: "tool".to_string(),
            content: content.to_string(),
            model: None,
            is_binary: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
            name: Some(tool_name.to_string()),
            task_id: task_id.map(|s| s.to_string()),
            parent_task_id: parent_task_id.map(|s| s.to_string()),
            task_depth,
            agent_name: agent_name.map(|s| s.to_string()),
        };
        if let Some(file) = &mut self.file {
            if let Ok(json) = serde_json::to_string(&entry) {
                if std::env::var("DEBUG_LOG").is_ok() {
                    eprintln!("[DEBUG] Writing tool result for {}: {}", tool_name, &content[..content.len().min(50)]);
                }
                if let Err(e) = file.write_all(json.as_bytes()).await {
                    eprintln!("[Logging error] {}", e);
                } else if let Err(e) = file.write_all(b"\n").await {
                    eprintln!("[Logging error] {}", e);
                } else {
                    // Flush to ensure it's written
                    let _ = file.flush().await;
                }
            }
        }
    }

    /// Close the logger (explicit drop). Called on graceful shutdown.
    pub async fn shutdown(&mut self) {
        if let Some(file) = self.file.take() {
            // Ensure data is flushed
            let _ = file.sync_all().await;
        }
    }
}
