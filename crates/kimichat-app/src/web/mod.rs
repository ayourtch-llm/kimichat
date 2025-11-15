// Web frontend module
pub mod protocol;
pub mod session_manager;
pub mod routes;
pub mod server;

pub use protocol::{ClientMessage, ServerMessage, SessionInfo};
pub use session_manager::{SessionManager, SessionId, SessionType};
pub use server::WebServer;
