use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::models::{ChatRequest, ModelType};
use crate::chat::history::safe_truncate;

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
pub fn log_request_to_file(url: &str, request: &ChatRequest, model: &ModelType, api_key: &str) -> Result<()> {
    // Create logs directory if it doesn't exist
    fs::create_dir_all("logs")?;

    // Generate timestamp for filename
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Create filename with timestamp and model name
    let model_name = model.as_str().replace('/', "-");
    let filename = format!("logs/req-{}-{}.txt", timestamp, model_name);

    // Build the log content
    let mut log_content = String::new();
    log_content.push_str(&format!("HTTP REQUEST LOG\n"));
    log_content.push_str(&format!("================\n\n"));
    log_content.push_str(&format!("Timestamp: {}\n", timestamp));
    log_content.push_str(&format!("Model: {}\n\n", model.as_str()));

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
    fs::write(&filename, log_content)
        .with_context(|| format!("Failed to write request log to {}", filename))?;

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
