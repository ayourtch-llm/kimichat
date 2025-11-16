use kimichat_toolcore::{param, Tool, ToolParameters, ToolResult, ParameterDefinition};
use kimichat_toolcore::tool_context::ToolContext;
use crate::open_file;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fs;
use colored::Colorize;
use chrono;

/// Tool for opening files with optional line range
pub struct OpenFileTool;

#[async_trait]
impl Tool for OpenFileTool {
    fn name(&self) -> &str {
        "open_file"
    }

    fn description(&self) -> &str {
        "Open a file and display its contents with optional line range"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("file_path", "string", "Path to the file relative to the work directory", required),
            param!("start_line", "integer", "Starting line number (1-based)", optional),
            param!("end_line", "integer", "Ending line number (1-based)", optional),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let file_path = match params.get_required::<String>("file_path") {
            Ok(path) => path,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let start_line = params.get_optional::<i32>("start_line").unwrap_or(None);
        let end_line = params.get_optional::<i32>("end_line").unwrap_or(None);

        let line_range = if let (Some(start), Some(end)) = (start_line, end_line) {
            if start > 0 && end > 0 {
                Some((start as usize)..=(end as usize))
            } else {
                None
            }
        } else {
            None
        };

        match open_file::open_file(&context.work_dir, &file_path, line_range).await {
            Ok(content) => ToolResult::success(content),
            Err(e) => ToolResult::error(format!("Failed to open file: {}", e)),
        }
    }
}

/// Tool for reading file previews (first 10 lines)
pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read a preview of a file (first 10 lines) with total line count. For reading specific line ranges, use open_file instead."
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("file_path", "string", "Path to the file relative to the work directory", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let file_path = match params.get_required::<String>("file_path") {
            Ok(path) => path,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        // Use the same logic as the original read_file method
        let full_path = context.work_dir.join(&file_path);
        if !full_path.exists() {
            // Check for directory with similar name
            if let Some(stem) = full_path.file_stem().and_then(|s| s.to_str()) {
                let parent = full_path.parent().unwrap_or(&context.work_dir);
                let possible_dir = parent.join(stem);

                if possible_dir.exists() && possible_dir.is_dir() {
                    return ToolResult::error(format!(
                        "File not found: {} (Note: Found a directory named '{}' at this location. Did you mean to list files in that directory instead?)",
                        file_path, stem
                    ));
                }
            }

            return ToolResult::error(format!("File not found: {}", file_path));
        }

        // Check if it's a directory
        if full_path.is_dir() {
            return ToolResult::error(format!(
                "Path '{}' is a directory, not a file. Use list_files to see its contents.",
                file_path
            ));
        }

        match fs::read_to_string(&full_path) {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().collect();
                let total_lines = lines.len();

                let preview = if total_lines <= 10 {
                    content
                } else {
                    let preview_lines = lines.iter().take(10).cloned().collect::<Vec<_>>().join("\n");
                    format!("{}\n[{} more lines]", preview_lines, total_lines - 10)
                };

                ToolResult::success(preview)
            }
            Err(e) => ToolResult::error(format!("Failed to read file: {}", e)),
        }
    }
}

/// Tool for writing content to files
pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file in the work directory"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("file_path", "string", "Path to the file relative to the work directory", required),
            param!("content", "string", "Content to write to the file", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let file_path = match params.get_required::<String>("file_path") {
            Ok(path) => path,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let content = match params.get_required::<String>("content") {
            Ok(content) => content,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let full_path = context.work_dir.join(&file_path);

        // Create parent directories if they don't exist
        if let Some(parent) = full_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                return ToolResult::error(format!("Failed to create directories: {}", e));
            }
        }

        match fs::write(&full_path, content) {
            Ok(_) => ToolResult::success(format!("Successfully wrote to file: {}", file_path)),
            Err(e) => ToolResult::error(format!("Failed to write file: {}", e)),
        }
    }
}

