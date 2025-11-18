use anyhow::Result;
use serde_json::json;
use std::path::PathBuf;

use crate::KimiChat;
use crate::cli::Cli;
use crate::config::ClientConfig;
use kimichat_policy::PolicyManager;

/// Subagent task summary structure
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SubagentSummary {
    /// Task description that was provided
    pub task: String,
    /// Whether the task was completed successfully
    pub success: bool,
    /// Summary of what was accomplished
    pub summary: String,
    /// Files that were modified during the task
    pub files_modified: Vec<String>,
    /// Tools that were used during the task
    pub tools_used: Vec<String>,
    /// Number of messages exchanged
    pub message_count: usize,
    /// Total time taken in milliseconds
    pub duration_ms: u64,
    /// Optional error message if task failed
    pub error: Option<String>,
    /// Optional metadata about the task
    pub metadata: serde_json::Value,
}

/// Run in subagent mode - execute task internally and return JSON summary
pub async fn run_subagent_mode(
    cli: &Cli,
    task_text: String,
    client_config: ClientConfig,
    work_dir: PathBuf,
    policy_manager: PolicyManager,
) -> Result<()> {
    let start_time = std::time::Instant::now();
    
    // Resolve terminal backend
    let backend_type = crate::resolve_terminal_backend(cli)?;

    // Create a subagent instance - this runs without showing output to user
    let mut subagent = KimiChat::new_with_config(
        client_config.clone(),
        work_dir.clone(),
        false, // Always use single-agent mode for subagent
        policy_manager.clone(),
        false, // No streaming in subagent mode
        cli.verbose,
        backend_type,
    );

    // Mark as non-interactive to prevent prompts
    subagent.non_interactive = true;

    // Disable logging for subagent mode to avoid clutter
    subagent.logger = None;

    // Track initial state to detect changes
    let initial_file_count = count_files(&work_dir)?;
    let mut tools_used = Vec::new();
    let mut files_modified = Vec::new();

    // Execute the task internally
    let result = if cli.agents && subagent.agent_coordinator.is_some() {
        // Use agent system if enabled and available
        subagent.process_with_agents(&task_text, None).await
    } else {
        // Use regular chat
        crate::chat::session::chat(&mut subagent, &task_text, None).await
    };

    let duration = start_time.elapsed();

    // Analyze the results and extract information
    let (success, summary, error) = match result {
        Ok(response) => {
            // Extract summary from the response
            let summary_text = extract_summary_from_response(&response);
            (true, summary_text, None)
        }
        Err(e) => {
            (false, String::new(), Some(e.to_string()))
        }
    };

    // Analyze what changed during execution
    analyze_changes(&subagent, &work_dir, initial_file_count, &mut tools_used, &mut files_modified);

    // Create summary
    let summary_obj = SubagentSummary {
        task: task_text.clone(),
        success,
        summary,
        files_modified,
        tools_used,
        message_count: subagent.messages.len(),
        duration_ms: duration.as_millis() as u64,
        error,
        metadata: json!({
            "agents_used": subagent.use_agents,
            "model_used": subagent.current_model.display_name(),
            "work_directory": work_dir.to_string_lossy().to_string(),
        }),
    };

    // Output the JSON summary
    if cli.pretty {
        println!("{}", serde_json::to_string_pretty(&summary_obj)?);
    } else {
        println!("{}", serde_json::to_string(&summary_obj)?);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_extract_summary_from_response_with_summary_indicator() {
        let response = "I worked on the code and made several improvements.\nSummary: Refactored the main function and added tests.\nThe changes are now complete.";
        let summary = extract_summary_from_response(response);
        assert_eq!(summary, "Refactored the main function and added tests. The changes are now complete.");
    }

    #[test]
    fn test_extract_summary_from_response_without_indicator() {
        let response = "First line of response.\nSecond line with important info.\nThird line with conclusion.";
        let summary = extract_summary_from_response(response);
        assert!(summary.contains("First line"));
        assert!(summary.contains("conclusion"));
    }

    #[test]
    fn test_extract_summary_from_response_truncates_long_response() {
        let long_response = "A".repeat(300);
        let summary = extract_summary_from_response(&long_response);
        assert!(summary.len() <= 203); // 200 chars + "..."
        assert!(summary.ends_with("..."));
    }

    #[test]
    fn test_count_files_in_directory() {
        // Create a temporary directory with some test files
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();
        
        // Create some test files
        std::fs::write(temp_path.join("file1.txt"), "test content").unwrap();
        std::fs::write(temp_path.join("file2.rs"), "fn main() {}").unwrap();
        
        // Create a subdirectory with a file
        std::fs::create_dir(temp_path.join("subdir")).unwrap();
        std::fs::write(temp_path.join("subdir").join("file3.txt"), "sub content").unwrap();
        
        let count = count_files(&temp_path.to_path_buf()).unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_subagent_summary_serialization() {
        let summary = SubagentSummary {
            task: "create test file".to_string(),
            success: true,
            summary: "Created test.txt with hello world".to_string(),
            files_modified: vec!["/tmp/test.txt".to_string()],
            tools_used: vec!["write_file".to_string()],
            message_count: 5,
            duration_ms: 1000,
            error: None,
            metadata: json!({
                "model": "test-model",
                "directory": "/tmp"
            }),
        };
        
        // Test JSON serialization
        let json_str = serde_json::to_string(&summary).unwrap();
        let parsed: SubagentSummary = serde_json::from_str(&json_str).unwrap();
        
        assert_eq!(parsed.task, "create test file");
        assert!(parsed.success);
        assert_eq!(parsed.summary, "Created test.txt with hello world");
        assert_eq!(parsed.files_modified.len(), 1);
        assert_eq!(parsed.tools_used.len(), 1);
        assert_eq!(parsed.message_count, 5);
        assert_eq!(parsed.duration_ms, 1000);
        assert!(parsed.error.is_none());
    }
}

/// Count files in directory recursively
fn count_files(dir: &PathBuf) -> Result<usize> {
    let mut count = 0;
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            count += 1;
        }
    }
    Ok(count)
}

