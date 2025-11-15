use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use anyhow::{Result, Context};
use chrono::{DateTime, Utc};
use serde_json::json;

use super::session::SessionId;

/// Session logger for PTY I/O and events
pub struct SessionLogger {
    session_id: SessionId,
    log_file: File,
    meta_file: File,
    capture_file: Option<File>,
    capture_start: Option<DateTime<Utc>>,
    capture_bytes: u64,
}

impl SessionLogger {
    /// Create a new session logger
    pub fn new(session_id: SessionId, log_dir: PathBuf) -> Result<Self> {
        // Ensure log directory exists
        std::fs::create_dir_all(&log_dir)
            .context("Failed to create log directory")?;

        // Create log files
        let log_path = log_dir.join(format!("session-{}.log", session_id));
        let meta_path = log_dir.join(format!("session-{}-meta.json", session_id));

        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .context("Failed to create log file")?;

        let meta_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&meta_path)
            .context("Failed to create metadata file")?;

        Ok(Self {
            session_id,
            log_file,
            meta_file,
            capture_file: None,
            capture_start: None,
            capture_bytes: 0,
        })
    }

    /// Log input to PTY
    pub fn log_input(&mut self, data: &str) -> Result<()> {
        self.log_event("in", data)
    }

    /// Log output from PTY
    pub fn log_output(&mut self, data: &str) -> Result<()> {
        self.log_event("out", data)?;

        // Also write to capture file if capturing
        if let Some(ref mut capture_file) = self.capture_file {
            let entry = json!({
                "timestamp": Utc::now().to_rfc3339(),
                "data": data,
            });
            writeln!(capture_file, "{}", entry.to_string())
                .context("Failed to write to capture file")?;
            capture_file.flush()?;
            self.capture_bytes += data.len() as u64;
        }

        Ok(())
    }

    /// Log resize event
    pub fn log_resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        let entry = json!({
            "timestamp": Utc::now().to_rfc3339(),
            "session_id": self.session_id,
            "event": "resize",
            "cols": cols,
            "rows": rows,
        });

        writeln!(self.log_file, "{}", entry.to_string())
            .context("Failed to write to log file")?;
        self.log_file.flush()?;

        Ok(())
    }

    /// Start capturing output to a separate file
    pub fn start_capture(&mut self) -> Result<PathBuf> {
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let capture_path = PathBuf::from(format!(
            "logs/terminals/session-{}-capture-{}.log",
            self.session_id, timestamp
        ));

        // Ensure directory exists
        if let Some(parent) = capture_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create capture directory")?;
        }

        let capture_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&capture_path)
            .context("Failed to create capture file")?;

        self.capture_file = Some(capture_file);
        self.capture_start = Some(Utc::now());
        self.capture_bytes = 0;

        // Log capture start
        self.log_capture_event("start", &capture_path)?;

        Ok(capture_path)
    }

    /// Stop capturing output
    pub fn stop_capture(&mut self) -> Result<(PathBuf, u64, f64)> {
        let capture_start = self.capture_start
            .ok_or_else(|| anyhow::anyhow!("Capture not started"))?;

        let duration = Utc::now()
            .signed_duration_since(capture_start)
            .num_milliseconds() as f64 / 1000.0;

        let bytes = self.capture_bytes;

        // Get capture file path before closing
        let capture_path = PathBuf::from(format!(
            "logs/terminals/session-{}-capture-{}.log",
            self.session_id,
            capture_start.format("%Y%m%d-%H%M%S")
        ));

        // Close capture file
        self.capture_file = None;
        self.capture_start = None;
        self.capture_bytes = 0;

        // Log capture stop
        self.log_capture_event("stop", &capture_path)?;

        Ok((capture_path, bytes, duration))
    }

    /// Log a generic event
    fn log_event(&mut self, direction: &str, data: &str) -> Result<()> {
        let entry = json!({
            "timestamp": Utc::now().to_rfc3339(),
            "session_id": self.session_id,
            "direction": direction,
            "data": data,
        });

        writeln!(self.log_file, "{}", entry.to_string())
            .context("Failed to write to log file")?;
        self.log_file.flush()?;

        Ok(())
    }

    /// Log capture start/stop event
    fn log_capture_event(&mut self, event: &str, capture_path: &PathBuf) -> Result<()> {
        let entry = json!({
            "timestamp": Utc::now().to_rfc3339(),
            "session_id": self.session_id,
            "event": format!("capture_{}", event),
            "capture_file": capture_path.to_string_lossy(),
        });

        writeln!(self.log_file, "{}", entry.to_string())
            .context("Failed to write to log file")?;
        self.log_file.flush()?;

        Ok(())
    }

    /// Write metadata
    pub fn write_metadata(&mut self, metadata: &serde_json::Value) -> Result<()> {
        let json_str = serde_json::to_string_pretty(metadata)
            .context("Failed to serialize metadata")?;

        self.meta_file.set_len(0)?;
        self.meta_file.write_all(json_str.as_bytes())
            .context("Failed to write metadata")?;
        self.meta_file.flush()?;

        Ok(())
    }
}
