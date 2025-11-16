// Terminal module
//
// Re-exports terminal functionality from kimichat-terminal and kimichat-tools crates.

// Re-export core types from kimichat-terminal
pub use kimichat_terminal::{TerminalManager, TerminalBackendType, MAX_CONCURRENT_SESSIONS};

// Re-export terminal tools from kimichat-tools
pub use kimichat_tools::{
    PtyLaunchTool, PtySendKeysTool, PtyGetScreenTool,
    PtyListTool, PtyKillTool, PtyGetCursorTool,
    PtyResizeTool, PtySetScrollbackTool,
    PtyStartCaptureTool, PtyStopCaptureTool,
    PtyRequestUserInputTool,
};
