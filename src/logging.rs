use chrono::{DateTime, Utc};
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
    timestamp: String, // ISO‑8601 UTC
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
}

pub struct ConversationLogger {
    file_path: PathBuf,
    file: Option<tokio::fs::File>,
}

impl ConversationLogger {
    /// Create a new logger; generates the file name based on the current UTC time.
    pub async fn new(workspace: &Path) -> Result<Self> {
        // Ensure workspace exists
        fs::create_dir_all(workspace).await?;
        
        // Create logs subdirectory if it doesn't exist
        let logs_dir = workspace.join("logs");
        fs::create_dir_all(&logs_dir).await?;
        
        let now: DateTime<Utc> = Utc::now();
        let filename = format!(
            "kchat-{}.jsonl",
            now.format("%Y-%m-%d-%H%M%S")
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
        let entry = LogEntry {
            timestamp: Utc::now().to_rfc3339(),
            role: role.to_string(),
            content: content.to_string(),
            model: model.map(|s| s.to_string()),
            is_binary: if is_binary { Some(true) } else { None },
            tool_calls: None,
            tool_call_id: None,
            name: None,
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
        let tool_call_info: Vec<ToolCallInfo> = tool_calls
            .into_iter()
            .map(|(id, name, arguments)| ToolCallInfo { id, name, arguments })
            .collect();

        let entry = LogEntry {
            timestamp: Utc::now().to_rfc3339(),
            role: role.to_string(),
            content: content.to_string(),
            model: model.map(|s| s.to_string()),
            is_binary: None,
            tool_calls: Some(tool_call_info),
            tool_call_id: None,
            name: None,
        };
        if let Some(file) = &mut self.file {
            if let Ok(json) = serde_json::to_string(&entry) {
                if let Err(e) = file.write_all(json.as_bytes()).await {
                    eprintln!("[Logging error] {}", e);
                } else if let Err(e) = file.write_all(b"\n").await {
                    eprintln!("[Logging error] {}", e);
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
        let entry = LogEntry {
            timestamp: Utc::now().to_rfc3339(),
            role: "tool".to_string(),
            content: content.to_string(),
            model: None,
            is_binary: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
            name: Some(tool_name.to_string()),
        };
        if let Some(file) = &mut self.file {
            if let Ok(json) = serde_json::to_string(&entry) {
                if let Err(e) = file.write_all(json.as_bytes()).await {
                    eprintln!("[Logging error] {}", e);
                } else if let Err(e) = file.write_all(b"\n").await {
                    eprintln!("[Logging error] {}", e);
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
