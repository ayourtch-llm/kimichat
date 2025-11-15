use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use anyhow::{Result, Context};
use portable_pty::{CommandBuilder, PtySize, PtySystem, native_pty_system};

/// Handles PTY process management
pub struct PtyHandler {
    pub(crate) pty: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
}

impl PtyHandler {
    /// Create a new PTY handler
    pub fn new(command: &str, working_dir: &PathBuf, cols: u16, rows: u16) -> Result<Self> {
        let pty_system = native_pty_system();

        // Create PTY with specified size
        let pty_pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to open PTY")?;

        let mut pty = pty_pair.master;
        let slave = pty_pair.slave;

        // Build command
        let mut cmd = CommandBuilder::new(command);
        cmd.cwd(working_dir);

        // Spawn child process
        let child = slave
            .spawn_command(cmd)
            .context("Failed to spawn command in PTY")?;

        // Get reader and writer from PTY
        let reader = pty.try_clone_reader()
            .context("Failed to clone PTY reader")?;
        let writer = pty.take_writer()
            .context("Failed to take PTY writer")?;

        Ok(Self {
            pty,
            child,
            reader,
            writer,
        })
    }

    /// Write data to PTY (send keys)
    pub fn write(&mut self, data: &str, special: bool) -> Result<()> {
        let processed = if special {
            self.process_special_keys(data)
        } else {
            data.to_string()
        };

        self.writer.write_all(processed.as_bytes())
            .context("Failed to write to PTY")?;
        self.writer.flush()
            .context("Failed to flush PTY writer")?;

        Ok(())
    }

    /// Read available data from PTY with timeout
    /// This handles the case where output doesn't end with newline (like shell prompts)
    /// Returns whatever data is available within the timeout period
    ///
    /// Uses polling approach to avoid orphaned threads that steal data
    pub fn read(&mut self) -> Result<String> {
        use std::io::ErrorKind;
        use std::time::{Duration, Instant};

        let mut accumulated = Vec::new();
        let mut buffer = vec![0u8; 4096];
        let start = Instant::now();
        let timeout_duration = Duration::from_millis(150); // Total time to wait for data
        let poll_interval = Duration::from_millis(10); // Check every 10ms

        loop {
            match self.reader.read(&mut buffer) {
                Ok(0) => {
                    // EOF - return what we have
                    break;
                }
                Ok(n) => {
                    // Got data - accumulate it
                    accumulated.extend_from_slice(&buffer[..n]);

                    // Keep reading while data is available (non-blocking)
                    // This handles cases where data arrives in multiple chunks
                    // But don't wait too long - if we got some data, check a few more times
                    // then return it so the terminal can be responsive
                    if accumulated.len() > 0 && start.elapsed() > Duration::from_millis(50) {
                        // We have some data and waited a bit for more - good enough
                        break;
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                    // No data available right now
                    if !accumulated.is_empty() {
                        // We already got some data earlier, return it
                        break;
                    }

                    // No data yet - check if we should keep waiting
                    if start.elapsed() >= timeout_duration {
                        // Timeout reached, return what we have (might be empty)
                        break;
                    }

                    // Wait a bit before next poll
                    std::thread::sleep(poll_interval);
                }
                Err(e) => {
                    // Real error
                    return Err(e).context("Failed to read from PTY");
                }
            }
        }

        Ok(String::from_utf8_lossy(&accumulated).to_string())
    }

    /// Resize the PTY
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.pty.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("Failed to resize PTY")?;
        Ok(())
    }

    /// Try to wait for child process (non-blocking)
    pub fn try_wait(&mut self) -> Option<i32> {
        self.child.try_wait()
            .ok()
            .flatten()
            .map(|status| status.exit_code() as i32)
    }

    /// Kill the child process
    pub fn kill(&mut self) -> Result<()> {
        self.child.kill()
            .context("Failed to kill child process")?;
        Ok(())
    }

    /// Process special key sequences
    fn process_special_keys(&self, input: &str) -> String {
        let mut result = String::new();
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '^' => {
                    // Control key sequence
                    if let Some(&next_ch) = chars.peek() {
                        chars.next();
                        let ctrl_char = match next_ch.to_ascii_uppercase() {
                            'A'..='Z' => {
                                // Ctrl+A is 0x01, Ctrl+B is 0x02, etc.
                                ((next_ch.to_ascii_uppercase() as u8) - b'A' + 1) as char
                            }
                            '@' => '\x00',
                            '[' => '\x1b',
                            '\\' => '\x1c',
                            ']' => '\x1d',
                            '^' => '\x1e',
                            '_' => '\x1f',
                            _ => {
                                // Not a valid control sequence, output as-is
                                result.push('^');
                                result.push(next_ch);
                                continue;
                            }
                        };
                        result.push(ctrl_char);
                    } else {
                        result.push(ch);
                    }
                }
                '[' => {
                    // Check for special key sequences like [UP], [DOWN], etc.
                    let rest: String = chars.clone().collect();
                    if let Some((key_seq, len)) = self.parse_special_key(&rest) {
                        result.push_str(&key_seq);
                        // Skip the characters we consumed
                        for _ in 0..len {
                            chars.next();
                        }
                    } else {
                        result.push(ch);
                    }
                }
                _ => result.push(ch),
            }
        }

        result
    }

    /// Parse special key sequences like [UP], [F1], etc.
    /// Returns (escape sequence, length consumed from input)
    fn parse_special_key(&self, input: &str) -> Option<(String, usize)> {
        let patterns = [
            ("UP]", "\x1b[A", 3),
            ("DOWN]", "\x1b[B", 5),
            ("RIGHT]", "\x1b[C", 6),
            ("LEFT]", "\x1b[D", 5),
            ("HOME]", "\x1b[H", 5),
            ("END]", "\x1b[F", 4),
            ("PGUP]", "\x1b[5~", 5),
            ("PGDN]", "\x1b[6~", 5),
            ("INSERT]", "\x1b[2~", 7),
            ("DELETE]", "\x1b[3~", 7),
            ("F1]", "\x1bOP", 3),
            ("F2]", "\x1bOQ", 3),
            ("F3]", "\x1bOR", 3),
            ("F4]", "\x1bOS", 3),
            ("F5]", "\x1b[15~", 3),
            ("F6]", "\x1b[17~", 3),
            ("F7]", "\x1b[18~", 3),
            ("F8]", "\x1b[19~", 3),
            ("F9]", "\x1b[20~", 3),
            ("F10]", "\x1b[21~", 4),
            ("F11]", "\x1b[23~", 4),
            ("F12]", "\x1b[24~", 4),
        ];

        for (pattern, escape_seq, len) in patterns {
            if input.starts_with(pattern) {
                return Some((escape_seq.to_string(), len));
            }
        }

        None
    }
}
