use anyhow::Result;
use colored::Colorize;

use crate::KimiChat;
use kimichat_models::{ModelType, Message};
use kimichat_logging::safe_truncate;

/// Main chat loop - handles user messages, tool calls, and model interactions
pub(crate) async fn chat(
    chat: &mut KimiChat,
    user_message: &str,
    cancellation_token: Option<tokio_util::sync::CancellationToken>,
) -> Result<String> {
        chat.messages.push(Message {
            role: "user".to_string(),
            content: user_message.to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning: None,
        });

        // Summarize ONCE before starting the tool-calling loop, not during it
        // This prevents discarding recent tool results mid-conversation
        crate::chat::history::summarize_and_trim_history(chat).await?;

        let mut tool_call_iterations = 0;
        let mut recent_tool_calls: Vec<(String, String)> = Vec::new(); // Track recent tool calls with results
        const MAX_TOOL_ITERATIONS: usize = 250; // Increased limit with intelligent evaluation
        const LOOP_DETECTION_WINDOW: usize = 8; // Check last 8 tool calls
        const PROGRESS_EVAL_INTERVAL: u32 = 50; // Evaluate progress every 50 tool calls
        const CONSECUTIVE_REPEAT_THRESHOLD: usize = 4; // Warn if same call 4+ times in a row
        const SCATTERED_REPEAT_THRESHOLD: usize = 6; // Warn if same call 6+ times in window

        // Initialize progress evaluator for all operations
        let blu_model_url = crate::config::get_api_url(&chat.client_config, &ModelType::BluModel);
        let blu_model_key = crate::config::get_api_key(&chat.client_config, &chat.api_key, &ModelType::BluModel);
        let mut progress_evaluator = Some(kimichat_agents::progress_evaluator::ProgressEvaluator::new(
            std::sync::Arc::new(kimichat_agents::GroqLlmClient::new(
                blu_model_key,
                "kimi".to_string(),
                blu_model_url,
                "progress_evaluator".to_string()
            )),
            0.6, // Minimum confidence threshold
            PROGRESS_EVAL_INTERVAL,
        ));

        // Track tool calls for progress evaluation
        let mut tool_call_history: Vec<kimichat_agents::progress_evaluator::ToolCallInfo> = Vec::new();
        let mut files_changed: std::collections::HashSet<String> = std::collections::HashSet::new();
        let start_time = std::time::Instant::now();
        let mut errors_encountered: Vec<String> = Vec::new();

        loop {
            // Check for cancellation at the start of each iteration
            if let Some(ref token) = cancellation_token {
                if token.is_cancelled() {
                    return Err(anyhow::anyhow!("Chat interrupted by user"));
                }
            }

            // Validate and fix tool calls in the conversation history before sending to API
            // This ensures fixes are permanent and consistent across requests (preserving cache)
            if let Ok(fixed) = crate::tools_execution::validation::validate_and_fix_tool_calls_in_place(chat) {
                if fixed {
                    eprintln!("{} Tool calls were automatically fixed in conversation history", "✅".green());
                }
            }

            // Race API call against cancellation token
            let (response, usage, current_model) = if let Some(ref token) = cancellation_token {
                tokio::select! {
                    result = async {
                        if chat.stream_responses {
                            // Check if this is an Anthropic model that should use the new system
                            let is_custom_claude = if let ModelType::Custom(ref name) = chat.current_model {
                                name.contains("claude")
                            } else {
                                false
                            };

                            let should_use_anthropic = matches!(chat.current_model, ModelType::AnthropicModel) ||
                                is_custom_claude ||
                                (chat.client_config.api_url_blu_model.as_ref().map(|u| u.contains("anthropic")).unwrap_or(false)) ||
                                (chat.client_config.api_url_grn_model.as_ref().map(|u| u.contains("anthropic")).unwrap_or(false));

                            if should_use_anthropic {
                                // Use the new streaming implementation for Anthropic
                                if chat.should_show_debug(1) {
                                    println!("🔧 DEBUG: Using Anthropic streaming with format translation");
                                }
                                crate::api::call_api_streaming_with_llm_client(chat, &chat.messages, &chat.current_model).await
                            } else {
                                // Use old streaming for OpenAI-compatible APIs
                                crate::api::call_api_streaming(chat, &chat.messages).await
                            }
                        } else {
                            crate::api::call_api(chat, &chat.messages).await
                        }
                    } => result?,
                    _ = token.cancelled() => {
                        return Err(anyhow::anyhow!("LLM call interrupted by user"));
                    }
                }
            } else {
                // No cancellation token, call normally
                if chat.stream_responses {
                    // Check if this is an Anthropic model that should use the new system
                    let is_custom_claude = if let ModelType::Custom(ref name) = chat.current_model {
                        name.contains("claude")
                    } else {
                        false
                    };

                    let should_use_anthropic = matches!(chat.current_model, ModelType::AnthropicModel) ||
                        is_custom_claude ||
                        (chat.client_config.api_url_blu_model.as_ref().map(|u| u.contains("anthropic")).unwrap_or(false)) ||
                        (chat.client_config.api_url_grn_model.as_ref().map(|u| u.contains("anthropic")).unwrap_or(false));

                    if should_use_anthropic {
                        // Use the new streaming implementation for Anthropic
                        if chat.should_show_debug(1) {
                            println!("🔧 DEBUG: Using Anthropic streaming with format translation");
                        }
                        crate::api::call_api_streaming_with_llm_client(chat, &chat.messages, &chat.current_model).await?
                    } else {
                        // Use old streaming for OpenAI-compatible APIs
                        crate::api::call_api_streaming(chat, &chat.messages).await?
                    }
                } else {
                    crate::api::call_api(chat, &chat.messages).await?
                }
            };

            if chat.current_model != current_model {
                println!("Forced model switch: {:?} -> {:?}", &chat.current_model, &current_model);
                chat.current_model = current_model.clone();

                // Add message to conversation history about model switch
                chat.messages.push(Message {
                    role: "system".to_string(),
                    content: format!("Model switched to: {} (reason: forced by API)", current_model.display_name()),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning: None,
                });
            }

            // Display token usage
            if let Some(usage) = &usage {
                chat.total_tokens_used += usage.total_tokens;
                println!(
                    "{} Prompt: {} | Completion: {} | Total: {} | Session: {}",
                    "📊".bright_black(),
                    usage.prompt_tokens.to_string().bright_black(),
                    usage.completion_tokens.to_string().bright_black(),
                    usage.total_tokens.to_string().bright_black(),
                    chat.total_tokens_used.to_string().cyan()
                );
            }

            if let Some(tool_calls) = &response.tool_calls {
                tool_call_iterations += 1;

                // Progressive session size management - check periodically during tool execution
                const PROGRESSIVE_CHECK_INTERVAL: usize = 25; // Check every 25 tool calls
                const MID_LOOP_SIZE_THRESHOLD: usize = 400_000; // 400KB for mid-loop compaction
                
                if tool_call_iterations % PROGRESSIVE_CHECK_INTERVAL == 0 {
                    let conversation_size = crate::chat::history::calculate_conversation_size(&chat.messages);
                    if conversation_size > MID_LOOP_SIZE_THRESHOLD {
                        println!(
                            "{} Session size reached {:.1} KB during tool execution (iteration {}), performing intelligent compaction...", 
                            "🗜️".yellow(), 
                            conversation_size as f64 / 1024.0,
                            tool_call_iterations
                        );
                        
                        // Perform intelligent compaction that preserves recent tool context
                        if let Err(e) = crate::chat::history::intelligent_compaction(chat, tool_call_iterations).await {
                            eprintln!("{} Intelligent compaction failed: {}", "⚠️".yellow(), e);
                            // Continue without compaction if it fails
                        }
                    }
                }

                // Enhanced loop detection with lower false positive rate
                let tool_signature = tool_calls.iter()
                    .map(|tc| format!("{}:{}", tc.function.name, tc.function.arguments))
                    .collect::<Vec<_>>()
                    .join("|");

                // We'll store the result signature later after execution
                // For now, just track the call signature
                recent_tool_calls.push((tool_signature.clone(), String::new()));

                // Keep only recent tool calls
                if recent_tool_calls.len() > LOOP_DETECTION_WINDOW {
                    recent_tool_calls.remove(0);
                }

                // Check for consecutive identical calls (stronger signal of being stuck)
                let consecutive_count = recent_tool_calls.iter()
                    .rev()
                    .take_while(|(sig, _)| sig == &tool_signature)
                    .count();

                // Check for scattered repetitions in the window
                let total_repetition_count = recent_tool_calls.iter()
                    .filter(|(sig, _)| sig == &tool_signature)
                    .count();

                // Detect if tool is read-only (less likely to be problematic loop)
                let is_read_only = tool_calls.iter().all(|tc|
                    tc.function.name == "open_file" ||
                    tc.function.name == "read_file" ||
                    tc.function.name == "list_files" ||
                    tc.function.name == "search_files" ||
                    tc.function.name == "grep_search"
                );

                // More strict threshold for consecutive repeats
                let is_likely_stuck = if is_read_only {
                    // Read-only tools can repeat more before we worry
                    consecutive_count >= CONSECUTIVE_REPEAT_THRESHOLD + 2 ||
                    total_repetition_count >= SCATTERED_REPEAT_THRESHOLD + 2
                } else {
                    // Write operations are more concerning
                    consecutive_count >= CONSECUTIVE_REPEAT_THRESHOLD ||
                    total_repetition_count >= SCATTERED_REPEAT_THRESHOLD
                };

                if is_likely_stuck {
                    let pattern_type = if consecutive_count >= CONSECUTIVE_REPEAT_THRESHOLD {
                        format!("{} consecutive identical calls", consecutive_count)
                    } else {
                        format!("{} identical calls in last {} operations", total_repetition_count, LOOP_DETECTION_WINDOW)
                    };

                    eprintln!(
                        "{} Detected repeated tool call pattern ({}). Likely stuck in a loop.",
                        "⚠️".red().bold(),
                        pattern_type
                    );
                    chat.messages.push(Message {
                        role: "assistant".to_string(),
                        content: format!(
                            "I apologize, but I'm calling the same tool repeatedly without making progress. \
                            Pattern detected: {}. Please try breaking down your request into smaller, \
                            more specific steps, or provide additional guidance.",
                            pattern_type
                        ),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                        reasoning: None,
                    });
                    return Ok("Repeated tool call pattern detected. Please refine your request.".to_string());
                }

                // Intelligent progress evaluation (replaces hard limit)
                if let Some(ref mut evaluator) = progress_evaluator {
                    // Debug: Show evaluation check
                    if tool_call_iterations % PROGRESS_EVAL_INTERVAL as usize == 0 && tool_call_iterations > 0 {
                        eprintln!("[DEBUG] Checking if evaluation should trigger at iteration {} (interval: {})",
                                 tool_call_iterations, PROGRESS_EVAL_INTERVAL);
                    }

                    if evaluator.should_evaluate(tool_call_iterations as u32) {
                        println!("{}", format!("🧠 Evaluating progress after {} tool calls...", tool_call_iterations).bright_blue());
                        eprintln!("[DEBUG] Progress evaluation triggered at iteration {}", tool_call_iterations);

                        // Create tool call summary
                        let mut tool_usage = std::collections::HashMap::new();
                        for call in &tool_call_history {
                            *tool_usage.entry(call.tool_name.clone()).or_insert(0) += 1;
                        }

                        let summary = kimichat_agents::progress_evaluator::ToolCallSummary {
                            total_calls: tool_call_iterations as u32,
                            tool_usage,
                            recent_calls: tool_call_history.iter().rev().take(10).cloned().collect(),
                            current_task: "Executing user request with tools".to_string(),
                            original_request: user_message.to_string(),
                            elapsed_seconds: start_time.elapsed().as_secs(),
                            errors: errors_encountered.clone(),
                            files_changed: files_changed.iter().cloned().collect(),
                        };

                        match evaluator.evaluate_progress(&summary).await {
                            Ok(evaluation) => {
                                println!("{}", format!("🎯 Progress Evaluation: {:.0}% complete", evaluation.progress_percentage * 100.0).bright_green());
                                println!("{}", format!("📊 Confidence: {:.0}%", evaluation.confidence * 100.0).bright_black());

                                if !evaluation.recommendations.is_empty() {
                                    println!("{}", "💡 Recommendations:".bright_cyan());
                                    for rec in &evaluation.recommendations {
                                        println!("  • {}", rec);
                                    }
                                }

                                if !evaluation.should_continue {
                                    println!("{}", "🛑 Agent evaluation suggests stopping or changing strategy".yellow());
                                    chat.messages.push(Message {
                                        role: "assistant".to_string(),
                                        content: format!(
                                            "Based on progress evaluation: {}\n\nRecommendations:\n{}\n\nReasoning: {}",
                                            if evaluation.change_strategy {
                                                "I should change my approach."
                                            } else {
                                                "I should stop and ask for guidance."
                                            },
                                            evaluation.recommendations.join("\n"),
                                            evaluation.reasoning
                                        ),
                                        tool_calls: None,
                                        tool_call_id: None,
                                        name: None,
                                        reasoning: None,
                                    });
                                    return Ok("Intelligent progress evaluation suggested stopping this approach.".to_string());
                                }

                                if evaluation.change_strategy {
                                    println!("{}", "🔄 Agent evaluation suggests changing strategy".bright_yellow());
                                    chat.messages.push(Message {
                                        role: "system".to_string(),
                                        content: format!(
                                            "Progress evaluation suggests changing approach. Reasoning: {}\nRecommendations:\n{}",
                                            evaluation.reasoning,
                                            evaluation.recommendations.join("\n")
                                        ),
                                        tool_calls: None,
                                        tool_call_id: None,
                                        name: None,
                                        reasoning: None,
                                    });
                                } else {
                                    // should_continue is true and no strategy change needed
                                    println!("{}", "✅ Progress evaluation: continuing execution with current approach".bright_green());
                                }
                            }
                            Err(e) => {
                                eprintln!("{} Progress evaluation failed: {}", "⚠️".yellow(), e);
                                // Continue with conservative fallback
                            }
                        }
                    }
                }

                // Conservative hard limit as final fallback
                if tool_call_iterations > MAX_TOOL_ITERATIONS {
                    eprintln!(
                        "{} Reached maximum tool call limit ({} iterations).",
                        "⚠️".yellow(),
                        MAX_TOOL_ITERATIONS
                    );
                    chat.messages.push(Message {
                        role: "assistant".to_string(),
                        content: format!(
                            "I've made {} tool calls for this request. Despite intelligent progress evaluation, \
                            I've reached the safety limit. Please break this down into smaller tasks or provide more specific direction.",
                            tool_call_iterations
                        ),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                        reasoning: None,
                    });
                    return Ok(format!(
                        "Reached maximum tool call limit ({} iterations). Please simplify your request.",
                        tool_call_iterations
                    ));
                }

                chat.messages.push(response.clone());

                // Log assistant message with tool calls
                if let Some(logger) = &mut chat.logger {
                    let tool_call_info: Vec<(String, String, String)> = tool_calls
                        .iter()
                        .map(|tc| (
                            tc.id.clone(),
                            tc.function.name.clone(),
                            tc.function.arguments.clone()
                        ))
                        .collect();

                    if std::env::var("DEBUG_LOG").is_ok() {
                        eprintln!("[DEBUG] Logging {} tool calls", tool_call_info.len());
                    }

                    let model_name = chat.current_model.as_str();
                    logger.log_with_tool_calls(
                        "assistant",
                        &response.content,
                        Some(&model_name),
                        tool_call_info,
                    ).await;
                }

                for tool_call in tool_calls {
                    println!(
                        "{} {} with args: {} (iteration {}/{})",
                        "🔧 Calling tool:".yellow(),
                        tool_call.function.name.cyan(),
                        tool_call.function.arguments.bright_black(),
                        tool_call_iterations,
                        MAX_TOOL_ITERATIONS
                    );

                    let tool_start_time = std::time::Instant::now();
                    let result = match chat.execute_tool(
                        &tool_call.function.name,
                        &tool_call.function.arguments,
                    ).await {
                        Ok(r) => r,
                        Err(e) => {
                            let error_msg = e.to_string();

                            // Track error for progress evaluation
                            errors_encountered.push(format!("{}: {}", tool_call.function.name, error_msg));
                            // Make cancellation errors very explicit to the model
                            if error_msg.contains("cancelled by user") ||
                               error_msg.contains("Edit cancelled") ||
                               error_msg.contains("Command cancelled") {
                                // Extract user's comment if present
                                let user_feedback = if error_msg.contains(" - ") {
                                    error_msg.split(" - ").skip(1).collect::<Vec<_>>().join(" - ")
                                } else {
                                    String::new()
                                };

                                let feedback_section = if !user_feedback.is_empty() {
                                    format!("\n\nUSER'S FEEDBACK: {}\nThis feedback explains why the operation was cancelled. Address this concern in your next approach.", user_feedback)
                                } else {
                                    String::new()
                                };

                                format!(
                                    "OPERATION CANCELLED BY USER. The user explicitly cancelled this operation. \
                                    DO NOT retry this same approach. Please acknowledge the cancellation and either:\n\
                                    1. Ask the user what they would like to do instead\n\
                                    2. Try a completely different approach that addresses the user's concerns\n\
                                    3. Stop if this was the only viable option\
                                    {}\n\
                                    \nOriginal message: {}",
                                    feedback_section,
                                    error_msg
                                )
                            } else {
                                format!("Error: {}", error_msg)
                            }
                        }
                    };

                    // Display result to user (truncate for file reading tools)
                    let display_result = if tool_call.function.name == "open_file" || tool_call.function.name == "read_file" {
                        let lines: Vec<&str> = result.lines().collect();
                        if lines.len() > 10 {
                            let first_10 = lines[..10].join("\n");
                            let remaining = lines.len() - 10;
                            format!("{}\n\n...and {} more lines", first_10, remaining)
                        } else {
                            result.clone()
                        }
                    } else {
                        result.clone()
                    };

                    println!("{} {}", "📋 Result:".green(), display_result.bright_black());

                    // Log tool result
                    if let Some(logger) = &mut chat.logger {
                        if std::env::var("DEBUG_LOG").is_ok() {
                            eprintln!("[DEBUG] Logging tool result for {}", tool_call.function.name);
                        }
                        logger.log_tool_result(
                            &result,
                            &tool_call.id,
                            &tool_call.function.name,
                        ).await;
                    }

                    // Track tool call for progress evaluation
                    let duration = tool_start_time.elapsed();
                    let result_summary = if result.chars().count() > 200 {
                        format!("{} (truncated)", safe_truncate(&result, 200))
                    } else {
                        result.clone()
                    };

                    // Track files that were changed
                    if tool_call.function.name.contains("write_file") ||
                       tool_call.function.name.contains("edit_file") {
                        if let Ok(args) = serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments) {
                            if let Some(file_path) = args.get("file_path").and_then(|v| v.as_str()) {
                                files_changed.insert(file_path.to_string());
                            }
                        }
                    }

                    let call_info = kimichat_agents::progress_evaluator::ToolCallInfo {
                        tool_name: tool_call.function.name.clone(),
                        parameters: tool_call.function.arguments.clone(),
                        success: !result.contains("failed") && !result.contains("cancelled"),
                        duration_ms: duration.as_millis() as u64,
                        result_summary: Some(result_summary),
                    };
                    tool_call_history.push(call_info);

                    chat.messages.push(Message {
                        role: "tool".to_string(),
                        content: result,
                        tool_calls: None,
                        tool_call_id: Some(tool_call.id.clone()),
                        name: Some(tool_call.function.name.clone()),
                        reasoning: None,
                    });
                }
            } else {
                chat.messages.push(response.clone());
                return Ok(response.content);
            }
        }
}
