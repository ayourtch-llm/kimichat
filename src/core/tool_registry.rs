use std::collections::HashMap;
use std::sync::Arc;
use super::tool::{Tool, ToolParameters, ToolResult};
use super::tool_context::ToolContext;

/// Registry for managing and discovering tools
#[derive(Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    categories: HashMap<String, Vec<String>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tool_count", &self.tools.len())
            .field("categories", &self.categories)
            .finish()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            categories: HashMap::new(),
        }
    }

    /// Register a new tool
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        let name = tool.name().to_string();
        let tool_arc = Arc::new(tool);
        self.tools.insert(name.clone(), tool_arc);
    }

    /// Register a tool with categories
    pub fn register_with_categories<T: Tool + 'static>(&mut self, tool: T, categories: Vec<String>) {
        let name = tool.name().to_string();
        self.register(tool);

        for category in categories {
            self.categories.entry(category).or_insert_with(Vec::new).push(name.clone());
        }
    }

    /// Get a tool by name
    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// Get all tools
    pub fn get_all_tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.values().cloned().collect()
    }

    /// Get tools by category
    pub fn get_tools_by_category(&self, category: &str) -> Vec<Arc<dyn Tool>> {
        if let Some(tool_names) = self.categories.get(category) {
            tool_names.iter()
                .filter_map(|name| self.tools.get(name))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Check if a tool exists
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get tool names
    pub fn get_tool_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Execute a tool by name
    pub async fn execute_tool(
        &self,
        name: &str,
        params: ToolParameters,
        context: &ToolContext,
    ) -> ToolResult {
        match self.get_tool(name) {
            Some(tool) => tool.execute(params, context).await,
            None => ToolResult::error(format!("Tool '{}' not found", name)),
        }
    }

    /// Get all tool definitions in OpenAI format
    pub fn get_openai_tool_definitions(&self) -> Vec<serde_json::Value> {
        let mut tools: Vec<_> = self.tools.iter().collect();
        // Sort by tool name to ensure consistent ordering (critical for prompt caching)
        tools.sort_by_key(|(name, _)| name.as_str());
        tools.into_iter()
            .map(|(_, tool)| tool.to_openai_definition())
            .collect()
    }

    /// Get categories
    pub fn get_categories(&self) -> Vec<String> {
        self.categories.keys().cloned().collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tool::ParameterDefinition;

    struct MockTool {
        name: String,
        description: String,
    }

    #[async_trait::async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.description
        }

        fn parameters(&self) -> HashMap<String, ParameterDefinition> {
            HashMap::new()
        }

        async fn execute(&self, _params: ToolParameters, _context: &ToolContext) -> ToolResult {
            ToolResult::success("mock result".to_string())
        }
    }

    #[tokio::test]
    async fn test_tool_registry() {
        let mut registry = ToolRegistry::new();
        let tool = MockTool {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
        };

        registry.register(tool);

        assert!(registry.has_tool("test_tool"));
        let retrieved_tool = registry.get_tool("test_tool");
        assert!(retrieved_tool.is_some());

        let params = ToolParameters { data: HashMap::new() };
        let context = ToolContext::new(
            std::path::PathBuf::from("/tmp"),
            "test_session".to_string(),
            crate::policy::PolicyManager::new(),
        );
        let result = registry.execute_tool("test_tool", params, &context).await;
        assert!(result.success);
    }
}