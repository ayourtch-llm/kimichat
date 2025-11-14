use anyhow::Result;
use colored::Colorize;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::env;
use std::path::PathBuf;

use crate::KimiChat;
use crate::cli::Cli;
use crate::config::ClientConfig;
use crate::policy::PolicyManager;
use crate::logging::ConversationLogger;
use crate::models::{ModelType, Message};

/// Run interactive REPL mode
pub async fn run_repl_mode(
    cli: &Cli,
    client_config: ClientConfig,
    work_dir: PathBuf,
    policy_manager: PolicyManager,
) -> Result<()> {
    println!("{}", "🤖 Kimi Chat - Claude Code-like Experience".bright_cyan().bold());
    println!("{}", format!("Working directory: {}", work_dir.display()).bright_black());

    if cli.agents {
        println!("{}", "🚀 Multi-Agent System ENABLED - Specialized agents will handle your tasks".green().bold());
    }

    println!("{}", "Type 'exit' or 'quit' to exit, or '/skills' to see available skill commands\n".bright_black());

    let mut chat = KimiChat::new_with_config(
        client_config,
        work_dir,
        cli.agents,
        policy_manager,
        cli.stream,
        cli.verbose,
    );

    // Show the actual current model configuration
    let current_model_display = match chat.current_model {
        ModelType::BluModel => format!("BluModel/{} (auto-switched from default)", chat.current_model.display_name()),
        ModelType::GrnModel => format!("GrnModel/{} (default)", chat.current_model.display_name()),
        ModelType::RedModel => format!("RedModel/{}", chat.current_model.display_name()),
        ModelType::AnthropicModel => format!("Anthropic/{}", chat.current_model.display_name()),
        ModelType::Custom(ref name) => format!("Custom/{}", name),
    };

    // Show what backends are being used
    let blu_backend = if chat.client_config.api_url_blu_model.as_ref().map(|u| u.contains("anthropic")).unwrap_or(false) ||
                       env::var("ANTHROPIC_AUTH_TOKEN_BLU").is_ok() {
        "Anthropic API 🧠"
    } else if chat.client_config.api_url_blu_model.is_some() {
        "llama.cpp 🦙"
    } else {
        "Groq API 🚀"
    };

    let grn_backend = if chat.client_config.api_url_grn_model.as_ref().map(|u| u.contains("anthropic")).unwrap_or(false) ||
                       env::var("ANTHROPIC_AUTH_TOKEN_GRN").is_ok() {
        "Anthropic API 🧠"
    } else if chat.client_config.api_url_grn_model.is_some() {
        "llama.cpp 🦙"
    } else {
        "Groq API 🚀"
    };

    println!("{}", format!("Default model: {} • BluModel uses {}, GrnModel uses {}",
        current_model_display, blu_backend, grn_backend).bright_black());

    // Debug info (shown at debug level 1+)
    if chat.should_show_debug(1) {
        println!("{}", format!("🔧 DEBUG: blu_model URL: {:?}", chat.client_config.api_url_blu_model).bright_black());
        println!("{}", format!("🔧 DEBUG: grn_model URL: {:?}", chat.client_config.api_url_grn_model).bright_black());
        println!("{}", format!("🔧 DEBUG: Current model: {:?}", chat.current_model).bright_black());
    }

    // Initialize logger (async) – logs go into the workspace directory
    chat.logger = match ConversationLogger::new(&chat.work_dir).await {
        Ok(l) => Some(l),
        Err(e) => {
            eprintln!("Logging disabled: {}", e);
            None
        }
    };

    // If logger was created, log the initial system message that KimiChat::new added
    if let Some(logger) = &mut chat.logger {
        // The first message in chat.messages is the system prompt
        if let Some(sys_msg) = chat.messages.first() {
            logger
                .log(
                    "system",
                    &sys_msg.content,
                    None,
                    false,
                )
                .await;
        }
    }

    // Run session-start hook to inject skill context
    let hook_path = chat.work_dir.join("hooks/session-start.sh");
    if hook_path.exists() {
        use std::process::Command;
        match Command::new(&hook_path)
            .arg(chat.work_dir.to_string_lossy().to_string())
            .output()
        {
            Ok(output) if output.status.success() => {
                let hook_content = String::from_utf8_lossy(&output.stdout).to_string();
                if !hook_content.trim().is_empty() {
                    // Add hook output as a system message
                    chat.messages.push(Message {
                        role: "system".to_string(),
                        content: hook_content,
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });

                    if cli.verbose {
                        println!("{}", "✓ Session-start hook executed".green());
                    }
                }
            }
            Ok(output) => {
                eprintln!("{} Session-start hook failed: {}",
                    "⚠️".yellow(),
                    String::from_utf8_lossy(&output.stderr));
            }
            Err(e) => {
                eprintln!("{} Failed to execute session-start hook: {}", "⚠️".yellow(), e);
            }
        }
    }

    let mut rl = DefaultEditor::new()?;

    // Read kimi.md if it exists to get project context
    let kimi_context = if let Ok(kimi_content) = chat.read_file("kimi.md") {
        println!("{} {}", "📖".bright_cyan(), "Reading project context from kimi.md...".bright_black());
        kimi_content
    } else {
        println!("{} {}", "📖".bright_cyan(), "No kimi.md found. Starting fresh.".bright_black());
        String::new()
    };

    if !kimi_context.is_empty() {
        let sys_msg = Message {
            role: "system".to_string(),
            content: format!("Project context: {}", kimi_context),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        };
        // Log this system addition
        if let Some(logger) = &mut chat.logger {
            logger
                .log("system", &sys_msg.content, None, false)
                .await;
        }
        chat.messages.push(sys_msg);
    }

    // Set up a persistent Ctrl-C handler for the entire REPL session
    // This holds the current operation's cancellation token
    let current_token: std::sync::Arc<std::sync::Mutex<Option<tokio_util::sync::CancellationToken>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let current_token_for_handler = current_token.clone();

    // Spawn a single Ctrl-C handler that will last the entire session
    tokio::spawn(async move {
        loop {
            if tokio::signal::ctrl_c().await.is_ok() {
                if let Ok(guard) = current_token_for_handler.lock() {
                    if let Some(ref token) = *guard {
                        println!("\n{}", "^C - Interrupting...".bright_yellow());
                        token.cancel();
                    }
                }
            }
        }
    });

    loop {
        let model_indicator = format!("[{}]", chat.current_model.display_name()).bright_magenta();
        let readline = rl.readline(&format!("{} {} ", model_indicator, "You:".bright_green().bold()));

        match readline {
            Ok(line) => {
                let line = line.trim();

                if line.is_empty() {
                    continue;
                }

                if line == "exit" || line == "quit" {
                    println!("{}", "Goodbye!".bright_cyan());
                    break;
                }

                // Handle /save and /load commands
                if line.starts_with("/save ") {
                    let file_path = line[6..].trim();
                    match chat.save_state(file_path) {
                        Ok(msg) => println!("{} {}", "💾".bright_green(), msg),
                        Err(e) => eprintln!("{} Failed to save: {}", "❌".bright_red(), e),
                    }
                    continue;
                }

                if line.starts_with("/load ") {
                    let file_path = line[6..].trim();
                    match chat.load_state(file_path) {
                        Ok(msg) => println!("{} {}", "📂".bright_green(), msg),
                        Err(e) => eprintln!("{} Failed to load: {}", "❌".bright_red(), e),
                    }
                    continue;
                }

                // Handle /debug command
                if line == "/debug" {
                    println!("{} Debug level: {} (binary: {:b})", "🔧".bright_cyan(), chat.get_debug_level(), chat.get_debug_level());
                    println!("{} Usage: /debug <level>", "💡".bright_yellow());
                    println!("  0 = off");
                    println!("  1 = basic (bit 0)");
                    println!("  2 = detailed (bit 1)");
                    println!("  4 = verbose (bit 2)");
                    println!("  Example: /debug 3 (enables basic + detailed)");
                    continue;
                }

                if line.starts_with("/debug ") {
                    let level_str = line[7..].trim();
                    match level_str.parse::<u32>() {
                        Ok(level) => {
                            chat.set_debug_level(level);
                            println!("{} Debug level set to {} (binary: {:b})", "🔧".bright_green(), level, level);
                        }
                        Err(_) => {
                            eprintln!("{} Invalid debug level: '{}'. Use a number like 0, 1, 3, 7, etc.", "❌".bright_red(), level_str);
                        }
                    }
                    continue;
                }

                // Handle /session commands
                if line == "/session" || line == "/session help" {
                    println!("{} Session commands:", "🖥️".bright_cyan());
                    println!("  /session list           - List all terminal sessions");
                    println!("  /session show <id>      - Show screen buffer of session");
                    println!("  /session help           - Show this help");
                    continue;
                }

                if line == "/session list" {
                    let manager = chat.terminal_manager.lock().unwrap();
                    let session_ids = manager.list_sessions();

                    if session_ids.is_empty() {
                        println!("{} No active terminal sessions", "ℹ️".bright_blue());
                    } else {
                        println!("{} Active terminal sessions:", "🖥️".bright_cyan());
                        for session_id in session_ids {
                            if let Ok(session_arc) = manager.get_session(session_id) {
                                let session = session_arc.lock().unwrap();
                                let metadata = session.metadata();

                                let status_icon = match &metadata.status {
                                    crate::terminal::SessionStatus::Running => "▶️",
                                    crate::terminal::SessionStatus::Exited(_) => "⏹️",
                                    crate::terminal::SessionStatus::Stopped => "⏸️",
                                };
                                let status_str = format!("{:?}", metadata.status);
                                println!("  {} Session {}: {} ({}x{}) - {}",
                                    status_icon,
                                    metadata.id,
                                    metadata.command,
                                    metadata.size.0,
                                    metadata.size.1,
                                    status_str
                                );
                            }
                        }
                    }
                    continue;
                }

                if line.starts_with("/session show ") {
                    let id_str = line[14..].trim();
                    match id_str.parse::<u32>() {
                        Ok(session_id) => {
                            let manager = chat.terminal_manager.lock().unwrap();
                            match manager.get_session(session_id) {
                                Ok(session_arc) => {
                                    let session = session_arc.lock().unwrap();
                                    match session.get_screen(false, true) {
                                        Ok(screen_contents) => {
                                            println!("{} Screen contents of session {}:", "📺".bright_cyan(), session_id);
                                            println!("┌{}┐", "─".repeat(session.metadata().size.0 as usize));
                                            println!("{}", screen_contents);
                                            println!("└{}┘", "─".repeat(session.metadata().size.0 as usize));
                                        }
                                        Err(e) => {
                                            eprintln!("{} Failed to get screen: {}", "❌".bright_red(), e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("{} Session {} not found: {}", "❌".bright_red(), session_id, e);
                                }
                            }
                        }
                        Err(_) => {
                            eprintln!("{} Invalid session ID: '{}'. Use a number.", "❌".bright_red(), id_str);
                        }
                    }
                    continue;
                }

                // Handle /skills command to show available skill commands
                if line == "/skills" || line == "/skills help" {
                    println!("{} Skill Commands:", "🎯".bright_cyan());
                    println!("  /brainstorm             - Use brainstorming skill for interactive design refinement");
                    println!("  /write-plan             - Use writing-plans skill to create detailed implementation plan");
                    println!("  /execute-plan           - Use executing-plans skill to execute plan with checkpoints");
                    println!("  /skills help            - Show this help");
                    continue;
                }

                // Handle /brainstorm command
                if line == "/brainstorm" {
                    if let Some(ref skill_registry) = chat.skill_registry {
                        match skill_registry.get_skill("brainstorming") {
                            Some(skill) => {
                                let skill_msg = Message {
                                    role: "system".to_string(),
                                    content: format!(
                                        "<skill_invocation>\n🎯 USING SKILL: {}\n\n{}\n\n**YOU MUST follow this skill exactly as written.**\n</skill_invocation>",
                                        skill.name, skill.content
                                    ),
                                    tool_calls: None,
                                    tool_call_id: None,
                                    name: None,
                                };
                                chat.messages.push(skill_msg.clone());

                                if let Some(logger) = &mut chat.logger {
                                    logger.log("system", &skill_msg.content, None, false).await;
                                }

                                println!("{} {} Brainstorming skill activated! 🎯", "✓".bright_green(), "Skill:".bright_cyan());
                                println!("{}", "Ask your question or describe what you want to brainstorm about.".bright_black());
                            }
                            None => {
                                eprintln!("{} Brainstorming skill not found. Ensure skills/ directory contains brainstorming/SKILL.md", "❌".bright_red());
                            }
                        }
                    } else {
                        eprintln!("{} Skill registry not available", "❌".bright_red());
                    }
                    continue;
                }

                // Handle /write-plan command
                if line == "/write-plan" {
                    if let Some(ref skill_registry) = chat.skill_registry {
                        match skill_registry.get_skill("writing-plans") {
                            Some(skill) => {
                                let skill_msg = Message {
                                    role: "system".to_string(),
                                    content: format!(
                                        "<skill_invocation>\n🎯 USING SKILL: {}\n\n{}\n\n**YOU MUST follow this skill exactly as written.**\n</skill_invocation>",
                                        skill.name, skill.content
                                    ),
                                    tool_calls: None,
                                    tool_call_id: None,
                                    name: None,
                                };
                                chat.messages.push(skill_msg.clone());

                                if let Some(logger) = &mut chat.logger {
                                    logger.log("system", &skill_msg.content, None, false).await;
                                }

                                println!("{} {} Writing-plans skill activated! 📋", "✓".bright_green(), "Skill:".bright_cyan());
                                println!("{}", "Describe what you want to plan and I'll create a detailed implementation plan.".bright_black());
                            }
                            None => {
                                eprintln!("{} Writing-plans skill not found. Ensure skills/ directory contains writing-plans/SKILL.md", "❌".bright_red());
                            }
                        }
                    } else {
                        eprintln!("{} Skill registry not available", "❌".bright_red());
                    }
                    continue;
                }

                // Handle /execute-plan command
                if line == "/execute-plan" {
                    if let Some(ref skill_registry) = chat.skill_registry {
                        match skill_registry.get_skill("executing-plans") {
                            Some(skill) => {
                                let skill_msg = Message {
                                    role: "system".to_string(),
                                    content: format!(
                                        "<skill_invocation>\n🎯 USING SKILL: {}\n\n{}\n\n**YOU MUST follow this skill exactly as written.**\n</skill_invocation>",
                                        skill.name, skill.content
                                    ),
                                    tool_calls: None,
                                    tool_call_id: None,
                                    name: None,
                                };
                                chat.messages.push(skill_msg.clone());

                                if let Some(logger) = &mut chat.logger {
                                    logger.log("system", &skill_msg.content, None, false).await;
                                }

                                println!("{} {} Executing-plans skill activated! 🚀", "✓".bright_green(), "Skill:".bright_cyan());
                                println!("{}", "I'll execute the plan in batches with review checkpoints.".bright_black());
                            }
                            None => {
                                eprintln!("{} Executing-plans skill not found. Ensure skills/ directory contains executing-plans/SKILL.md", "❌".bright_red());
                            }
                        }
                    } else {
                        eprintln!("{} Skill registry not available", "❌".bright_red());
                    }
                    continue;
                }

                rl.add_history_entry(line)?;

                // Log the user message before sending
                if let Some(logger) = &mut chat.logger {
                    logger.log("user", line, None, false).await;
                }

                let response = if chat.use_agents && chat.agent_coordinator.is_some() {
                    // Create cancellation token for this agent request
                    let cancel_token = tokio_util::sync::CancellationToken::new();

                    // Register this token with the persistent Ctrl-C handler
                    {
                        let mut guard = current_token.lock().unwrap();
                        *guard = Some(cancel_token.clone());
                    }

                    // Use agent system with cancellation support
                    let result = chat.process_with_agents(line, Some(cancel_token.clone())).await;

                    // Clear the current token after operation completes
                    {
                        let mut guard = current_token.lock().unwrap();
                        *guard = None;
                    }

                    match result {
                        Ok(response) => response,
                        Err(e) if e.to_string().contains("cancelled") || e.to_string().contains("interrupted") => {
                            println!("{}", "Task interrupted by user".bright_yellow());
                            continue;
                        }
                        Err(e) => {
                            eprintln!("{} {}\n", "Agent Error:".bright_red().bold(), e);
                            // Fallback to regular chat with same cancellation token
                            match crate::chat::session::chat(&mut chat, line, Some(cancel_token.clone())).await {
                                Ok(response) => response,
                                Err(e) if e.to_string().contains("interrupted") => {
                                    println!("{}", "Operation interrupted by user".bright_yellow());
                                    continue;
                                }
                                Err(e) => {
                                    eprintln!("{} {}\n", "Error:".bright_red().bold(), e);
                                    continue;
                                }
                            }
                        }
                    }
                } else {
                    // Use regular chat with cancellation support
                    let cancel_token = tokio_util::sync::CancellationToken::new();

                    // Register this token with the persistent Ctrl-C handler
                    {
                        let mut guard = current_token.lock().unwrap();
                        *guard = Some(cancel_token.clone());
                    }

                    let result = crate::chat::session::chat(&mut chat, line, Some(cancel_token.clone())).await;

                    // Clear the current token after operation completes
                    {
                        let mut guard = current_token.lock().unwrap();
                        *guard = None;
                    }

                    match result {
                        Ok(response) => response,
                        Err(e) if e.to_string().contains("interrupted") => {
                            println!("{}", "Operation interrupted by user".bright_yellow());
                            continue;
                        }
                        Err(e) => {
                            eprintln!("{} {}\n", "Error:".bright_red().bold(), e);
                            continue;
                        }
                    }
                };

                // Log assistant response
                if let Some(logger) = &mut chat.logger {
                    logger.log("assistant", &response, None, false).await;
                }

                // Display response if not streaming (streaming already displayed it)
                if !chat.stream_responses {
                    let model_label = format!("[{}]", chat.current_model.display_name()).bright_magenta();
                    let assistant_label = "Assistant:".bright_blue().bold();
                    println!("\n{} {} {}\n", model_label, assistant_label, response);
                } else {
                    // Add extra newline after streaming to separate from next prompt
                    println!();
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("{}", "^C".bright_black());
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!("{}", "Goodbye!".bright_cyan());
                break;
            }
            Err(err) => {
                eprintln!("{} {}", "Error:".bright_red().bold(), err);
                break;
            }
        }
    }

    // Graceful shutdown of logger (flush & close)
    if let Some(logger) = &mut chat.logger {
        logger.shutdown().await;
    }

    Ok(())
}
