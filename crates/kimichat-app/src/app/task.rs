use anyhow::Result;
use colored::Colorize;

use crate::KimiChat;
use crate::cli::Cli;
use crate::config::ClientConfig;
use crate::policy::PolicyManager;
use crate::logging::ConversationLogger;
use std::path::PathBuf;

/// Run in task mode - execute a single task and exit
pub async fn run_task_mode(
    cli: &Cli,
    task_text: String,
    client_config: ClientConfig,
    work_dir: PathBuf,
    policy_manager: PolicyManager,
) -> Result<()> {
    println!("{}", "🤖 Kimi Chat - Task Mode".bright_cyan().bold());
    println!("{}", format!("Working directory: {}", work_dir.display()).bright_black());

    if cli.agents {
        println!("{}", "🚀 Multi-Agent System ENABLED".green().bold());
    }

    println!("{}", format!("Task: {}", task_text).bright_yellow());
    println!();

    // Resolve terminal backend
    let backend_type = crate::resolve_terminal_backend(cli)?;

    let mut chat = KimiChat::new_with_config(
        client_config.clone(),
        work_dir.clone(),
        cli.agents,
        policy_manager.clone(),
        cli.stream,
        cli.verbose,
        backend_type,
    );

    // Initialize logger for task mode
    chat.logger = match ConversationLogger::new_task_mode(&chat.work_dir).await {
        Ok(l) => Some(l),
        Err(e) => {
            eprintln!("Task logging disabled: {}", e);
            None
        }
    };

    let response = if chat.use_agents && chat.agent_coordinator.is_some() {
        // Use agent system
        match chat.process_with_agents(&task_text, None).await {
            Ok(response) => response,
            Err(e) => {
                eprintln!("{} {}\n", "Agent Error:".bright_red().bold(), e);
                // Fallback to regular chat (no cancellation in task mode)
                match crate::chat::session::chat(&mut chat, &task_text, None).await {
                    Ok(response) => response,
                    Err(e) => {
                        eprintln!("{} {}\n", "Error:".bright_red().bold(), e);
                        return Ok(());
                    }
                }
            }
        }
    } else {
        // Use regular chat (no cancellation in task mode)
        match crate::chat::session::chat(&mut chat, &task_text, None).await {
            Ok(response) => response,
            Err(e) => {
                eprintln!("{} {}\n", "Error:".bright_red().bold(), e);
                return Ok(());
            }
        }
    };

    if cli.pretty {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "response": response,
                "agents_used": chat.use_agents
            }))
            .unwrap_or_else(|_| response.to_string())
        );
    } else {
        println!("{}", response);
    }

    Ok(())
}
