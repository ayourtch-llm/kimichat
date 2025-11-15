use anyhow::{Context, Result};
use colored::Colorize;
use std::env;
use std::time::Duration;
use tokio::time::sleep;

use crate::KimiChat;
use crate::models::{ModelType, Message, Usage, ChatRequest, ChatResponse};
use crate::agents::agent::ToolDefinition;
use crate::logging::{log_request, log_request_to_file, log_response};
use crate::chat::history::safe_truncate;
use crate::tools_execution::parse_xml_tool_calls;
use crate::MAX_RETRIES;
use crate::agents::agent::ChatMessage;

/// Non-streaming API call for Groq-style APIs
pub(crate) async fn call_api(
    chat: &KimiChat,
    orig_messages: &[Message],
) -> Result<(Message, Option<Usage>, ModelType)> {
    let mut current_model = chat.current_model.clone();
    // Clone messages and strip reasoning field (only supported by some models like Groq)
    let mut messages: Vec<Message> = orig_messages.iter().map(|m| {
        let mut msg = m.clone();
        msg.reasoning = None; // Strip reasoning field to avoid compatibility issues
        msg
    }).collect();

    // Check if we need to use the new LlmClient system for Anthropic
    let is_custom_claude = if let ModelType::Custom(ref name) = current_model {
        name.contains("claude")
    } else {
        false
    };

    let should_use_anthropic = matches!(current_model, ModelType::AnthropicModel) ||
        is_custom_claude ||
        (chat.client_config.api_url_blu_model.as_ref().map(|u| u.contains("anthropic")).unwrap_or(false)) ||
        (chat.client_config.api_url_grn_model.as_ref().map(|u| u.contains("anthropic")).unwrap_or(false)) ||
        (chat.client_config.api_url_red_model.as_ref().map(|u| u.contains("anthropic")).unwrap_or(false));

    if chat.should_show_debug(1) {
        println!("🔧 DEBUG: current_model = {:?}", current_model);
        println!("🔧 DEBUG: should_use_anthropic = {}", should_use_anthropic);
    }
    if should_use_anthropic {
        if chat.should_show_debug(1) {
            println!("🔧 DEBUG: Using call_api_with_llm_client for Anthropic");
        }
        return call_api_with_llm_client(chat, &messages, &current_model).await;
    } else {
        if chat.should_show_debug(1) {
            println!("🔧 DEBUG: Using regular OpenAI-style call_api");
        }
    }

    // Retry logic with exponential backoff
    let mut retry_count = 0;
    loop {
        let request = ChatRequest {
            model: current_model.as_str().to_string(),
            messages: messages.clone(),
            tools: chat.get_tools(),
            tool_choice: "auto".to_string(),
            stream: None,
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

        // Handle rate limiting with exponential backoff
        if status == 429 {
            if retry_count >= MAX_RETRIES {
                anyhow::bail!("Rate limit exceeded after {} retries", MAX_RETRIES);
            }

            let wait_time = Duration::from_secs(2u64.pow(retry_count));
            println!(
                "{} Rate limited. Waiting {} seconds before retry {}/{}...",
                "⏳".yellow(),
                wait_time.as_secs(),
                retry_count + 1,
                MAX_RETRIES
            );
            sleep(wait_time).await;
            retry_count += 1;
            continue;
        }

        // Check for errors and provide detailed debugging
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_else(|_| "Unable to read error body".to_string());

            // Log error response in verbose mode
            log_response(&status, &headers, &error_body, chat.verbose);

            // Check if this is a tool-related error
            if status == 400 && error_body.contains("tool_use_failed") {
                eprintln!("{}", "❌ Tool calling error detected!".red().bold());
                eprintln!("{}", error_body.yellow());
                // No automatic model switching - let the error propagate
            }

            eprintln!("{}", "=== API Error Details ===".red());
            eprintln!("Status: {}", status);
            eprintln!("Error body: {}", error_body);

            // Try to show the request that caused the error
            eprintln!("\n{}", "Request details:".yellow());
            eprintln!("Messages count: {}", messages.len());
            if let Ok(req_json) = serde_json::to_string_pretty(&request) {
                // Truncate very long requests
                if req_json.chars().count() > 2000 {
                    eprintln!("Request (truncated): {}...", safe_truncate(&req_json, 2000));
                } else {
                    eprintln!("Request: {}", req_json);
                }
            }
            eprintln!("{}", "======================".red());

            return Err(anyhow::anyhow!("API request failed with status {}: {}", status, error_body));
        }

        let response_text = response.text().await?;

        // Log successful response in verbose mode
        log_response(&status, &headers, &response_text, chat.verbose);

        let chat_response: ChatResponse = serde_json::from_str(&response_text)
            .with_context(|| format!("Failed to parse API response: {}", response_text))?;

        let mut message = chat_response
            .choices
            .into_iter()
            .next()
            .map(|c| c.message)
            .context("No response from API")?;

        // If no structured tool calls were received, check for XML format in content
        if message.tool_calls.is_none() {
            if let Some(parsed_calls) = parse_xml_tool_calls(&message.content) {
                eprintln!("{} Detected XML-format tool calls, parsing {} call(s)", "🔧".bright_yellow(), parsed_calls.len());
                message.tool_calls = Some(parsed_calls);
                // Clear the XML from content to avoid displaying it
                message.content = String::new();
            }
        }

        return Ok((message, chat_response.usage, current_model));
    }
}

