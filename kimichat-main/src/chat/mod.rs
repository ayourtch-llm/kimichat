// Chat module - conversation state management, history, and session handling
pub mod state;
pub mod history;
pub mod session;

// Re-export commonly used items
pub use state::{save_state, load_state};
