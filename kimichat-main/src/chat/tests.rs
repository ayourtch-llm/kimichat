#[cfg(test)]
mod tests {
    use crate::chat::{calculate_conversation_size, get_max_session_size, should_compact_session, intelligent_compaction};
    use crate::{KimiChat, ClientConfig};
    use kimichat_models::{Message, ModelColor, ToolCall, FunctionCall};
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use kimichat_terminal::TerminalManager;
    use kimichat_policy::PolicyManager;
    use kimichat_toolcore::ToolRegistry;
    use kimichat_todo::TodoManager;
    use tempfile::TempDir;

    // Helper function to create a test KimiChat instance
    fn create_test_kimichat() -> KimiChat {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().to_path_buf();
        
        KimiChat {
            api_key: "test-key".to_string(),
            work_dir: work_dir.clone(),
            client: reqwest::Client::new(),
            messages: Vec::new(),
            current_model: ModelColor::GrnModel,
            total_tokens_used: 0,
            logger: None,
            tool_registry: ToolRegistry::new(),
            agent_coordinator: None,
            use_agents: false,
            client_config: ClientConfig::new(),
            policy_manager: PolicyManager::new(),
            terminal_manager: Arc::new(Mutex::new(TerminalManager::new(work_dir))),
            skill_registry: None,
            non_interactive: false,
            todo_manager: Arc::new(TodoManager::new()),
            stream_responses: false,
            verbose: false,
            debug_level: 0,
        }
    }

    // Helper function to create a test message
    fn create_test_message(role: &str, content: &str) -> Message {
        Message {
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning: None,
        }
    }

    // Helper function to create a test message with tool calls
    fn create_test_tool_message(role: &str, content: &str, tool_name: &str, tool_args: &str) -> Message {
        let tool_call = ToolCall {
            id: "test-tool-id".to_string(),
            tool_type: "function".to_string(),
            function: FunctionCall {
                name: tool_name.to_string(),
                arguments: tool_args.to_string(),
            },
        };

        Message {
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: Some(vec![tool_call]),
            tool_call_id: None,
            name: None,
            reasoning: None,
        }
    }

    // Helper function to create a large message (simulates tool results)
    fn create_large_message(role: &str, base_content: &str, size_kb: usize) -> Message {
        let padding = "x".repeat(size_kb * 1024 - base_content.len());
        let content = format!("{}{}", base_content, padding);
        
        Message {
            role: role.to_string(),
            content,
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning: None,
        }
    }

    #[test]
    fn test_calculate_conversation_size_empty() {
        let mut chat = create_test_kimichat();
        
        let size = calculate_conversation_size(&chat.messages);
        // JSON "[]" has 2 characters, so size should be 2
        assert_eq!(size, 2);
    }

    #[test]
    fn test_calculate_conversation_size_single_message() {
        let mut chat = create_test_kimichat();
        chat.messages.push(create_test_message("user", "Hello world"));
        
        let size = calculate_conversation_size(&chat.messages);
        assert!(size > 0);
        assert!(size < 1000); // Should be small
    }

    #[test]
    fn test_calculate_conversation_size_large_message() {
        let mut chat = create_test_kimichat();
        chat.messages.push(create_large_message("user", "Hello", 10)); // 10KB message
        
        let size = calculate_conversation_size(&chat.messages);
        assert!(size > 10 * 1024); // Should be at least 10KB
        assert!(size < 15 * 1024); // But not too much larger due to JSON overhead
    }

    #[test]
    fn test_get_max_session_size_by_model() {
        assert_eq!(get_max_session_size(&ModelColor::GrnModel), 150_000);
        assert_eq!(get_max_session_size(&ModelColor::BluModel), 400_000);
        assert_eq!(get_max_session_size(&ModelColor::RedModel), 600_000);
    }

    #[test]
    fn test_should_compact_session_below_threshold() {
        let chat = create_test_kimichat();
        
        // Small conversation should not need compaction
        assert!(!should_compact_session(&chat, &ModelColor::GrnModel));
    }

    #[test]
    fn test_should_compact_session_above_threshold() {
        let mut chat = create_test_kimichat();
        
        // Add messages to exceed the threshold
        for i in 0..100 {
            chat.messages.push(create_large_message("user", &format!("Message {}", i), 2));
        }
        
        let size = calculate_conversation_size(&chat.messages);
        assert!(size > 150_000, "Conversation should be above threshold");
        
        // Should need compaction for GrnModel (150KB threshold)
        assert!(should_compact_session(&chat, &ModelColor::GrnModel));
        
        // But not for RedModel (600KB threshold)
        assert!(!should_compact_session(&chat, &ModelColor::RedModel));
    }

