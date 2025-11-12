// Chat module - conversation state management, history, and session handling
pub mod state;

// Re-export commonly used items
pub use state::{ChatState, save_state, load_state};
