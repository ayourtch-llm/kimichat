// Terminal session management module
//
// This module provides stateful PTY terminal session support with full VT100/ANSI
// escape sequence interpretation, allowing LLMs to launch, interact with, and monitor
// terminal sessions.

mod manager;
mod session;
mod pty_handler;
mod screen_buffer;
mod logger;
pub mod backend;
mod pty_backend;
mod tmux_backend;

// Re-export public API
pub use manager::TerminalManager;
pub use backend::TerminalBackendType;

// Constants
pub const MAX_CONCURRENT_SESSIONS: usize = 15;
pub const DEFAULT_SCROLLBACK_LINES: usize = 1000;
pub const USER_INPUT_TIMEOUT_SECS: u64 = 300; // 5 minutes
