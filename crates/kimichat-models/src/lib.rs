// Models module - data structures for API communication
pub mod requests;
pub mod responses;
pub mod types;

#[cfg(test)]
mod tests;

// Re-export commonly used types
pub use requests::{ChatRequest, FunctionDef, Tool};
pub use responses::{ChatResponse, StreamChunk, Usage};
pub use types::{FunctionCall, Message, ModelColor, SwitchModelArgs, ToolCall};
