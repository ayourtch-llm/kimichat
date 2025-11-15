use anyhow::Result;
use axum::Router;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};

use crate::config::ClientConfig;
use crate::policy::PolicyManager;
use crate::web::{routes, session_manager::SessionManager};

/// Web server configuration
pub struct WebServerConfig {
    pub bind_addr: SocketAddr,
    pub work_dir: PathBuf,
    pub client_config: ClientConfig,
    pub policy_manager: PolicyManager,
    pub web_dir: Option<PathBuf>,
}

/// Web server instance
pub struct WebServer {
    config: WebServerConfig,
    session_manager: Arc<SessionManager>,
}

impl WebServer {
    /// Create a new web server
    pub fn new(config: WebServerConfig) -> Self {
        let session_manager = Arc::new(SessionManager::new(
            config.work_dir.clone(),
            config.client_config.clone(),
            config.policy_manager.clone(),
        ));

        Self {
            config,
            session_manager,
        }
    }

    /// Start the web server
    pub async fn start(self) -> Result<()> {
        let app_state = routes::AppState {
            session_manager: self.session_manager.clone(),
        };

        // Create router
        let mut app = routes::create_router(app_state);

        // Add CORS layer for development
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        app = app.layer(cors);

        // Serve static files if web_dir is provided
        if let Some(web_dir) = &self.config.web_dir {
            if web_dir.exists() {
                println!("Serving static files from: {}", web_dir.display());
                let serve_dir = ServeDir::new(web_dir);
                app = app.nest_service("/static", serve_dir);
            }
        }

        // Start server
        println!("🌐 Web server starting on http://{}", self.config.bind_addr);
        println!("   WebSocket endpoint: ws://{}/ws/{{session_id}}", self.config.bind_addr);
        println!("   API endpoints: http://{}/api/sessions", self.config.bind_addr);

        let listener = tokio::net::TcpListener::bind(&self.config.bind_addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    /// Get the session manager (for integration with TUI)
    pub fn session_manager(&self) -> Arc<SessionManager> {
        self.session_manager.clone()
    }
}