/// Tool for editing files by replacing content
pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Edit a file by replacing old content with new content"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("file_path", "string", "Path to the file relative to the work directory", required),
            param!("old_content", "string", "Old content to find and replace (must not be empty)", required),
            param!("new_content", "string", "New content to replace with", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let file_path = match params.get_required::<String>("file_path") {
            Ok(path) => path,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let old_content = match params.get_required::<String>("old_content") {
            Ok(content) => content,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let new_content = match params.get_required::<String>("new_content") {
            Ok(content) => content,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        if old_content.trim().is_empty() {
            return ToolResult::error("old_content must not be empty".to_string());
        }

        let full_path = context.work_dir.join(&file_path);

        if !full_path.exists() {
            return ToolResult::error(format!("File not found: {}", file_path));
        }

        // Read current content
        let current_content = match fs::read_to_string(&full_path) {
            Ok(content) => content,
            Err(e) => return ToolResult::error(format!("Failed to read file: {}", e)),
        };

        // Check if old content exists
        if !current_content.contains(&old_content) {
            // Log the failure for debugging
            let log_entry = serde_json::json!({
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "error": "old_content_not_found",
                "file_path": file_path,
                "old_content": old_content,
                "new_content": new_content,
                "current_file_contents": current_content,
                "old_content_length": old_content.len(),
                "current_content_length": current_content.len(),
            });

            // Create logs directory if it doesn't exist
            let log_dir = context.work_dir.join("logs");
            let mut log_file_path_str = String::new();
            if let Err(e) = fs::create_dir_all(&log_dir) {
                eprintln!("Warning: Failed to create log directory: {}", e);
            } else {
                // Write to a timestamped log file
                let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S_%3f");
                let log_file = log_dir.join(format!("edit_failure_{}.json", timestamp));
                log_file_path_str = log_file.display().to_string();

                match fs::write(&log_file, serde_json::to_string_pretty(&log_entry).unwrap_or_else(|_| log_entry.to_string())) {
                    Ok(_) => eprintln!("Edit failure logged to: {}", log_file.display()),
                    Err(e) => eprintln!("Warning: Failed to write edit failure log: {}", e),
                }
            }

            // Return the JSON data directly to the LLM - it's better at parsing JSON than reading formatted text
            let error_message = format!(
                "Edit failed: old_content not found in file. Analysis data:\n{}",
                serde_json::to_string_pretty(&log_entry).unwrap_or_else(|_| log_entry.to_string())
            );

            return ToolResult::error(error_message);
        }

        // Calculate replacement
        let new_content_full = current_content.replace(&old_content, &new_content);
        let occurrences = current_content.matches(&old_content).count();

        // Show diff and ask for confirmation
        println!("{}", "═".repeat(60).bright_blue());
        println!("{} {}", "📝 Editing:".bright_cyan().bold(), file_path.bright_white());
        println!("{}", "═".repeat(60).bright_black());

        // Simple diff display
        println!("{}", "─ Old content:".red());
        for line in old_content.lines() {
            println!("{} {}", "-".red(), line);
        }
        println!();
        println!("{}", "+ New content:".green());
        for line in new_content.lines() {
            println!("{} {}", "+".green(), line);
        }
        println!("{}", "═".repeat(60).bright_black());

        if occurrences > 1 {
            println!("{}", format!("⚠️  Warning: {} occurrences will be replaced", occurrences).yellow());
        }

        // Check permission using policy system
        let (approved, rejection_reason) = match context.check_permission(
            kimichat_policy::ActionType::FileEdit,
            &file_path,
            "Apply these changes? [Y/n]"
        ) {
            Ok((approved, reason)) => (approved, reason),
            Err(e) => return ToolResult::error(format!("Permission check failed: {}", e)),
        };

        if approved {
            // Write back to file
            match fs::write(&full_path, new_content_full) {
                Ok(_) => ToolResult::success(format!("✅ Successfully edited {} ({} replacement(s))", file_path, occurrences)),
                Err(e) => ToolResult::error(format!("Failed to write file: {}", e)),
            }
        } else {
            let error_msg = if let Some(reason) = rejection_reason {
                format!("Edit cancelled by user: {}", reason)
            } else {
                "Edit cancelled by user or policy".to_string()
            };
            ToolResult::error(error_msg)
        }
    }
}

/// Tool for listing files with glob patterns
pub struct ListFilesTool;

#[async_trait]
impl Tool for ListFilesTool {
    fn name(&self) -> &str {
        "list_files"
    }

    fn description(&self) -> &str {
        "List files matching a glob pattern. Respects .gitignore files (excludes ignored files). Limited to 1000 results. Supports recursive search with **."
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("pattern", "string", "Glob pattern (e.g., 'src/**/*.rs', '**/*.json'). Use ** for recursive search. Defaults to '*' (files in current directory). Respects .gitignore and limits to 1000 results.", optional, "*"),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let pattern = params.get_optional::<String>("pattern")
            .unwrap_or(Some("*".to_string()))
            .unwrap_or_else(|| "*".to_string());

        eprintln!("[DEBUG] list_files with pattern: '{}' in work_dir: {:?}", pattern, context.work_dir);

        const MAX_FILES: usize = 1000;

        // Use ignore crate's WalkBuilder which respects .gitignore
        let mut builder = ignore::WalkBuilder::new(&context.work_dir);
        builder
            .hidden(false)  // Show hidden files (but still respect .gitignore)
            .git_ignore(true)  // Respect .gitignore files
            .git_global(true)  // Respect global gitignore
            .git_exclude(true);  // Respect .git/info/exclude

        // Parse the glob pattern to determine search scope
        let glob_matcher = match glob::Pattern::new(&pattern) {
            Ok(matcher) => matcher,
            Err(e) => return ToolResult::error(format!("Invalid glob pattern: {}", e)),
        };

        let mut files = Vec::new();
        let mut total_matched = 0;
        let mut ignored_count = 0;

        for entry in builder.build() {
            match entry {
                Ok(entry) => {
                    let path = entry.path();

                    // Skip directories, only list files
                    if !path.is_file() {
                        continue;
                    }

                    // Get relative path
                    if let Ok(relative_path) = path.strip_prefix(&context.work_dir) {
                        if let Some(path_str) = relative_path.to_str() {
                            // Check if path matches the glob pattern
                            if glob_matcher.matches(path_str) {
                                total_matched += 1;
                                if files.len() < MAX_FILES {
                                    files.push(path_str.to_string());
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    ignored_count += 1;
                }
            }
        }

        files.sort();
        let result = if files.is_empty() && total_matched == 0 {
            format!(
                "No files found matching pattern: '{}'\nSearched in: {:?}\n{} files were ignored (respecting .gitignore)\nTip: Use ** for recursive search (e.g., 'src/**/*.rs')",
                pattern, context.work_dir, ignored_count
            )
        } else if total_matched > MAX_FILES {
            format!(
                "⚠️  Found {} matching file(s), but showing only first {} ({} files ignored by .gitignore):\n{}\n\n\
                Tip: Use a more specific pattern to reduce results (e.g., 'src/**/*.rs' instead of '**/*')",
                total_matched,
                MAX_FILES,
                ignored_count,
                files.join("\n")
            )
        } else {
            let ignore_note = if ignored_count > 0 {
                format!(" ({} files ignored by .gitignore)", ignored_count)
            } else {
                String::new()
            };
            format!(
                "Found {} file(s) matching '{}'{}:\n{}",
                files.len(),
                pattern,
                ignore_note,
                files.join("\n")
            )
        };

        ToolResult::success(result)
    }
}
