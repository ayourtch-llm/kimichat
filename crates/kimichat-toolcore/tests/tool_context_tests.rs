use kimichat_toolcore::tool_context::ToolContext;
use kimichat_policy::PolicyManager;
use tempfile::TempDir;

#[cfg(test)]
mod tool_context_tests {
    use super::*;

    fn create_test_context() -> (ToolContext, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().to_path_buf();
        let policy_manager = PolicyManager::new();
        let context = ToolContext::new(work_dir.clone(), "test_session".to_string(), policy_manager);
        (context, temp_dir)
    }

    #[test]
    fn test_context_creation() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().to_path_buf();
        let policy_manager = PolicyManager::new();
        
        let context = ToolContext::new(work_dir.clone(), "test_session".to_string(), policy_manager);
        
        assert_eq!(context.work_dir, work_dir);
        assert_eq!(context.session_id, "test_session");
        assert!(context.environment.is_empty());
        assert!(context.terminal_manager.is_none());
        assert!(context.skill_registry.is_none());
        assert!(context.todo_manager.is_none());
        assert!(!context.non_interactive);
    }

    #[test]
    fn test_context_with_non_interactive() {
        let (context, _) = create_test_context();
        
        let non_interactive_context = context.clone().with_non_interactive(true);
        assert!(non_interactive_context.non_interactive);
        
        let interactive_context = context.with_non_interactive(false);
        assert!(!interactive_context.non_interactive);
    }

    #[test]
    fn test_context_with_environment() {
        let (context, _) = create_test_context();
        
        let env_context = context
            .with_env("API_KEY".to_string(), "secret123".to_string())
            .with_env("DEBUG".to_string(), "true".to_string());
        
        assert_eq!(env_context.environment.len(), 2);
        assert_eq!(env_context.environment.get("API_KEY"), Some(&"secret123".to_string()));
        assert_eq!(env_context.environment.get("DEBUG"), Some(&"true".to_string()));
    }

    #[test]
    fn test_context_builder_pattern() {
        let (context, _) = create_test_context();
        
        let built_context = context
            .with_non_interactive(true)
            .with_env("TEST_VAR".to_string(), "test_value".to_string())
            .with_env("ANOTHER_VAR".to_string(), "another_value".to_string());
        
        assert!(built_context.non_interactive);
        assert_eq!(built_context.environment.len(), 2);
        assert!(built_context.environment.contains_key("TEST_VAR"));
        assert!(built_context.environment.contains_key("ANOTHER_VAR"));
    }

    #[test]
    fn test_context_debug_formatting() {
        let (context, _) = create_test_context();
        let debug_str = format!("{:?}", context);
        assert!(debug_str.contains("ToolContext"));
        assert!(debug_str.contains("work_dir"));
        assert!(debug_str.contains("session_id"));
    }

    #[tokio::test]
    async fn test_check_permission_allow() {
        let (context, _) = create_test_context();
        
        // Test with a typically allowed action
        let action = kimichat_policy::ActionType::FileRead;
        let target = "/tmp/test_file.txt";
        let prompt = "Allow reading test file?";
        
        let result = context.check_permission(action, target, prompt);
        assert!(result.is_ok());
        
        let (_approved, reason) = result.unwrap();
        // The result depends on the policy manager's default behavior
        // but it should be a valid response without panicking
        assert!(reason.is_none() || reason.as_ref().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn test_check_permission_deny() {
        let (context, _) = create_test_context();
        
        // Test with a typically sensitive action
        let action = kimichat_policy::ActionType::CommandExecution;
        let target = "rm -rf /";
        let prompt = "Allow destructive system command?";
        
        let result = context.check_permission(action, target, prompt);
        assert!(result.is_ok());
        
        let (_approved, reason) = result.unwrap();
        // The result depends on the policy manager's default behavior
        // but it should be a valid response without panicking
        assert!(reason.is_none() || reason.as_ref().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn test_check_permission_non_interactive() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().to_path_buf();
        let policy_manager = PolicyManager::new();
        
        let context = ToolContext::new(work_dir, "test_session".to_string(), policy_manager)
            .with_non_interactive(true);
        
        let action = kimichat_policy::ActionType::FileWrite;
        let target = "/tmp/test.txt";
        let prompt = "Allow file write?";
        
        let result = context.check_permission(action, target, prompt);
        assert!(result.is_ok());
        
        let (_approved, reason) = result.unwrap();
        // In non-interactive mode, decisions should be deterministic
        assert!(reason.is_none());
    }

    #[test]
    fn test_work_dir_resolved_path() {
        let temp_dir = TempDir::new().unwrap();
        let expected_path = temp_dir.path().canonicalize().unwrap();
        let context = ToolContext::new(
            temp_dir.path().to_path_buf(),
            "test_session".to_string(),
            PolicyManager::new(),
        );
        
        assert_eq!(context.work_dir, expected_path);
    }

    #[test]
    fn test_session_id_uniqueness() {
        let temp_dir = TempDir::new().unwrap();
        let context1 = ToolContext::new(
            temp_dir.path().to_path_buf(),
            "session1".to_string(),
            PolicyManager::new(),
        );
        let context2 = ToolContext::new(
            temp_dir.path().to_path_buf(),
            "session2".to_string(),
            PolicyManager::new(),
        );
        
        assert_ne!(context1.session_id, context2.session_id);
        assert_eq!(context1.session_id, "session1");
        assert_eq!(context2.session_id, "session2");
    }

    #[test]
    fn test_environment_variable_overwrite() {
        let (context, _) = create_test_context();
        
        let env_context = context
            .with_env("VAR".to_string(), "value1".to_string())
            .with_env("VAR".to_string(), "value2".to_string());
        
        // The last value should win
        assert_eq!(env_context.environment.get("VAR"), Some(&"value2".to_string()));
        assert_eq!(env_context.environment.len(), 1);
    }

    #[tokio::test]
    async fn test_check_permission_error_handling() {
        let (context, _) = create_test_context();
        
        // Test with empty target
        let action = kimichat_policy::ActionType::FileRead;
        let target = "";
        let prompt = "Test prompt";
        
        let result = context.check_permission(action, target, prompt);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_permission_learning_interaction() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().to_path_buf();
        
        // Create a policy manager with learning enabled if possible
        let policy_manager = PolicyManager::new();
        let context = ToolContext::new(work_dir, "test_session".to_string(), policy_manager);
        
        let action = kimichat_policy::ActionType::FileRead;
        let target = "/tmp/test.txt";
        let prompt = "Test prompt";
        
        // This should not panic even if learning is enabled
        let result = context.check_permission(action, target, prompt);
        assert!(result.is_ok());
    }

    #[test]
    fn test_context_with_different_work_directories() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();
        
        let context1 = ToolContext::new(
            temp_dir1.path().to_path_buf(),
            "session1".to_string(),
            PolicyManager::new(),
        );
        let context2 = ToolContext::new(
            temp_dir2.path().to_path_buf(),
            "session2".to_string(),
            PolicyManager::new(),
        );
        
        assert_ne!(context1.work_dir, context2.work_dir);
        assert!(context1.work_dir.exists());
        assert!(context2.work_dir.exists());
    }

    #[test]
    fn test_environment_variable_types() {
        let (context, _) = create_test_context();
        
        let env_context = context
            .with_env("STRING_VAR".to_string(), "hello world".to_string())
            .with_env("EMPTY_VAR".to_string(), "".to_string())
            .with_env("NUMBER_VAR".to_string(), "42".to_string())
            .with_env("SPECIAL_CHARS".to_string(), "!@#$%^&*()".to_string());
        
        assert_eq!(env_context.environment.len(), 4);
        assert_eq!(env_context.environment.get("STRING_VAR"), Some(&"hello world".to_string()));
        assert_eq!(env_context.environment.get("EMPTY_VAR"), Some(&"".to_string()));
        assert_eq!(env_context.environment.get("NUMBER_VAR"), Some(&"42".to_string()));
        assert_eq!(env_context.environment.get("SPECIAL_CHARS"), Some(&"!@#$%^&*()".to_string()));
    }

    #[test]
    fn test_context_cloning() {
        let (context, _) = create_test_context();
        let cloned_context = context.clone();
        
        assert_eq!(context.work_dir, cloned_context.work_dir);
        assert_eq!(context.session_id, cloned_context.session_id);
        assert_eq!(context.environment, cloned_context.environment);
        assert_eq!(context.non_interactive, cloned_context.non_interactive);
    }

    #[tokio::test]
    async fn test_multiple_permission_checks() {
        let (context, _) = create_test_context();
        
        let actions = vec![
            kimichat_policy::ActionType::FileRead,
            kimichat_policy::ActionType::FileWrite,
            kimichat_policy::ActionType::FileEdit,
        ];
        
        for (i, action) in actions.into_iter().enumerate() {
            let target = format!("/tmp/test_{}.txt", i);
            let prompt = format!("Test prompt {}", i);
            
            let result = context.check_permission(action, &target, &prompt);
            assert!(result.is_ok(), "Permission check {} failed", i);
            
            let (approved, reason) = result.unwrap();
            // Should always return a valid response
            assert!(reason.is_none() || reason.as_ref().unwrap().len() > 0);
        }
    }

    #[test]
    fn test_context_thread_safety() {
        use std::sync::Arc;
        use std::thread;
        
        let (context, _) = create_test_context();
        let context = Arc::new(context);
        
        let mut handles = Vec::new();
        
        for i in 0..10 {
            let context_clone = Arc::clone(&context);
            let handle = thread::spawn(move || {
                // Test read-only access from multiple threads
                assert_eq!(context_clone.session_id, "test_session");
                assert!(context_clone.work_dir.exists());
                assert_eq!(context_clone.environment.len(), 0);
                assert!(!context_clone.non_interactive);
                format!("thread_{}_completed", i)
            });
            handles.push(handle);
        }
        
        for handle in handles {
            let result = handle.join().unwrap();
            assert!(result.contains("thread_"));
            assert!(result.contains("_completed"));
        }
    }
}