use serde::{Deserialize, Deserializer, Serialize};

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
            "anthropic" | "claude" | "anthropic_model" | "anthropic-model" => ModelType::AnthropicModel,
            _ => ModelType::Custom(s.to_string()),
        }
    }
}

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
// Tool Argument Types
// ============================================================================

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
