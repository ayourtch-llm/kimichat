// Models module - data structures for API communication
pub mod types;
pub mod requests;
pub mod responses;

// Re-export commonly used types
pub use types::{
    ModelType, Message, ToolCall, FunctionCall,
    SwitchModelArgs,
};
pub use requests::{ChatRequest, Tool, FunctionDef};
pub use responses::{
    ChatResponse, Usage,
    StreamChunk,
};
