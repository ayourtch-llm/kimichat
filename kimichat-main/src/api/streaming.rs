use anyhow::Result;
use colored::Colorize;
use std::io::Write;

use crate::KimiChat;
use kimichat_models::{ModelType, Message, Usage, ChatRequest, StreamChunk};
use kimichat_agents::{ToolDefinition, ChatMessage};
use kimichat_logging::{log_request, log_request_to_file, log_response, log_response_to_file, log_raw_response_to_file, log_stream_chunk};
use kimichat_toolcore::parse_xml_tool_calls;
use crate::{ToolCall, FunctionCall};

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
        model: current_model.as_str(
            chat.client_config.model_blu_model_override.as_deref(),
            chat.client_config.model_grn_model_override.as_deref(),
            chat.client_config.model_red_model_override.as_deref()
        ).to_string(),
        messages,
        tools: chat.get_tools(),
        tool_choice: "auto".to_string(),
        stream: Some(true),
    };

    // Get the appropriate API URL based on the current model
    let api_url = crate::config::get_api_url(&chat.client_config, &current_model);

    // Capture request timestamp for response logging correlation
    let request_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

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
        let _ = log_response_to_file(&status, &headers, &error_body, request_timestamp, &current_model);
        let _ = log_raw_response_to_file(&error_body, request_timestamp, &current_model);

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
    let mut raw_response_body = String::new(); // Capture raw response body

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
                let chunk_str = String::from_utf8_lossy(&bytes);
                raw_response_body.push_str(&chunk_str); // Capture raw response
                buffer.push_str(&chunk_str);

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

    // Create a response body representation for logging with raw content
    let response_body_for_logging = if accumulated_tool_calls.is_empty() {
        // Simple text response - log raw accumulated content
        if accumulated_reasoning.is_empty() {
            accumulated_content.clone()
        } else {
            // Include reasoning in the logged response
            format!("<reasoning>\n{}\n</reasoning>\n\n{}", accumulated_reasoning, accumulated_content)
        }
    } else {
        // Response with tool calls - create raw format similar to API response
        let mut result = String::new();
        
        // Add reasoning if present
        if !accumulated_reasoning.is_empty() {
            result.push_str(&format!("<reasoning>\n{}\n</reasoning>\n\n", accumulated_reasoning));
        }
        
        // Add content if present
        if !accumulated_content.is_empty() {
            result.push_str(&accumulated_content);
        }
        
        // Add tool calls representation
        result.push_str("\n\n<tool_calls>\n");
        for (i, tool_call) in accumulated_tool_calls.iter().enumerate() {
            result.push_str(&format!("Tool Call {}:\n", i + 1));
            result.push_str(&format!("  ID: {}\n", tool_call.id));
            result.push_str(&format!("  Function: {}\n", tool_call.function.name));
            result.push_str(&format!("  Arguments: {}\n", tool_call.function.arguments));
        }
        result.push_str("</tool_calls>");
        
        result
    };

    // Log successful streaming response to file
    let _ = log_response_to_file(&status, &headers, &response_body_for_logging, request_timestamp, &current_model);
    
    // Also log the raw response body without any transformation
    let _ = log_raw_response_to_file(&raw_response_body, request_timestamp, &current_model);

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
                calls.into_iter().map(|call| kimichat_agents::ToolCall {
                    id: call.id,
                    function: kimichat_agents::FunctionCall {
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

    // Create the appropriate LlmClient using the centralized helper
    let llm_client = crate::config::create_client_for_model_type(
        model,
        &chat.client_config,
        &chat.api_key,
    );

    // Get the appropriate API URL based on the current model
    let _api_url = crate::config::get_api_url(&chat.client_config, model);

    // Capture request timestamp for response logging correlation
    let request_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

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

    // Log the final response for LlmClient streaming
    let response_body = format!("Role: {}\n\nContent:\n{}\n\nTool calls: {}\n\nUsage: {:?}",
        message.role,
        message.content,
        message.tool_calls.as_ref().map_or("None".to_string(), |calls| {
            format!("{} calls", calls.len())
        }),
        usage
    );
    
    // Create mock status and headers for logging
    let status = reqwest::StatusCode::OK;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    headers.insert("x-streaming", "anthropic".parse().unwrap());
    
    let _ = log_response_to_file(&status, &headers, &response_body, request_timestamp, model);

    Ok((message, usage, model.clone()))
}
