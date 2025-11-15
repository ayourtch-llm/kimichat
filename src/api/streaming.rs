use anyhow::Result;
use colored::Colorize;
use futures_util::StreamExt;
use std::io::{self, Write};
use std::env;

use crate::KimiChat;
use crate::models::{ModelType, Message, Usage, ChatRequest, StreamChunk};
use crate::agents::agent::ToolDefinition;
use crate::logging::{log_request, log_request_to_file, log_response, log_stream_chunk};
use crate::tools_execution::parse_xml_tool_calls;
use crate::{ToolCall, FunctionCall};
use crate::agents::agent::ChatMessage;

/// Handle streaming API response for Groq-style APIs
pub(crate) async fn call_api_streaming(
    chat: &KimiChat,
    orig_messages: &[Message],
) -> Result<(Message, Option<Usage>, ModelType)> {
    use std::io::{self, Write};
    use futures_util::StreamExt;

    let current_model = chat.current_model.clone();

    // Strip reasoning field from messages (only supported by some models like Groq)
    let messages: Vec<Message> = orig_messages.iter().map(|m| {
        let mut msg = m.clone();
        msg.reasoning = None; // Strip reasoning field to avoid compatibility issues
        msg
    }).collect();

    let request = ChatRequest {
        model: current_model.as_str().to_string(),
        messages,
        tools: chat.get_tools(),
        tool_choice: "auto".to_string(),
        stream: Some(true),
    };

    // Get the appropriate API URL based on the current model
    let api_url = crate::config::get_api_url(&chat.client_config, &current_model);

    // Log request details in verbose mode
    log_request(&api_url, &request, &chat.api_key, chat.verbose);

    // Log request to file for persistent debugging
    let _ = log_request_to_file(&api_url, &request, &current_model, &chat.api_key);

    let api_key = crate::config::get_api_key(&chat.client_config, &chat.api_key, &current_model);
    let response = chat
        .client
        .post(&api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    let status = response.status();
    let headers = response.headers().clone();

    if !status.is_success() {
        let error_body = response.text().await.unwrap_or_else(|_| "Unable to read error body".to_string());

        // Log error response
        log_response(&status, &headers, &error_body, chat.verbose);

        return Err(anyhow::anyhow!("API request failed with status {}: {}", status, error_body));
    }

    if chat.verbose {
        println!("\n{}", "📡 Starting streaming response...".bright_cyan());
        println!("{}", "═".repeat(80).bright_cyan());
    }

    // Process streaming response
    let mut accumulated_content = String::new();
    let mut accumulated_reasoning = String::new();
    let mut accumulated_tool_calls: Vec<ToolCall> = Vec::new();
    let mut role = String::new();
    let mut usage: Option<Usage> = None;
    let mut buffer = String::new();

    // Show thinking indicator
    print!("🤔 Thinking...");
    io::stdout().flush().unwrap();
    let mut first_chunk = true;
    let mut first_reasoning = true;

    // Read the response as a stream of bytes
    let mut stream = response.bytes_stream();
    let mut chunk_counter = 0;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(bytes) => {
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                // Process complete lines (SSE format: "data: {json}\n\n")
                while let Some(line_end) = buffer.find("\n\n") {
                    let line = buffer[..line_end].to_string();
                    buffer = buffer[line_end + 2..].to_string();

                    // Skip empty lines and non-data lines
                    if line.trim().is_empty() || !line.starts_with("data: ") {
                        continue;
                    }

                    let data = &line[6..]; // Skip "data: " prefix

                    // Log stream chunk in verbose mode
                    chunk_counter += 1;
                    log_stream_chunk(chunk_counter, data, chat.verbose);

                    // Check for stream end marker
                    if data.trim() == "[DONE]" {
                        if chat.verbose {
                            println!("{}", "✓ Stream completed".bright_green());
                            println!("{}", "═".repeat(80).bright_green());
                        }
                        break;
                    }

                    // Parse the JSON chunk
                    if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                        if let Some(usage_data) = chunk.usage {
                            usage = Some(usage_data);
                        }

                        if let Some(choice) = chunk.choices.first() {
                            let delta = &choice.delta;

                            // Update role if present
                            if let Some(r) = &delta.role {
                                role = r.clone();
                            }

                            // Accumulate and display reasoning content (thinking process)
                            if let Some(reasoning) = &delta.reasoning_content {
                                if first_chunk {
                                    // Clear thinking indicator
                                    print!("\r\x1B[K");
                                    io::stdout().flush().unwrap();
                                    first_chunk = false;
                                }

                                if first_reasoning {
                                    // Show reasoning header
                                    print!("{}", "💭 ".bright_black());
                                    first_reasoning = false;
                                }

                                accumulated_reasoning.push_str(reasoning);
                                // Display reasoning in dim color to distinguish from actual response
                                print!("{}", reasoning.bright_black());
                                io::stdout().flush().unwrap();
                            }

                            // Accumulate content and display it
                            if let Some(content) = &delta.content {
                                if first_chunk {
                                    // Clear thinking indicator
                                    print!("\r\x1B[K");
                                    io::stdout().flush().unwrap();
                                    first_chunk = false;
                                }

                                // If we just finished reasoning, add separator
                                if !first_reasoning && accumulated_content.is_empty() {
                                    println!(); // New line after reasoning
                                }

                                accumulated_content.push_str(content);
                                print!("{}", content);
                                io::stdout().flush().unwrap();
                            }

                            // Accumulate tool calls if present (streaming deltas)
                            if let Some(tool_call_deltas) = &delta.tool_calls {
                                if first_chunk {
                                    // Clear thinking indicator
                                    print!("\r\x1B[K");
                                    print!("🔧 Tool calls...");
                                    io::stdout().flush().unwrap();
                                    first_chunk = false;
                                }

                                for delta_call in tool_call_deltas {
                                    // Ensure we have enough slots in the accumulated array
                                    while accumulated_tool_calls.len() <= delta_call.index {
                                        accumulated_tool_calls.push(ToolCall {
                                            id: String::new(),
                                            tool_type: "function".to_string(),
                                            function: FunctionCall {
                                                name: String::new(),
                                                arguments: String::new(),
                                            },
                                        });
                                    }

                                    let tool_call = &mut accumulated_tool_calls[delta_call.index];

                                    // Merge the delta into the accumulated tool call
                                    if let Some(id) = &delta_call.id {
                                        tool_call.id = id.clone();
                                    }
                                    if let Some(tool_type) = &delta_call.tool_type {
                                        tool_call.tool_type = tool_type.clone();
                                    }
                                    if let Some(function_delta) = &delta_call.function {
                                        if let Some(name) = &function_delta.name {
                                            tool_call.function.name = name.clone();
                                        }
                                        if let Some(args) = &function_delta.arguments {
                                            tool_call.function.arguments.push_str(args);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => return Err(anyhow::anyhow!("Error reading stream: {}", e)),
        }
    }

    // Clear thinking indicator if it was never cleared (no content received)
    if first_chunk {
        print!("\r\x1B[K");
        io::stdout().flush().unwrap();
    }

    println!(); // New line after streaming complete

    // Build the final message
    let mut message = Message {
        role: if role.is_empty() { "assistant".to_string() } else { role },
        content: accumulated_content.clone(),
        tool_calls: if accumulated_tool_calls.is_empty() { None } else { Some(accumulated_tool_calls) },
        tool_call_id: None,
        name: None,
        reasoning: None,
    };

    // If no structured tool calls were received, check for XML format in content
    if message.tool_calls.is_none() {
        if let Some(parsed_calls) = parse_xml_tool_calls(&accumulated_content) {
            eprintln!("{} Detected XML-format tool calls, parsing {} call(s)", "🔧".bright_yellow(), parsed_calls.len());
            message.tool_calls = Some(parsed_calls);
            // Clear the XML from content to avoid displaying it
            message.content = String::new();
        }
    }

    Ok((message, usage, current_model))
}

/// Streaming API call using the new LlmClient system (for Anthropic and llama.cpp)
pub(crate) async fn call_api_streaming_with_llm_client(
    chat: &KimiChat,
    messages: &[Message],
    model: &ModelType,
) -> Result<(Message, Option<Usage>, ModelType)> {
    if chat.should_show_debug(1) {
        println!("🔧 DEBUG: call_api_streaming_with_llm_client called with model: {:?}", model);
    }

    // Convert old Message format to new ChatMessage format
    let chat_messages: Vec<ChatMessage> = messages.iter().map(|msg| {
        ChatMessage {
            role: msg.role.clone(),
            content: msg.content.clone(),
            tool_calls: msg.tool_calls.clone().map(|calls| {
                calls.into_iter().map(|call| crate::agents::agent::ToolCall {
                    id: call.id,
                    function: crate::agents::agent::FunctionCall {
                        name: call.function.name,
                        arguments: call.function.arguments,
                    },
                }).collect()
            }),
            tool_call_id: msg.tool_call_id.clone(),
            name: msg.name.clone(),
            reasoning: None,
        }
    }).collect();

    // Convert tools to the new format
    let tools: Vec<ToolDefinition> = chat.get_tools().into_iter().map(|tool| {
        ToolDefinition {
            name: tool.function.name,
            description: tool.function.description,
            parameters: tool.function.parameters,
        }
    }).collect();

    // Create the appropriate LlmClient using the same logic as call_api_with_llm_client
    let llm_client: std::sync::Arc<dyn crate::agents::agent::LlmClient> =
        if matches!(model, ModelType::BluModel) {
            // Blu model logic (same as agent mode)
            if let Some(ref api_url) = chat.client_config.api_url_blu_model {
                if api_url.contains("anthropic") {
                    println!("{} Using Anthropic streaming API for 'blu_model' at: {}", "🧠".cyan(), api_url);
                    std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                        chat.client_config.api_key_blu_model.clone().unwrap_or_default(),
                        model.as_str(),
                        api_url.clone(),
                        "blu_model".to_string()
                    ))
                } else {
                    println!("{} Using llama.cpp for 'blu_model' at: {}", "🦙".cyan(), api_url);
                    std::sync::Arc::new(crate::agents::llama_cpp_client::LlamaCppClient::new(
                        api_url.clone(),
                        model.as_str()
                    ))
                }
            } else if env::var("ANTHROPIC_AUTH_TOKEN_BLU").is_ok() ||
                      (env::var("ANTHROPIC_AUTH_TOKEN").is_ok() &&
                       (chat.client_config.model_blu_model_override.as_ref()
                        .map(|m| m.contains("claude") || m.contains("anthropic"))
                        .unwrap_or(false))) {
                println!("{} Using Anthropic streaming API for 'blu_model'", "🧠".cyan());
                let anthropic_key = env::var("ANTHROPIC_AUTH_TOKEN_BLU")
                    .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
                    .unwrap_or_default();
                std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                    anthropic_key,
                    model.as_str(),
                    "https://api.anthropic.com".to_string(),
                    "blu_model".to_string()
                ))
            } else {
                println!("{} Using Groq API for 'blu_model'", "🚀".cyan());
                std::sync::Arc::new(crate::agents::groq_client::GroqLlmClient::new(
                    chat.client_config.api_key.clone(),
                    model.as_str(),
                    crate::GROQ_API_URL.to_string(),
                    "blu_model".to_string()
                ))
            }
        } else if matches!(model, ModelType::AnthropicModel) {
            // Anthropic model logic - use raw URL
            let api_url = env::var("ANTHROPIC_BASE_URL")
                .or_else(|_| env::var("ANTHROPIC_BASE_URL_BLU"))
                .or_else(|_| env::var("ANTHROPIC_BASE_URL_GRN"))
                .or_else(|_| env::var("ANTHROPIC_BASE_URL_RED"))
                .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
            println!("{} Using Anthropic streaming API for 'anthropic' at: {}", "🧠".cyan(), api_url);
            let api_key = env::var("ANTHROPIC_API_KEY")
                .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
                .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN_BLU"))
                .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN_GRN"))
                .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN_RED"))
                .unwrap_or_else(|_| chat.client_config.api_key.clone());
            std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                api_key,
                model.as_str(),
                api_url,
                "anthropic".to_string()
            ))
        } else if matches!(model, ModelType::RedModel) {
            // Red model logic (same as agent mode)
            if let Some(ref api_url) = chat.client_config.api_url_red_model {
                if api_url.contains("anthropic") {
                    println!("{} Using Anthropic streaming API for 'red_model' at: {}", "🧠".cyan(), api_url);
                    std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                        chat.client_config.api_key_red_model.clone().unwrap_or_default(),
                        model.as_str(),
                        api_url.clone(),
                        "red_model".to_string()
                    ))
                } else {
                    println!("{} Using llama.cpp for 'red_model' at: {}", "🦙".cyan(), api_url);
                    std::sync::Arc::new(crate::agents::llama_cpp_client::LlamaCppClient::new(
                        api_url.clone(),
                        model.as_str()
                    ))
                }
            } else if env::var("ANTHROPIC_AUTH_TOKEN_RED").is_ok() ||
                      (env::var("ANTHROPIC_AUTH_TOKEN").is_ok() &&
                       (chat.client_config.model_red_model_override.as_ref()
                        .map(|m| m.contains("claude") || m.contains("anthropic"))
                        .unwrap_or(false))) {
                println!("{} Using Anthropic streaming API for 'red_model'", "🧠".cyan());
                let anthropic_key = env::var("ANTHROPIC_AUTH_TOKEN_RED")
                    .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
                    .unwrap_or_default();
                std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                    anthropic_key,
                    model.as_str(),
                    "https://api.anthropic.com".to_string(),
                    "red_model".to_string()
                ))
            } else {
                println!("{} Using Groq API for 'red_model'", "🚀".cyan());
                std::sync::Arc::new(crate::agents::groq_client::GroqLlmClient::new(
                    chat.client_config.api_key.clone(),
                    model.as_str(),
                    crate::GROQ_API_URL.to_string(),
                    "red_model".to_string()
                ))
            }
        } else {
            // Grn model logic (same as agent mode)
            if let Some(ref api_url) = chat.client_config.api_url_grn_model {
                if api_url.contains("anthropic") {
                    println!("{} Using Anthropic streaming API for 'grn_model' at: {}", "🧠".cyan(), api_url);
                    std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                        chat.client_config.api_key_grn_model.clone().unwrap_or_default(),
                        model.as_str(),
                        api_url.clone(),
                        "grn_model".to_string()
                    ))
                } else {
                    println!("{} Using llama.cpp for 'grn_model' at: {}", "🦙".cyan(), api_url);
                    std::sync::Arc::new(crate::agents::llama_cpp_client::LlamaCppClient::new(
                        api_url.clone(),
                        model.as_str()
                    ))
                }
            } else if env::var("ANTHROPIC_AUTH_TOKEN_GRN").is_ok() ||
                      (env::var("ANTHROPIC_AUTH_TOKEN").is_ok() &&
                       (chat.client_config.model_grn_model_override.as_ref()
                        .map(|m| m.contains("claude") || m.contains("anthropic"))
                        .unwrap_or(false))) {
                println!("{} Using Anthropic streaming API for 'grn_model'", "🧠".cyan());
                let anthropic_key = env::var("ANTHROPIC_AUTH_TOKEN_GRN")
                    .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
                    .unwrap_or_default();
                std::sync::Arc::new(crate::agents::anthropic_client::AnthropicLlmClient::new(
                    anthropic_key,
                    model.as_str(),
                    "https://api.anthropic.com".to_string(),
                    "grn_model".to_string()
                ))
            } else {
                println!("{} Using Groq API for 'grn_model'", "🚀".cyan());
                std::sync::Arc::new(crate::agents::groq_client::GroqLlmClient::new(
                    chat.client_config.api_key.clone(),
                    model.as_str(),
                    crate::GROQ_API_URL.to_string(),
                    "grn_model".to_string()
                ))
            }
        };

    println!("\n{}", "📡 Starting Anthropic streaming response...".bright_cyan());

    // Initialize response accumulation
    let mut accumulated_content = String::new();

    // Get the streaming response
    let mut stream = llm_client.chat_streaming(chat_messages.clone(), tools.clone()).await?;

    // Process the stream with minimal buffering
    use futures::StreamExt;
    use std::io::{self, Write};

    // Ensure stdout is flushed immediately for each chunk
    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                // Print the delta immediately without any buffering
                if !chunk.delta.is_empty() {
                    // Use direct write and flush for minimal latency
                    io::stdout().write_all(chunk.delta.as_bytes()).unwrap();
                    io::stdout().flush().unwrap();
                    accumulated_content.push_str(&chunk.delta);
                }

                // Check if we're done
                if let Some(ref reason) = chunk.finish_reason {
                    if reason == "stop" {
                        break;
                    }
                }
            }
            Err(e) => {
                eprintln!("{} Streaming error: {}", "❌".red(), e);
                break;
            }
        }
    }

    println!(); // New line after streaming complete

    // For now, we'll make a non-streaming call to get the complete response with tool calls
    // This is a limitation of the current format translation approach
    let response = llm_client.chat(chat_messages, tools).await?;

    // Convert the response back to the old format
    let message = Message {
        role: response.message.role,
        content: response.message.content,
        tool_calls: response.message.tool_calls.map(|calls| {
            calls.into_iter().map(|call| crate::ToolCall {
                id: call.id,
                tool_type: "function".to_string(),
                function: crate::FunctionCall {
                    name: call.function.name,
                    arguments: call.function.arguments,
                },
            }).collect()
        }),
        tool_call_id: response.message.tool_call_id,
        name: response.message.name,
        reasoning: None,
    };

    let usage = response.usage.map(|u| Usage {
        prompt_tokens: u.prompt_tokens as usize,
        completion_tokens: u.completion_tokens as usize,
        total_tokens: u.total_tokens as usize,
    });

    Ok((message, usage, model.clone()))
}
