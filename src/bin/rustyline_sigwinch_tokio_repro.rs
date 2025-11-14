/// Accurate rustyline SIGWINCH panic reproduction with tokio + nested editors
///
/// This closely simulates the actual kimichat multi-agent scenario:
/// - Main thread runs tokio runtime
/// - Agents execute async tasks
/// - Tool confirmations create temporary rustyline editors DURING async execution
/// - Window resize while tokio is parked/waiting → panic
///
/// To reproduce:
/// 1. Run: cargo run --bin rustyline_sigwinch_tokio_repro
/// 2. Type "run" to start an agent task
/// 3. Agent will ask for tool confirmations (creates nested rustyline editors)
/// 4. Answer the confirmation prompts
/// 5. While agent is working, resize terminal window
/// 6. May panic with "fd != -1"

use rustyline::DefaultEditor;
use std::io::{self, Write};
use std::time::Duration;
use tokio::time::sleep;

/// Simulates a tool confirmation using temporary rustyline editor
/// This creates a nested editor while the main REPL editor exists
fn get_tool_confirmation(tool_name: &str) -> bool {
    println!("\n  🔧 Tool: {}", tool_name);
    println!("  [Creating temporary rustyline editor for confirmation]");

    // Create temporary editor - this is the problematic pattern!
    let mut rl = match DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("  Failed to create confirmation editor: {}", e);
            return false;
        }
    };

    match rl.readline("  Execute this tool? (y/n) >>> ") {
        Ok(line) => {
            let response = line.trim().to_lowercase();
            response == "y" || response == "yes"
        }
        Err(_) => false,
    }
    // Editor dropped here, but we're about to go back into tokio execution!
}

/// Simulates an agent executing multiple tool calls
async fn run_agent_task(task_name: &str) {
    println!("\n🤖 Agent started: {}", task_name);
    println!("════════════════════════════════════════════════════════\n");

    // Simulate agent making multiple tool calls
    for i in 1..=3 {
        println!("📋 Agent iteration {}/3", i);

        // Simulate thinking
        sleep(Duration::from_millis(500)).await;

        // Agent wants to call a tool - needs confirmation
        // This creates a nested rustyline editor!
        if !get_tool_confirmation(&format!("read_file_{}.txt", i)) {
            println!("  ✗ Tool cancelled");
            continue;
        }

        println!("  ✓ Tool executed");
        println!("  [Temporary editor dropped, back in tokio async context]\n");

        println!("╔═══════════════════════════════════════════════════════════╗");
        println!("║  👉 RESIZE TERMINAL NOW (while agent is working)          ║");
        println!("║                                                           ║");
        println!("║  The nested editor was dropped. Now in tokio async       ║");
        println!("║  execution. SIGWINCH during tokio parking may panic.     ║");
        println!("╚═══════════════════════════════════════════════════════════╝\n");

        // Simulate tool execution and agent thinking
        for j in 1..=5 {
            print!("\r  ⏱️  Agent processing... {} seconds (resize window now!)", j);
            io::stdout().flush().unwrap();
            sleep(Duration::from_secs(1)).await;
        }
        println!();
    }

    println!("\n✅ Agent task completed: {}", task_name);
    println!("════════════════════════════════════════════════════════\n");
}

#[tokio::main]
async fn main() {
    println!("=== rustyline SIGWINCH + Tokio + Nested Editors Reproduction ===\n");
    println!("This simulates the exact kimichat multi-agent scenario:\n");
    println!("1. Main REPL uses rustyline DefaultEditor");
    println!("2. Agents execute async tasks in tokio runtime");
    println!("3. Tool confirmations create NESTED temporary editors");
    println!("4. After confirmation, execution returns to tokio async context");
    println!("5. Window resize while tokio is parked → SIGWINCH → PANIC\n");

    // Create main REPL editor
    let mut main_rl = match DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("Failed to create main REPL editor: {}", e);
            return;
        }
    };

    println!("Main REPL started. Commands:");
    println!("  run   - Start an agent task (creates nested editors for confirmations)");
    println!("  quit  - Exit\n");

    loop {
        match main_rl.readline(">>> ") {
            Ok(line) => {
                let line = line.trim();

                if line == "quit" || line == "exit" {
                    println!("Exiting...");
                    break;
                }

                if line == "run" {
                    // Launch agent task
                    // The agent will create nested rustyline editors for confirmations
                    // Then return to tokio async execution
                    run_agent_task("Example Task").await;

                    println!("Try running 'run' again and resize during agent execution!");
                } else if !line.is_empty() {
                    println!("Unknown command: {}", line);
                    println!("Try: run, quit");
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                println!("\n^C - Use 'quit' to exit");
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                println!("\n^D");
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }
}
