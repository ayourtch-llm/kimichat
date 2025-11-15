use anyhow::Result;
use std::net::SocketAddr;
use std::path::PathBuf;

use crate::cli::Cli;
use crate::config::ClientConfig;
use crate::policy::PolicyManager;
use crate::web::server::{WebServer, WebServerConfig};

/// Run the web server
pub async fn run_web_server(
    cli: &Cli,
    client_config: ClientConfig,
    work_dir: PathBuf,
    policy_manager: PolicyManager,
) -> Result<()> {
    // Parse bind address
    let addr: SocketAddr = format!("{}:{}", cli.web_bind, cli.web_port).parse()?;

    println!("🌐 Starting KimiChat web server...");
    println!("   Address: {}", addr);
    println!("   Working directory: {}", work_dir.display());

    // Create web server config
    let config = WebServerConfig {
        bind_addr: addr,
        work_dir,
        client_config,
        policy_manager,
        web_dir: None, // Could be made configurable
    };

    // Create and start server
    let server = WebServer::new(config);
    server.start().await?;

    Ok(())
}
