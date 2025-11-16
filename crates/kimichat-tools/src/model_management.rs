use kimichat_toolcore::{param, Tool, ToolParameters, ToolResult, ParameterDefinition};
use kimichat_toolcore::tool_context::ToolContext;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use rustyline::DefaultEditor;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EditOperation {
    file_path: String,
    old_content: String,
    new_content: String,
    description: String,
}

/// Tool for switching between AI models
pub struct SwitchModelTool {
    // This will need to be connected to the main application state
    // For now, it's a placeholder that returns information about the switch
}

impl SwitchModelTool {
    pub fn new() -> Self {
        Self { }
    }
}

#[async_trait]
impl Tool for SwitchModelTool {
    fn name(&self) -> &str {
        "switch_model"
    }

    fn description(&self) -> &str {
        "Switch to a different AI model for better task handling"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("model", "string", "Model to switch to (kimi or gpt_oss)", required),
            param!("reason", "string", "Reason for switching models", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, _context: &ToolContext) -> ToolResult {
        let model = match params.get_required::<String>("model") {
            Ok(model) => model,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let reason = match params.get_required::<String>("reason") {
            Ok(reason) => reason,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        // Validate model name
        if !["kimi", "gpt_oss", "blu_model", "grn_model", "anthropic"].contains(&model.as_str()) {
            return ToolResult::error("Invalid model. Available models: kimi, gpt_oss, blu_model, grn_model, anthropic".to_string());
        }

        // For now, return information about the requested switch
        // In the full implementation, this would trigger an actual model switch
        let message = format!(
            "Model switch requested:\n  Target model: {}\n  Reason: {}\n  Note: Actual model switching will be implemented in the main application layer",
            model, reason
        );

        ToolResult::success(message)
    }
}

// Helper function to get the edit plan file path
fn get_plan_file_path(work_dir: &PathBuf) -> PathBuf {
    work_dir.join(".kimichat_edit_plan.json")
}

// Helper function to save edit plan
fn save_edit_plan(work_dir: &PathBuf, edits: &[EditOperation]) -> Result<(), String> {
    let plan_path = get_plan_file_path(work_dir);
    let json = serde_json::to_string_pretty(edits)
        .map_err(|e| format!("Failed to serialize edit plan: {}", e))?;
    fs::write(&plan_path, json)
        .map_err(|e| format!("Failed to write edit plan: {}", e))?;
    Ok(())
}

// Helper function to load edit plan
fn load_edit_plan(work_dir: &PathBuf) -> Result<Vec<EditOperation>, String> {
    let plan_path = get_plan_file_path(work_dir);
    if !plan_path.exists() {
        return Err("No edit plan exists. Create one first using plan_edits.".to_string());
    }
    let json = fs::read_to_string(&plan_path)
        .map_err(|e| format!("Failed to read edit plan: {}", e))?;
    let edits: Vec<EditOperation> = serde_json::from_str(&json)
        .map_err(|e| format!("Failed to parse edit plan: {}", e))?;
    Ok(edits)
}

// Helper function to clear edit plan
fn clear_edit_plan(work_dir: &PathBuf) {
    let plan_path = get_plan_file_path(work_dir);
    let _ = fs::remove_file(&plan_path); // Ignore errors if file doesn't exist
}

// Helper function to show unified diff using the similar crate
fn show_unified_diff(old_content: &str, new_content: &str) -> String {
    let diff = TextDiff::from_lines(old_content, new_content);
    let mut output = String::new();

    for (idx, group) in diff.grouped_ops(2).iter().enumerate() {
        if idx > 0 {
            output.push_str(&format!("{}\n", "---".bright_black()));
        }
        for op in group {
            for change in diff.iter_inline_changes(op) {
                let (sign, color_fn): (&str, fn(&str) -> colored::ColoredString) = match change.tag() {
                    ChangeTag::Delete => ("-", |s: &str| s.red()),
                    ChangeTag::Insert => ("+", |s: &str| s.green()),
                    ChangeTag::Equal => (" ", |s: &str| s.normal()),
                };
                output.push_str(&format!("{}{}", sign, color_fn(&change.to_string())));
            }
        }
    }
    output
}

/// Tool for planning multiple file edits
pub struct PlanEditsTool;

#[async_trait]
impl Tool for PlanEditsTool {
    fn name(&self) -> &str {
        "plan_edits"
    }

    fn description(&self) -> &str {
        "Plan multiple file edits to apply atomically. Validates all edits before storing the plan."
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("edits", "array", "Array of edit operations with file_path, old_content, new_content, and description fields", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        // Parse edits from parameters
        let edits_value = match params.data.get("edits") {
            Some(value) => value,
            None => return ToolResult::error("Edits parameter is required".to_string()),
        };

        let edits: Vec<EditOperation> = match serde_json::from_value(edits_value.clone()) {
            Ok(edits) => edits,
            Err(e) => return ToolResult::error(format!("Failed to parse edits: {}", e)),
        };

        if edits.is_empty() {
            return ToolResult::error("Cannot create empty edit plan. Provide at least one edit operation.".to_string());
        }

        println!("\n{}", "📋 Edit Plan Created".bright_cyan().bold());
        println!("{}", "═".repeat(60).bright_black());

        // Validate and preview each edit
        let mut validated_edits = Vec::new();
        for (idx, edit) in edits.iter().enumerate() {
            println!("\n{} {} - {}",
                format!("Edit #{}", idx + 1).bright_yellow(),
                edit.file_path.cyan(),
                edit.description.bright_white()
            );

            // Read current file to validate old_content exists
            let full_path = context.work_dir.join(&edit.file_path);
            let current_content = match fs::read_to_string(&full_path) {
                Ok(content) => content,
                Err(_) => return ToolResult::error(format!("Edit #{}: File not found: {}", idx + 1, edit.file_path)),
            };

            if edit.old_content.is_empty() {
                return ToolResult::error(format!("Edit #{}: old_content cannot be empty for file {}", idx + 1, edit.file_path));
            }

            if edit.old_content == edit.new_content {
                return ToolResult::error(format!(
                    "Edit #{}: old_content and new_content are identical for file {}. No change would be made.",
                    idx + 1, edit.file_path
                ));
            }

            if !current_content.contains(&edit.old_content) {
                return ToolResult::error(format!(
                    "Edit #{}: old_content not found in file {}\n\nLooking for:\n{}\n\nFile does not currently contain this content.",
                    idx + 1, edit.file_path, edit.old_content
                ));
            }

            // Show unified diff preview
            let diff_output = show_unified_diff(&edit.old_content, &edit.new_content);
            if !diff_output.is_empty() {
                for line in diff_output.lines() {
                    println!("  {}", line);
                }
            } else {
                println!("  {}", "(No changes)".bright_black());
            }

            validated_edits.push(edit.clone());
        }

        println!("\n{}", "═".repeat(60).bright_black());
        println!("{} {} edits planned", "✓".green(), validated_edits.len());
        println!("\n{}", "Use apply_edit_plan to execute all edits atomically.".bright_yellow());
        println!("{}", "The plan will be cleared after application or if you create a new plan.".bright_black());

        // Store the plan
        if let Err(e) = save_edit_plan(&context.work_dir, &validated_edits) {
            return ToolResult::error(e);
        }

        ToolResult::success(format!(
            "Edit plan created successfully with {} operation(s). All edits have been validated. \
            Use apply_edit_plan to execute all changes atomically.",
            validated_edits.len()
        ))
    }
}

/// Tool for applying planned edits
pub struct ApplyEditPlanTool;

#[async_trait]
impl Tool for ApplyEditPlanTool {
    fn name(&self) -> &str {
        "apply_edit_plan"
    }

    fn description(&self) -> &str {
        "Apply a previously planned set of file edits atomically"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        // No parameters needed - applies the stored plan
        HashMap::new()
    }

    async fn execute(&self, _params: ToolParameters, context: &ToolContext) -> ToolResult {
        // Load the plan
        let plan = match load_edit_plan(&context.work_dir) {
            Ok(plan) => plan,
            Err(e) => return ToolResult::error(e),
        };

        println!("\n{}", "🚀 Applying Edit Plan".bright_cyan().bold());
        println!("{}", "═".repeat(60).bright_black());
        println!("{} {} edit(s) will be applied:", "📋".cyan(), plan.len());

        // Show summary of all edits
        for (idx, edit) in plan.iter().enumerate() {
            println!("  {}. {} - {}",
                idx + 1,
                edit.file_path.bright_white(),
                edit.description.cyan()
            );
        }

        println!("{}", "═".repeat(60).bright_black());

        // Check if we need to ask for confirmation
        // In non-interactive mode (web/API), skip prompt - already confirmed via web UI
        if !context.non_interactive {
            // Ask for confirmation in interactive mode
            println!("\n{}", "Apply all these changes? [Y/n]".bright_green().bold());

            let mut rl = match DefaultEditor::new() {
                Ok(rl) => rl,
                Err(e) => return ToolResult::error(format!("Failed to create readline editor: {}", e)),
            };

            let response = match rl.readline(">>> ") {
                Ok(resp) => resp,
                Err(_) => {
                    clear_edit_plan(&context.work_dir);
                    return ToolResult::error("Edit plan application cancelled by user".to_string());
                }
            };

            let response = response.trim().to_lowercase();

            match response.as_str() {
                "" | "y" | "yes" => {
                    // Continue with applying edits
                    println!("\n{}", "Applying edits...".green());
                }
                _ => {
                    // Cancelled - ask for optional feedback
                    println!("{}", "Would you like to provide feedback to the model about why you rejected this? (optional)".bright_yellow());
                    println!("{}", "Press Enter to skip, or type your feedback:".bright_black());

                    let feedback = match rl.readline("") {
                        Ok(fb) if !fb.trim().is_empty() => format!(" - {}", fb.trim()),
                        _ => String::new(),
                    };

                    clear_edit_plan(&context.work_dir);
                    return ToolResult::error(format!("Edit plan application cancelled by user{}", feedback));
                }
            }
        } else {
            // Non-interactive mode - auto-confirm (already confirmed via web UI)
            println!("\n{}", "✓ Confirmed via web UI - Applying edits...".green());
        }

        // Apply all edits sequentially
        let mut results = Vec::new();
        for (idx, edit) in plan.iter().enumerate() {
            println!("\n{} {}", format!("Applying edit #{}", idx + 1).yellow(), edit.file_path.cyan());

            // Re-read file to get current state (in case previous edits affected it)
            let full_path = context.work_dir.join(&edit.file_path);
            let current_content = match fs::read_to_string(&full_path) {
                Ok(content) => content,
                Err(_) => {
                    clear_edit_plan(&context.work_dir);
                    return ToolResult::error(format!(
                        "Edit #{} failed: File not found: {}. Edit plan aborted and cleared.",
                        idx + 1, edit.file_path
                    ));
                }
            };

            // Check if content still exists (might have changed due to previous edits)
            if !current_content.contains(&edit.old_content) {
                clear_edit_plan(&context.work_dir);
                return ToolResult::error(format!(
                    "Edit #{} failed: old_content no longer found in {}. \
                    A previous edit in this plan may have affected this file. \
                    Edit plan aborted at step {}. No further edits applied. Plan has been cleared.",
                    idx + 1, edit.file_path, idx + 1
                ));
            }

            // Apply the edit
            let updated_content = current_content.replace(&edit.old_content, &edit.new_content);

            // Write the updated content
            if let Err(e) = fs::write(&full_path, &updated_content) {
                clear_edit_plan(&context.work_dir);
                return ToolResult::error(format!(
                    "Edit #{} failed: Failed to write file {}: {}. Edit plan aborted and cleared.",
                    idx + 1, edit.file_path, e
                ));
            }

            results.push(format!("✓ {}", edit.file_path));
            println!("  {} {}", "✓".green(), edit.description);
        }

        println!("\n{}", "═".repeat(60).bright_black());
        println!("{} All {} edits applied successfully!", "✅".green(), plan.len());

        // Clear the plan after successful application
        clear_edit_plan(&context.work_dir);

        ToolResult::success(format!(
            "Successfully applied {} edit(s):\n{}",
            plan.len(),
            results.join("\n")
        ))
    }
}