//! Core types and structures for kimichat
//!
//! This crate provides the foundational types used across all kimichat crates.

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

// ============================================================================
// Constants
// ============================================================================

/// Maximum context tokens to keep conversation under rate limits
pub const MAX_CONTEXT_TOKENS: usize = 100_000;

/// Maximum number of retries for API calls
pub const MAX_RETRIES: u32 = 3;

// ============================================================================
// Model Types
// ============================================================================

/// Model types supported by the system
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelType {
    BluModel,
    GrnModel,
    RedModel,
    AnthropicModel,
    Custom(String),
}

impl ModelType {
    pub fn as_str(&self) -> String {
        match self {
            ModelType::BluModel => "moonshotai/kimi-k2-instruct-0905".to_string(),
            ModelType::GrnModel => "openai/gpt-oss-120b".to_string(),
            ModelType::RedModel => "meta-llama/llama-3.1-70b-versatile".to_string(),
            ModelType::AnthropicModel => "claude-3-5-sonnet-20241022".to_string(),
            ModelType::Custom(name) => name.clone(),
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            ModelType::BluModel => "Kimi-K2-Instruct-0905".to_string(),
            ModelType::GrnModel => "GPT-OSS-120B".to_string(),
            ModelType::RedModel => "Llama-3.1-70B-Versatile".to_string(),
            ModelType::AnthropicModel => "Claude-3.5-Sonnet".to_string(),
            ModelType::Custom(name) => name.clone(),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "blu_model" | "blu-model" | "blumodel" => ModelType::BluModel,
            "grn_model" | "grn-model" | "grnmodel" => ModelType::GrnModel,
            "red_model" | "red-model" | "redmodel" => ModelType::RedModel,
            "anthropic" | "claude" | "anthropic_model" | "anthropic-model" => {
                ModelType::AnthropicModel
            }
            _ => ModelType::Custom(s.to_string()),
        }
    }
}

// ============================================================================
// Message Types
// ============================================================================

/// Helper function to deserialize string or null values
pub fn deserialize_string_or_null<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Null => Ok(String::new()),
        _ => Ok(String::new()),
    }
}

/// Message structure for chat API
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Message {
    #[serde(default)]
    pub role: String,
    #[serde(deserialize_with = "deserialize_string_or_null", default)]
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reasoning: Option<String>,
}

/// Tool call structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionCall,
}

/// Function call structure within a tool call
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

// ============================================================================
// Tool Parameter Types
// ============================================================================

/// Helper function to provide default pattern
fn default_pattern() -> String {
    "*".to_string()
}

#[derive(Debug, Deserialize)]
pub struct ReadFileArgs {
    pub file_path: String,
}

#[derive(Debug, Deserialize)]
pub struct WriteFileArgs {
    pub file_path: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ListFilesArgs {
    #[serde(default = "default_pattern")]
    pub pattern: String,
}

#[derive(Debug, Deserialize)]
pub struct EditFileArgs {
    pub file_path: String,
    pub old_content: String,
    pub new_content: String,
}

#[derive(Debug, Deserialize)]
pub struct SwitchModelArgs {
    pub model: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct RunCommandArgs {
    pub command: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchFilesArgs {
    #[serde(default)]
    pub query: String,
    #[serde(default = "default_pattern")]
    pub pattern: String,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub case_insensitive: bool,
    #[serde(default)]
    pub max_results: u32,
}

#[derive(Debug, Deserialize)]
pub struct OpenFileArgs {
    pub file_path: String,
    #[serde(default)]
    pub start_line: usize,
    #[serde(default)]
    pub end_line: usize,
}

// ============================================================================
// Tool Execution Types
// ============================================================================

/// Tool parameters
#[derive(Debug, Clone)]
pub struct ToolParameters {
    pub data: HashMap<String, serde_json::Value>,
}

impl ToolParameters {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn from_json(json_str: &str) -> anyhow::Result<Self> {
        let data: HashMap<String, serde_json::Value> = serde_json::from_str(json_str)?;
        Ok(Self { data })
    }

    pub fn set<T: Serialize>(&mut self, key: &str, value: T) {
        if let Ok(json_value) = serde_json::to_value(value) {
            self.data.insert(key.to_string(), json_value);
        }
    }

    pub fn get_required<T>(&self, key: &str) -> anyhow::Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let value = self
            .data
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("Required parameter '{}' missing", key))?;

        serde_json::from_value(value.clone())
            .map_err(|e| anyhow::anyhow!("Failed to parse parameter '{}': {}", key, e))
    }

    pub fn get_optional<T>(&self, key: &str) -> anyhow::Result<Option<T>>
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

impl Default for ToolParameters {
    fn default() -> Self {
        Self::new()
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
    pub default: Option<serde_json::Value>,
}

/// Helper macro for creating parameter definitions
#[macro_export]
macro_rules! param {
    ($name:expr, $type:expr, $desc:expr, required) => {
        (
            $name.to_string(),
            $crate::ParameterDefinition {
                param_type: $type.to_string(),
                description: $desc.to_string(),
                required: true,
                default: None,
            },
        )
    };
    ($name:expr, $type:expr, $desc:expr, optional, $default:expr) => {
        (
            $name.to_string(),
            $crate::ParameterDefinition {
                param_type: $type.to_string(),
                description: $desc.to_string(),
                required: false,
                default: Some(serde_json::Value::from($default)),
            },
        )
    };
    ($name:expr, $type:expr, $desc:expr, optional) => {
        (
            $name.to_string(),
            $crate::ParameterDefinition {
                param_type: $type.to_string(),
                description: $desc.to_string(),
                required: false,
                default: None,
            },
        )
    };
}

// ============================================================================
// Client Configuration
// ============================================================================

/// Client configuration for multi-model backends
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub blu_api_url: Option<String>,
    pub grn_api_url: Option<String>,
    pub red_api_url: Option<String>,
    pub anthropic_api_url: Option<String>,
    pub blu_api_key: Option<String>,
    pub grn_api_key: Option<String>,
    pub red_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub blu_model_name: Option<String>,
    pub grn_model_name: Option<String>,
    pub red_model_name: Option<String>,
    pub anthropic_model_name: Option<String>,
    pub blu_backend: Option<String>,
    pub grn_backend: Option<String>,
    pub red_backend: Option<String>,
    pub anthropic_backend: Option<String>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            blu_api_url: None,
            grn_api_url: None,
            red_api_url: None,
            anthropic_api_url: None,
            blu_api_key: None,
            grn_api_key: None,
            red_api_key: None,
            anthropic_api_key: None,
            blu_model_name: None,
            grn_model_name: None,
            red_model_name: None,
            anthropic_model_name: None,
            blu_backend: None,
            grn_backend: None,
            red_backend: None,
            anthropic_backend: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_type_conversion() {
        assert_eq!(ModelType::from_str("blu_model"), ModelType::BluModel);
        assert_eq!(ModelType::from_str("grn-model"), ModelType::GrnModel);
        assert_eq!(ModelType::from_str("anthropic"), ModelType::AnthropicModel);
    }

    #[test]
    fn test_tool_result() {
        let success = ToolResult::success("test".to_string());
        assert!(success.success);
        assert_eq!(success.content, "test");

        let error = ToolResult::error("error".to_string());
        assert!(!error.success);
        assert_eq!(error.error, Some("error".to_string()));
    }
}
