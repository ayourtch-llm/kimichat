use crate::{param, core::tool::{Tool, ToolParameters, ToolResult, ParameterDefinition}};
use crate::core::tool_context::ToolContext;
use async_trait::async_trait;
use std::collections::HashMap;

/// Tool for agents to request additional iterations when needed
pub struct RequestMoreIterationsTool;

#[async_trait]
impl Tool for RequestMoreIterationsTool {
    fn name(&self) -> &str {
        "request_more_iterations"
    }

    fn description(&self) -> &str {
        "Request additional iterations when the current limit is insufficient for completing the task. \
        Requires strong justification and evidence of productive progress."
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("additional_iterations", "integer", "Number of additional iterations requested (1-10)", required),
            param!("justification", "string", "Detailed explanation of why more iterations are needed and what will be accomplished", required),
            param!("progress_summary", "string", "Summary of progress made so far and current findings", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, _context: &ToolContext) -> ToolResult {
        let additional = match params.get_required::<i32>("additional_iterations") {
            Ok(val) => val,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let justification = match params.get_required::<String>("justification") {
            Ok(val) => val,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let progress_summary = match params.get_required::<String>("progress_summary") {
            Ok(val) => val,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        // Validate request
        if additional < 1 || additional > 10 {
            return ToolResult::error("Additional iterations must be between 1 and 10".to_string());
        }

        if justification.len() < 50 {
            return ToolResult::error("Justification must be at least 50 characters - provide detailed reasoning".to_string());
        }

        if progress_summary.len() < 30 {
            return ToolResult::error("Progress summary must be at least 30 characters".to_string());
        }

        // Simple heuristic evaluation (in real implementation, this would use ProgressEvaluator)
        // For now, approve reasonable requests
        let approved = additional <= 5 &&
                      justification.len() >= 100 &&
                      !justification.to_lowercase().contains("just in case");

        if approved {
            ToolResult::success(format!(
                "✅ APPROVED: {} additional iteration(s) granted.\n\nJustification: {}\n\nProgress: {}\n\n\
                Use these iterations wisely to complete your task.",
                additional, justification, progress_summary
            ))
        } else {
            ToolResult::error(format!(
                "❌ DENIED: Request for {} additional iterations was not approved.\n\n\
                Reason: Insufficient justification or excessive request.\n\n\
                Please provide your response based on the information already gathered, \
                or improve your justification with specific details about what remains to be done.",
                additional
            ))
        }
    }
}
