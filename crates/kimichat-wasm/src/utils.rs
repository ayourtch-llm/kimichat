use wasm_bindgen::JsValue;

/// Get the current protocol (ws or wss) based on the page protocol
pub fn get_ws_protocol() -> String {
    let location = web_sys::window()
        .and_then(|w| w.location().protocol().ok())
        .unwrap_or_else(|| "http:".to_string());

    if location == "https:" {
        "wss:".to_string()
    } else {
        "ws:".to_string()
    }
}

/// Get the current host
pub fn get_host() -> Result<String, JsValue> {
    web_sys::window()
        .and_then(|w| w.location().host().ok())
        .ok_or_else(|| JsValue::from_str("Failed to get host"))
}

/// Build WebSocket URL for a session
pub fn build_ws_url(session_id: &str) -> Result<String, JsValue> {
    let protocol = get_ws_protocol();
    let host = get_host()?;
    Ok(format!("{}//{}/ws/{}", protocol, host, session_id))
}

/// Format timestamp to human-readable string
pub fn format_time(timestamp: &str) -> String {
    // For simplicity, just return the timestamp as-is
    // In a real implementation, you might want to parse and format it
    timestamp.to_string()
}

/// Escape HTML to prevent XSS
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Get session ID from URL path
pub fn get_session_id_from_url() -> Result<String, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;
    let location = window.location();
    let pathname = location.pathname()
        .map_err(|_| JsValue::from_str("Failed to get pathname"))?;

    // Expected format: /session/{session_id}
    let parts: Vec<&str> = pathname.split('/').collect();
    if parts.len() >= 3 && parts[1] == "session" {
        Ok(parts[2].to_string())
    } else {
        Err(JsValue::from_str("Invalid session URL"))
    }
}
