// Terminal tool wrappers
//
// This module provides LLM-accessible tool wrappers for terminal functionality.
// The core terminal implementation is in the kimichat-terminal crate.

mod tools;

// Re-export tools
pub use tools::{
    PtyLaunchTool, PtySendKeysTool, PtyGetScreenTool,
    PtyListTool, PtyKillTool, PtyGetCursorTool,
    PtyResizeTool, PtySetScrollbackTool,
    PtyStartCaptureTool, PtyStopCaptureTool,
    PtyRequestUserInputTool,
};

// Re-export core types from kimichat-terminal
pub use kimichat_terminal::{TerminalManager, TerminalBackendType, MAX_CONCURRENT_SESSIONS};
