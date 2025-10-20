use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use async_trait::async_trait;

/// Tool parameters
#[derive(Debug, Clone)]
pub struct ToolParameters {
    pub data: HashMap<String, Value>,
}

impl ToolParameters {
    pub fn from_json(json_str: &str) -> Result<Self> {
        let data: HashMap<String, Value> = serde_json::from_str(json_str)?;
        Ok(Self { data })
    }

    pub fn get_required<T>(&self, key: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let value = self.data.get(key)
            .ok_or_else(|| anyhow::anyhow!("Required parameter '{}' missing", key))?;

        serde_json::from_value(value.clone())
            .map_err(|e| anyhow::anyhow!("Failed to parse parameter '{}': {}", key, e))
    }

    pub fn get_optional<T>(&self, key: &str) -> Result<Option<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        match self.data.get(key) {
            Some(value) => {
                let parsed: T = serde_json::from_value(value.clone())
                    .map_err(|e| anyhow::anyhow!("Failed to parse parameter '{}': {}", key, e))?;
                Ok(Some(parsed))
            }
            None => Ok(None),
        }
    }
}

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub content: String,
    pub error: Option<String>,
}

impl ToolResult {
    pub fn success(content: String) -> Self {
        Self {
            success: true,
            content,
            error: None,
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            success: false,
            content: String::new(),
            error: Some(error),
        }
    }
}

/// Tool parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDefinition {
    pub param_type: String,
    pub description: String,
    pub required: bool,
    pub default: Option<Value>,
}

/// Tool trait that all tools must implement
#[async_trait]
pub trait Tool: Send + Sync {
    /// Name of the tool (must be unique)
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// Parameter definitions
    fn parameters(&self) -> HashMap<String, ParameterDefinition>;

    /// Execute the tool
    async fn execute(&self, params: ToolParameters, context: &crate::core::tool_context::ToolContext) -> ToolResult;

    /// Get OpenAI-compatible tool definition
    fn to_openai_definition(&self) -> serde_json::Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for (name, param_def) in self.parameters() {
            let param_json = serde_json::json!({
                "type": param_def.param_type,
                "description": param_def.description,
                "default": param_def.default
            });
            properties.insert(name.clone(), param_json);

            if param_def.required {
                required.push(name);
            }
        }

        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": {
                    "type": "object",
                    "properties": properties,
                    "required": required
                }
            }
        })
    }
}

/// Helper macro for creating parameter definitions
#[macro_export]
macro_rules! param {
    ($name:expr, $type:expr, $desc:expr, required) => {
        (
            $name.to_string(),
            ParameterDefinition {
                param_type: $type.to_string(),
                description: $desc.to_string(),
                required: true,
                default: None,
            }
        )
    };
    ($name:expr, $type:expr, $desc:expr, optional, $default:expr) => {
        (
            $name.to_string(),
            ParameterDefinition {
                param_type: $type.to_string(),
                description: $desc.to_string(),
                required: false,
                default: Some(serde_json::Value::from($default)),
            }
        )
    };
    ($name:expr, $type:expr, $desc:expr, optional) => {
        (
            $name.to_string(),
            ParameterDefinition {
                param_type: $type.to_string(),
                description: $desc.to_string(),
                required: false,
                default: None,
            }
        )
    };
}