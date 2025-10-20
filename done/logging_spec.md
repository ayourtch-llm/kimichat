# Specification: Conversation Logging for `kimi-chat`

## Overview
Add a feature that records every exchanged message (user, assistant, system, tool) to a **JSONL** (JSON Lines) file. Each line represents a single message object. The file name must include the timestamp of the chat session start in the format:

```
kchat-YYYY-MM-DD-HHMMSS.jsonl
```

The logging should be performed **automatically** by the application without requiring user intervention.

---

## Goals
1. **Persist conversation history** for later analysis, debugging, or replay.
2. **Human‑readable** yet machine‑parseable format (JSONL).
3. Minimal performance impact and no change to existing CLI behaviour.
4. Compatibility with the existing `workspace/` directory handling.
5. Ensure log files are created **once per session** and appended to throughout the session.

---

## Functional Requirements
| ID | Description |
|----|-------------|
| FR‑1 | On application start, generate a log file name `kchat-<timestamp>.jsonl` where `<timestamp>` is the UTC time of start formatted as `YYYY-MM-DD-HHMMSS`. |
| FR‑2 | Create the log file inside the **workspace directory** (`workspace/`). If the directory does not exist, create it. |
| FR‑3 | For every message that is added to the conversation history (including tool calls and system messages), append a JSON object on a new line to the log file. |
| FR‑4 | The JSON object must contain at least the following fields: `timestamp` (ISO‑8601 UTC), `role` (e.g., `user`, `assistant`, `system`, `tool`), `content` (raw text), and `model` (the model used for that turn, if applicable). |
| FR‑5 | If a message contains binary data (e.g., file content), store it as a **base64‑encoded** string in the `content` field and set an additional field `is_binary: true`. |
| FR‑6 | Provide a **graceful fallback**: if writing to the file fails (e.g., permission error), log the error to stderr but do not crash the application. |
| FR‑7 | When the user exits the CLI (Ctrl‑C or `exit` command), close the file handle cleanly. |

---

## Non‑Functional Requirements
- **Performance**: Logging must be asynchronous or buffered to avoid blocking the main event loop.
- **Reliability**: Use `tokio::fs::OpenOptions` with `append` mode and `create_new` for the first write.
- **Safety**: All file paths must be resolved relative to the workspace directory to prevent directory‑traversal attacks.
- **Portability**: Works on Windows, macOS, and Linux.

---

## Design & Implementation Details
### 1. New Module: `src/logging.rs`
Create a small module that encapsulates all logging logic.
```rust
use chrono::{Utc, DateTime};
use serde::Serialize;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct LogEntry {
    timestamp: String, // ISO‑8601 UTC
    role: String,
    content: String,
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_binary: Option<bool>,
}

pub struct ConversationLogger {
    file_path: PathBuf,
    file: Option<tokio::fs::File>,
}

impl ConversationLogger {
    /// Create a new logger; generates the file name based on the current UTC time.
    pub async fn new(workspace: &Path) -> anyhow::Result<Self> {
        // Ensure workspace exists
        fs::create_dir_all(workspace).await?;
        let now: DateTime<Utc> = Utc::now();
        let filename = format!(
            "kchat-{}.jsonl",
            now.format("%Y-%m-%d-%H%M%S")
        );
        let file_path = workspace.join(filename);
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
        };
        if let Some(file) = &mut self.file {
            if let Ok(json) = serde_json::to_string(&entry) {
                if let Err(e) = file.write_all(json.as_bytes()).await
                    .and_then(|_| file.write_all(b"\n").await) {
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
```

### 2. Integration Points
- **`main.rs`**: Instantiate the logger early (after parsing CLI args) using the workspace path (`workspace/`). Store it in a `Arc<Mutex<ConversationLogger>>` so it can be accessed from async tasks.
- **Message Insertion**: Wherever the code pushes a new `Message` into the conversation history (e.g., after user input, after assistant response, after tool result), call `logger.log(...)` with appropriate fields.
- **Tool Calls**: For tool‑generated messages (`role: "tool"`), set `is_binary` based on the tool payload.
- **Shutdown Hook**: Register a handler for `Ctrl+C` (`tokio::signal::ctrl_c`) that calls `logger.shutdown().await` before exiting.

### 3. Dependency Updates
Add to `Cargo.toml`:
```toml
[dependencies]
chrono = { version = "0.4", features = ["serde", "alloc"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```
(If `chrono` is already present, ensure the `serde` feature is enabled.)

### 4. Error Handling Strategy
- All logger methods return `Result` internally, but the public `log` function swallows errors after printing to `stderr` to avoid breaking the main flow.
- During initialization, if the logger cannot be created, the application should **continue** but print a warning: *"Conversation logging disabled – failed to create log file: <error>"*.

### 5. Testing Plan
1. **Unit Tests** for `ConversationLogger::new` – verify filename format and file creation.
2. **Integration Test**: Simulate a short session, ensure the generated JSONL file contains a line for each message with correct fields.
3. **Failure Simulation**: Change permissions of the workspace directory to read‑only and confirm the app logs a warning without panicking.

---

## Migration Steps
1. **Add `logging.rs`** module.
2. **Update `main.rs`** to create a logger instance and share it.
3. Insert `logger.log(...)` calls after each point where a `Message` is pushed to the conversation history.
4. Add the required dependencies to `Cargo.toml`.
5. Run `cargo test` and `cargo run` to verify functionality.
6. Update README with a brief description of the new logging feature.

---

## Documentation Update
- Add a **Logging** section in `README.md` describing where logs are stored and how to parse them.
- Mention the timestamp format and that logs are JSONL, suitable for tools like `jq`.

---

## Example Log Entry
```json
{"timestamp":"2025-10-19T12:34:56Z","role":"assistant","content":"Here is the answer...","model":"Kimi-K2-Instruct-0905"}
```
If the content is binary:
```json
{"timestamp":"2025-10-19T12:35:00Z","role":"tool","content":"aGVsbG8gd29ybGQ=","model":null,"is_binary":true}
```
---

*Prepared by GPT‑OSS‑120B based on the project context.*