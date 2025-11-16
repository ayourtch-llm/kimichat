use kimichat_toolcore::{param, Tool, ToolParameters, ToolResult, ParameterDefinition};
use kimichat_toolcore::tool_context::ToolContext;
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::process::Command as AsyncCommand;

/// Tool for analyzing project structure and dependencies
pub struct ProjectAnalysisTool;

#[async_trait]
impl Tool for ProjectAnalysisTool {
    fn name(&self) -> &str {
        "project_analysis"
    }

    fn description(&self) -> &str {
        "Analyze project structure, dependencies, and file relationships"
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("analysis_type", "string", "Type of analysis to perform (structure, dependencies, file_types)", required),
            param!("target_path", "string", "Target path for analysis (optional)", optional),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let analysis_type = match params.get_required::<String>("analysis_type") {
            Ok(t) => t,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let target_path = params.get_optional::<String>("target_path").unwrap_or(None);
        let path = target_path.as_deref().unwrap_or(".");

        match analysis_type.as_str() {
            "structure" => {
                self.analyze_structure(context, path).await
            }
            "dependencies" => {
                self.analyze_dependencies(context, path).await
            }
            "file_types" => {
                self.analyze_file_types(context, path).await
            }
            _ => ToolResult::error("Invalid analysis type. Available: structure, dependencies, file_types".to_string())
        }
    }
}

impl ProjectAnalysisTool {
    async fn analyze_structure(&self, context: &ToolContext, path: &str) -> ToolResult {
        // List files with structure
        let command = format!("find {} -type f | head -20", path);
        let output = match AsyncCommand::new("bash")
            .args(["-c", &command])
            .current_dir(&context.work_dir)
            .output()
            .await
        {
            Ok(output) => output,
            Err(e) => {
                return ToolResult::error(format!("Failed to execute structure analysis: {}", e));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let result = if !stderr.is_empty() {
            format!("Structure analysis failed: {}", stderr)
        } else {
            format!("Project structure (first 20 files):\n{}", stdout.trim())
        };

        ToolResult::success(result)
    }

    async fn analyze_dependencies(&self, context: &ToolContext, path: &str) -> ToolResult {
        // Check for dependency files
        let mut result = String::new();
        let mut found_deps = false;

        // Look for common dependency files
        let dep_files = vec!["Cargo.toml", "package.json", "requirements.txt", "build.gradle"];
        for dep_file in dep_files {
            let full_path = context.work_dir.join(path).join(dep_file);
            if full_path.exists() {
                found_deps = true;
                result.push_str(&format!("Found dependency file: {}\n", dep_file));
                // Try to read the file
                match std::fs::read_to_string(&full_path) {
                    Ok(content) => {
                        result.push_str(&format!("Content:\n{}\n\n", content));
                    }
                    Err(e) => {
                        result.push_str(&format!("Failed to read {}: {}\n\n", dep_file, e));
                    }
                }
            }
        }

        if !found_deps {
            result = "No dependency files found in project root.\n".to_string();
        }

        ToolResult::success(result)
    }

    async fn analyze_file_types(&self, context: &ToolContext, path: &str) -> ToolResult {
        // Count file types
        let command = format!("find {} -type f -name '*.*' | awk -F'.' '{{print $NF}}' | sort | uniq -c | sort -nr", path);
        let output = match AsyncCommand::new("bash")
            .args(["-c", &command])
            .current_dir(&context.work_dir)
            .output()
            .await
        {
            Ok(output) => output,
            Err(e) => {
                return ToolResult::error(format!("Failed to execute file type analysis: {}", e));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let result = if !stderr.is_empty() {
            format!("File type analysis failed: {}", stderr)
        } else {
            format!("File type distribution:\n{}", stdout.trim())
        };

        ToolResult::success(result)
    }
}
