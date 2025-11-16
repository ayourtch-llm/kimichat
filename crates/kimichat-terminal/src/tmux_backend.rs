/// Tmux-based terminal backend
use super::backend::{TerminalBackend, SessionInfo, CursorPosition};
use anyhow::{Result, bail};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;

/// Tmux backend using external tmux multiplexer
pub struct TmuxBackend {
    /// Map of session IDs to tmux session names
    session_map: HashMap<String, String>,
    /// Directory for session logs
    log_dir: PathBuf,
    /// Maximum concurrent sessions
    max_sessions: usize,
    /// Process ID for session naming
    pid: u32,
    /// Active capture files
    capture_files: HashMap<String, PathBuf>,
}

impl TmuxBackend {
    /// Create a new tmux backend
    pub fn new(log_dir: PathBuf, max_sessions: usize) -> Result<Self> {
        // Verify tmux is available
        let output = Command::new("tmux").arg("-V").output()?;
        if !output.status.success() {
            bail!("tmux command failed - ensure tmux is installed and working");
        }

        // Create log directory
        std::fs::create_dir_all(&log_dir)?;

        let pid = std::process::id();

        Ok(Self {
            session_map: HashMap::new(),
            log_dir,
            max_sessions,
            pid,
            capture_files: HashMap::new(),
        })
    }

    /// Generate tmux session name from session ID
    /// Format: kimichat-{pid}-{session_id}
    fn tmux_session_name(&self, session_id: &str) -> String {
        format!("kimichat-{}-{}", self.pid, session_id)
    }

    /// Check if a tmux session exists
    fn tmux_session_exists(&self, tmux_name: &str) -> bool {
        Command::new("tmux")
            .args(["has-session", "-t", tmux_name])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Run a tmux command and return stdout
    fn run_tmux_command(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("tmux")
            .args(args)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("tmux command failed: {}", stderr);
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Get the pane ID for a tmux session
    fn get_pane_id(&self, tmux_name: &str) -> Result<String> {
        let output = self.run_tmux_command(&[
            "display-message",
            "-t",
            tmux_name,
            "-p",
            "#{pane_id}",
        ])?;
        Ok(output.trim().to_string())
    }
}

#[async_trait]
impl TerminalBackend for TmuxBackend {
    async fn launch_session(
        &mut self,
        id: String,
        command: String,
        rows: u16,
        cols: u16,
        working_dir: Option<String>,
    ) -> Result<String> {
        // Check session limit
        if self.session_map.len() >= self.max_sessions {
            bail!(
                "Maximum number of terminal sessions ({}) reached",
                self.max_sessions
            );
        }

        let tmux_name = self.tmux_session_name(&id);

        // Create new tmux session with clean environment
        let mut cmd = Command::new("tmux");
        cmd.args(["new-session", "-d", "-s", &tmux_name]);

        // Set size
        cmd.args(["-x", &cols.to_string(), "-y", &rows.to_string()]);

        // Set working directory if provided
        if let Some(ref dir) = working_dir {
            cmd.args(["-c", dir]);
        }

        // Set the command to run (or default shell)
        if !command.is_empty() && command != "default shell" {
            cmd.arg(command);
        }

        let output = cmd.output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to create tmux session: {}", stderr);
        }

        // Store session mapping
        self.session_map.insert(id.clone(), tmux_name);

        Ok(id)
    }

    async fn send_keys(&mut self, session_id: &str, keys: &str) -> Result<()> {
        let tmux_name = self.session_map.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Send keys to tmux session
        // Note: tmux send-keys automatically handles special sequences
        let output = Command::new("tmux")
            .args(["send-keys", "-t", tmux_name, keys])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to send keys: {}", stderr);
        }

        Ok(())
    }

    async fn get_screen(&self, session_id: &str, include_colors: bool, _include_cursor: bool) -> Result<String> {
        let tmux_name = self.session_map.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Capture pane contents
        let mut args = vec!["capture-pane", "-t", tmux_name, "-p"];

        // Include ANSI color codes if requested
        if include_colors {
            args.push("-e");
        }

        let output = self.run_tmux_command(&args)?;
        Ok(output)
    }

    async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let mut sessions = Vec::new();

