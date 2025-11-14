/// Minimal reproduction of rustyline SIGWINCH panic with nested editors
///
/// This demonstrates the bug by creating multiple DefaultEditor instances,
/// simulating a REPL that also creates temporary editors for confirmations.
///
/// To reproduce:
/// 1. Run this program: cargo run --bin rustyline_sigwinch_repro
/// 2. At first prompt, type: confirm
/// 3. At confirmation prompt, type: y
/// 4. Program will loop waiting for commands
/// 5. Resize your terminal window multiple times
/// 6. May panic with "fd != -1" from rustyline's signal handler
///
/// The bug is more likely to occur with nested editor creation.

use rustyline::DefaultEditor;
use std::io::{self, Write};

fn get_confirmation_with_rustyline() -> bool {
    println!("\n  [Creating temporary rustyline editor for confirmation]");

    // This creates a SECOND rustyline editor while the main one exists
    // Multiple signal handler registrations may conflict
    let mut rl = match DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("Failed to create confirmation editor: {}", e);
            return false;
        }
    };

    match rl.readline("  Confirm? (y/n) >>> ") {
        Ok(line) => {
            let response = line.trim().to_lowercase();
            response == "y" || response == "yes"
        }
        Err(_) => false,
    }
    // Temporary editor dropped here!
}

fn main() {
    println!("=== rustyline SIGWINCH Nested Editors Reproduction ===\n");
    println!("This simulates a REPL with nested rustyline instances:");
    println!("- Main REPL uses DefaultEditor");
    println!("- Confirmation prompts create temporary DefaultEditor");
    println!("- Multiple signal handler registrations may conflict\n");

    // Create the main REPL editor
    let mut main_rl = match DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("Failed to create main editor: {}", e);
            return;
        }
    };

    println!("Main REPL started. Available commands:");
    println!("  confirm  - Trigger a nested rustyline confirmation prompt");
    println!("  quit     - Exit");
    println!("\nAfter typing 'confirm' and answering, try resizing the window!\n");

    loop {
        match main_rl.readline(">>> ") {
            Ok(line) => {
                let line = line.trim();

                if line == "quit" || line == "exit" {
                    println!("Exiting...");
                    break;
                }

                if line == "confirm" {
                    // Create nested rustyline editor
                    if get_confirmation_with_rustyline() {
                        println!("✓ Confirmed! (nested editor dropped)");
                    } else {
                        println!("✗ Cancelled! (nested editor dropped)");
                    }

                    println!("\n╔═══════════════════════════════════════════════════════════╗");
                    println!("║  👉 RESIZE YOUR TERMINAL WINDOW NOW                       ║");
                    println!("║                                                           ║");
                    println!("║  The nested editor was just dropped but signal           ║");
                    println!("║  handlers may still be active. Resize may trigger panic. ║");
                    println!("╚═══════════════════════════════════════════════════════════╝\n");
                } else if !line.is_empty() {
                    println!("Unknown command: {}", line);
                    println!("Try: confirm, quit");
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                println!("^C");
                break;
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                println!("^D");
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }
}
