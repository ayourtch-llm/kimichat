use crate::{param, core::tool::{Tool, ToolParameters, ToolResult, ParameterDefinition}};
use crate::core::tool_context::ToolContext;
use crate::tools::helpers::build_glob_pattern;
use async_trait::async_trait;
use std::collections::HashMap;
use regex::Regex;
use std::fs;

/// Tool for searching text across files
pub struct SearchFilesTool;

#[async_trait]
impl Tool for SearchFilesTool {
    fn name(&self) -> &str {
        "search_files"
    }

    fn description(&self) -> &str {
        "Search for text across files using glob patterns. Supports recursive search with **."
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("query", "string", "Text or pattern to search for", required),
            param!("pattern", "string", "File pattern to search in (e.g., 'src/**/*.rs', '**/*.py'). Use ** for recursive search. Defaults to '**/*.rs' (all Rust files)", optional, "**/*.rs"),
            param!("regex", "boolean", "Use regex search instead of plain text", optional, false),
            param!("case_insensitive", "boolean", "Case insensitive search", optional, false),
            param!("max_results", "integer", "Maximum number of results to return", optional, 50),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let query = match params.get_required::<String>("query") {
            Ok(query) => query,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let pattern = params.get_optional::<String>("pattern")
            .unwrap_or(Some("**/*.rs".to_string()))
            .unwrap_or_else(|| "**/*.rs".to_string());

        let use_regex = params.get_optional::<bool>("regex")
            .unwrap_or(Some(false))
            .unwrap_or(false);

        let case_insensitive = params.get_optional::<bool>("case_insensitive")
            .unwrap_or(Some(false))
            .unwrap_or(false);

        let max_results = params.get_optional::<i32>("max_results")
            .unwrap_or(Some(50))
            .unwrap_or(50) as usize;

        // Build search pattern
        let search_regex = if use_regex {
            match Regex::new(&query) {
                Ok(regex) => regex,
                Err(e) => return ToolResult::error(format!("Invalid regex pattern: {}", e)),
            }
        } else {
            let escaped_query = regex::escape(&query);
            let regex_str = if case_insensitive {
                format!("(?i){}", escaped_query)
            } else {
                escaped_query
            };
            Regex::new(&regex_str).unwrap()
        };

        let glob_pattern = build_glob_pattern(&pattern, &context.work_dir);

        eprintln!("[DEBUG] Searching with pattern: '{}' in work_dir: {:?}", glob_pattern, context.work_dir);

        let mut results = Vec::new();
        let mut files_searched = 0;

        match glob::glob(&glob_pattern) {
            Ok(paths) => {
                for path in paths {
                    if results.len() >= max_results {
                        break;
                    }

                    match path {
                        Ok(path) if path.is_file() => {
                            files_searched += 1;

                            match fs::read_to_string(&path) {
                                Ok(content) => {
                                    let relative_path = path.strip_prefix(&context.work_dir)
                                        .unwrap_or(&path)
                                        .to_string_lossy();

                                    for (line_num, line) in content.lines().enumerate() {
                                        if search_regex.is_match(line) {
                                            results.push(format!(
                                                "{}:{}:{}",
                                                relative_path,
                                                line_num + 1,
                                                line.trim()
                                            ));

                                            if results.len() >= max_results {
                                                break;
                                            }
                                        }
                                    }
                                }
                                Err(_) => {
                                    // Skip files that can't be read
                                    continue;
                                }
                            }
                        }
                        _ => {
                            // Skip non-files or path errors
                            continue;
                        }
                    }
                }

                let result = if results.is_empty() {
                    if files_searched == 0 {
                        format!("No files matched pattern '{}'. Searched in: {:?}\nTry a different pattern (e.g., 'src/**/*.rs' for recursive search in src/)", pattern, context.work_dir)
                    } else {
                        format!("No matches found for '{}' in {} files (pattern: '{}')", query, files_searched, pattern)
                    }
                } else {
                    let truncated = if results.len() >= max_results {
                        format!(" (showing first {} results)", max_results)
                    } else {
                        String::new()
                    };

                    format!(
                        "Found {} matches for '{}' in {} files{}:\n{}",
                        results.len(),
                        query,
                        files_searched,
                        truncated,
                        results.join("\n")
                    )
                };

                ToolResult::success(result)
            }
            Err(e) => ToolResult::error(format!("Invalid glob pattern: {}", e)),
        }
    }
}