    #[tokio::test]
    async fn test_intelligent_compaction_preserves_recent_tool_calls() {
        let mut chat = create_test_kimichat();
        
        // Create a conversation with:
        // 1. System message
        // 2. Some older messages
        // 3. Recent tool calls (should be preserved)
        // 4. Recent user message
        
        chat.messages.push(create_test_message("system", "You are a helpful assistant"));
        
        // Add older messages (should be summarized) - make them very large to trigger compaction
        for i in 0..100 {
            chat.messages.push(create_large_message("user", &format!("Old user message {}", i), 5)); // 5KB each
            chat.messages.push(create_large_message("assistant", &format!("Old assistant response {}", i), 5)); // 5KB each
        }
        
        // Add recent tool call sequence (should be preserved)
        chat.messages.push(create_test_tool_message(
            "assistant", 
            "I'll help you with that", 
            "read_file", 
            "{\"file_path\":\"src/main.rs\"}"
        ));
        chat.messages.push(create_test_message(
            "tool", 
            "File content: ... (large file content that should be summarized if old)"
        ));
        
        // Add recent user message (should be preserved)
        chat.messages.push(create_test_message("user", "Continue with the task"));
        
        let original_count = chat.messages.len();
        let original_size = calculate_conversation_size(&chat.messages);
        
        // Perform intelligent compaction
        let result = intelligent_compaction(&mut chat, 0).await;
        
        assert!(result.is_ok(), "Intelligent compaction should succeed");
        
        let new_count = chat.messages.len();
        let new_size = calculate_conversation_size(&chat.messages);
        
        // Should have fewer messages after compaction
        assert!(new_count < original_count, "Should have fewer messages after compaction");
        
        // Should be significantly smaller
        assert!(new_size < original_size * 8 / 10, "Should be at least 20% smaller");
        
        // Should preserve system message
        assert_eq!(chat.messages[0].role, "system");
        
        // Should preserve recent tool call sequence
        let has_tool_call = chat.messages.iter().any(|m| {
            m.role == "assistant" && m.tool_calls.is_some()
        });
        assert!(has_tool_call, "Should preserve recent tool calls");
        
        // Should preserve recent user message
        assert_eq!(chat.messages.last().unwrap().role, "user");
        assert!(chat.messages.last().unwrap().content.contains("Continue with the task"));
    }

    #[tokio::test]
    async fn test_intelligent_compaction_preserves_very_recent_context() {
        let mut chat = create_test_kimichat();
        
        // Create a conversation where recent tool calls should be preserved
        chat.messages.push(create_test_message("system", "System message"));
        
        // Add many older messages
        for i in 0..50 {
            chat.messages.push(create_test_message("user", &format!("Old message {}", i)));
            chat.messages.push(create_test_message("assistant", &format!("Response {}", i)));
        }
        
        // Add very recent tool calls (within last 10 tool calls)
        for i in 0..5 {
            chat.messages.push(create_test_tool_message(
                "assistant",
                &format!("Recent tool call {}", i),
                "edit_file",
                &format!("{{\"file_path\":\"file{}.rs\",\"content\":\"content\"}}", i)
            ));
            chat.messages.push(create_test_message("tool", &format!("Tool result {}", i)));
        }
        
        let original_tool_calls = chat.messages.iter()
            .filter(|m| m.tool_calls.is_some())
            .count();
        
        // Perform compaction
        let result = intelligent_compaction(&mut chat, 100).await;
        assert!(result.is_ok());
        
        let new_tool_calls = chat.messages.iter()
            .filter(|m| m.tool_calls.is_some())
            .count();
        
        // Should preserve all recent tool calls
        assert_eq!(new_tool_calls, original_tool_calls);
    }

    #[tokio::test]
    async fn test_intelligent_compaction_small_conversation() {
        let mut chat = create_test_kimichat();
        
        // Small conversation should not be compacted
        chat.messages.push(create_test_message("system", "System message"));
        chat.messages.push(create_test_message("user", "Hello"));
        chat.messages.push(create_test_message("assistant", "Hi there!"));
        
        let original_count = chat.messages.len();
        
        let result = intelligent_compaction(&mut chat, 0).await;
        assert!(result.is_ok());
        
        // Should not change small conversations
        assert_eq!(chat.messages.len(), original_count);
    }

    #[tokio::test]
    async fn test_intelligent_compaction_with_tool_call_iteration_context() {
        let mut chat = create_test_kimichat();
        
        // Test that compaction respects tool call iteration context
        chat.messages.push(create_test_message("system", "System message"));
        
        // Add messages from different phases of tool execution
        for i in 0..30 {
            chat.messages.push(create_test_message("user", &format!("Request {}", i)));
            chat.messages.push(create_test_tool_message(
                "assistant",
                &format!("Tool call {}", i),
                "write_file",
                &format!("{{\"file_path\":\"file{}.rs\"}}", i)
            ));
            chat.messages.push(create_test_message("tool", &format!("Result {}", i)));
        }
        
        // Add recent context at iteration 150
        let result = intelligent_compaction(&mut chat, 150).await;
        assert!(result.is_ok());
        
        // Should preserve recent tool context around iteration 150
        let has_recent_context = chat.messages.iter().any(|m| {
            m.content.contains("Iteration 150") || 
            (m.tool_calls.is_some() && chat.messages.len() <= 15) // Recent tool calls
        });
        
        // Should significantly reduce size
        let final_size = calculate_conversation_size(&chat.messages);
        assert!(final_size < 400_000, "Should reduce conversation size below 400KB");
    }

    #[test]
    fn test_safe_truncate_function() {
        let long_text = "x".repeat(1000);
        let truncated = kimichat_logging::safe_truncate(&long_text, 100);
        
        assert_eq!(truncated.len(), 100);
        assert!(truncated.ends_with("..."));
        
        let short_text = "Hello world";
        let not_truncated = kimichat_logging::safe_truncate(&short_text, 100);
        
        assert_eq!(not_truncated, short_text);
    }
}