use std::collections::HashMap;
use std::sync::Arc;
use kimichat_toolcore::tool_registry::ToolRegistry;
use kimichat_toolcore::tool::{Tool, ToolParameters, ToolResult, ParameterDefinition};
use kimichat_toolcore::tool_context::ToolContext;
use tempfile::TempDir;

// Mock tool implementations for testing
#[derive(Debug, Clone)]
struct TestTool {
    name: String,
    description: String,
    parameters: HashMap<String, ParameterDefinition>,
    should_fail: bool,
}

impl TestTool {
    fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            parameters: HashMap::new(),
            should_fail: false,
        }
    }

    fn with_parameters(mut self, parameters: HashMap<String, ParameterDefinition>) -> Self {
        self.parameters = parameters;
        self
    }

    fn failing(mut self) -> Self {
        self.should_fail = true;
        self
    }
}

#[async_trait::async_trait]
impl Tool for TestTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        self.parameters.clone()
    }

    async fn execute(&self, params: ToolParameters, _context: &ToolContext) -> ToolResult {
        if self.should_fail {
            ToolResult::error("Test tool failed intentionally".to_string())
        } else {
            let param_count = params.data.len();
            ToolResult::success(format!("Executed {} with {} parameters", self.name, param_count))
        }
    }
}

fn create_test_context() -> ToolContext {
    let temp_dir = TempDir::new().unwrap();
    let policy_manager = kimichat_policy::PolicyManager::new();
    ToolContext::new(
        temp_dir.path().to_path_buf(),
        "test_session".to_string(),
        policy_manager,
    )
}

fn create_parameter_definition(name: &str, param_type: &str, required: bool) -> ParameterDefinition {
    ParameterDefinition {
        param_type: param_type.to_string(),
        description: format!("Test parameter {}", name),
        required,
        default: None,
    }
}

#[tokio::test]
async fn test_registry_initialization() {
    let registry = ToolRegistry::new();
    assert_eq!(registry.get_all_tools().len(), 0);
    assert_eq!(registry.get_tool_names().len(), 0);
    assert_eq!(registry.get_categories().len(), 0);
    assert!(!registry.has_tool("any_tool"));
}

#[tokio::test]
async fn test_single_tool_registration() {
    let mut registry = ToolRegistry::new();
    let tool = TestTool::new("test_tool", "A test tool for testing");
    
    registry.register(tool);
    
    assert!(registry.has_tool("test_tool"));
    assert_eq!(registry.get_all_tools().len(), 1);
    assert_eq!(registry.get_tool_names(), vec!["test_tool"]);
    
    let retrieved_tool = registry.get_tool("test_tool");
    assert!(retrieved_tool.is_some());
    assert_eq!(retrieved_tool.unwrap().name(), "test_tool");
}

#[tokio::test]
async fn test_multiple_tool_registration() {
    let mut registry = ToolRegistry::new();
    
    let tool1 = TestTool::new("tool1", "First test tool");
    let tool2 = TestTool::new("tool2", "Second test tool");
    let tool3 = TestTool::new("tool3", "Third test tool");
    
    registry.register(tool1);
    registry.register(tool2);
    registry.register(tool3);
    
    assert_eq!(registry.get_all_tools().len(), 3);
    assert_eq!(registry.get_tool_names().len(), 3);
    
    let tool_names = registry.get_tool_names();
    assert!(tool_names.contains(&"tool1".to_string()));
    assert!(tool_names.contains(&"tool2".to_string()));
    assert!(tool_names.contains(&"tool3".to_string()));
}

#[tokio::test]
async fn test_duplicate_tool_registration() {
    let mut registry = ToolRegistry::new();
    
    let tool1 = TestTool::new("duplicate_tool", "First instance");
    let tool2 = TestTool::new("duplicate_tool", "Second instance");
    
    registry.register(tool1);
    registry.register(tool2); // Should overwrite the first one
    
    assert_eq!(registry.get_all_tools().len(), 1);
    let retrieved_tool = registry.get_tool("duplicate_tool").unwrap();
    assert_eq!(retrieved_tool.description(), "Second instance");
}

#[tokio::test]
async fn test_tool_registration_with_categories() {
    let mut registry = ToolRegistry::new();
    
    let tool = TestTool::new("categorized_tool", "A categorized tool");
    registry.register_with_categories(tool, vec!["category1".to_string(), "category2".to_string()]);
    
    assert!(registry.has_tool("categorized_tool"));
    
    let categories = registry.get_categories();
    assert_eq!(categories.len(), 2);
    assert!(categories.contains(&"category1".to_string()));
    assert!(categories.contains(&"category2".to_string()));
    
    let category1_tools = registry.get_tools_by_category("category1");
    assert_eq!(category1_tools.len(), 1);
    assert_eq!(category1_tools[0].name(), "categorized_tool");
}

#[tokio::test]
async fn test_tool_execution_success() {
    let mut registry = ToolRegistry::new();
    let tool = TestTool::new("exec_tool", "Tool for execution testing");
    registry.register(tool);
    
    let context = create_test_context();
    let params = ToolParameters { data: HashMap::new() };
    
    let result = registry.execute_tool("exec_tool", params, &context).await;
    assert!(result.success);
    assert_eq!(result.content, "Executed exec_tool with 0 parameters");
}

