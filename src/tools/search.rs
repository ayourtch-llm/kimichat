use crate::{param, core::tool::{Tool, ToolParameters, ToolResult, ParameterDefinition}};
use crate::core::tool_context::ToolContext;
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
        "Search for text across files using glob patterns. Respects .gitignore files (excludes ignored files). Supports recursive search with **."
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

        eprintln!("[DEBUG] Searching with pattern: '{}' in work_dir: {:?}", pattern, context.work_dir);

        // Parse the glob pattern
        let glob_matcher = match glob::Pattern::new(&pattern) {
            Ok(matcher) => matcher,
            Err(e) => return ToolResult::error(format!("Invalid glob pattern: {}", e)),
        };

        // Use ignore crate's WalkBuilder which respects .gitignore
        let mut builder = ignore::WalkBuilder::new(&context.work_dir);
        builder
            .hidden(false)  // Show hidden files (but still respect .gitignore)
            .git_ignore(true)  // Respect .gitignore files
            .git_global(true)  // Respect global gitignore
            .git_exclude(true);  // Respect .git/info/exclude

        let mut results = Vec::new();
        let mut files_searched = 0;
        let mut ignored_count = 0;

        for entry in builder.build() {
            if results.len() >= max_results {
                break;
            }

            match entry {
                Ok(entry) => {
                    let path = entry.path();

                    // Skip directories, only search files
                    if !path.is_file() {
                        continue;
                    }

                    // Get relative path and check if it matches the glob pattern
                    if let Ok(relative_path) = path.strip_prefix(&context.work_dir) {
                        if let Some(path_str) = relative_path.to_str() {
                            // Check if path matches the glob pattern
                            if !glob_matcher.matches(path_str) {
                                continue;
                            }

                            files_searched += 1;

                            // Read and search file
                            match fs::read_to_string(path) {
                                Ok(content) => {
                                    for (line_num, line) in content.lines().enumerate() {
                                        if search_regex.is_match(line) {
                                            results.push(format!(
                                                "{}:{}:{}",
                                                path_str,
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
                    }
                }
                Err(_) => {
                    ignored_count += 1;
                }
            }
        }

        let result = if results.is_empty() {
            if files_searched == 0 {
                format!(
                    "No files matched pattern '{}'. Searched in: {:?}\n{} files were ignored (respecting .gitignore)\nTry a different pattern (e.g., 'src/**/*.rs' for recursive search in src/)",
                    pattern, context.work_dir, ignored_count
                )
            } else {
                format!(
                    "No matches found for '{}' in {} files (pattern: '{}', {} files ignored by .gitignore)",
                    query, files_searched, pattern, ignored_count
                )
            }
        } else {
            let truncated = if results.len() >= max_results {
                format!(" (showing first {} results)", max_results)
            } else {
                String::new()
            };

            let ignore_note = if ignored_count > 0 {
                format!(", {} files ignored by .gitignore", ignored_count)
            } else {
                String::new()
            };

            format!(
                "Found {} matches for '{}' in {} files{}{}:\n{}",
                results.len(),
                query,
                files_searched,
                ignore_note,
                truncated,
                results.join("\n")
            )
        };

        ToolResult::success(result)
    }
}