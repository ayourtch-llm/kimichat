use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    api::call_api,
    models::Message as ChatMessage,
    web::{
        protocol::{ClientMessage, ServerMessage, SessionConfig, SessionId, SessionInfo},
        session_manager::SessionManager,
    },
    KimiChat,
};

/// Application state shared across routes
#[derive(Clone)]
pub struct AppState {
    pub session_manager: Arc<SessionManager>,
}

/// Create router with all routes
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // API routes
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route(
            "/api/sessions/:id",
            get(get_session_details).delete(close_session),
        )
        // WebSocket endpoint
        .route("/ws/:session_id", get(websocket_handler))
        // Static files (HTML pages)
        .route("/", get(serve_index))
        .route("/session/:id", get(serve_session))
        .with_state(state)
}

/// GET /api/sessions - List all active sessions
async fn list_sessions(State(state): State<AppState>) -> Json<serde_json::Value> {
    let sessions = state.session_manager.list_sessions().await;
    Json(serde_json::json!({ "sessions": sessions }))
}

/// POST /api/sessions - Create a new session
async fn create_session(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let config: SessionConfig = serde_json::from_value(
        payload
            .get("config")
            .cloned()
            .unwrap_or(serde_json::json!({})),
    )?;

    let session_id = state.session_manager.create_session(config).await?;

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "websocket_url": format!("/ws/{}", session_id),
    })))
}

/// GET /api/sessions/:id - Get session details
async fn get_session_details(
    State(state): State<AppState>,
    Path(id): Path<SessionId>,
) -> Result<Json<SessionInfo>, AppError> {
    let session = state
        .session_manager
        .get_session(&id)
        .await
        .ok_or_else(|| AppError::NotFound("Session not found".into()))?;

    Ok(Json(session.get_info().await))
}

/// DELETE /api/sessions/:id - Close a session
async fn close_session(
    State(state): State<AppState>,
    Path(id): Path<SessionId>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.session_manager.remove_session(&id).await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Session closed successfully",
    })))
}

/// GET /ws/:session_id - WebSocket endpoint
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(session_id): Path<SessionId>,
) -> Response {
    ws.on_upgrade(move |socket| handle_websocket(socket, state, session_id))
}

