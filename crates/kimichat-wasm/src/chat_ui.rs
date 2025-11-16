use wasm_bindgen::JsValue;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{Document, HtmlElement, HtmlTextAreaElement, Element};
use std::rc::Rc;
use std::cell::RefCell;
use gloo_net::websocket::futures::WebSocket;
use futures::{StreamExt, SinkExt};
use crate::protocol::{ClientMessage, ServerMessage, Message};
use crate::dom;
use crate::markdown;
use crate::utils;

pub struct ChatApp {
    session_id: String,
    document: Document,
    state: Rc<RefCell<ChatState>>,
}

struct ChatState {
    current_model: String,
    session_total_tokens: usize,
    markdown_enabled: bool,
    current_assistant_message: Option<String>,
    current_message_element: Option<Element>,
    active_tasks: std::collections::HashMap<String, TaskInfo>,
    sink: Option<Rc<RefCell<futures::stream::SplitSink<WebSocket, gloo_net::websocket::Message>>>>,
}

struct TaskInfo {
    _agent_name: String,
    description: String,
    progress: f32,
    element: Element,
}

impl ChatApp {
    pub fn new(session_id: String) -> Result<Self, JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;
        let document = window.document().ok_or_else(|| JsValue::from_str("No document"))?;

        let state = ChatState {
            current_model: String::new(),
            session_total_tokens: 0,
            markdown_enabled: true,
            current_assistant_message: None,
            current_message_element: None,
            active_tasks: std::collections::HashMap::new(),
            sink: None,
        };

