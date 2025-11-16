use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Types of actions that can be governed by policies
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    /// Reading file contents
    FileRead,
    /// Writing to a file (create or overwrite)
    FileWrite,
    /// Editing an existing file
    FileEdit,
    /// Deleting a file
    FileDelete,
    /// Executing a shell command
    CommandExecution,
    /// Planning batch edits
    PlanEdits,
    /// Applying a batch edit plan
    ApplyEditPlan,
}

impl std::fmt::Display for ActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionType::FileRead => write!(f, "file_read"),
            ActionType::FileWrite => write!(f, "file_write"),
            ActionType::FileEdit => write!(f, "file_edit"),
            ActionType::FileDelete => write!(f, "file_delete"),
            ActionType::CommandExecution => write!(f, "command_execution"),
            ActionType::PlanEdits => write!(f, "plan_edits"),
            ActionType::ApplyEditPlan => write!(f, "apply_edit_plan"),
        }
    }
}

/// Policy decision for an action
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    /// Allow the action without asking
    Allow,
    /// Deny the action without asking
    Deny,
    /// Ask the user for confirmation
    Ask,
}

impl std::fmt::Display for Decision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Decision::Allow => write!(f, "allow"),
            Decision::Deny => write!(f, "deny"),
            Decision::Ask => write!(f, "ask"),
        }
    }
}

/// A single policy rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    /// Type of action this rule applies to
    pub action: ActionType,
    /// Pattern to match against the target (glob for files, string pattern for commands)
    pub pattern: String,
    /// Decision to make when this rule matches
    pub decision: Decision,
    /// Optional description explaining the rule
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl PolicyRule {
    pub fn new(action: ActionType, pattern: String, decision: Decision) -> Self {
        Self {
            action,
            pattern,
            decision,
            description: None,
        }
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Check if this rule matches the given action and target
    pub fn matches(&self, action: &ActionType, target: &str) -> bool {
        if &self.action != action {
            return false;
        }

        // For file operations, use glob matching
        match action {
            ActionType::FileRead
            | ActionType::FileWrite
            | ActionType::FileEdit
            | ActionType::FileDelete => {
                // Simple glob matching - we can enhance this later
                glob_match(&self.pattern, target)
            }
            ActionType::CommandExecution => {
                // For commands, use prefix matching or wildcards
                command_match(&self.pattern, target)
            }
            ActionType::PlanEdits | ActionType::ApplyEditPlan => {
                // These don't have specific targets, match all
                true
            }
        }
    }
}

/// Policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    /// Default decision when no rules match
    #[serde(default = "default_decision")]
    pub default: Decision,
    /// List of policy rules
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
}

fn default_decision() -> Decision {
    Decision::Ask
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            default: Decision::Ask,
            rules: Vec::new(),
        }
    }
}

impl PolicyConfig {
    /// Create a policy config that allows everything
    pub fn allow_all() -> Self {
        Self {
            default: Decision::Allow,
            rules: Vec::new(),
        }
    }

    /// Load policy from TOML file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: PolicyConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save policy to TOML file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Evaluate an action against the policy
    pub fn evaluate(&self, action: &ActionType, target: &str) -> Decision {
        // Find the first matching rule
        for rule in &self.rules {
            if rule.matches(action, target) {
                return rule.decision.clone();
            }
        }
        // No matching rule, use default
        self.default.clone()
    }

    /// Add a new rule to the policy
    pub fn add_rule(&mut self, rule: PolicyRule) {
        self.rules.push(rule);
    }

    /// Check if a rule already exists for the given action and target
    pub fn has_rule_for(&self, action: &ActionType, target: &str) -> bool {
        self.rules.iter().any(|rule| rule.matches(action, target))
    }
}

/// Policy manager that handles policy loading, evaluation, and learning
#[derive(Clone, Debug)]
pub struct PolicyManager {
    config: Arc<RwLock<PolicyConfig>>,
    policy_file: Option<PathBuf>,
    learn_mode: bool,
}

