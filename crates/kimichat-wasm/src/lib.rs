use wasm_bindgen::prelude::*;
use web_sys::{Window, Document};

mod protocol;
mod websocket;
mod dom;
mod session_list;
mod chat_ui;
mod markdown;
mod utils;

pub use protocol::*;

/// Initialize the WASM application
/// This sets up panic hooks and logging
#[wasm_bindgen(start)]
pub fn init() {
    // Set panic hook for better error messages
    console_error_panic_hook::set_once();

    // Initialize logging
    wasm_logger::init(wasm_logger::Config::default());

    log::info!("KimiChat WASM initialized");
}

/// Initialize the session list page
#[wasm_bindgen]
pub async fn init_session_list() -> Result<(), JsValue> {
    log::info!("Initializing session list page");
    session_list::SessionListApp::new()?.start().await
}

/// Initialize the chat session page
#[wasm_bindgen]
pub async fn init_chat_session(session_id: String) -> Result<(), JsValue> {
    log::info!("Initializing chat session: {}", session_id);
    chat_ui::ChatApp::new(session_id)?.start().await
}

/// Get the window object
fn window() -> Result<Window, JsValue> {
    web_sys::window().ok_or_else(|| JsValue::from_str("No window object"))
}

/// Get the document object
fn document() -> Result<Document, JsValue> {
    window()?
        .document()
        .ok_or_else(|| JsValue::from_str("No document object"))
}
