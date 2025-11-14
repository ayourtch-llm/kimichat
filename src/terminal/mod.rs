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
mod tools;
pub mod backend;
mod pty_backend;

// Re-export public API
pub use manager::TerminalManager;
pub use session::{TerminalSession, SessionId, SessionMetadata, SessionStatus};
pub use screen_buffer::ScreenBuffer;
pub use logger::SessionLogger;
pub use backend::{TerminalBackend, TerminalBackendType, SessionInfo, CursorPosition};
pub use pty_backend::PtyBackend;
pub use tools::{
    PtyLaunchTool, PtySendKeysTool, PtyGetScreenTool,
    PtyListTool, PtyKillTool, PtyGetCursorTool,
    PtyResizeTool, PtySetScrollbackTool,
    PtyStartCaptureTool, PtyStopCaptureTool,
    PtyRequestUserInputTool,
};

// Constants
pub const MAX_CONCURRENT_SESSIONS: usize = 15;
pub const DEFAULT_SCROLLBACK_LINES: usize = 1000;
pub const USER_INPUT_TIMEOUT_SECS: u64 = 300; // 5 minutes
