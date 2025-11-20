use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use kimichat_models::{ChatRequest, ModelColor};
use crate::{safe_truncate, get_logs_dir};

/// Log HTTP request details for debugging (console output)
pub fn log_request(url: &str, request: &ChatRequest, api_key: &str, verbose: bool) {
    if !verbose {
        return;
    }

    println!("\n{}", "═".repeat(80).bright_cyan());
    println!("{}", "🔍 HTTP REQUEST DEBUG".bright_cyan().bold());
    println!("{}", "═".repeat(80).bright_cyan());

    // Parse URL to show host and port
    if let Ok(parsed_url) = reqwest::Url::parse(url) {
        println!("{}: {}", "URL".bright_yellow(), url);
        println!("{}: {}", "Host".bright_yellow(), parsed_url.host_str().unwrap_or("unknown"));
        println!("{}: {}", "Port".bright_yellow(), parsed_url.port().map(|p| p.to_string()).unwrap_or_else(||
            if parsed_url.scheme() == "https" { "443 (default)".to_string() } else { "80 (default)".to_string() }
        ));
        println!("{}: {}", "Scheme".bright_yellow(), parsed_url.scheme());
    } else {
        println!("{}: {}", "URL".bright_yellow(), url);
    }

    println!("\n{}", "Headers:".bright_yellow());
    println!("  Content-Type: application/json");
    println!("  Authorization: Bearer {}***", &api_key.chars().take(10).collect::<String>());

    println!("\n{}", "Request Body:".bright_yellow());
    match serde_json::to_string_pretty(&request) {
        Ok(json) => {
            // Truncate very long requests for readability
            if json.chars().count() > 5000 {
                println!("{}", safe_truncate(&json, 5000));
                println!("\n{}", format!("... (truncated, total {} bytes)", json.len()).bright_black());
            } else {
                println!("{}", json);
            }
        }
        Err(e) => println!("{}", format!("Error serializing request: {}", e).red()),
    }

    println!("{}", "═".repeat(80).bright_cyan());
    println!();
}

/// Log HTTP request to file for persistent debugging
pub fn log_request_to_file(url: &str, request: &ChatRequest, model: &ModelColor, api_key: &str) -> Result<()> {
    // Use shared logs directory from utility function
    let logs_dir = get_logs_dir()?;

    // Generate timestamp for filename
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Create filename with timestamp and model name
    let model_name = model.as_str_default().replace('/', "-");
    let filename = format!("req-{}-{}.txt", timestamp, model_name);
    let file_path = logs_dir.join(filename.clone());

    // Build the log content
    let mut log_content = String::new();
    log_content.push_str(&format!("HTTP REQUEST LOG\n"));
    log_content.push_str(&format!("================\n\n"));
    log_content.push_str(&format!("Timestamp: {}\n", timestamp));
    log_content.push_str(&format!("Model: {}\n\n", model.as_str_default()));

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
    log_content.push_str(&format!("  Authorization: Bearer {}***\n\n", &api_key.chars().take(10).collect::<String>()));

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
    fs::write(&file_path, log_content)
        .with_context(|| format!("Failed to write request log to {}", file_path.display()))?;

    // Print the filename to console
    println!("{}", format!("📝 Request logged to: {}", filename).bright_blue());

    Ok(())
}