        Ok(Self {
            session_id,
            document,
            state: Rc::new(RefCell::new(state)),
        })
    }

    pub async fn start(self) -> Result<(), JsValue> {
        // Set up UI event listeners
        self.setup_message_input()?;
        self.setup_markdown_toggle()?;
        self.setup_model_switcher()?;

        // Simple reconnection loop
        let mut retry_count = 0;
        loop {
            let ws_url = utils::build_ws_url(&self.session_id)?;

            if retry_count == 0 {
                log::info!("Connecting to: {}", ws_url);
            } else {
                log::info!("Reconnecting (attempt {})...", retry_count + 1);
            }

            let ws = match WebSocket::open(&ws_url) {
                Ok(ws) => ws,
                Err(e) => {
                    log::error!("Failed to connect: {:?}", e);
                    if retry_count < 10 {
                        let delay_ms = (1000 * 2_u32.pow(retry_count)).min(60000);
                        log::info!("Retrying in {}ms...", delay_ms);
                        let _ = self.show_system_message(&format!("Connection lost. Retrying in {}s...", delay_ms / 1000));
                        gloo_timers::future::TimeoutFuture::new(delay_ms).await;
                        retry_count += 1;
                        continue;
                    } else {
                        self.show_error("Connection failed after multiple retries. Please refresh the page.", false)?;
                        return Err(JsValue::from_str("Max retries exceeded"));
                    }
                }
            };

            // Run message loop
            match self.message_loop(ws).await {
                Ok(()) => {
                    log::info!("WebSocket closed normally");
                    return Ok(());
                }
                Err(e) => {
                    log::error!("WebSocket error: {:?}", e);
                    if retry_count < 10 {
                        let delay_ms = (1000 * 2_u32.pow(retry_count)).min(60000);
                        log::info!("Connection error. Retrying in {}ms...", delay_ms);
                        let _ = self.show_system_message(&format!("Connection lost. Retrying in {}s...", delay_ms / 1000));
                        gloo_timers::future::TimeoutFuture::new(delay_ms).await;
                        retry_count += 1;
                        continue;
                    } else {
                        self.show_error("Connection failed after multiple retries. Please refresh the page.", false)?;
                        return Err(e);
                    }
                }
            }
        }
    }

    async fn message_loop(&self, ws: WebSocket) -> Result<(), JsValue> {
        let (mut sink, mut stream) = ws.split();
        let document = self.document.clone();
        let state = self.state.clone();

        // Send JoinSession message
        let join_msg = ClientMessage::JoinSession {
            session_id: self.session_id.clone(),
        };
        let json = serde_json::to_string(&join_msg)
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize: {}", e)))?;
        sink.send(gloo_net::websocket::Message::Text(json)).await
            .map_err(|e| JsValue::from_str(&format!("Failed to send: {:?}", e)))?;

        // Set up message sender for UI events
        let sink = Rc::new(RefCell::new(sink));
        state.borrow_mut().sink = Some(sink.clone());
        self.setup_message_sender(sink.clone())?;

        // Process incoming messages
        while let Some(msg_result) = stream.next().await {
            match msg_result {
                Ok(gloo_net::websocket::Message::Text(text)) => {
                    log::debug!("Received: {}", text);
                    let server_msg: ServerMessage = serde_json::from_str(&text)
                        .map_err(|e| JsValue::from_str(&format!("Parse error: {}", e)))?;

                    self.handle_server_message(&document, &state, server_msg)?;
                }
                Ok(_) => {
                    log::warn!("Received non-text message");
                }
                Err(e) => {
                    log::error!("WebSocket error: {:?}", e);
                    // Return error to trigger reconnection
                    return Err(JsValue::from_str(&format!("WebSocket error: {:?}", e)));
                }
            }
        }

        log::info!("WebSocket closed");
        Ok(())
    }

    fn setup_message_sender(&self, sink: Rc<RefCell<futures::stream::SplitSink<WebSocket, gloo_net::websocket::Message>>>) -> Result<(), JsValue> {
        let document = self.document.clone();
        let _state = self.state.clone();

        // Send button
        let send_btn = dom::get_element_by_id(&document, "sendButton")?;
        let sink_clone = sink.clone();
        let doc_clone = document.clone();

        let closure = Closure::wrap(Box::new(move || {
            let sink = sink_clone.clone();
            let doc = doc_clone.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Err(e) = send_message_handler(sink, doc).await {
                    log::error!("Failed to send message: {:?}", e);
                }
            });
        }) as Box<dyn FnMut()>);

        send_btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
        closure.forget();

        // Enter key handler
        let input = dom::get_textarea_by_id(&document, "messageInput")?;
        let sink_clone = sink.clone();
        let doc_clone = document.clone();

        let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
            if event.key() == "Enter" && !event.shift_key() {
                event.prevent_default();
                let sink = sink_clone.clone();
                let doc = doc_clone.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Err(e) = send_message_handler(sink, doc).await {
                        log::error!("Failed to send message: {:?}", e);
                    }
                });
            }
        }) as Box<dyn FnMut(_)>);

        input.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())?;
        closure.forget();

        Ok(())
    }

    fn handle_server_message(
        &self,
        document: &Document,
        state: &Rc<RefCell<ChatState>>,
        msg: ServerMessage,
    ) -> Result<(), JsValue> {
        match msg {
            ServerMessage::SessionJoined {
                session_id,
                session_type,
                created_at: _,
                current_model,
                history,
            } => {
                log::info!("Joined session: {}", session_id);
                state.borrow_mut().current_model = current_model.clone();
                self.update_session_info(&session_type, &current_model)?;
                self.render_history(document, state, history)?;
            }

            ServerMessage::UserMessage { content } => {
                // User message from another client in the same session
                // Check if this is a duplicate (we already rendered it immediately when sending)
                let container = dom::get_element_by_id(document, "messagesContainer")?;

                // Check the last message in the container
                let is_duplicate = if let Ok(Some(last)) = container.query_selector(".message:last-child") {
                    if last.class_name().contains("user") {
                        if let Ok(Some(content_div)) = last.query_selector(".message-content") {
                            // Compare text content (strip HTML)
                            let existing_text = content_div.text_content().unwrap_or_default();
                            existing_text.trim() == content.trim()
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !is_duplicate {
                    let msg = Message {
                        role: "user".to_string(),
                        content,
                        ..Default::default()
                    };
                    self.render_message(document, state, &msg)?;
                    dom::scroll_to_bottom(&container);
                }
            }

            ServerMessage::AssistantMessage { content, streaming } => {
                if streaming {
                    // Streaming message - treat as chunk
                    self.handle_message_chunk(document, state, content)?;
                } else {
                    // Complete message - render directly
                    let msg = Message {
                        role: "assistant".to_string(),
                        content,
                        ..Default::default()
                    };
                    self.render_message(document, state, &msg)?;
                    dom::scroll_to_bottom(&dom::get_element_by_id(document, "messagesContainer")?);
                }
            }

            ServerMessage::AssistantMessageChunk { chunk } => {
                self.handle_message_chunk(document, state, chunk)?;
            }

            ServerMessage::AssistantMessageComplete => {
                self.handle_message_complete(state)?;
            }

            ServerMessage::ToolCallRequest {
                tool_call_id,
                name,
                arguments,
                requires_confirmation,
                diff,
                iteration,
                max_iterations,
            } => {
                self.handle_tool_request(
                    document,
                    state,
                    tool_call_id,
                    name,
                    arguments,
                    requires_confirmation,
                    diff,
                    iteration,
                    max_iterations,
                )?;
            }

            ServerMessage::ToolCallResult {
                tool_call_id,
                result,
                success,
                formatted_result,
            } => {
                self.handle_tool_result(document, tool_call_id, result, success, formatted_result)?;
            }

            ServerMessage::TaskProgress {
                task_id,
                agent_name,
                status,
                progress,
                description,
            } => {
                self.handle_task_progress(document, state, task_id, agent_name, status, progress, description)?;
            }

            ServerMessage::AgentAssigned {
                agent_name,
                task_id,
                task_description,
            } => {
                self.handle_agent_assigned(document, state, agent_name, task_id, task_description)?;
            }

            ServerMessage::ModelSwitched {
                old_model,
                new_model,
                reason,
            } => {
                state.borrow_mut().current_model = new_model.clone();
                self.update_model_badge(&new_model)?;
                self.show_system_message(&format!("Model switched from {} to {}: {}", old_model, new_model, reason))?;
            }

            ServerMessage::TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens,
                session_total,
            } => {
                state.borrow_mut().session_total_tokens = session_total;
                self.update_token_display(prompt_tokens, completion_tokens, total_tokens, session_total)?;
            }

            ServerMessage::Error { message, recoverable } => {
                self.show_error(&message, recoverable)?;
            }

            ServerMessage::SessionCreated { session_id, created_at } => {
                log::info!("Session created: {} at {}", session_id, created_at);
            }

            ServerMessage::SessionList { sessions } => {
                log::info!("Received session list: {} sessions", sessions.len());
            }

            ServerMessage::SessionError { error } => {
                self.show_error(&error, true)?;
            }

            ServerMessage::SessionTitleUpdated { title } => {
                self.update_session_title(title)?;
            }

            _ => {
                log::warn!("Unhandled message type: {:?}", msg);
            }
        }

        Ok(())
    }

    fn setup_message_input(&self) -> Result<(), JsValue> {
        let input = dom::get_textarea_by_id(&self.document, "messageInput")?;

        // Auto-resize textarea
        let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
            if let Some(target) = event.target() {
                if let Ok(textarea) = target.dyn_into::<HtmlTextAreaElement>() {
                    if let Ok(html_element) = textarea.clone().dyn_into::<HtmlElement>() {
                        let _ = html_element.style().set_property("height", "auto");
                        let scroll_height = textarea.scroll_height();
                        let _ = html_element.style().set_property("height", &format!("{}px", scroll_height));
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);

        input.add_event_listener_with_callback("input", closure.as_ref().unchecked_ref())?;
        closure.forget();

        Ok(())
    }

    fn setup_markdown_toggle(&self) -> Result<(), JsValue> {
        let toggle = dom::get_element_by_id(&self.document, "markdownToggle")?;
        let state = self.state.clone();

        let closure = Closure::wrap(Box::new(move || {
            let mut s = state.borrow_mut();
            s.markdown_enabled = !s.markdown_enabled;
            log::info!("Markdown enabled: {}", s.markdown_enabled);
        }) as Box<dyn FnMut()>);

        toggle.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
        closure.forget();

        Ok(())
    }

    fn setup_model_switcher(&self) -> Result<(), JsValue> {
        // Model switcher would be implemented here if needed
        Ok(())
    }

    fn render_history(
        &self,
        document: &Document,
        state: &Rc<RefCell<ChatState>>,
        history: Vec<Message>,
    ) -> Result<(), JsValue> {
        let container = dom::get_element_by_id(document, "messagesContainer")?;
        dom::clear_element(&container);

        for msg in history {
            self.render_message(document, state, &msg)?;
        }

        dom::scroll_to_bottom(&container);

        Ok(())
    }

    fn render_message(
        &self,
        document: &Document,
        state: &Rc<RefCell<ChatState>>,
        msg: &Message,
    ) -> Result<(), JsValue> {
        let container = dom::get_element_by_id(document, "messagesContainer")?;

        let msg_div = document.create_element("div")?;
        msg_div.set_class_name(&format!("message {}", msg.role));

        let markdown_enabled = state.borrow().markdown_enabled;
        let content_html = markdown::render_message_content(&msg.content, markdown_enabled);

        let markdown_class = if markdown_enabled { " markdown" } else { "" };
        let html = format!(
            r#"<div class="message-role">{}</div><div class="message-content{}">{}</div>"#,
            msg.role, markdown_class, content_html
        );

        msg_div.set_inner_html(&html);
        container.append_child(&msg_div)?;

        Ok(())
    }

    fn handle_message_chunk(
        &self,
        document: &Document,
        state: &Rc<RefCell<ChatState>>,
        chunk: String,
    ) -> Result<(), JsValue> {
        // First update the message
        {
            let mut s = state.borrow_mut();
            let current = s.current_assistant_message.get_or_insert(String::new());
            current.push_str(&chunk);

            // Create message element if needed
            if s.current_message_element.is_none() {
                let container = dom::get_element_by_id(document, "messagesContainer")?;
                let msg_div = document.create_element("div")?;
                msg_div.set_class_name("message assistant streaming");

                let markdown_class = if s.markdown_enabled { " markdown" } else { "" };
                let html = format!(
                    r#"<div class="message-role">assistant</div><div class="message-content{}"></div><span class="cursor"></span>"#,
                    markdown_class
                );
                msg_div.set_inner_html(&html);

                container.append_child(&msg_div)?;
                s.current_message_element = Some(msg_div);
            }
        }

        // Then work with the values
        let s = state.borrow();
        let current_content = s.current_assistant_message.as_ref().unwrap().clone();
        let markdown_enabled = s.markdown_enabled;
        let element = s.current_message_element.clone();

        // Update content
        if let Some(ref element) = element {
            if let Some(content_div) = element.query_selector(".message-content")? {
                let rendered = markdown::render_message_content(&current_content, markdown_enabled);
                content_div.set_inner_html(&rendered);
            }
        }

        // Scroll to bottom
        let container = dom::get_element_by_id(document, "messagesContainer")?;
        dom::scroll_to_bottom(&container);

        Ok(())
    }

    fn handle_message_complete(&self, state: &Rc<RefCell<ChatState>>) -> Result<(), JsValue> {
        let mut s = state.borrow_mut();

        // Remove streaming cursor
        if let Some(ref element) = s.current_message_element {
            element.set_class_name("message assistant");
            if let Some(cursor) = element.query_selector(".cursor")? {
                cursor.remove();
            }
        }

        // Clear current message state
        s.current_assistant_message = None;
        s.current_message_element = None;

        Ok(())
    }

    fn handle_tool_request(
        &self,
        document: &Document,
        state: &Rc<RefCell<ChatState>>,
        tool_call_id: String,
        name: String,
        arguments: serde_json::Value,
        requires_confirmation: bool,
        diff: Option<String>,
        iteration: Option<usize>,
        max_iterations: Option<usize>,
    ) -> Result<(), JsValue> {
        let container = dom::get_element_by_id(document, "messagesContainer")?;

        let tool_div = document.create_element("div")?;
        tool_div.set_class_name("tool-call");
        tool_div.set_id(&format!("tool-{}", tool_call_id));

        let args_formatted = serde_json::to_string_pretty(&arguments).unwrap_or_default();

        let mut html = format!(
            r#"<div class="tool-header">🔧 Tool: {}</div>"#,
            utils::escape_html(&name)
        );

        if let (Some(iter), Some(max)) = (iteration, max_iterations) {
            html.push_str(&format!(
                r#"<div class="tool-iteration">Iteration {}/{}</div>"#,
                iter, max
            ));
        }

        html.push_str(&format!(
            r#"<div class="tool-args"><pre><code>{}</code></pre></div>"#,
            utils::escape_html(&args_formatted)
        ));

        if let Some(diff_content) = diff {
            html.push_str(&format!(
                r#"<div class="tool-diff"><pre><code>{}</code></pre></div>"#,
                utils::escape_html(&diff_content)
            ));
        }

        if requires_confirmation {
            html.push_str(&format!(
                r#"<div class="tool-confirmation-actions">
                    <button class="tool-confirm-btn confirm" data-tool-id="{}">✓ Confirm</button>
                    <button class="tool-confirm-btn deny" data-tool-id="{}">✗ Deny</button>
                </div>"#,
                tool_call_id, tool_call_id
            ));
        } else {
            html.push_str(r#"<div class="tool-status">Executing...</div>"#);
        }

        tool_div.set_inner_html(&html);
        container.append_child(&tool_div)?;

        // Set up confirmation buttons if needed
        if requires_confirmation {
            self.setup_tool_confirmation_buttons(document, state, &tool_call_id)?;
        }

        dom::scroll_to_bottom(&container);

        Ok(())
    }

    fn setup_tool_confirmation_buttons(
        &self,
        document: &Document,
        state: &Rc<RefCell<ChatState>>,
        tool_call_id: &str
    ) -> Result<(), JsValue> {
        let tool_id = tool_call_id.to_string();

        // Get the sink from state
        let sink = state.borrow().sink.clone();
        if sink.is_none() {
            log::error!("WebSocket sink not available for tool confirmation");
            return Ok(());
        }
        let sink = sink.unwrap();

        // Find the confirm button
        if let Ok(Some(btn)) = document.query_selector(&format!("button.confirm[data-tool-id='{}']", tool_id)) {
            let tool_id_clone = tool_id.clone();
            let sink_clone = sink.clone();
            let doc_clone = document.clone();
            let closure = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                let tool_id = tool_id_clone.clone();
                let sink = sink_clone.clone();
                let doc = doc_clone.clone();

                // Update UI immediately
                if let Ok(Some(tool_elem)) = doc.query_selector(&format!("#tool-{}", tool_id)) {
                    if let Ok(Some(actions)) = tool_elem.query_selector(".tool-confirmation-actions") {
                        actions.set_inner_html(r#"<div class="tool-status confirmed">✓ Confirmed - Executing...</div>"#);
                    }
                }

                wasm_bindgen_futures::spawn_local(async move {
                    let msg = ClientMessage::ConfirmTool {
                        tool_call_id: tool_id,
                        confirmed: true,
                    };
                    if let Ok(json) = serde_json::to_string(&msg) {
                        let _ = sink.borrow_mut().send(gloo_net::websocket::Message::Text(json)).await;
                    }
                });
            }) as Box<dyn FnMut(_)>);

            btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
            closure.forget();
        }

        // Find the deny button
        if let Ok(Some(btn)) = document.query_selector(&format!("button.deny[data-tool-id='{}']", tool_id)) {
            let tool_id_clone = tool_id.clone();
            let sink_clone = sink.clone();
            let doc_clone = document.clone();
            let closure = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                let tool_id = tool_id_clone.clone();
                let sink = sink_clone.clone();
                let doc = doc_clone.clone();

                // Update UI immediately
                if let Ok(Some(tool_elem)) = doc.query_selector(&format!("#tool-{}", tool_id)) {
                    if let Ok(Some(actions)) = tool_elem.query_selector(".tool-confirmation-actions") {
                        actions.set_inner_html(r#"<div class="tool-status denied">✗ Denied</div>"#);
                    }
                }

                wasm_bindgen_futures::spawn_local(async move {
                    let msg = ClientMessage::ConfirmTool {
                        tool_call_id: tool_id,
                        confirmed: false,
                    };
                    if let Ok(json) = serde_json::to_string(&msg) {
                        let _ = sink.borrow_mut().send(gloo_net::websocket::Message::Text(json)).await;
                    }
                });
            }) as Box<dyn FnMut(_)>);

            btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
            closure.forget();
        }

        Ok(())
    }

    fn handle_tool_result(
        &self,
        document: &Document,
        tool_call_id: String,
        result: String,
        success: bool,
        formatted_result: Option<String>,
    ) -> Result<(), JsValue> {
        if let Some(tool_element) = document.get_element_by_id(&format!("tool-{}", tool_call_id)) {
            // Remove confirmation buttons/status if present
            if let Ok(Some(confirmation)) = tool_element.query_selector(".tool-confirmation") {
                confirmation.remove();
            }
            if let Ok(Some(actions)) = tool_element.query_selector(".tool-confirmation-actions") {
                actions.remove();
            }
            if let Ok(Some(status)) = tool_element.query_selector(".tool-status") {
                status.remove();
            }

            // Add result
            let result_div = document.create_element("div")?;
            result_div.set_class_name(if success { "tool-result success" } else { "tool-result error" });

            let display_result = formatted_result.unwrap_or(result);
            result_div.set_inner_html(&format!(
                r#"<div class="result-header">{}</div><pre><code>{}</code></pre>"#,
                if success { "✓ Success" } else { "✗ Error" },
                utils::escape_html(&display_result)
            ));

            tool_element.append_child(&result_div)?;
        }

        Ok(())
    }

    fn handle_task_progress(
        &self,
        _document: &Document,
        state: &Rc<RefCell<ChatState>>,
        task_id: String,
        _agent_name: String,
        status: String,
        progress: f32,
        description: String,
    ) -> Result<(), JsValue> {
        let mut s = state.borrow_mut();

        if let Some(task_info) = s.active_tasks.get_mut(&task_id) {
            // Update existing task
            task_info.progress = progress;
            task_info.description = description.clone();

            if let Some(progress_bar) = task_info.element.query_selector(".progress-fill")? {
                if let Ok(html_element) = progress_bar.dyn_into::<HtmlElement>() {
                    let _ = html_element.style().set_property("width", &format!("{}%", progress * 100.0));
                }
            }

            if let Some(desc_element) = task_info.element.query_selector(".task-description")? {
                desc_element.set_text_content(Some(&description));
            }

            // Remove if complete
            if status == "Completed" || progress >= 1.0 {
                let element = task_info.element.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                    element.remove();
                });
                s.active_tasks.remove(&task_id);
            }
        }

        Ok(())
    }

    fn handle_agent_assigned(
        &self,
        document: &Document,
        state: &Rc<RefCell<ChatState>>,
        agent_name: String,
        task_id: String,
        task_description: String,
    ) -> Result<(), JsValue> {
        let container = dom::get_element_by_id(document, "agentProgress")?;

        let task_div = document.create_element("div")?;
        task_div.set_class_name("agent-task");

        let html = format!(
            r#"
            <div class="task-agent">{}</div>
            <div class="task-description">{}</div>
            <div class="progress-bar">
                <div class="progress-fill" style="width: 0%"></div>
            </div>
            "#,
            utils::escape_html(&agent_name),
            utils::escape_html(&task_description)
        );

        task_div.set_inner_html(&html);
        container.append_child(&task_div)?;

        let task_info = TaskInfo {
            _agent_name: agent_name,
            description: task_description,
            progress: 0.0,
            element: task_div,
        };

        state.borrow_mut().active_tasks.insert(task_id, task_info);

        Ok(())
    }

    fn update_session_info(&self, session_type: &str, current_model: &str) -> Result<(), JsValue> {
        if let Ok(element) = dom::get_element_by_id(&self.document, "sessionType") {
            element.set_text_content(Some(session_type));
        }

        // Update connection status
        if let Ok(element) = dom::get_element_by_id(&self.document, "connectionStatus") {
            element.set_text_content(Some("Connected"));
        }

        self.update_model_badge(current_model)?;

        Ok(())
    }

    fn update_model_badge(&self, model: &str) -> Result<(), JsValue> {
        if let Ok(element) = dom::get_element_by_id(&self.document, "currentModel") {
            element.set_text_content(Some(model));
        }
        Ok(())
    }

    fn update_session_title(&self, title: Option<String>) -> Result<(), JsValue> {
        if let Ok(element) = dom::get_element_by_id(&self.document, "sessionTitle") {
            if let Some(title_text) = title {
                element.set_text_content(Some(&title_text));
                element.set_class_name("session-title-display");
            } else {
                element.set_text_content(Some("Untitled Session"));
                element.set_class_name("session-title-display untitled");
            }
        }
        Ok(())
    }

    fn update_token_display(
        &self,
        prompt_tokens: usize,
        completion_tokens: usize,
        total_tokens: usize,
        session_total: usize,
    ) -> Result<(), JsValue> {
        if let Ok(element) = dom::get_element_by_id(&self.document, "tokenUsage") {
            let html = format!(
                r#"📊 Tokens: {} prompt + {} completion = {} total (Session: {})"#,
                prompt_tokens, completion_tokens, total_tokens, session_total
            );
            element.set_inner_html(&html);
            dom::show_element(&element.dyn_into::<HtmlElement>()?);
        }
        Ok(())
    }

    fn show_error(&self, message: &str, _recoverable: bool) -> Result<(), JsValue> {
        let container = dom::get_element_by_id(&self.document, "messagesContainer")?;

        let error_div = self.document.create_element("div")?;
        error_div.set_class_name("message error");

        let html = format!(
            r#"<div class="error-icon">⚠️</div><div class="error-message">{}</div>"#,
            utils::escape_html(message)
        );

        error_div.set_inner_html(&html);
        container.append_child(&error_div)?;

        dom::scroll_to_bottom(&container);

        Ok(())
    }

    fn show_system_message(&self, message: &str) -> Result<(), JsValue> {
        let container = dom::get_element_by_id(&self.document, "messagesContainer")?;

        let msg_div = self.document.create_element("div")?;
        msg_div.set_class_name("message system");
        msg_div.set_text_content(Some(message));

        container.append_child(&msg_div)?;
        dom::scroll_to_bottom(&container);

        Ok(())
    }
}

