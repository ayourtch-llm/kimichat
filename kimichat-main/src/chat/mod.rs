// Chat module - conversation state management, history, and session handling
pub mod state;
pub mod history;
pub mod session;

// Re-export commonly used items
pub use state::{save_state, load_state};
pub use history::{calculate_conversation_size, get_max_session_size, should_compact_session, intelligent_compaction};

// Include test module
#[cfg(test)]
mod tests;