/// Log HTTP response to file for persistent debugging
pub fn log_response_to_file(
    status: &reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
    body: &str,
    request_timestamp: u64,
    model: &ModelColor,
) -> Result<()> {
    // Use shared logs directory from utility function
    let logs_dir = get_logs_dir()?;

    // Create filename with timestamp and model name to match request file
    let model_name = model.as_str_default().replace('/', "-");
    let filename = format!("resp-{}-{}.txt", request_timestamp, model_name);
    let file_path = logs_dir.join(filename.clone());

    // Build the log content
    let mut log_content = String::new();
    log_content.push_str(&format!("HTTP RESPONSE LOG\n"));
    log_content.push_str(&format!("=================\n\n"));
    log_content.push_str(&format!("Timestamp: {}\n", request_timestamp));
    log_content.push_str(&format!("Model: {}\n\n", model.as_str_default()));

    // Log status information
    log_content.push_str(&format!("Status: {} {}\n\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or("Unknown")
    ));

    // Log headers
    log_content.push_str("Headers:\n");
    for (name, value) in headers.iter() {
        if let Ok(val_str) = value.to_str() {
            log_content.push_str(&format!("  {}: {}\n", name.as_str(), val_str));
        }
    }

    log_content.push_str("\nResponse Body:\n");
    // Try to pretty-print JSON, fall back to raw text
    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(body) {
        match serde_json::to_string_pretty(&json_val) {
            Ok(pretty) => {
                log_content.push_str(&pretty);
                log_content.push_str("\n");
            }
            Err(_) => {
                log_content.push_str(body);
                log_content.push_str("\n");
            }
        }
    } else {
        // Not JSON, show raw
        log_content.push_str(body);
        log_content.push_str("\n");
    }

    // Add metadata at the end
    log_content.push_str(&format!("\n---\n"));
    log_content.push_str(&format!("Response Size: {} bytes\n", body.len()));
    log_content.push_str(&format!("Content-Type: {}\n",
        headers.get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("unknown")
    ));

    // Write to file
    fs::write(&file_path, log_content)
        .with_context(|| format!("Failed to write response log to {}", file_path.display()))?;

    // Print the filename to console
    println!("{}", format!("📄 Response logged to: {}", filename).bright_blue());

    Ok(())
}

/// Log pure raw response to file without any transformation or massage
pub fn log_raw_response_to_file(
    raw_response: &str,
    request_timestamp: u64,
    model: &ModelColor,
) -> Result<()> {
    // Use shared logs directory from utility function
    let logs_dir = get_logs_dir()?;

    // Create filename for raw response with timestamp and model name
    let model_name = model.as_str_default().replace('/', "-");
    let filename = format!("resp-raw-{}-{}.txt", request_timestamp, model_name);
    let file_path = logs_dir.join(filename.clone());

    // Write the pure raw response without any modification
    fs::write(&file_path, raw_response)
        .with_context(|| format!("Failed to write raw response log to {}", file_path.display()))?;

    // Print the filename to console
    println!("{}", format!("📄 Raw response logged to: {}", filename).bright_blue());

    Ok(())
}

/// Log HTTP response details for debugging (console output)
pub fn log_response(status: &reqwest::StatusCode, headers: &reqwest::header::HeaderMap, body: &str, verbose: bool) {
    if !verbose {
        return;
    }

    println!("\n{}", "═".repeat(80).bright_green());
    println!("{}", "📥 HTTP RESPONSE DEBUG".bright_green().bold());
    println!("{}", "═".repeat(80).bright_green());

    println!("{}: {} {}",
        "Status".bright_yellow(),
        status.as_u16(),
        status.canonical_reason().unwrap_or("Unknown")
    );

    println!("\n{}", "Headers:".bright_yellow());
    for (name, value) in headers.iter() {
        if let Ok(val_str) = value.to_str() {
            println!("  {}: {}", name.as_str().bright_white(), val_str);
        }
    }

    println!("\n{}", "Response Body:".bright_yellow());
    // Try to pretty-print JSON, fall back to raw text
    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(body) {
        match serde_json::to_string_pretty(&json_val) {
            Ok(pretty) => {
                if pretty.chars().count() > 5000 {
                    println!("{}", safe_truncate(&pretty, 5000));
                    println!("\n{}", format!("... (truncated, total {} bytes)", pretty.len()).bright_black());
                } else {
                    println!("{}", pretty);
                }
            }
            Err(_) => println!("{}", body),
        }
    } else {
        // Not JSON, show raw
        if body.chars().count() > 5000 {
            println!("{}", safe_truncate(body, 5000));
            println!("\n{}", format!("... (truncated, total {} bytes)", body.len()).bright_black());
        } else {
            println!("{}", body);
        }
    }

    println!("{}", "═".repeat(80).bright_green());
    println!();
}

/// Log streaming chunk for debugging (console output)
pub fn log_stream_chunk(chunk_num: usize, data: &str, verbose: bool) {
    if !verbose {
        return;
    }

    println!("{}", format!("📦 Stream Chunk #{}: {}", chunk_num,
        if data.chars().count() > 200 {
            format!("{}... ({} bytes)", safe_truncate(data, 200), data.len())
        } else {
            data.to_string()
        }
    ).bright_black());
}
