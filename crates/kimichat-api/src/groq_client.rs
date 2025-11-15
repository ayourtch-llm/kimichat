use crate::agents::agent::{LlmClient, LlmResponse, ChatMessage, ToolDefinition};
use anyhow::{Result, Context};
use async_trait::async_trait;
use serde_json::Value;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

/// Groq LLM client implementation that bridges with the existing KimiChat system
pub struct GroqLlmClient {
    api_key: String,
    model: String,
    api_url: String,
    agent_name: String,
    client: reqwest::Client,
}

impl GroqLlmClient {
    pub fn new(api_key: String, model: String, api_url: String, agent_name: String) -> Self {
        Self {
            api_key,
            model,
            api_url,
            agent_name,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LlmClient for GroqLlmClient {
    async fn chat(&self, messages: Vec<ChatMessage>, tools: Vec<ToolDefinition>) -> Result<LlmResponse> {
        let request = self.build_chat_request(messages, tools).await?;

        // Log request to file for persistent debugging
        let _ = self.log_request_to_file(&self.api_url, &request);

        let response = self.client
            .post(&self.api_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Groq API error: {}", error_text));
        }

        let response_text = response.text().await?;
        let chat_response: crate::ChatResponse = serde_json::from_str(&response_text)?;

        let message = if let Some(choice) = chat_response.choices.into_iter().next() {
            ChatMessage {
                role: choice.message.role,
                content: choice.message.content,
                tool_calls: choice.message.tool_calls.map(|calls| {
                    calls.into_iter().map(|call| crate::agents::agent::ToolCall {
                        id: call.id,
                        function: crate::agents::agent::FunctionCall {
                            name: call.function.name,
                            arguments: call.function.arguments,
                        },
                    }).collect()
                }),
                tool_call_id: choice.message.tool_call_id,
                name: choice.message.name,
                reasoning: None,
            }
        } else {
            ChatMessage {
                role: "assistant".to_string(),
                content: "No response generated".to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning: None,
            }
        };

        Ok(LlmResponse {
            message,
            usage: chat_response.usage.map(|usage| crate::agents::agent::TokenUsage {
                prompt_tokens: usage.prompt_tokens as u32,
                completion_tokens: usage.completion_tokens as u32,
                total_tokens: usage.total_tokens as u32,
            }),
        })
    }

    async fn chat_completion(&self, messages: &[ChatMessage]) -> Result<String> {
        // For progress evaluation, make a simple API call without tools
        let api_request = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "temperature": 0.1,
            "max_tokens": 2000
        });

        // Log request to file for persistent debugging
        let _ = self.log_request_to_file(&self.api_url, &api_request);

        let client = reqwest::Client::new();
        let response = client
            .post(&self.api_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&api_request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("API request failed: {} - {}", status, error_text));
        }

        let response_text = response.text().await?;
        let chat_response: serde_json::Value = serde_json::from_str(&response_text)?;

        if let Some(content) = chat_response["choices"][0]["message"]["content"].as_str() {
            Ok(content.to_string())
        } else {
            Err(anyhow::anyhow!("No content in response"))
        }
    }
}

impl GroqLlmClient {
    fn log_request_to_file(&self, url: &str, request: &serde_json::Value) -> Result<()> {
        // Create logs directory if it doesn't exist
        fs::create_dir_all("logs")?;

        // Generate timestamp for filename
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Create filename with timestamp and model name
        let model_name = self.model.replace('/', "-");
        let filename = format!("logs/req-{}-{}-agent-{}.txt", timestamp, model_name, self.agent_name);

        // Build the log content
        let mut log_content = String::new();
        log_content.push_str(&format!("HTTP REQUEST LOG (AGENT)\n"));
        log_content.push_str(&format!("========================\n\n"));
        log_content.push_str(&format!("Timestamp: {}\n", timestamp));
        log_content.push_str(&format!("Model: {}\n", self.model));
        log_content.push_str(&format!("Agent: {}\n\n", self.agent_name));

        // Parse URL to show host and port
        if let Ok(parsed_url) = reqwest::Url::parse(url) {
            log_content.push_str(&format!("URL: {}\n", url));
            log_content.push_str(&format!("Host: {}\n", parsed_url.host_str().unwrap_or("unknown")));
            log_content.push_str(&format!("Port: {}\n",
                parsed_url.port().map(|p| p.to_string()).unwrap_or_else(||
                    if parsed_url.scheme() == "https" { "443 (default)".to_string() } else { "80 (default)".to_string() }
                )
            ));
            log_content.push_str(&format!("Scheme: {}\n\n", parsed_url.scheme()));
        } else {
            log_content.push_str(&format!("URL: {}\n\n", url));
        }

        log_content.push_str("Headers:\n");
        log_content.push_str("  Content-Type: application/json\n");
        log_content.push_str(&format!("  Authorization: Bearer {}***\n\n", &self.api_key.chars().take(10).collect::<String>()));

        log_content.push_str("Request Body:\n");
        match serde_json::to_string_pretty(&request) {
            Ok(json) => {
                log_content.push_str(&json);
                log_content.push_str("\n");
            }
            Err(e) => {
                log_content.push_str(&format!("Error serializing request: {}\n", e));
            }
        }

        // Write to file
        fs::write(&filename, log_content)
            .with_context(|| format!("Failed to write request log to {}", filename))?;

        Ok(())
    }

    async fn build_chat_request(&self, messages: Vec<ChatMessage>, tools: Vec<ToolDefinition>) -> Result<serde_json::Value> {
        let chat_messages: Vec<crate::Message> = messages.into_iter().map(|msg| {
            crate::Message {
                role: msg.role,
                content: msg.content,
                tool_calls: msg.tool_calls.map(|calls| {
                    calls.into_iter().map(|call| crate::ToolCall {
                        id: call.id,
                        tool_type: "function".to_string(),
                        function: crate::FunctionCall {
                            name: call.function.name,
                            arguments: call.function.arguments,
                        },
                    }).collect()
                }),
                tool_call_id: msg.tool_call_id,
                name: msg.name,
                reasoning: None,
            }
        }).collect();

        let tool_definitions: Vec<crate::Tool> = tools.into_iter().map(|tool| {
            crate::Tool {
                tool_type: "function".to_string(),
                function: crate::FunctionDef {
                    name: tool.name,
                    description: tool.description,
                    parameters: tool.parameters,
                },
            }
        }).collect();

        Ok(serde_json::json!({
            "model": self.model,
            "messages": chat_messages,
            "tools": tool_definitions,
            "tool_choice": "auto"
        }))
    }
}