        for (session_id, tmux_name) in &self.session_map {
            if !self.tmux_session_exists(tmux_name) {
                continue;
            }

            // Get session information from tmux
            let info = self.run_tmux_command(&[
                "display-message",
                "-t",
                tmux_name,
                "-p",
                "#{pane_width},#{pane_height},#{pane_current_command},#{pane_current_path}",
            ])?;

            let parts: Vec<&str> = info.trim().split(',').collect();
            if parts.len() >= 4 {
                let cols = parts[0].parse().unwrap_or(80);
                let rows = parts[1].parse().unwrap_or(24);
                let command = parts[2].to_string();
                let working_dir = parts[3].to_string();

                sessions.push(SessionInfo {
                    id: session_id.clone(),
                    command,
                    created_at: SystemTime::now(), // tmux doesn't track creation time easily
                    rows,
                    cols,
                    working_dir: Some(working_dir),
                    status: "Running".to_string(), // tmux sessions are always running
                });
            }
        }

        Ok(sessions)
    }

    async fn kill_session(&mut self, session_id: &str) -> Result<()> {
        let tmux_name = self.session_map.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?
            .clone();

        // Kill tmux session
        let output = Command::new("tmux")
            .args(["kill-session", "-t", &tmux_name])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to kill session: {}", stderr);
        }

        // Remove from tracking
        self.session_map.remove(session_id);
        self.capture_files.remove(session_id);

        Ok(())
    }

    async fn get_cursor_position(&self, session_id: &str) -> Result<CursorPosition> {
        let tmux_name = self.session_map.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Get cursor position from tmux
        let info = self.run_tmux_command(&[
            "display-message",
            "-t",
            tmux_name,
            "-p",
            "#{cursor_y},#{cursor_x}",
        ])?;

        let parts: Vec<&str> = info.trim().split(',').collect();
        if parts.len() >= 2 {
            let row = parts[0].parse().unwrap_or(0);
            let col = parts[1].parse().unwrap_or(0);
            Ok(CursorPosition { row, col })
        } else {
            Ok(CursorPosition { row: 0, col: 0 })
        }
    }

    async fn resize_session(&mut self, session_id: &str, rows: u16, cols: u16) -> Result<()> {
        let tmux_name = self.session_map.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Resize tmux window
        let output = Command::new("tmux")
            .args([
                "resize-window",
                "-t",
                tmux_name,
                "-x",
                &cols.to_string(),
                "-y",
                &rows.to_string(),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to resize session: {}", stderr);
        }

        Ok(())
    }

    async fn get_scrollback(&self, session_id: &str, lines: usize) -> Result<Option<String>> {
        let tmux_name = self.session_map.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Capture scrollback from tmux
        let output = self.run_tmux_command(&[
            "capture-pane",
            "-t",
            tmux_name,
            "-p",
            "-S",
            &format!("-{}", lines),
        ])?;

        Ok(Some(output))
    }

    async fn set_scrollback(&mut self, session_id: &str, lines: usize) -> Result<()> {
        let tmux_name = self.session_map.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Set history limit for tmux pane
        let output = Command::new("tmux")
            .args([
                "set-option",
                "-t",
                tmux_name,
                "history-limit",
                &lines.to_string(),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to set scrollback: {}", stderr);
        }

        Ok(())
    }

    async fn capture_start(&mut self, session_id: &str, _output_file: String) -> Result<()> {
        let tmux_name = self.session_map.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Generate capture file path
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let capture_file = self.log_dir.join(format!("session_{}_capture_{}.log", session_id, timestamp));

        // Start tmux pipe-pane to capture output
        let output = Command::new("tmux")
            .args([
                "pipe-pane",
                "-t",
                tmux_name,
                "-o",
                &format!("cat >> {}", capture_file.display()),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to start capture: {}", stderr);
        }

        self.capture_files.insert(session_id.to_string(), capture_file);
        Ok(())
    }

    async fn capture_stop(&mut self, session_id: &str) -> Result<(String, usize, f64)> {
        let tmux_name = self.session_map.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        let capture_file = self.capture_files.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("No active capture for session: {}", session_id))?
            .clone();

        // Stop tmux pipe-pane
        let output = Command::new("tmux")
            .args(["pipe-pane", "-t", tmux_name])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to stop capture: {}", stderr);
        }

        // Get file size
        let bytes = std::fs::metadata(&capture_file)
            .map(|m| m.len() as usize)
            .unwrap_or(0);

        // Duration is not tracked - return 0
        let duration = 0.0;

        self.capture_files.remove(session_id);

        Ok((capture_file.display().to_string(), bytes, duration))
    }

    async fn session_exists(&self, session_id: &str) -> bool {
        if let Some(tmux_name) = self.session_map.get(session_id) {
            self.tmux_session_exists(tmux_name)
        } else {
            false
        }
    }

    fn backend_name(&self) -> &str {
        "tmux"
    }
}
