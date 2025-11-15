use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};

use super::pty_handler::PtyHandler;
use super::screen_buffer::ScreenBuffer;
use super::logger::SessionLogger;
use super::DEFAULT_SCROLLBACK_LINES;

/// Session ID type
pub type SessionId = u32;

/// Terminal session status
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SessionStatus {
    Running,
    Stopped,
    Exited(i32),
}

/// Session metadata
#[derive(Debug, Clone)]
pub struct SessionMetadata {
    pub id: SessionId,
    pub created_at: DateTime<Utc>,
    pub command: String,
    pub working_dir: PathBuf,
    pub status: SessionStatus,
    pub size: (u16, u16),
}

/// Represents a single terminal session
pub struct TerminalSession {
    id: SessionId,
    pty_handler: PtyHandler,
    screen_buffer: ScreenBuffer,
    scrollback_lines: usize,
    capture_enabled: bool,
    capture_file: Option<PathBuf>,
    logger: SessionLogger,
    metadata: SessionMetadata,
    // Background reader thread
    reader_thread: Option<JoinHandle<()>>,
    reader_stop_flag: Arc<AtomicBool>,
}

impl TerminalSession {
    /// Create a new terminal session
    pub fn new(
        id: SessionId,
        command: Option<String>,
        working_dir: Option<PathBuf>,
        cols: Option<u16>,
        rows: Option<u16>,
        log_dir: PathBuf,
    ) -> Result<Self> {
        let cols = cols.unwrap_or(80);
        let rows = rows.unwrap_or(24);

        let working_dir = working_dir.unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
        });

        // Determine shell command
        let command = command.unwrap_or_else(|| {
            if cfg!(windows) {
                "cmd.exe".to_string()
            } else {
                std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
            }
        });

        // Create PTY handler
        let pty_handler = PtyHandler::new(&command, &working_dir, cols, rows)?;

        // Create screen buffer
        let screen_buffer = ScreenBuffer::new(cols, rows);

        // Create logger
        let logger = SessionLogger::new(id, log_dir)?;

        let metadata = SessionMetadata {
            id,
            created_at: Utc::now(),
            command: command.clone(),
            working_dir: working_dir.clone(),
            status: SessionStatus::Running,
            size: (cols, rows),
        };

        Ok(Self {
            id,
            pty_handler,
            screen_buffer,
            scrollback_lines: DEFAULT_SCROLLBACK_LINES,
            capture_enabled: false,
            capture_file: None,
            logger,
            metadata,
            reader_thread: None,
            reader_stop_flag: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Start background reader thread to continuously update screen buffer
    /// This should be called after the session is wrapped in Arc<Mutex>
    pub fn start_background_reader(session_arc: Arc<Mutex<Self>>) -> Result<()> {
        let stop_flag = {
            let session = session_arc.lock().unwrap();
            Arc::clone(&session.reader_stop_flag)
        };

        // Clone the PTY reader for the background thread
        let mut pty_reader = {
            let mut session = session_arc.lock().unwrap();
            session.pty_handler.pty.try_clone_reader()
                .map_err(|e| anyhow::anyhow!("Failed to clone PTY reader: {}", e))?
        };

        // Spawn background thread
        let session_clone = Arc::clone(&session_arc);
        let handle = thread::spawn(move || {
            let mut buffer = vec![0u8; 4096];

            loop {
                // Check if we should stop
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }

                // Try to read from PTY
                // This might block, but when PTY is killed it should get EOF or error
                match pty_reader.read(&mut buffer) {
                    Ok(0) => {
                        // EOF - process exited
                        break;
                    }
                    Ok(n) => {
                        // Got data - update screen buffer
                        let data = String::from_utf8_lossy(&buffer[..n]).to_string();

                        if let Ok(mut session) = session_clone.lock() {
                            session.screen_buffer.process_output(&data);
                            let _ = session.logger.log_output(&data); // Ignore logging errors

                            if session.capture_enabled {
                                // TODO: Write to capture file
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // No data available - sleep briefly
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(_) => {
                        // Read error - probably PTY closed
                        break;
                    }
                }
            }
            // Thread finished - no completion signal needed, just exit
        });

        // Store thread handle
        {
            let mut session = session_arc.lock().unwrap();
            session.reader_thread = Some(handle);
        }

        Ok(())
    }

    /// Get session ID
    pub fn id(&self) -> SessionId {
        self.id
    }

    /// Get session metadata
    pub fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    /// Send keys to the terminal
    pub fn send_keys(&mut self, keys: &str, special: bool) -> Result<()> {
        self.pty_handler.write(keys, special)?;
        self.logger.log_input(keys)?;
        Ok(())
    }

    /// Read output from PTY and update screen buffer
    pub fn update_screen(&mut self) -> Result<()> {
        let output = self.pty_handler.read()?;
        if !output.is_empty() {
            self.screen_buffer.process_output(&output);
            self.logger.log_output(&output)?;

            if self.capture_enabled {
                if let Some(ref capture_file) = self.capture_file {
                    // TODO: Write to capture file with timestamp
                }
            }
        }
        Ok(())
    }

    /// Get current screen contents
    pub fn get_screen(&self, include_colors: bool, include_cursor: bool) -> Result<String> {
        Ok(self.screen_buffer.get_contents(include_colors, include_cursor))
    }

    /// Get cursor position
    pub fn get_cursor(&self) -> (u16, u16) {
        self.screen_buffer.cursor_position()
    }

    /// Resize terminal
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.pty_handler.resize(cols, rows)?;
        self.screen_buffer.resize(cols, rows);
        self.metadata.size = (cols, rows);
        self.logger.log_resize(cols, rows)?;
        Ok(())
    }

    /// Set scrollback buffer size
    pub fn set_scrollback(&mut self, lines: usize) -> Result<()> {
        self.scrollback_lines = lines;
        self.screen_buffer.set_scrollback_lines(lines);
        Ok(())
    }

    /// Start capturing output to file
    pub fn start_capture(&mut self) -> Result<PathBuf> {
        if self.capture_enabled {
            bail!("Capture already enabled for session {}", self.id);
        }

        let capture_file = self.logger.start_capture()?;
        self.capture_file = Some(capture_file.clone());
        self.capture_enabled = true;

        Ok(capture_file)
    }

    /// Stop capturing output
    pub fn stop_capture(&mut self) -> Result<(PathBuf, u64, f64)> {
        if !self.capture_enabled {
            bail!("Capture not enabled for session {}", self.id);
        }

        let (file, bytes, duration) = self.logger.stop_capture()?;
        self.capture_enabled = false;
        self.capture_file = None;

        Ok((file, bytes, duration))
    }

    /// Kill the session
    pub fn kill(&mut self) -> Result<()> {
        // Signal background thread to stop
        self.reader_stop_flag.store(true, Ordering::Relaxed);

        // Kill the PTY process - this will cause the reader thread to get EOF
        self.pty_handler.kill()?;
        self.metadata.status = SessionStatus::Stopped;

        // Don't wait for background thread - it might be blocked in read()
        // The thread will exit naturally when it gets EOF from the killed PTY
        // We just drop the handle, allowing the thread to finish asynchronously
        let _ = self.reader_thread.take();

        Ok(())
    }

    /// Check if session is finished
    pub fn is_finished(&self) -> bool {
        matches!(self.metadata.status, SessionStatus::Exited(_) | SessionStatus::Stopped)
    }

    /// Update session status
    pub fn update_status(&mut self) {
        if let Some(exit_code) = self.pty_handler.try_wait() {
            self.metadata.status = SessionStatus::Exited(exit_code);
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        // Signal background thread to stop
        self.reader_stop_flag.store(true, Ordering::Relaxed);

        // Kill the PTY process if not already killed
        let _ = self.pty_handler.kill();

        // Don't wait for thread to finish - it might be blocked
        // Just drop the handle and let it finish asynchronously
        let _ = self.reader_thread.take();
    }
}
