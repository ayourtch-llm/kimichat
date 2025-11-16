use wasm_bindgen::JsValue;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{Document, HtmlInputElement, HtmlSelectElement};
use gloo_net::http::Request;
use crate::protocol::{SessionInfo, SessionConfig};
use crate::dom;

pub struct SessionListApp {
    document: Document,
}

impl SessionListApp {
    pub fn new() -> Result<Self, JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;
        let document = window.document().ok_or_else(|| JsValue::from_str("No document"))?;

        Ok(Self { document })
    }

    pub async fn start(self) -> Result<(), JsValue> {
        // Set up event listeners
        self.setup_create_session_form()?;
        self.setup_refresh_button()?;

        // Load sessions
        self.load_sessions().await?;

        Ok(())
    }

    fn setup_create_session_form(&self) -> Result<(), JsValue> {
        let document = self.document.clone();
        let form = dom::get_element_by_id(&document, "createSessionForm")?;

        let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
            event.prevent_default();
            wasm_bindgen_futures::spawn_local(async move {
                if let Err(e) = create_session_handler().await {
                    log::error!("Failed to create session: {:?}", e);
                }
            });
        }) as Box<dyn FnMut(_)>);

        form.add_event_listener_with_callback("submit", closure.as_ref().unchecked_ref())?;
        closure.forget();

        Ok(())
    }

    fn setup_refresh_button(&self) -> Result<(), JsValue> {
        let button = dom::get_element_by_id(&self.document, "refreshButton")?;
        let document = self.document.clone();

        let closure = Closure::wrap(Box::new(move || {
            let doc = document.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Err(e) = load_sessions_handler(doc).await {
                    log::error!("Failed to load sessions: {:?}", e);
                }
            });
        }) as Box<dyn FnMut()>);

        button.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
        closure.forget();

        Ok(())
    }

    async fn load_sessions(&self) -> Result<(), JsValue> {
        load_sessions_handler(self.document.clone()).await
    }
}

async fn load_sessions_handler(document: Document) -> Result<(), JsValue> {
    let response = Request::get("/api/sessions")
        .send()
        .await
        .map_err(|e| JsValue::from_str(&format!("Request failed: {:?}", e)))?;

    let sessions: Vec<SessionInfo> = response
        .json()
        .await
        .map_err(|e| JsValue::from_str(&format!("Failed to parse response: {:?}", e)))?;

    render_sessions(&document, sessions)?;

    Ok(())
}

fn render_sessions(document: &Document, sessions: Vec<SessionInfo>) -> Result<(), JsValue> {
    let container = dom::get_element_by_id(document, "sessionsList")?;
    dom::clear_element(&container);

    if sessions.is_empty() {
        let empty_msg = document.create_element("div")?;
        empty_msg.set_class_name("empty-state");
        empty_msg.set_text_content(Some("No active sessions. Create one to get started!"));
        container.append_child(&empty_msg)?;
        return Ok(());
    }

    for session in sessions {
        let card = create_session_card(document, session)?;
        container.append_child(&card)?;
    }

    Ok(())
}

fn create_session_card(document: &Document, session: SessionInfo) -> Result<web_sys::Element, JsValue> {
    let card = document.create_element("div")?;
    card.set_class_name("session-card");

    let session_id = session.id.clone();
    let onclick = Closure::wrap(Box::new(move || {
        let url = format!("/session/{}", session_id);
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href(&url);
        }
    }) as Box<dyn FnMut()>);

    card.add_event_listener_with_callback("click", onclick.as_ref().unchecked_ref())?;
    onclick.forget();

    let title_html = if let Some(ref title) = session.title {
        format!(r#"<div class="session-title">{}</div>"#, title)
    } else {
        r#"<div class="session-title untitled">Untitled Session</div>"#.to_string()
    };

    let html = format!(
        r#"
        <div class="session-header">
            {}
            <div class="session-type">{}</div>
            <div class="session-id">{}</div>
        </div>
        <div class="session-info">
            <div class="info-row">
                <span class="label">Model:</span>
                <span class="value">{}</span>
            </div>
            <div class="info-row">
                <span class="label">Messages:</span>
                <span class="value">{}</span>
            </div>
            <div class="info-row">
                <span class="label">Clients:</span>
                <span class="value">{}</span>
            </div>
            <div class="info-row">
                <span class="label">Created:</span>
                <span class="value">{}</span>
            </div>
        </div>
        "#,
        title_html,
        session.session_type,
        session.id,
        session.current_model,
        session.message_count,
        session.active_clients,
        crate::utils::format_time(&session.created_at)
    );

    card.set_inner_html(&html);

    Ok(card)
}

async fn create_session_handler() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;
    let document = window.document().ok_or_else(|| JsValue::from_str("No document"))?;

    // Get form values
    let model_select = document
        .get_element_by_id("modelSelect")
        .ok_or_else(|| JsValue::from_str("Model select not found"))?
        .dyn_into::<HtmlSelectElement>()?;

    let agents_checkbox = document
        .get_element_by_id("agentsEnabled")
        .ok_or_else(|| JsValue::from_str("Agents checkbox not found"))?
        .dyn_into::<HtmlInputElement>()?;

    let model = model_select.value();
    let model = if model.is_empty() { None } else { Some(model) };
    let agents_enabled = agents_checkbox.checked();

    // Create session config
    let config = SessionConfig {
        model,
        agents_enabled,
        stream_responses: true,
    };

    // Send request
    let response = Request::post("/api/sessions")
        .json(&config)
        .map_err(|e| JsValue::from_str(&format!("Failed to serialize config: {:?}", e)))?
        .send()
        .await
        .map_err(|e| JsValue::from_str(&format!("Request failed: {:?}", e)))?;

    #[derive(serde::Deserialize)]
    struct CreateResponse {
        id: String,
    }

    let result: CreateResponse = response
        .json()
        .await
        .map_err(|e| JsValue::from_str(&format!("Failed to parse response: {:?}", e)))?;

    // Redirect to new session
    let url = format!("/session/{}", result.id);
    window.location().set_href(&url)?;

    Ok(())
}
