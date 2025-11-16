use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use kimichat_policy::PolicyManager;
use kimichat_terminal::TerminalManager;
use kimichat_skills::SkillRegistry;
use kimichat_todo::TodoManager;

/// Tool execution context
///
/// This struct provides the execution context for tools, including:
/// - Working directory for file operations
/// - Session identifier for tracking operations
/// - Environment variables for configuration
/// - Policy manager for permission checking
/// - Terminal manager for PTY session management
/// - Skill registry for accessing skills
/// - Todo manager for task tracking
/// - Non-interactive flag for web/API mode
#[derive(Debug, Clone)]
pub struct ToolContext {
    pub work_dir: PathBuf,
    pub session_id: String,
    pub environment: HashMap<String, String>,
    pub policy_manager: PolicyManager,
    pub terminal_manager: Option<Arc<Mutex<TerminalManager>>>,
    pub skill_registry: Option<Arc<SkillRegistry>>,
    pub todo_manager: Option<Arc<TodoManager>>,
    pub non_interactive: bool,
}

impl ToolContext {
    pub fn new(work_dir: PathBuf, session_id: String, policy_manager: PolicyManager) -> Self {
        Self {
            work_dir,
            session_id,
            environment: HashMap::new(),
            policy_manager,
            terminal_manager: None,
            skill_registry: None,
            todo_manager: None,
            non_interactive: false,
        }
    }

    pub fn with_non_interactive(mut self, non_interactive: bool) -> Self {
        self.non_interactive = non_interactive;
        self
    }

    pub fn with_env(mut self, key: String, value: String) -> Self {
        self.environment.insert(key, value);
        self
    }

    pub fn with_terminal_manager(mut self, terminal_manager: Arc<Mutex<TerminalManager>>) -> Self {
        self.terminal_manager = Some(terminal_manager);
        self
    }

    pub fn with_skill_registry(mut self, skill_registry: Arc<SkillRegistry>) -> Self {
        self.skill_registry = Some(skill_registry);
        self
    }

    pub fn with_todo_manager(mut self, todo_manager: Arc<TodoManager>) -> Self {
        self.todo_manager = Some(todo_manager);
        self
    }

    /// Check if an action is permitted by the policy
    /// Returns (approved: bool, rejection_reason: Option<String>)
    pub fn check_permission(
        &self,
        action: kimichat_policy::ActionType,
        target: &str,
        prompt_message: &str,
    ) -> anyhow::Result<(bool, Option<String>)> {
        use kimichat_policy::Decision;
        use colored::Colorize;
        use std::io::{self, BufRead, Write};

        let decision = self.policy_manager.evaluate(&action, target);

        match decision {
            Decision::Allow => Ok((true, None)),
            Decision::Deny => Ok((false, Some("Denied by policy".to_string()))),
            Decision::Ask => {
                // In non-interactive mode (web/API), auto-approve since confirmation
                // was already handled via web UI
                if self.non_interactive {
                    println!("{} {}", "✓".green(), "Auto-confirmed (web UI)".bright_black());
                    return Ok((true, None));
                }

                // Ask the user for confirmation in interactive mode
                println!("\n{}", prompt_message.bright_green().bold());
                print!(">>> ");
                io::stdout().flush()?;

                let stdin = io::stdin();
                let mut handle = stdin.lock();
                let mut response = String::new();
                handle.read_line(&mut response)?;

                let response = response.trim();
                let response_lower = response.to_lowercase();
                let approved = response_lower.is_empty() || response_lower == "y" || response_lower == "yes";

                let rejection_reason = if !approved {
                    // Ask for reason if rejected
                    println!("{}", "Why not? (optional - helps the AI understand):".bright_yellow());
                    print!(">>> ");
                    io::stdout().flush()?;

                    let mut reason = String::new();
                    match handle.read_line(&mut reason) {
                        Ok(_) => {
                            let reason = reason.trim();
                            if reason.is_empty() {
                                None
                            } else {
                                Some(reason.to_string())
                            }
                        }
                        Err(_) => None,
                    }
                } else {
                    None
                };

                // Learn from the user's decision if learning is enabled
                if self.policy_manager.is_learning() {
                    let decision = if approved { Decision::Allow } else { Decision::Deny };
                    let _ = self.policy_manager.learn(action, target.to_string(), decision, rejection_reason.clone());
                }

                Ok((approved, rejection_reason))
            }
        }
    }
}