async fn send_message_handler(
    sink: Rc<RefCell<futures::stream::SplitSink<WebSocket, gloo_net::websocket::Message>>>,
    document: Document,
) -> Result<(), JsValue> {
    let input = dom::get_textarea_by_id(&document, "messageInput")?;
    let content = input.value();

    if content.trim().is_empty() {
        return Ok(());
    }

    // Render user message immediately
    let container = dom::get_element_by_id(&document, "messagesContainer")?;
    let msg_div = document.create_element("div")?;
    msg_div.set_class_name("message user");

    let content_html = crate::utils::escape_html(&content).replace('\n', "<br>");
    let html = format!(
        r#"<div class="message-role">user</div><div class="message-content">{}</div>"#,
        content_html
    );
    msg_div.set_inner_html(&html);
    container.append_child(&msg_div)?;
    dom::scroll_to_bottom(&container);

    // Clear input
    input.set_value("");
    if let Ok(html_element) = input.dyn_into::<HtmlElement>() {
        let _ = html_element.style().set_property("height", "auto");
    }

    // Send message
    let msg = ClientMessage::SendMessage { content };
    let json = serde_json::to_string(&msg)
        .map_err(|e| JsValue::from_str(&format!("Failed to serialize: {}", e)))?;

    sink.borrow_mut().send(gloo_net::websocket::Message::Text(json)).await
        .map_err(|e| JsValue::from_str(&format!("Failed to send: {:?}", e)))?;

    Ok(())
}
