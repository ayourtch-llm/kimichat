use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::policy::PolicyManager;
use crate::terminal::TerminalManager;
use crate::skills::SkillRegistry;

/// Tool execution context
///
/// This struct provides the execution context for tools, including:
/// - Working directory for file operations
/// - Session identifier for tracking operations
/// - Environment variables for configuration
/// - Policy manager for permission checking
/// - Terminal manager for PTY session management
/// - Skill registry for accessing skills
#[derive(Debug, Clone)]
pub struct ToolContext {
    pub work_dir: PathBuf,
    pub session_id: String,
    pub environment: HashMap<String, String>,
    pub policy_manager: PolicyManager,
    pub terminal_manager: Option<Arc<Mutex<TerminalManager>>>,
    pub skill_registry: Option<Arc<SkillRegistry>>,
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
        }
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

    /// Check if an action is permitted by the policy
    pub fn check_permission(
        &self,
        action: crate::policy::ActionType,
        target: &str,
        prompt_message: &str,
    ) -> anyhow::Result<bool> {
        use crate::policy::Decision;
        use colored::Colorize;
        use rustyline::DefaultEditor;

        let decision = self.policy_manager.evaluate(&action, target);

        match decision {
            Decision::Allow => Ok(true),
            Decision::Deny => Ok(false),
            Decision::Ask => {
                // Ask the user for confirmation
                println!("\n{}", prompt_message.bright_green().bold());

                let mut rl = DefaultEditor::new()
                    .map_err(|e| anyhow::anyhow!("Failed to create readline editor: {}", e))?;

                let response = rl.readline(">>> ")
                    .map_err(|_| anyhow::anyhow!("Cancelled by user"))?;

                let response = response.trim().to_lowercase();
                let approved = response.is_empty() || response == "y" || response == "yes";

                // Learn from the user's decision if learning is enabled
                if self.policy_manager.is_learning() {
                    let decision = if approved { Decision::Allow } else { Decision::Deny };
                    let _ = self.policy_manager.learn(action, target.to_string(), decision);
                }

                Ok(approved)
            }
        }
    }
}