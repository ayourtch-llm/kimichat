use anyhow::Result;
use colored::Colorize;

use crate::KimiChat;
use crate::models::{ModelType, Message, ChatRequest, ChatResponse};
use crate::logging::log_request_to_file;

/// Summarize and trim conversation history when it gets too long
/// Uses another model to summarize the middle portion of the conversation
pub(crate) async fn summarize_and_trim_history(chat: &mut KimiChat) -> Result<()> {
    const MAX_CONVERSATION_SIZE_BYTES: usize = 200_000; // 200KB
    const KEEP_RECENT_MESSAGES: usize = 5;

    // Calculate conversation size by serializing to JSON
    let conversation_size = match serde_json::to_string(&chat.messages) {
        Ok(json) => json.len(),
        Err(_) => return Ok(()), // If serialization fails, skip summarization
    };

    // Only summarize if conversation exceeds size limit
    if conversation_size <= MAX_CONVERSATION_SIZE_BYTES {
        return Ok(());
    }

    // Use the "other" model for summarization
    let summary_model = match chat.current_model {
        ModelType::BluModel => ModelType::GrnModel,
        ModelType::GrnModel => ModelType::BluModel,
        ModelType::RedModel => ModelType::BluModel, // Use BluModel for summarization when using RedModel
        ModelType::AnthropicModel => ModelType::GrnModel, // Prefer GrnModel for summarization when using Anthropic
        ModelType::Custom(_) => ModelType::BluModel, // Default to BluModel for custom models
    };

    println!(
        "{} History getting large ({:.1} KB, {} messages). Asking {} to summarize...",
        "📝".yellow(),
        conversation_size as f64 / 1024.0,
        chat.messages.len(),
        summary_model.display_name()
    );

    // Keep system message and recent messages
    let system_message = chat.messages.first().cloned();
    let recent_messages: Vec<Message> = chat.messages
        .iter()
        .rev()
        .take(KEEP_RECENT_MESSAGES)
        .rev()
        .cloned()
        .collect();

    // Get messages to summarize (everything except system and recent)
    let to_summarize: Vec<Message> = chat.messages
        .iter()
        .skip(1) // Skip system
        .take(chat.messages.len() - KEEP_RECENT_MESSAGES - 1)
        .cloned()
        .collect();

    // Build summary request
    let mut summary_history = vec![Message {
        role: "system".to_string(),
        content: format!(
            "You are {}. You are being asked to summarize a conversation that was handled by {}. \
            After summarizing, you may recommend switching to yourself if you believe you would be \
            better suited for the ongoing work based on the context.",
            summary_model.display_name(),
            chat.current_model.display_name()
        ),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    }];

    // Format the conversation to summarize
    let conversation_text = to_summarize.iter()
        .map(|m| {
            let role = &m.role;
            let content = if m.content.len() > 500 {
                // Use char-boundary-safe truncation to avoid panic with multibyte chars (emojis, etc.)
                let truncated: String = m.content.chars().take(500).collect();
                format!("{}... [truncated]", truncated)
            } else {
                m.content.clone()
            };
            format!("{}: {}", role, content)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    summary_history.push(Message {
        role: "user".to_string(),
        content: format!(
            "Summarize this conversation history in 2-3 concise sentences, focusing on key context, decisions, and file changes:\n\n{}\n\n\
            Then, based on the recent context and what seems to be the ongoing work, add a separate line starting with 'RECOMMENDATION: ' \
            followed by either 'STAY' (keep current model) or 'SWITCH' (switch to you) and briefly explain why in one sentence.",
            conversation_text
        ),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    });

    // Call API to get summary using the OTHER model
    let request = ChatRequest {
        model: summary_model.as_str().to_string(),
        messages: summary_history,
        tools: vec![],
        tool_choice: "none".to_string(),
        stream: None,
    };

    // Get the appropriate API URL for the summary model
    let api_url = crate::config::get_api_url(&chat.client_config, &summary_model);

    // Log request to file for persistent debugging
    let _ = log_request_to_file(&api_url, &request, &summary_model, &chat.api_key);

    let api_key = crate::config::get_api_key(&chat.client_config, &chat.api_key, &summary_model);
    let response = chat.client
        .post(&api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        // If summarization fails, just trim without summarizing
        println!("{} Summarization failed, doing simple trim", "⚠️".yellow());
        chat.messages = vec![system_message.unwrap()];
        chat.messages.extend(recent_messages);
        return Ok(());
    }

    let response_text = response.text().await?;
    let chat_response: ChatResponse = serde_json::from_str(&response_text)?;

    if let Some(summary_msg) = chat_response.choices.into_iter().next().map(|c| c.message) {
        let full_response = summary_msg.content;

        // Parse recommendation
        let (summary, recommendation_text) = if let Some(rec_pos) = full_response.find("RECOMMENDATION:") {
            let summary = full_response[..rec_pos].trim().to_string();
            let recommendation = full_response[rec_pos..].trim().to_string();

            println!("{} {}", "💡".bright_cyan(), recommendation);
            (summary, Some(recommendation))
        } else {
            (full_response, None)
        };

        // Rebuild history with summary
        let mut new_history = vec![];

        if let Some(sys_msg) = system_message {
            new_history.push(sys_msg);
        }

        // Add summary as a system-level context message
        new_history.push(Message {
            role: "system".to_string(),
            content: format!("Previous conversation summary: {}", summary),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });

        // Add recent messages
        new_history.extend(recent_messages);

        chat.messages = new_history;

        // Calculate new size
        let new_size = serde_json::to_string(&chat.messages)
            .map(|json| json.len())
            .unwrap_or(0);

        println!(
            "{} History summarized and trimmed to {} messages ({:.1} KB)",
            "✅".green(),
            chat.messages.len(),
            new_size as f64 / 1024.0
        );

        // If there's a SWITCH recommendation, ask the current model to decide
        if let Some(rec_text) = recommendation_text {
            if rec_text.contains("SWITCH") {
                println!(
                    "{} {} suggests switching. Asking {} to decide...",
                    "🤔".yellow(),
                    summary_model.display_name(),
                    chat.current_model.display_name()
                );

                // Ask current model to decide
                let decision_prompt = vec![
                    Message {
                        role: "system".to_string(),
                        content: format!(
                            "You are {}. You have been handling this conversation.",
                            chat.current_model.display_name()
                        ),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    },
                    Message {
                        role: "user".to_string(),
                        content: format!(
                            "{} has reviewed the conversation history and made the following recommendation:\n\n{}\n\n\
                            Based on this recommendation and your understanding of the current context, do you agree to switch to {}? \
                            Respond with only 'AGREE' or 'DECLINE' followed by a brief one-sentence explanation.",
                            summary_model.display_name(),
                            rec_text,
                            summary_model.display_name()
                        ),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    },
                ];

                let decision_request = ChatRequest {
                    model: chat.current_model.as_str().to_string(),
                    messages: decision_prompt,
                    tools: vec![],
                    tool_choice: "none".to_string(),
                    stream: None,
                };

                // Get the appropriate API URL for the current model
                let decision_api_url = crate::config::get_api_url(&chat.client_config, &chat.current_model);

                // Log request to file for persistent debugging
                let _ = log_request_to_file(&decision_api_url, &decision_request, &chat.current_model, &chat.api_key);

                let api_key = crate::config::get_api_key(&chat.client_config, &chat.api_key, &chat.current_model);
                let decision_response = chat.client
                    .post(&decision_api_url)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("Content-Type", "application/json")
                    .json(&decision_request)
                    .send()
                    .await?;

                if decision_response.status().is_success() {
                    let decision_text = decision_response.text().await?;
                    if let Ok(decision_chat) = serde_json::from_str::<ChatResponse>(&decision_text) {
                        if let Some(decision_msg) = decision_chat.choices.into_iter().next().map(|c| c.message) {
                            let decision = decision_msg.content;
                            println!("{} {} says: {}", "💬".bright_green(), chat.current_model.display_name(), decision);

                            if decision.to_uppercase().contains("AGREE") {
                                println!(
                                    "{} Switching to {} by mutual agreement",
                                    "🔄".bright_cyan(),
                                    summary_model.display_name()
                                );
                                chat.current_model = summary_model.clone();

                                // Add message to conversation history about model switch
                                chat.messages.push(Message {
                                    role: "system".to_string(),
                                    content: format!("Model switched to: {}", summary_model.display_name()),
                                    tool_calls: None,
                                    tool_call_id: None,
                                    name: None,
                                });
                            } else {
                                println!(
                                    "{} Staying with {}",
                                    "✋".yellow(),
                                    chat.current_model.display_name()
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
