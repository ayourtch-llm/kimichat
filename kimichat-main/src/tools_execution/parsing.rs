use kimichat_models::{ToolCall, FunctionCall};

/// Parse tool calls from XML-like format used by some models (e.g., glm-4.6)
/// Format: <tool_call>TOOL_NAME\n<arg_key>KEY</arg_key>\n<arg_value>VALUE</arg_value>\n...</tool_call>
pub fn parse_xml_tool_calls(content: &str) -> Option<Vec<ToolCall>> {
    if !content.contains("<tool_call>") {
        return None;
    }

    let mut tool_calls = Vec::new();
    let mut idx = 0;

    // Find all <tool_call>...</tool_call> blocks
    while let Some(start) = content[idx..].find("<tool_call>") {
        let abs_start = idx + start;
        if let Some(end) = content[abs_start..].find("</tool_call>") {
            let abs_end = abs_start + end;
            let block = &content[abs_start + 11..abs_end]; // Skip "<tool_call>"

            // Extract tool name (first line before any tags)
            let tool_name = if let Some(first_tag) = block.find('<') {
                block[..first_tag].trim().to_string()
            } else {
                block.trim().to_string()
            };

            // Extract arguments
            let mut args = std::collections::HashMap::new();
            let mut block_idx = 0;

            while let Some(key_start) = block[block_idx..].find("<arg_key>") {
                let abs_key_start = block_idx + key_start + 9; // Skip "<arg_key>"
                if let Some(key_end) = block[abs_key_start..].find("</arg_key>") {
                    let abs_key_end = abs_key_start + key_end;
                    let key = block[abs_key_start..abs_key_end].trim();

                    // Find corresponding value
                    if let Some(val_start) = block[abs_key_end..].find("<arg_value>") {
                        let abs_val_start = abs_key_end + val_start + 11; // Skip "<arg_value>"
                        if let Some(val_end) = block[abs_val_start..].find("</arg_value>") {
                            let abs_val_end = abs_val_start + val_end;
                            let value = block[abs_val_start..abs_val_end].trim();
                            args.insert(key.to_string(), value.to_string());
                            block_idx = abs_val_end;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            // Build JSON arguments from extracted key-value pairs
            let json_args = if args.is_empty() {
                "{}".to_string()
            } else {
                let mut json_obj = serde_json::Map::new();
                for (k, v) in args {
                    // Try to parse value as number if possible
                    if let Ok(num) = v.parse::<i64>() {
                        json_obj.insert(k, serde_json::json!(num));
                    } else if v == "true" || v == "false" {
                        json_obj.insert(k, serde_json::json!(v == "true"));
                    } else {
                        json_obj.insert(k, serde_json::json!(v));
                    }
                }
                serde_json::to_string(&json_obj).unwrap_or_else(|_| "{}".to_string())
            };

            // Create ToolCall structure
            tool_calls.push(ToolCall {
                id: format!("call_{}", tool_calls.len()),
                tool_type: "function".to_string(),
                function: FunctionCall {
                    name: tool_name,
                    arguments: json_args,
                },
            });

            idx = abs_end + 12; // Move past "</tool_call>"
        } else {
            break;
        }
    }

    if tool_calls.is_empty() {
        None
    } else {
        Some(tool_calls)
    }
}
