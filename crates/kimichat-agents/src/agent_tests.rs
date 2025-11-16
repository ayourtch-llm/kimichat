#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;
    use std::collections::HashMap;

    /// Test fixture for creating test agents
    fn create_test_agent() -> Agent {
        Agent::new(
            "test-agent".to_string(),
            vec![Capability::CodeAnalysis, Capability::FileOperations],
        )
    }

    /// Test fixture for creating test tasks
    fn create_test_task(task_id: &str, description: &str) -> Task {
        Task {
            id: task_id.to_string(),
            description: description.to_string(),
            task_type: TaskType::Simple,
            priority: TaskPriority::Medium,
            metadata: HashMap::new(),
        }
    }

    /// Test fixture for creating test tasks with metadata
    fn create_test_task_with_metadata(
        task_id: &str, 
        description: &str, 
        metadata: HashMap<String, String>
    ) -> Task {
        Task {
            id: task_id.to_string(),
            description: description.to_string(),
            task_type: TaskType::Simple,
            priority: TaskPriority::Medium,
            metadata,
        }
    }

    #[tokio::test]
    async fn test_agent_creation() {
        // RED: Test that agent creation works correctly
        let agent = create_test_agent();
        
        assert_eq!(agent.name, "test-agent");
        assert_eq!(agent.capabilities.len(), 2);
        assert!(agent.capabilities.contains(&Capability::CodeAnalysis));
        assert!(agent.capabilities.contains(&Capability::FileOperations));
    }

    #[tokio::test]
    async fn test_agent_has_capability() {
        // RED: Test capability checking logic
        let agent = create_test_agent();
        
        assert!(agent.has_capability(&Capability::CodeAnalysis));
        assert!(agent.has_capability(&Capability::FileOperations));
        assert!(!agent.has_capability(&Capability::Search));
        assert!(!agent.has_capability(&Capability::SystemOperations));
    }

    #[tokio::test]
    async fn test_task_creation() {
        // RED: Test basic task creation
        let task = create_test_task("task-1", "Analyze this code");
        
        assert_eq!(task.id, "task-1");
        assert_eq!(task.description, "Analyze this code");
        assert_eq!(task.task_type, TaskType::Simple);
        assert_eq!(task.priority, TaskPriority::Medium);
        assert!(task.metadata.is_empty());
    }

    #[tokio::test]
    async fn test_task_with_metadata() {
        // RED: Test task creation with metadata
        let mut metadata = HashMap::new();
        metadata.insert("file_path".to_string(), "src/main.rs".to_string());
        metadata.insert("language".to_string(), "rust".to_string());
        
        let task = create_test_task_with_metadata("task-2", "Review Rust code", metadata);
        
        assert_eq!(task.metadata.len(), 2);
        assert_eq!(task.metadata.get("file_path").unwrap(), "src/main.rs");
        assert_eq!(task.metadata.get("language").unwrap(), "rust");
    }

    #[tokio::test]
    async fn test_task_priority_ordering() {
        // RED: Test that task priority ordering works correctly
        assert!(TaskPriority::Critical > TaskPriority::High);
        assert!(TaskPriority::High > TaskPriority::Medium);
        assert!(TaskPriority::Medium > TaskPriority::Low);
        
        let priorities = vec![
            TaskPriority::Low,
            TaskPriority::High, 
            TaskPriority::Medium,
            TaskPriority::Critical,
        ];
        
        let mut sorted_priorities = priorities.clone();
        sorted_priorities.sort();
        
        assert_eq!(sorted_priorities, vec![
            TaskPriority::Low,
            TaskPriority::Medium,
            TaskPriority::High,
            TaskPriority::Critical,
        ]);
    }

    #[tokio::test]
    async fn test_agent_result_creation() {
        // RED: Test agent result creation and defaults
        let result = AgentResult::success(
            "Analysis complete".to_string(),
            "task-1".to_string(),
            "test-agent".to_string(),
        );
        
        assert!(result.success);
        assert_eq!(result.content, "Analysis complete");
        assert_eq!(result.task_id, "task-1");
        assert_eq!(result.agent_name, "test-agent");
        assert_eq!(result.execution_time, 0);
        assert!(result.metadata.is_empty());
        assert!(result.next_tasks.is_none());
    }

    #[tokio::test]
    async fn test_agent_result_with_next_tasks() {
        // RED: Test agent result with subsequent tasks
        let next_task = create_test_task("task-2", "Implement suggestions");
        
        let mut result = AgentResult::success(
            "Review complete".to_string(),
            "task-1".to_string(),
            "review-agent".to_string(),
        );
        
        result.next_tasks = Some(vec![next_task]);
        result.execution_time = 1500; // 1.5 seconds
        
        assert!(result.next_tasks.is_some());
        assert_eq!(result.next_tasks.as_ref().unwrap().len(), 1);
        assert_eq!(result.execution_time, 1500);
    }

    #[tokio::test]
    async fn test_complex_task_types() {
        // RED: Test complex task type creation
        let subtask1 = create_test_task("subtask-1", "Parse AST");
        let subtask2 = create_test_task("subtask-2", "Analyze dependencies");
        
        let parallel_task = Task {
            id: "parallel-task".to_string(),
            description: "Analyze code in parallel".to_string(),
            task_type: TaskType::Parallel(vec![subtask1.clone(), subtask2.clone()]),
            priority: TaskPriority::High,
            metadata: HashMap::new(),
        };
        
        let sequential_task = Task {
            id: "sequential-task".to_string(),
            description: "Analyze code sequentially".to_string(),
            task_type: TaskType::Sequential(vec![subtask1, subtask2]),
            priority: TaskPriority::High,
            metadata: HashMap::new(),
        };
        
        match parallel_task.task_type {
            TaskType::Parallel(subtasks) => {
                assert_eq!(subtasks.len(), 2);
                assert_eq!(subtasks[0].id, "subtask-1");
                assert_eq!(subtasks[1].id, "subtask-2");
            }
            _ => panic!("Expected parallel task type"),
        }
        
        match sequential_task.task_type {
            TaskType::Sequential(subtasks) => {
                assert_eq!(subtasks.len(), 2);
                assert_eq!(subtasks[0].id, "subtask-1");
                assert_eq!(subtasks[1].id, "subtask-2");
            }
            _ => panic!("Expected sequential task type"),
        }
    }

    #[tokio::test]
    async fn test_capability_from_string() {
        // RED: Test capability string parsing
        assert_eq!(Capability::from_string("code_analysis"), Capability::CodeAnalysis);
        assert_eq!(Capability::from_string("file_operations"), Capability::FileOperations);
        assert_eq!(Capability::from_string("search"), Capability::Search);
        assert_eq!(Capability::from_string("system_operations"), Capability::SystemOperations);
        
        // Test fallback to default
        assert_eq!(Capability::from_string("unknown_capability"), Capability::CodeAnalysis);
        assert_eq!(Capability::from_string(""), Capability::CodeAnalysis);
    }

    #[tokio::test]
    async fn test_agent_can_handle_task() {
        // RED: Test agent capability matching with tasks
        let code_agent = Agent::new(
            "code-agent".to_string(),
            vec![Capability::CodeAnalysis, Capability::CodeReview, Capability::Testing],
        );
        
        let file_agent = Agent::new(
            "file-agent".to_string(),
            vec![Capability::FileOperations, Capability::Search],
        );
        
        let code_task = Task {
            id: "code-task".to_string(),
            description: "Analyze Rust code quality".to_string(),
            task_type: TaskType::Simple,
            priority: TaskPriority::Medium,
            metadata: HashMap::new(),
        };
        
        let file_task = Task {
            id: "file-task".to_string(),
            description: "Search for configuration files".to_string(),
            task_type: TaskType::Simple,
            priority: TaskPriority::Low,
            metadata: HashMap::new(),
        };
        
        // These assertions would need actual implementation logic
        // For now, let's verify the agents have the expected capabilities
        assert!(code_agent.has_capability(&Capability::CodeAnalysis));
        assert!(code_agent.has_capability(&Capability::CodeReview));
        assert!(code_agent.has_capability(&Capability::Testing));
        assert!(!code_agent.has_capability(&Capability::FileOperations));
        
        assert!(file_agent.has_capability(&Capability::FileOperations));
        assert!(file_agent.has_capability(&Capability::Search));
        assert!(!file_agent.has_capability(&Capability::CodeAnalysis));
    }
}