use crate::agents::agent::{LlmClient, LlmResponse, ChatMessage, ToolDefinition};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

/// llama.cpp server LLM client implementation with OpenAI-compatible API
pub struct LlamaCppClient {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl LlamaCppClient {
    pub fn new(base_url: String, model: String) -> Self {
        // Ensure base_url doesn't end with a slash
        let base_url = base_url.trim_end_matches('/').to_string();
        Self {
            base_url,
            model,
            client: reqwest::Client::new(),
        }
    }

    fn get_chat_completions_url(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url)
    }
}

#[async_trait]
impl LlmClient for LlamaCppClient {
    async fn chat(&self, messages: Vec<ChatMessage>, tools: Vec<ToolDefinition>) -> Result<LlmResponse> {
        let request = self.build_chat_request(messages, tools).await?;

        let response = self.client
            .post(self.get_chat_completions_url())
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("llama.cpp API error: {}", error_text));
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
            }
        } else {
            ChatMessage {
                role: "assistant".to_string(),
                content: "No response generated".to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
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

        let response = self.client
            .post(self.get_chat_completions_url())
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

impl LlamaCppClient {
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
