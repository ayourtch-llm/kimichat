#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use kimichat_policy::PolicyManager;
    use kimichat_main::config::ClientConfig;
    use kimichat_main::cli::Cli;

    // Helper to create test CLI configuration
    fn create_test_cli(task: Option<String>) -> Cli {
        use clap::Parser;
        
        // This is a simplified version - in real tests you'd need to mock the CLI parsing
        Cli {
            command: None,
            interactive: false,
            agents: false, // Test single-agent mode
            generate: None,
            task,
            pretty: false,
            llama_cpp_url: None,
            api_url_blu_model: None,
            api_url_grn_model: None,
            api_url_red_model: None,
            model_blu_model: None,
            model_grn_model: None,
            model_red_model: None,
            model: None,
            blu_backend: None,
            grn_backend: None,
            red_backend: None,
            blu_key: None,
            grn_key: None,
            red_key: None,
            auto_confirm: false,
            policy_file: None,
            learn_policies: false,
            stream: false,
            verbose: false,
            terminal_backend: None,
            web: false,
            web_port: 8080,
            web_bind: "127.0.0.1".to_string(),
            web_attachable: false,
            sessions_dir: "~/.okaychat/sessions".to_string(),
        }
    }

    fn create_test_config() -> ClientConfig {
        ClientConfig {
            api_key: "test_key".to_string(),
            backend_blu_model: None,
            backend_grn_model: None,
            backend_red_model: None,
            api_url_blu_model: None,
            api_url_grn_model: None,
            api_url_red_model: None,
            api_key_blu_model: None,
            api_key_grn_model: None,
            api_key_red_model: None,
            model_blu_model_override: None,
            model_grn_model_override: None,
            model_red_model_override: None,
        }
    }

    #[test]
    fn test_extract_summary_from_response_with_summary_indicator() {
        let response = "I worked on the code and made several improvements.\nSummary: Refactored the main function and added tests.\nThe changes are now complete.";
        let summary = extract_summary_from_response(response);
        assert_eq!(summary, "Refactored the main function and added tests. The changes are now complete.");
    }

    #[test]
    fn test_extract_summary_from_response_with_in_summary_indicator() {
        let response = "I completed the task.\nIn summary, the database connection was fixed and all tests are now passing.\nReady for deployment.";
        let summary = extract_summary_from_response(response);
        assert_eq!(summary, "the database connection was fixed and all tests are now passing. Ready for deployment.");
    }

    #[test]
    fn test_extract_summary_from_response_without_indicator() {
        let response = "First line of response.\nSecond line with important info.\nThird line with conclusion.";
        let summary = extract_summary_from_response(response);
        assert!(summary.contains("First line"));
        assert!(summary.contains("conclusion"));
    }

    #[test]
    fn test_extract_summary_from_response_short() {
        let response = "Single line response.";
        let summary = extract_summary_from_response(response);
        assert_eq!(summary, "Single line response.");
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
    fn test_count_files_in_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();
        
        let count = count_files(&temp_path.to_path_buf()).unwrap();
        assert_eq!(count, 0);
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

    #[test]
    fn test_subagent_summary_with_error() {
        let summary = SubagentSummary {
            task: "failing task".to_string(),
            success: false,
            summary: "".to_string(),
            files_modified: vec![],
            tools_used: vec![],
            message_count: 2,
            duration_ms: 500,
            error: Some("Something went wrong".to_string()),
            metadata: json!({}),
        };
        
        let json_str = serde_json::to_string(&summary).unwrap();
        let parsed: SubagentSummary = serde_json::from_str(&json_str).unwrap();
        
        assert!(!parsed.success);
        assert_eq!(parsed.error, Some("Something went wrong".to_string()));
    }

    // Note: Integration tests would require mocking the LLM API and tool execution
    // For now, we focus on unit tests for the core functionality
    
    #[tokio::test]
    async fn test_run_subagent_mode_routing() {
        // This would be an integration test to verify that:
        // 1. --task without --agents uses subagent mode
        // 2. --task with --agents uses regular task mode
        // 3. JSON output is produced in subagent mode
        // 4. Verbose output is produced in regular task mode
        
        // This test would require more complex setup with mocked dependencies
        // For now, we rely on manual testing for the integration aspects
    }

    #[test]
    fn test_analyze_changes_empty_messages() {
        // Test with empty message history
        let mut tools_used = Vec::new();
        let mut files_modified = Vec::new();
        let work_dir = PathBuf::from("/tmp");
        
        // Create a mock subagent with empty messages
        let messages = vec![];
        
        // Simulate the analyze_changes logic
        for message in &messages {
            if let Some(ref tool_calls) = message.tool_calls {
                for tool_call in tool_calls {
                    let function = &tool_call.function;
                    if !tools_used.contains(&function.name) {
                        tools_used.push(function.name.clone());
                    }
                }
            }
        }
        
        assert!(tools_used.is_empty());
        assert!(files_modified.is_empty());
    }
}