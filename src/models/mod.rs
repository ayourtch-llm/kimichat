// Models module - data structures for API communication
pub mod types;
pub mod requests;
pub mod responses;

// Re-export commonly used types
pub use types::{
    ModelType, Message, ToolCall, FunctionCall,
    ReadFileArgs, WriteFileArgs, ListFilesArgs, EditFileArgs,
    SwitchModelArgs, RunCommandArgs, SearchFilesArgs, OpenFileArgs,
};
pub use requests::{ChatRequest, Tool, FunctionDef};
pub use responses::{
    ChatResponse, Choice, Usage,
    StreamChunk, StreamChoice, StreamDelta, StreamToolCallDelta, StreamFunctionDelta,
};