/// Handle WebSocket connection
async fn handle_websocket(socket: WebSocket, state: AppState, session_id: SessionId) {
    let client_id = Uuid::new_v4();

    // Get or verify session exists
    let session = match state.session_manager.get_session(&session_id).await {
        Some(s) => s,
        None => {
            eprintln!("WebSocket: Session {} not found", session_id);
            return;
        }
    };

    // Create channel for sending messages to this client
    let (ws_sender, mut ws_receiver) = mpsc::unbounded_channel();

    // Add client to session
    session.add_client(client_id, ws_sender).await;

    // Send SessionJoined message
    let kimichat = session.kimichat.lock().await;
    let history = kimichat.messages.clone();
    let current_model = kimichat.current_model.display_name();
    drop(kimichat);

    let join_msg = ServerMessage::SessionJoined {
        session_id,
        session_type: session.session_type.as_str().to_string(),
        created_at: session.created_at.to_rfc3339(),
        current_model,
        history,
    };

    let _ = session.send_to_client(client_id, join_msg).await;

    // Split socket
    let (mut ws_sink, mut ws_stream) = socket.split();

    // Spawn task to send messages from channel to WebSocket
    let session_clone = session.clone();
    let send_task = tokio::spawn(async move {
        while let Some(msg) = ws_receiver.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                if ws_sink.send(WsMessage::Text(json)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Handle incoming WebSocket messages
    while let Some(Ok(msg)) = ws_stream.next().await {
        if let WsMessage::Text(text) = msg {
            eprintln!("📨 Received WebSocket message: {}", text);
            match serde_json::from_str::<ClientMessage>(&text) {
                Ok(client_msg) => {
                    eprintln!("✅ Parsed message: {:?}", client_msg);
                    handle_client_message(client_id, client_msg, &session_clone, &state).await;
                }
                Err(e) => {
                    eprintln!("❌ Failed to parse message: {} - Error: {}", text, e);
                }
            }
        }
    }

    // Client disconnected
    session_clone.remove_client(client_id).await;
    send_task.abort();
}

/// Handle a message from a client
async fn handle_client_message(
    client_id: Uuid,
    message: ClientMessage,
    session: &Arc<crate::web::session_manager::Session>,
    state: &AppState,
) {
    use ClientMessage::*;

    match message {
        SendMessage { content } => {
            // Spawn chat handling in separate task to avoid blocking WebSocket reader
            // This is critical: if we await here, the WebSocket reader can't receive
            // confirmation messages because it's blocked waiting for this to complete
            let session_clone = Arc::clone(session);
            tokio::spawn(async move {
                handle_send_message(client_id, content, &session_clone).await;
            });
        }
        ConfirmTool {
            tool_call_id,
            confirmed,
        } => {
            eprintln!("🔔 Received ConfirmTool: id={}, confirmed={}", tool_call_id, confirmed);
            // Respond to pending confirmation
            let found = session.respond_to_confirmation(&tool_call_id, confirmed).await;
            eprintln!("🔔 Confirmation response sent: found={}", found);
        }
        ListSessions => {
            let sessions = state.session_manager.list_sessions().await;
            let msg = ServerMessage::SessionList { sessions };
            session.send_to_client(client_id, msg).await;
        }
        SwitchModel { model, reason } => {
            handle_switch_model(model, reason, session).await;
        }
        _ => {
            // TODO: Implement other message handlers
            eprintln!("Unhandled client message: {:?}", message);
        }
    }
}

/// Check if tool requires confirmation and extract plan/diff
async fn check_tool_confirmation(
    tool_name: &str,
    tool_args: &str,
    work_dir: &std::path::Path,
) -> (bool, Option<String>) {
    match tool_name {
        "apply_edit_plan" => {
            // Try to load and format the edit plan
            let plan_path = work_dir.join(".kimichat_edit_plan.json");
            if let Ok(content) = tokio::fs::read_to_string(&plan_path).await {
                if let Ok(plan) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                    let mut diff_text = String::new();
                    for (idx, edit) in plan.iter().enumerate() {
                        diff_text.push_str(&format!(
                            "Edit #{} {} - {}\n",
                            idx + 1,
                            edit.get("file_path").and_then(|v| v.as_str()).unwrap_or("?"),
                            edit.get("description").and_then(|v| v.as_str()).unwrap_or("?")
                        ));
                        if let Some(old) = edit.get("old_content").and_then(|v| v.as_str()) {
                            for line in old.lines() {
                                diff_text.push_str(&format!("  -{}\n", line));
                            }
                        }
                        if let Some(new) = edit.get("new_content").and_then(|v| v.as_str()) {
                            for line in new.lines() {
                                diff_text.push_str(&format!("  +{}\n", line));
                            }
                        }
                        diff_text.push('\n');
                    }
                    return (true, Some(diff_text));
                }
            }
            (true, None)
        }
        "write_file" | "edit_file" => (true, None), // These also need confirmation but no pre-extracted diff
        _ => (false, None),
    }
}

/// Chat loop with WebSocket broadcasts (single LLM mode)
async fn handle_chat_with_broadcast(
    session: &Arc<crate::web::session_manager::Session>,
) -> anyhow::Result<()> {
    const MAX_TOOL_ITERATIONS: usize = 100;
    let mut tool_call_iterations = 0;

    loop {
        let kimichat = session.kimichat.lock().await;

        // Make API call
        let (response, usage, _model) = call_api(
            &kimichat,
            &kimichat.messages,
        )
        .await?;

        drop(kimichat); // Release lock

        // Broadcast token usage
        if let Some(usage) = &usage {
            let mut kimichat = session.kimichat.lock().await;
            kimichat.total_tokens_used += usage.total_tokens;
            let session_total = kimichat.total_tokens_used;
            drop(kimichat);

            let token_msg = ServerMessage::TokenUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
                session_total,
            };
            session.broadcast(token_msg).await;
        }

        // Add assistant response to history
        session.kimichat.lock().await.messages.push(response.clone());

        // Handle tool calls
        if let Some(tool_calls) = &response.tool_calls {
            tool_call_iterations += 1;

            for tool_call in tool_calls {
                // Check if tool requires confirmation
                let work_dir = session.kimichat.lock().await.work_dir.clone();
                let (requires_confirmation, diff) = check_tool_confirmation(
                    &tool_call.function.name,
                    &tool_call.function.arguments,
                    &work_dir,
                )
                .await;

                // Broadcast tool call request
                let tool_msg = ServerMessage::ToolCallRequest {
                    tool_call_id: tool_call.id.clone(),
                    name: tool_call.function.name.clone(),
                    arguments: serde_json::from_str(&tool_call.function.arguments)
                        .unwrap_or(serde_json::json!({})),
                    requires_confirmation,
                    diff,
                    iteration: Some(tool_call_iterations),
                    max_iterations: Some(MAX_TOOL_ITERATIONS),
                };
                session.broadcast(tool_msg).await;

                // If requires confirmation, wait for user response
                if requires_confirmation {
                    eprintln!("⏳ Registering confirmation for tool_call_id: {}", tool_call.id);
                    let confirmation_rx = session
                        .register_confirmation(
                            tool_call.id.clone(),
                            tool_call.function.name.clone(),
                            tool_call.function.arguments.clone(),
                        )
                        .await;

                    eprintln!("⏳ Waiting for user confirmation...");
                    // Wait for confirmation (with timeout)
                    let confirmed = match tokio::time::timeout(
                        std::time::Duration::from_secs(300), // 5 minute timeout
                        confirmation_rx,
                    )
                    .await
                    {
                        Ok(Ok(confirmed)) => {
                            eprintln!("✅ Received confirmation: {}", confirmed);
                            confirmed
                        }
                        Ok(Err(_)) => {
                            eprintln!("❌ Confirmation channel closed");
                            false
                        }
                        Err(_) => {
                            // Timeout
                            eprintln!("⏱️  Confirmation timeout");
                            let error_msg = ServerMessage::Error {
                                message: "Tool confirmation timeout (5 minutes)".to_string(),
                                recoverable: true,
                            };
                            session.broadcast(error_msg).await;
                            false
                        }
                    };

                    if !confirmed {
                        eprintln!("🚫 Tool execution denied");

                        // User denied, send error result
                        let error_str = "Tool execution cancelled by user".to_string();
                        let result_msg = ServerMessage::ToolCallResult {
                            tool_call_id: tool_call.id.clone(),
                            result: error_str.clone(),
                            success: false,
                            formatted_result: Some(error_str.clone()),
                        };
                        session.broadcast(result_msg).await;

                        // Add cancellation to history
                        session.kimichat.lock().await.messages.push(ChatMessage {
                            role: "tool".to_string(),
                            content: error_str,
                            tool_calls: None,
                            tool_call_id: Some(tool_call.id.clone()),
                            name: Some(tool_call.function.name.clone()),
                            reasoning: None,
                        });
                        continue; // Skip to next tool call
                    }
                }

                // Execute tool (either confirmed or doesn't need confirmation)
                let mut kimichat = session.kimichat.lock().await;
                let result = kimichat
                    .execute_tool(&tool_call.function.name, &tool_call.function.arguments)
                    .await;
                drop(kimichat);

                // Broadcast tool result
                match result {
                    Ok(result_str) => {
                        let result_msg = ServerMessage::ToolCallResult {
                            tool_call_id: tool_call.id.clone(),
                            result: result_str.clone(),
                            success: true,
                            formatted_result: Some(result_str.clone()),
                        };
                        session.broadcast(result_msg).await;

                        // Add tool result to history
                        session.kimichat.lock().await.messages.push(ChatMessage {
                            role: "tool".to_string(),
                            content: result_str,
                            tool_calls: None,
                            tool_call_id: Some(tool_call.id.clone()),
                            name: Some(tool_call.function.name.clone()),
                            reasoning: None,
                        });
                    }
                    Err(e) => {
                        let error_str = format!("Error: {}", e);
                        let result_msg = ServerMessage::ToolCallResult {
                            tool_call_id: tool_call.id.clone(),
                            result: error_str.clone(),
                            success: false,
                            formatted_result: Some(error_str.clone()),
                        };
                        session.broadcast(result_msg).await;

                        // Add error to history
                        session.kimichat.lock().await.messages.push(ChatMessage {
                            role: "tool".to_string(),
                            content: error_str,
                            tool_calls: None,
                            tool_call_id: Some(tool_call.id.clone()),
                            name: Some(tool_call.function.name.clone()),
                            reasoning: None,
                        });
                    }
                }
            }

            // Check iteration limit
            if tool_call_iterations >= MAX_TOOL_ITERATIONS {
                let error_msg = ServerMessage::Error {
                    message: format!("Maximum tool iterations ({}) reached", MAX_TOOL_ITERATIONS),
                    recoverable: false,
                };
                session.broadcast(error_msg).await;
                break;
            }

            // Continue loop for next API call
            continue;
        }

        // No tool calls - send final response and complete
        let msg = ServerMessage::AssistantMessage {
            content: response.content,
            streaming: false,
        };
        session.broadcast(msg).await;
        session.broadcast(ServerMessage::AssistantMessageComplete).await;
        break;
    }

    Ok(())
}

/// Handle SendMessage
async fn handle_send_message(
    _client_id: Uuid,
    content: String,
    session: &Arc<crate::web::session_manager::Session>,
) {
    let mut kimichat = session.kimichat.lock().await;

    // Add user message
    kimichat.messages.push(crate::models::Message {
        role: "user".to_string(),
        content: content.clone(),
        tool_calls: None,
        tool_call_id: None,
        name: None,
        reasoning: None,
    });

    // Handle based on mode
    if kimichat.use_agents {
        // Multi-agent mode - use existing process_with_agents
        drop(kimichat); // Release lock before async call
        match session.kimichat.lock().await
            .process_with_agents(&content, None)
            .await
        {
            Ok(response) => {
                let msg = ServerMessage::AssistantMessage {
                    content: response,
                    streaming: false,
                };
                session.broadcast(msg).await;
                session.broadcast(ServerMessage::AssistantMessageComplete).await;
            }
            Err(e) => {
                let error_msg = ServerMessage::Error {
                    message: format!("Agent processing failed: {}", e),
                    recoverable: true,
                };
                session.broadcast(error_msg).await;
            }
        }
    } else {
        // Single LLM mode - use custom loop with broadcasts
        drop(kimichat); // Release lock
        if let Err(e) = handle_chat_with_broadcast(session).await {
            let error_msg = ServerMessage::Error {
                message: format!("Chat failed: {}", e),
                recoverable: true,
            };
            session.broadcast(error_msg).await;
        }
    }
}

/// Handle SwitchModel
async fn handle_switch_model(
    model: String,
    reason: String,
    session: &Arc<crate::web::session_manager::Session>,
) {
    let mut kimichat = session.kimichat.lock().await;
    let old_model = kimichat.current_model.display_name();

    match kimichat.switch_model(&model, &reason) {
        Ok(_) => {
            let new_model = kimichat.current_model.display_name();
            let msg = ServerMessage::ModelSwitched {
                old_model,
                new_model,
                reason,
            };
            session.broadcast(msg).await;
        }
        Err(e) => {
            let error_msg = ServerMessage::Error {
                message: format!("Model switch failed: {}", e),
                recoverable: true,
            };
            session.broadcast(error_msg).await;
        }
    }
}

/// GET / - Serve index page
async fn serve_index() -> Html<&'static str> {
    Html(include_str!("../../web/index.html"))
}

/// GET /session/:id - Serve session page
async fn serve_session(Path(_id): Path<SessionId>) -> Html<&'static str> {
    Html(include_str!("../../web/session.html"))
}

/// Error handling
#[derive(Debug)]
enum AppError {
    Anyhow(anyhow::Error),
    NotFound(String),
    SerdeJson(serde_json::Error),
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Anyhow(err)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::SerdeJson(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::Anyhow(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::SerdeJson(err) => (StatusCode::BAD_REQUEST, err.to_string()),
        };

        let body = Json(serde_json::json!({
            "error": message,
            "status": status.as_u16(),
        }));

        (status, body).into_response()
    }
}