/// Extract a concise summary from the AI response
fn extract_summary_from_response(response: &str) -> String {
    // Try to extract the key points from the response
    let lines: Vec<&str> = response.lines().collect();
    
    // Look for summary indicators
    let summary_indicators = ["summary:", "in summary", "to summarize", "accomplished:", "completed:", "result:"];
    
    for (i, line) in lines.iter().enumerate() {
        let line_lower = line.to_lowercase();
        if summary_indicators.iter().any(|indicator| line_lower.contains(indicator)) {
            // Found a summary line, take the next few lines
            let summary_start = i + 1;
            let summary_lines: Vec<String> = lines
                .iter()
                .skip(summary_start)
                .take(3) // Take up to 3 lines after the indicator
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            
            if !summary_lines.is_empty() {
                return summary_lines.join(" ");
            }
        }
    }
    
    // If no explicit summary found, create one from the first and last parts
    if lines.len() > 2 {
        let first_part = lines.first().unwrap_or(&"").trim();
        let last_part = lines.last().unwrap_or(&"").trim();
        
        if !first_part.is_empty() && !last_part.is_empty() && first_part != last_part {
            return format!("{} - {}", first_part, last_part);
        } else if !first_part.is_empty() {
            return first_part.to_string();
        }
    }
    
    // Fallback to first 200 characters of the response
    let truncated = response.chars().take(200).collect::<String>();
    if truncated.len() < response.len() {
        format!("{}...", truncated)
    } else {
        truncated
    }
}

/// Analyze what changed during subagent execution
fn analyze_changes(
    subagent: &KimiChat,
    work_dir: &PathBuf,
    _initial_file_count: usize,
    tools_used: &mut Vec<String>,
    files_modified: &mut Vec<String>,
) {
    // Extract tools used from message history
    for message in &subagent.messages {
        if let Some(ref tool_calls) = message.tool_calls {
            for tool_call in tool_calls {
                // The function field is already a FunctionCall, not an Option
                let function = &tool_call.function;
                if !tools_used.contains(&function.name) {
                    tools_used.push(function.name.clone());
                }
                
                // Extract file information from specific tools
                match function.name.as_str() {
                    "write_file" | "edit_file" | "apply_edit_plan" => {
                        if let Ok(args) = serde_json::from_str::<serde_json::Value>(&function.arguments) {
                            if let Some(file_path) = args.get("file_path").and_then(|v| v.as_str()) {
                                let full_path = work_dir.join(file_path);
                                if let Some(path_str) = full_path.to_str() {
                                    if !files_modified.contains(&path_str.to_string()) {
                                        files_modified.push(path_str.to_string());
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    
    // Sort for consistent output
    tools_used.sort();
    files_modified.sort();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_summary_from_response() {
        let response = "I worked on the code and made several improvements.\nSummary: Refactored the main function and added tests.\nThe changes are now complete.";
        let summary = extract_summary_from_response(response);
        assert_eq!(summary, "Refactored the main function and added tests. The changes are now complete.");
    }

    #[test]
    fn test_extract_summary_without_indicator() {
        let response = "First line of response.\nSecond line with important info.\nThird line with conclusion.";
        let summary = extract_summary_from_response(response);
        assert!(summary.contains("First line"));
        assert!(summary.contains("conclusion"));
    }

    #[test]
    fn test_extract_summary_truncates_long_response() {
        let long_response = "A".repeat(300);
        let summary = extract_summary_from_response(&long_response);
        assert!(summary.len() <= 203); // 200 chars + "..."
        assert!(summary.ends_with("..."));
    }
}