/// Call API using the new LlmClient system (for Anthropic and llama.cpp backends)
pub(crate) async fn call_api_with_llm_client(
    chat: &KimiChat,
    messages: &[Message],
    model: &ModelType,
) -> Result<(Message, Option<Usage>, ModelType)> {
    if chat.should_show_debug(1) {
        println!("🔧 DEBUG: call_api_with_llm_client called with model: {:?}", model);
    }
    if chat.should_show_debug(2) {
        println!("🔧 DEBUG: client_config.api_url_blu_model: {:?}", chat.client_config.api_url_blu_model);
        println!("🔧 DEBUG: client_config.api_url_grn_model: {:?}", chat.client_config.api_url_grn_model);
        println!("🔧 DEBUG: client_config.api_url_red_model: {:?}", chat.client_config.api_url_red_model);
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

    // Create the appropriate LlmClient using the same logic as agent mode
    let llm_client: std::sync::Arc<dyn crate::agents::agent::LlmClient> =
        if matches!(model, ModelType::BluModel) {
            // Blu model logic (same as agent mode)
            if let Some(ref api_url) = chat.client_config.api_url_blu_model {
                if api_url.contains("anthropic") {
                    println!("{} Using Anthropic API for 'blu_model' at: {}", "🧠".cyan(), api_url);
                    if chat.should_show_debug(2) {
                        println!("🔧 DEBUG: BluModel Anthropic URL: '{}', API Key present: {}", api_url, chat.client_config.api_key_blu_model.is_some());
                    }
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
                println!("{} Using Anthropic API for 'blu_model'", "🧠".cyan());
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
            println!("{} Using Anthropic API for 'anthropic' at: {}", "🧠".cyan(), api_url);
            if chat.should_show_debug(2) {
                println!("🔧 DEBUG: AnthropicModel URL: '{}'", api_url);
            }
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
                    println!("{} Using Anthropic API for 'red_model' at: {}", "🧠".cyan(), api_url);
                    if chat.should_show_debug(2) {
                        println!("🔧 DEBUG: RedModel Anthropic URL: '{}', API Key present: {}", api_url, chat.client_config.api_key_red_model.is_some());
                    }
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
                println!("{} Using Anthropic API for 'red_model'", "🧠".cyan());
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
                    println!("{} Using Anthropic API for 'grn_model' at: {}", "🧠".cyan(), api_url);
                    if chat.should_show_debug(2) {
                        println!("🔧 DEBUG: GrnModel Anthropic URL: '{}', API Key present: {}", api_url, chat.client_config.api_key_grn_model.is_some());
                    }
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
                println!("{} Using Anthropic API for 'grn_model'", "🧠".cyan());
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

    // Make the API call
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