#[tokio::test]
async fn test_tool_execution_failure() {
    let mut registry = ToolRegistry::new();
    let tool = TestTool::new("failing_tool", "A tool that always fails").failing();
    registry.register(tool);
    
    let context = create_test_context();
    let params = ToolParameters { data: HashMap::new() };
    
    let result = registry.execute_tool("failing_tool", params, &context).await;
    assert!(!result.success);
    assert!(result.error.is_some());
    assert_eq!(result.error.unwrap(), "Test tool failed intentionally");
}

#[tokio::test]
async fn test_tool_execution_not_found() {
    let registry = ToolRegistry::new();
    let context = create_test_context();
    let params = ToolParameters { data: HashMap::new() };
    
    let result = registry.execute_tool("nonexistent_tool", params, &context).await;
    assert!(!result.success);
    assert!(result.error.is_some());
    assert!(result.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_tool_execution_with_parameters() {
    let mut registry = ToolRegistry::new();
    let tool = TestTool::new("param_tool", "Tool with parameters");
    registry.register(tool);
    
    let context = create_test_context();
    let mut params_data = HashMap::new();
    params_data.insert("param1".to_string(), serde_json::Value::String("value1".to_string()));
    params_data.insert("param2".to_string(), serde_json::Value::Number(42.into()));
    let params = ToolParameters { data: params_data };
    
    let result = registry.execute_tool("param_tool", params, &context).await;
    assert!(result.success);
    assert_eq!(result.content, "Executed param_tool with 2 parameters");
}

#[tokio::test]
async fn test_get_tools_by_category_empty() {
    let registry = ToolRegistry::new();
    let tools = registry.get_tools_by_category("nonexistent_category");
    assert!(tools.is_empty());
}

#[tokio::test]
async fn test_get_tools_by_category_with_tools() {
    let mut registry = ToolRegistry::new();
    
    let tool1 = TestTool::new("tool1", "First tool");
    let tool2 = TestTool::new("tool2", "Second tool");
    let tool3 = TestTool::new("tool3", "Third tool");
    
    registry.register_with_categories(tool1, vec!["cat1".to_string()]);
    registry.register_with_categories(tool2, vec!["cat1".to_string(), "cat2".to_string()]);
    registry.register_with_categories(tool3, vec!["cat2".to_string()]);
    
    let cat1_tools = registry.get_tools_by_category("cat1");
    assert_eq!(cat1_tools.len(), 2);
    
    let cat2_tools = registry.get_tools_by_category("cat2");
    assert_eq!(cat2_tools.len(), 2);
    
    let cat3_tools = registry.get_tools_by_category("cat3");
    assert_eq!(cat3_tools.len(), 0);
}

#[tokio::test]
async fn test_openai_tool_definitions() {
    let mut registry = ToolRegistry::new();
    
    let mut parameters = HashMap::new();
    parameters.insert(
        "file_path".to_string(),
        create_parameter_definition("file_path", "string", true),
    );
    parameters.insert(
        "line_count".to_string(),
        create_parameter_definition("line_count", "number", false),
    );
    
    let tool = TestTool::new("openai_tool", "Tool for OpenAI format testing")
        .with_parameters(parameters);
    registry.register(tool);
    
    let definitions = registry.get_openai_tool_definitions();
    assert_eq!(definitions.len(), 1);
    
    let definition = &definitions[0];
    assert!(definition.get("type").is_some());
    assert!(definition.get("function").is_some());
    
    let function = definition.get("function").unwrap();
    assert_eq!(function["name"], "openai_tool");
    assert_eq!(function["description"], "Tool for OpenAI format testing");
    assert!(function.get("parameters").is_some());
}

#[tokio::test]
async fn test_registry_debug_formatting() {
    let mut registry = ToolRegistry::new();
    
    let tool = TestTool::new("debug_tool", "Tool for debug testing");
    registry.register(tool);
    
    let debug_str = format!("{:?}", registry);
    assert!(debug_str.contains("ToolRegistry"));
    assert!(debug_str.contains("tool_count"));
    assert!(debug_str.contains("1"));
}

#[tokio::test]
async fn test_default_implementation() {
    let registry: ToolRegistry = Default::default();
    assert_eq!(registry.get_all_tools().len(), 0);
}

#[tokio::test]
async fn test_registry_clone() {
    let mut registry = ToolRegistry::new();
    let tool = TestTool::new("clone_test", "Tool for clone testing");
    registry.register(tool);
    
    let cloned_registry = registry.clone();
    assert!(cloned_registry.has_tool("clone_test"));
    assert_eq!(cloned_registry.get_all_tools().len(), 1);
}

#[tokio::test]
async fn test_concurrent_access() {
    let mut registry = ToolRegistry::new();
    
    // Register multiple tools
    for i in 0..10 {
        let tool = TestTool::new(&format!("tool_{}", i), &format!("Test tool {}", i));
        registry.register(tool);
    }
    
    // Test concurrent access
    let registry = Arc::new(registry);
    let mut handles = Vec::new();
    
    for i in 0..10 {
        let registry_clone = Arc::clone(&registry);
        let handle = tokio::spawn(async move {
            let tool_name = format!("tool_{}", i);
            assert!(registry_clone.has_tool(&tool_name));
            let retrieved_tool = registry_clone.get_tool(&tool_name);
            assert!(retrieved_tool.is_some());
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
}