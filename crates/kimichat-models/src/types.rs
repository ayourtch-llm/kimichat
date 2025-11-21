use serde::{Deserialize, Deserializer, Serialize};

/// Backend type for LLM models
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BackendType {
    Groq,
    Anthropic,
    Llama,
    OpenAI,
}

impl BackendType {
    /// Parse backend type from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "groq" => Some(Self::Groq),
            "anthropic" | "claude" => Some(Self::Anthropic),
            "llama" | "llamacpp" | "llama.cpp" | "llama-cpp" => Some(Self::Llama),
            "openai" => Some(Self::OpenAI),
            _ => None,
        }
    }

    /// Get string representation
    pub fn as_str(&self) -> &str {
        match self {
            Self::Groq => "groq",
            Self::Anthropic => "anthropic",
            Self::Llama => "llama",
            Self::OpenAI => "openai",
        }
    }
}

/// Model colors supported by the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelColor {
    BluModel = 0,
    GrnModel = 1,
    RedModel = 2,
}

/// Model provider configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct ModelProvider {
    /// Model name (e.g., "moonshotai/kimi-k2-instruct-0905")
    pub model_name: String,
    /// Backend type (Groq, Anthropic, Llama, etc.)
    pub backend: Option<BackendType>,
    /// API URL for the provider
    pub api_url: Option<String>,
    /// API key for the provider
    pub api_key: Option<String>,
}

impl std::fmt::Debug for ModelProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let masked_key = match &self.api_key {
            Some(key) if key.len() > 3 => format!("{}***", &key[..3]),
            Some(key) => format!("{}***", &key),
            None => "None".to_string(),
        };
        
        f.debug_struct("ModelProvider")
            .field("model_name", &self.model_name)
            .field("backend", &self.backend)
            .field("api_url", &self.api_url)
            .field("api_key", &masked_key)
            .finish()
    }
}

impl ModelProvider {
    /// Create a new ModelProvider with minimal configuration
    pub fn new(model_name: String) -> Self {
        Self {
            model_name,
            backend: None,
            api_url: None,
            api_key: None,
        }
    }
    
    /// Create a new ModelProvider with all fields
    pub fn with_config(model_name: String, backend: Option<BackendType>, api_url: Option<String>, api_key: Option<String>) -> Self {
        Self {
            model_name,
            backend,
            api_url,
            api_key,
        }
    }
}

/// CLI configuration for a specific model
#[derive(Debug, Clone, Default)]
pub struct ModelConfig {
    /// Backend type for this model
    pub backend: Option<String>,
    /// API URL for this model
    pub api_url: Option<String>,
    /// API key for this model
    pub api_key: Option<String>,
    /// Model name override for this model
    pub model: Option<String>,
}

impl ModelColor {
    /// Total number of model colors
    pub const COUNT: usize = 3;
    
    /// Get an iterator over all model colors
    pub fn iter() -> impl Iterator<Item = ModelColor> {
        [ModelColor::BluModel, ModelColor::GrnModel, ModelColor::RedModel]
            .iter().copied()
    }

    /// Get the display name for the model
    pub fn display_name(&self) -> &'static str {
        match self {
            ModelColor::BluModel => "Kimi-K2 (BluModel)",
            ModelColor::GrnModel => "GPT-OSS (GrnModel)",
            ModelColor::RedModel => "Llama-3.1-70B (RedModel)",
        }
    }

    /// Get the default model for this color
    pub fn default_model(&self) -> String {
        match self {
            ModelColor::BluModel => "moonshotai/kimi-k2-instruct-0905".to_string(),
            ModelColor::GrnModel => "openai/gpt-oss-120b".to_string(),
            ModelColor::RedModel => "meta-llama/llama-3.1-70b-versatile".to_string(),
        }
    }

    pub fn as_str_default(&self) -> String {
        match self {
            ModelColor::BluModel => "moonshotai/kimi-k2-instruct-0905".to_string(),
            ModelColor::GrnModel => "openai/gpt-oss-120b".to_string(),
            ModelColor::RedModel => "meta-llama/llama-3.1-70b-versatile".to_string(),
        }
    }

    /// Get the lowercase string representation of the model color
    pub fn as_str_lowercase(&self) -> &'static str {
        match self {
            ModelColor::BluModel => "blu",
            ModelColor::GrnModel => "grn",
            ModelColor::RedModel => "red",
        }
    }

    /// Get model identifier with optional overrides
    pub fn as_str(
        &self,
        blu_model_override: Option<&str>,
        grn_model_override: Option<&str>,
        red_model_override: Option<&str>,
    ) -> String {
        match self {
            ModelColor::BluModel => blu_model_override
                .map(|s| s.to_string())
                .unwrap_or_else(|| self.as_str_default()),
            ModelColor::GrnModel => grn_model_override
                .map(|s| s.to_string())
                .unwrap_or_else(|| self.as_str_default()),
            ModelColor::RedModel => red_model_override
                .map(|s| s.to_string())
                .unwrap_or_else(|| self.as_str_default()),
        }
    }

    pub fn from_string(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "blu_model" | "blu-model" | "blumodel" => ModelColor::BluModel,
            "grn_model" | "grn-model" | "grnmodel" => ModelColor::GrnModel,
            "red_model" | "red-model" | "redmodel" => ModelColor::RedModel,
            _ => {
                // For backward compatibility:
                // - Anthropic models default to BluModel
                // - Custom models default to GrnModel
                if s.to_lowercase().contains("anthropic") || s.to_lowercase().contains("claude") {
                    ModelColor::BluModel // Anthropic models map to BluModel
                } else if s.to_lowercase().contains("openai") || s.to_lowercase().contains("gpt") {
                    ModelColor::GrnModel // OpenAI models map to GrnModel
                } else {
                    ModelColor::GrnModel // Default to GrnModel for other custom models
                }
            }
        }
    }
}

impl std::str::FromStr for ModelColor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ModelColor::from_string(s))
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
