// Chat module - conversation state management, history, and session handling
pub mod state;
pub mod history;

// Re-export commonly used items
pub use state::{ChatState, save_state, load_state};
