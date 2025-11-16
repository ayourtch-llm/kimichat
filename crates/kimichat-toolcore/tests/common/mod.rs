use std::collections::HashMap;
use tempfile::{TempDir, NamedTempFile};
use kimichat_models::tools::{Tool, ToolParameter, ToolParameterType, ToolCategory};
use serde_json::Value;

/// Common test utilities for toolcore testing
pub struct TestFixtures {
    pub temp_dir: TempDir,
}

impl TestFixtures {
    pub fn new() -> Self {
        Self {
            temp_dir: TempDir::new().expect("Failed to create temp directory"),
        }
    }

    /// Create a test tool with basic metadata
    pub fn create_test_tool(
        name: &str,
        description: &str,
        parameters: Vec<ToolParameter>,
    ) -> Tool {
        Tool {
            name: name.to_string(),
            description: description.to_string(),
            parameters,
            category: ToolCategory::FileOps,
            async_tool: false,
            context_required: false,
        }
    }

    /// Create a simple test file with content
    pub fn create_test_file(&self, name: &str, content: &str) -> std::path::PathBuf {
        let file_path = self.temp_dir.path().join(name);
        std::fs::write(&file_path, content).expect("Failed to write test file");
        file_path
    }

    /// Create a test parameter
    pub fn create_test_parameter(
        name: &str,
        param_type: ToolParameterType,
        required: bool,
        description: &str,
    ) -> ToolParameter {
        ToolParameter {
            name: name.to_string(),
            param_type,
            required,
            description: description.to_string(),
            default_value: if required { None } else { Some(Value::String("default".to_string())) },
        }
    }

    /// Create a simple string parameter
    pub fn create_string_parameter(name: &str, required: bool) -> ToolParameter {
        Self::create_test_parameter(
            name,
            ToolParameterType::String,
            required,
            &format!("Test parameter {}", name),
        )
    }

    /// Create a simple boolean parameter
    pub fn create_boolean_parameter(name: &str, required: bool) -> ToolParameter {
        Self::create_test_parameter(
            name,
            ToolParameterType::Boolean,
            required,
            &format!("Test boolean parameter {}", name),
        )
    }

    /// Create a basic file operations tool for testing
    pub fn create_file_read_tool() -> Tool {
        Self::create_test_tool(
            "file_read",
            "Read contents of a file",
            vec![
                Self::create_string_parameter("file_path", true),
                Self::create_boolean_parameter("include_line_numbers", false),
            ],
        )
    }

    /// Create a basic search tool for testing
    pub fn create_search_tool() -> Tool {
        Self::create_test_tool(
            "search",
            "Search for text in files",
            vec![
                Self::create_string_parameter("query", true),
                Self::create_string_parameter("pattern", false),
                Self::create_boolean_parameter("case_sensitive", false),
            ],
        )
    }
}

/// Common test assertions
pub mod assertions {
    use pretty_assertions::assert_eq;
    use kimichat_models::tools::{Tool, ToolParameter};

    /// Assert that two tools are equal (ignoring minor metadata differences)
    pub fn assert_tools_equal(expected: &Tool, actual: &Tool) {
        assert_eq!(expected.name, actual.name, "Tool names don't match");
        assert_eq!(expected.description, actual.description, "Tool descriptions don't match");
        assert_eq!(expected.category, actual.category, "Tool categories don't match");
        assert_eq!(expected.parameters.len(), actual.parameters.len(), "Parameter counts don't match");
        
        for (expected_param, actual_param) in expected.parameters.iter().zip(actual.parameters.iter()) {
            assert_parameters_equal(expected_param, actual_param);
        }
    }

    /// Assert that two tool parameters are equal
    pub fn assert_parameters_equal(expected: &ToolParameter, actual: &ToolParameter) {
        assert_eq!(expected.name, actual.name, "Parameter names don't match");
        assert_eq!(expected.param_type, actual.param_type, "Parameter types don't match");
        assert_eq!(expected.required, actual.required, "Parameter required status doesn't match");
    }
}

/// Mock data generators
pub mod mocks {
    use super::*;
    use kimichat_models::tools::ToolCategory;

    /// Generate a collection of mock tools for testing
    pub fn generate_mock_tools() -> Vec<Tool> {
        vec![
            TestFixtures::create_file_read_tool(),
            TestFixtures::create_search_tool(),
            TestFixtures::create_test_tool(
                "system_command",
                "Execute a system command",
                vec![
                    TestFixtures::create_string_parameter("command", true),
                    TestFixtures::create_string_parameter("working_dir", false),
                ],
            ),
            TestFixtures::create_test_tool(
                "web_request",
                "Make an HTTP request",
                vec![
                    TestFixtures::create_string_parameter("url", true),
                    TestFixtures::create_string_parameter("method", false),
                    TestFixtures::create_boolean_parameter("follow_redirects", false),
                ],
            ),
        ]
    }

    /// Generate mock tool parameters for testing validation
    pub fn generate_mock_parameters() -> Vec<ToolParameter> {
        vec![
            TestFixtures::create_string_parameter("string_param", true),
            TestFixtures::create_boolean_parameter("bool_param", false),
            TestFixtures::create_test_parameter(
                "number_param",
                ToolParameterType::Number,
                false,
                "A numeric parameter",
            ),
            TestFixtures::create_test_parameter(
                "array_param",
                ToolParameterType::Array,
                false,
                "An array parameter",
            ),
            TestFixtures::create_test_parameter(
                "object_param",
                ToolParameterType::Object,
                false,
                "An object parameter",
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_fixtures_creation() {
        let fixtures = TestFixtures::new();
        assert!(fixtures.temp_dir.path().exists());
    }

    #[test]
    fn test_test_file_creation() {
        let fixtures = TestFixtures::new();
        let test_content = "Hello, world!";
        let file_path = fixtures.create_test_file("test.txt", test_content);
        
        assert!(file_path.exists());
        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, test_content);
    }

    #[test]
    fn test_mock_tools_generation() {
        let tools = mocks::generate_mock_tools();
        assert_eq!(tools.len(), 4);
        
        let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
        assert!(tool_names.contains(&"file_read".to_string()));
        assert!(tool_names.contains(&"search".to_string()));
        assert!(tool_names.contains(&"system_command".to_string()));
        assert!(tool_names.contains(&"web_request".to_string()));
    }

    #[test]
    fn test_tool_parameter_creation() {
        let string_param = TestFixtures::create_string_parameter("test", true);
        assert_eq!(string_param.name, "test");
        assert_eq!(string_param.required, true);
        assert!(matches!(string_param.param_type, ToolParameterType::String));
    }
}