impl PolicyManager {
    /// Create a new policy manager with default (ask everything) policy
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(PolicyConfig::default())),
            policy_file: None,
            learn_mode: false,
        }
    }

    /// Create a policy manager that allows everything (auto-pilot mode)
    pub fn allow_all() -> Self {
        Self {
            config: Arc::new(RwLock::new(PolicyConfig::allow_all())),
            policy_file: None,
            learn_mode: false,
        }
    }

    /// Create a policy manager from a file
    pub fn from_file<P: AsRef<Path>>(path: P, learn_mode: bool) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let config = if path_buf.exists() {
            PolicyConfig::load_from_file(&path_buf)?
        } else {
            // Create default policy file if it doesn't exist
            let config = PolicyConfig::default();
            if let Some(parent) = path_buf.parent() {
                std::fs::create_dir_all(parent)?;
            }
            config.save_to_file(&path_buf)?;
            eprintln!("📋 Created default policy file: {}", path_buf.display());
            config
        };

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            policy_file: Some(path_buf),
            learn_mode,
        })
    }

    /// Evaluate an action against the policy
    pub fn evaluate(&self, action: &ActionType, target: &str) -> Decision {
        let config = self.config.read().unwrap();
        config.evaluate(action, target)
    }

    /// Learn from a user decision (saves to policy file if in learn mode)
    pub fn learn(&self, action: ActionType, target: String, decision: Decision, reason: Option<String>) -> Result<()> {
        if !self.learn_mode {
            return Ok(());
        }

        let mut config = self.config.write().unwrap();

        // Don't add duplicate rules
        if config.has_rule_for(&action, &target) {
            return Ok(());
        }

        // Create a new rule based on the user's decision
        let description = if let Some(ref reason_text) = reason {
            format!("Learned from user decision: {}", reason_text)
        } else {
            "Learned from user decision".to_string()
        };

        let rule = PolicyRule::new(action.clone(), target.clone(), decision.clone())
            .with_description(description);

        config.add_rule(rule);

        // Save to file if we have a policy file path
        if let Some(ref path) = self.policy_file {
            config.save_to_file(path)?;
            if let Some(reason_text) = reason {
                eprintln!(
                    "📚 Learned policy: {} {} -> {} (reason: {})",
                    action, target, decision, reason_text
                );
            } else {
                eprintln!(
                    "📚 Learned policy: {} {} -> {}",
                    action, target, decision
                );
            }
        }

        Ok(())
    }

    /// Get the policy file path
    pub fn policy_file(&self) -> Option<&Path> {
        self.policy_file.as_deref()
    }

    /// Check if learning mode is enabled
    pub fn is_learning(&self) -> bool {
        self.learn_mode
    }

    /// Export current policy to a file
    pub fn export_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let config = self.config.read().unwrap();
        config.save_to_file(path)
    }
}

impl Default for PolicyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple glob matching implementation
fn glob_match(pattern: &str, target: &str) -> bool {
    // Handle common glob patterns
    if pattern == "**" || pattern == "*" {
        return true;
    }

    // Convert glob pattern to regex-like matching
    // For simplicity, we support:
    // * - matches anything in a path component
    // ** - matches anything including path separators
    // ? - matches single character

    let pattern = pattern.replace('\\', "/");
    let target = target.replace('\\', "/");

    // Handle ** wildcard
    if pattern.contains("**") {
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1].trim_start_matches('/');

            let prefix_match = if prefix.is_empty() {
                true
            } else {
                target.starts_with(prefix)
            };

            let suffix_match = if suffix.is_empty() {
                true
            } else {
                target.ends_with(suffix) || target.contains(&format!("/{}", suffix))
            };

            return prefix_match && suffix_match;
        }
    }

    // Simple * wildcard matching
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        let mut pos = 0;
        for (i, part) in parts.iter().enumerate() {
            if i == 0 {
                if !target[pos..].starts_with(part) {
                    return false;
                }
                pos += part.len();
            } else if i == parts.len() - 1 {
                return target[pos..].ends_with(part);
            } else {
                if let Some(found_pos) = target[pos..].find(part) {
                    pos += found_pos + part.len();
                } else {
                    return false;
                }
            }
        }
        return true;
    }

    // Exact match
    pattern == target
}

/// Simple command matching implementation
fn command_match(pattern: &str, command: &str) -> bool {
    // Support wildcards in command patterns
    if pattern == "*" {
        return true;
    }

    // Handle "command *" pattern (e.g., "cargo *")
    if pattern.ends_with(" *") {
        let prefix = pattern.trim_end_matches(" *");
        return command.starts_with(prefix);
    }

    // Handle "* command" pattern
    if pattern.starts_with("* ") {
        let suffix = pattern.trim_start_matches("* ");
        return command.ends_with(suffix);
    }

    // Exact match
    pattern == command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match() {
        assert!(glob_match("**/*.rs", "src/main.rs"));
        assert!(glob_match("**/*.rs", "src/tools/system.rs"));
        assert!(!glob_match("**/*.rs", "README.md"));
        assert!(glob_match("src/**", "src/main.rs"));
        assert!(glob_match("src/**", "src/tools/system.rs"));
        assert!(glob_match("*.md", "README.md"));
        assert!(!glob_match("*.md", "src/main.rs"));
    }

    #[test]
    fn test_command_match() {
        assert!(command_match("cargo *", "cargo build"));
        assert!(command_match("cargo *", "cargo test --all"));
        assert!(!command_match("cargo *", "rustc main.rs"));
        assert!(command_match("*", "any command"));
    }

    #[test]
    fn test_policy_evaluation() {
        let mut config = PolicyConfig::default();
        config.add_rule(
            PolicyRule::new(
                ActionType::FileWrite,
                "**/*.md".to_string(),
                Decision::Allow,
            )
        );
        config.add_rule(
            PolicyRule::new(
                ActionType::CommandExecution,
                "rm *".to_string(),
                Decision::Deny,
            )
        );

        assert_eq!(
            config.evaluate(&ActionType::FileWrite, "README.md"),
            Decision::Allow
        );
        assert_eq!(
            config.evaluate(&ActionType::CommandExecution, "rm file.txt"),
            Decision::Deny
        );
        assert_eq!(
            config.evaluate(&ActionType::FileEdit, "src/main.rs"),
            Decision::Ask
        );
    }
}
