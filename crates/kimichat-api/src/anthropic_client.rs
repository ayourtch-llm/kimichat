use crate::agents::agent::{LlmClient, LlmResponse, ChatMessage, ToolDefinition, StreamingChunk};
use anyhow::{Result, Context};
use async_trait::async_trait;
use serde_json::Value;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use futures::Stream;
use futures::StreamExt;
use async_stream::stream;

/// Anthropic LLM client implementation using native Anthropic API
pub struct AnthropicLlmClient {
    api_key: String,
    model: String,
    base_url: String,
    agent_name: String,
    client: reqwest::Client,
}

impl AnthropicLlmClient {
    pub fn new(api_key: String, model: String, base_url: String, agent_name: String) -> Self {
        // Ensure base_url doesn't end with a slash
        let base_url = base_url.trim_end_matches('/').to_string();
        Self {
            api_key,
            model,
            base_url,
            agent_name,
            client: reqwest::Client::new(),
        }
    }

    fn get_messages_url(&self) -> String {
        format!("{}/v1/messages", self.base_url)
    }

    fn convert_messages_to_anthropic_format(&self, messages: Vec<ChatMessage>) -> Vec<Value> {
        messages.into_iter().filter_map(|msg| {
            // Skip system messages as they should be handled separately
            if msg.role == "system" {
                return None;
            }

            // Convert role to Anthropic format (only user/assistant allowed)
            let anthropic_role = if msg.role == "user" || msg.role == "assistant" {
                msg.role.clone()
            } else {
                // Convert any other roles to "user"
                "user".to_string()
            };

            let content = if msg.role == "tool" {
                // Tool result messages need special handling
                vec![
                    serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": msg.tool_call_id.unwrap_or_default(),
                        "content": msg.content
                    })
                ]
            } else if let Some(tool_calls) = msg.tool_calls {
                // Assistant message with tool calls
                let mut content = vec![];
                if !msg.content.is_empty() {
                    content.push(serde_json::json!({
                        "type": "text",
                        "text": msg.content
                    }));
                }
                for tool_call in tool_calls {
                    content.push(serde_json::json!({
                        "type": "tool_use",
                        "id": tool_call.id,
                        "name": tool_call.function.name,
                        "input": serde_json::from_str::<Value>(&tool_call.function.arguments)
                            .unwrap_or_else(|_| serde_json::json!({}))
                    }));
                }
                content
            } else {
                // Regular text message
                vec![serde_json::json!({
                    "type": "text",
                    "text": msg.content
                })]
            };

            Some(serde_json::json!({
                "role": anthropic_role,
                "content": content
            }))
        }).collect()
    }

    fn convert_tools_to_anthropic_format(&self, tools: Vec<ToolDefinition>) -> Vec<Value> {
        tools.into_iter().map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.parameters
            })
        }).collect()
    }

    fn convert_anthropic_response_to_chat_message(&self, response: &Value) -> ChatMessage {
        let empty_vec = vec![];
        let content = response["content"].as_array().unwrap_or(&empty_vec);

        let mut text_content = String::new();
        let mut tool_calls = Vec::new();

        for item in content {
            if let Some(content_type) = item["type"].as_str() {
                match content_type {
                    "text" => {
                        if let Some(text) = item["text"].as_str() {
                            text_content.push_str(text);
                        }
                    }
                    "tool_use" => {
                        if let Some(name) = item["name"].as_str() {
                            if let Some(id) = item["id"].as_str() {
                                let input = item["input"].clone();
                                tool_calls.push(crate::agents::agent::ToolCall {
                                    id: id.to_string(),
                                    function: crate::agents::agent::FunctionCall {
                                        name: name.to_string(),
                                        arguments: input.to_string(),
                                    },
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        ChatMessage {
            role: response["role"].as_str().unwrap_or("assistant").to_string(),
            content: text_content,
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            tool_call_id: None,
            name: None,
            reasoning: None,
        }
    }
}

#[async_trait]
impl LlmClient for AnthropicLlmClient {
    async fn chat(&self, messages: Vec<ChatMessage>, tools: Vec<ToolDefinition>) -> Result<LlmResponse> {
        // Extract system messages and combine them
        let system_messages: Vec<String> = messages.iter()
            .filter(|msg| msg.role == "system")
            .map(|msg| msg.content.clone())
            .collect();

        let combined_system = if system_messages.is_empty() {
            None
        } else {
            Some(system_messages.join("\n\n"))
        };

        let anthropic_messages = self.convert_messages_to_anthropic_format(messages);
        let anthropic_tools = self.convert_tools_to_anthropic_format(tools);

        let mut request = serde_json::json!({
            "model": self.model,
            "messages": anthropic_messages,
            "max_tokens": 4096,
            "tools": anthropic_tools,
            "tool_choice": {"type": "auto"}
        });

        // Add system message if present
        if let Some(system_content) = combined_system {
            request["system"] = serde_json::Value::String(system_content);
        }

        // Log request to file for persistent debugging
        let _ = self.log_request_to_file(&self.get_messages_url(), &request);

        let response = self.client
            .post(&self.get_messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .header("Connection", "keep-alive")  // Keep connection alive for streaming
            .header("Cache-Control", "no-cache")  // Prevent caching of streaming data
            .header("Accept", "text/event-stream")  // Explicitly accept SSE
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Anthropic API error: {}", error_text));
        }

        let response_text = response.text().await?;
        let response_json: Value = serde_json::from_str(&response_text)?;

        let message = self.convert_anthropic_response_to_chat_message(&response_json);

        let usage = response_json.get("usage").map(|u| {
            crate::agents::agent::TokenUsage {
                prompt_tokens: u["input_tokens"].as_u64().unwrap_or(0) as u32,
                completion_tokens: u["output_tokens"].as_u64().unwrap_or(0) as u32,
                total_tokens: (u["input_tokens"].as_u64().unwrap_or(0) +
                               u["output_tokens"].as_u64().unwrap_or(0)) as u32,
            }
        });

        Ok(LlmResponse {
            message,
            usage,
        })
    }

    async fn chat_streaming(&self, messages: Vec<ChatMessage>, tools: Vec<ToolDefinition>) -> Result<Box<dyn Stream<Item = Result<StreamingChunk>> + Send + Unpin>> {
        // Extract system messages and combine them
        let system_messages: Vec<String> = messages.iter()
            .filter(|msg| msg.role == "system")
            .map(|msg| msg.content.clone())
            .collect();

        let combined_system = if system_messages.is_empty() {
            None
        } else {
            Some(system_messages.join("\n\n"))
        };

        let anthropic_messages = self.convert_messages_to_anthropic_format(messages);
        let anthropic_tools = self.convert_tools_to_anthropic_format(tools);

        let mut request = serde_json::json!({
            "model": self.model,
            "messages": anthropic_messages,
            "max_tokens": 4096,
            "tools": anthropic_tools,
            "tool_choice": {"type": "auto"},
            "stream": true
        });

        // Add system message if present
        if let Some(system_content) = combined_system {
            request["system"] = serde_json::Value::String(system_content);
        }

        // Log request to file for persistent debugging
        let _ = self.log_request_to_file(&self.get_messages_url(), &request);

        let response = self.client
            .post(&self.get_messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .header("Connection", "keep-alive")  // Keep connection alive for streaming
            .header("Cache-Control", "no-cache")  // Prevent caching of streaming data
            .header("Accept", "text/event-stream")  // Explicitly accept SSE
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Anthropic API streaming error: {}", error_text));
        }

        let byte_stream = response.bytes_stream();

        let stream = stream! {
            let mut buffer = String::new();
            let mut byte_stream = byte_stream;
            let mut event_buffer = String::new();

            while let Some(chunk_result) = byte_stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        let chunk_str = String::from_utf8_lossy(&chunk);
                        
                        // Process character by character for minimal latency
                        for ch in chunk_str.chars() {
                            buffer.push(ch);
                            event_buffer.push(ch);

                            // When we hit a newline, process the complete SSE line
                            if ch == '\n' {
                                let line = std::mem::take(&mut event_buffer);
                                
                                // Parse SSE line immediately and yield if we get content
                                if let Some(streaming_chunk) = Self::parse_sse_line(&line) {
                                    yield Ok(streaming_chunk);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(anyhow::anyhow!("Stream error: {}", e));
                        break;
                    }
                }
            }

            // Process any remaining data in the buffer
            if !event_buffer.is_empty() {
                if let Some(streaming_chunk) = Self::parse_sse_line(&event_buffer) {
                    yield Ok(streaming_chunk);
                }
            }
        };

        Ok(Box::new(Box::pin(stream)))
    }

    async fn chat_completion(&self, messages: &[ChatMessage]) -> Result<String> {
        let anthropic_messages = self.convert_messages_to_anthropic_format(messages.to_vec());

        let request = serde_json::json!({
            "model": self.model,
            "messages": anthropic_messages,
            "max_tokens": 2000,
            "temperature": 0.1
        });

        // Log request to file for persistent debugging
        let _ = self.log_request_to_file(&self.get_messages_url(), &request);

        let response = self.client
            .post(&self.get_messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .header("Connection", "keep-alive")  // Keep connection alive for streaming
            .header("Cache-Control", "no-cache")  // Prevent caching of streaming data
            .header("Accept", "text/event-stream")  // Explicitly accept SSE
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("API request failed: {} - {}", status, error_text));
        }

        let response_text = response.text().await?;
        let response_json: serde_json::Value = serde_json::from_str(&response_text)?;

        if let Some(content) = response_json["content"].as_array() {
            let mut text = String::new();
            for item in content {
                if item["type"] == "text" {
                    if let Some(text_content) = item["text"].as_str() {
                        text.push_str(text_content);
                    }
                }
            }
            Ok(text)
        } else {
            Err(anyhow::anyhow!("No content in response"))
        }
    }
}

impl AnthropicLlmClient {
    /// Parse a single SSE line and return a streaming chunk if it contains text
    fn parse_sse_line(line: &str) -> Option<StreamingChunk> {
        // Only process data lines
        if !line.starts_with("data: ") {
            return None;
        }

        let data = &line[6..];

        // Check for stream end
        if data.trim() == "[DONE]" {
            return Some(StreamingChunk {
                content: String::new(),
                delta: String::new(),
                finish_reason: Some("stop".to_string()),
            });
        }

        // Parse JSON event
        if let Ok(json) = serde_json::from_str::<Value>(data) {
            if let Some(content_type) = json["type"].as_str() {
                match content_type {
                    "content_block_delta" => {
                        // This is the main event type for streaming text content
                        if let Some(delta) = json.get("delta").and_then(|v| v.as_object()) {
                            if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                // Return immediately for minimal latency
                                return Some(StreamingChunk {
                                    content: String::new(),
                                    delta: text.to_string(),
                                    finish_reason: None,
                                });
                            }
                        }
                    }
                    "content_block_start" => {
                        // Handle initial content block (less common for text)
                        if let Some(content_block) = json.get("content_block").and_then(|v| v.as_object()) {
                            if let Some(text) = content_block.get("text").and_then(|v| v.as_str()) {
                                return Some(StreamingChunk {
                                    content: text.to_string(),
                                    delta: text.to_string(),
                                    finish_reason: None,
                                });
                            }
                        }
                    }
                    "message_stop" => {
                        return Some(StreamingChunk {
                            content: String::new(),
                            delta: String::new(),
                            finish_reason: Some("stop".to_string()),
                        });
                    }
                    "error" => {
                        // Handle errors - these will be propagated as Err results
                        if let Some(error) = json.get("error").and_then(|v| v.as_object()) {
                            let error_msg = error.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                            // Don't return a chunk for errors - let the stream consumer handle them
                        }
                    }
                    // Other event types (ping, message_start, content_block_stop, etc.) are ignored
                    _ => {}
                }
            }
        }

        None
    }

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
        log_content.push_str(&format!("HTTP REQUEST LOG (ANTHROPIC)\n"));
        log_content.push_str(&format!("===============================\n\n"));
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
        log_content.push_str(&format!("  x-api-key: {}***\n", &self.api_key.chars().take(10).collect::<String>()));
        log_content.push_str("  anthropic-version: 2023-06-01\n\n");

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
}