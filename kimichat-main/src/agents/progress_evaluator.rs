use serde::{Serialize, Deserialize};
use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;

/// Progress evaluation result from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvaluation {
    /// Should the agent continue with current approach?
    pub should_continue: bool,
    /// Should the agent change strategy?
    pub should_change_strategy: bool,
    /// Confidence in current approach (0-1)
    pub confidence: f32,
    /// Estimated completion percentage (0-1)
    pub completion_percentage: f32,
    /// Next recommended action
    pub next_action: String,
    /// Reasoning for the evaluation
    pub reasoning: String,
    /// Suggested improvements
    pub improvements: Vec<String>,
    /// Risk assessment
    pub risk_level: RiskLevel,
    /// Estimated time remaining
    pub estimated_time_remaining: Option<u64>, // seconds
    // Compatibility fields for main.rs
    /// Progress percentage (alias for completion_percentage)
    pub progress_percentage: f32,
    /// Recommendations (alias for improvements)
    pub recommendations: Vec<String>,
    /// Change strategy flag (alias for should_change_strategy)
    pub change_strategy: bool,
}

/// Information about a single tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub tool_name: String,
    pub parameters: String,
    pub success: bool,
    pub duration_ms: u64,
    pub result_summary: Option<String>,
}

/// Summary of tool calls for progress evaluation
#[derive(Debug, Clone)]
pub struct ToolCallSummary {
    pub total_calls: u32,
    pub tool_usage: HashMap<String, u32>,
    pub recent_calls: Vec<ToolCallInfo>,
    pub current_task: String,
    pub original_request: String,
    pub elapsed_seconds: u64,
    pub errors: Vec<String>,
    pub files_changed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Enhanced progress evaluator with better tracking and assessment
pub struct ProgressEvaluator {
    evaluation_history: Vec<ProgressEvaluation>,
    performance_thresholds: PerformanceThresholds,
    strategy_history: Vec<StrategyRecord>,
    min_confidence: f32,
    eval_interval: u32,
    last_eval_iteration: u32,
    llm_client: Option<std::sync::Arc<crate::agents::GroqLlmClient>>,
}

#[derive(Debug, Clone)]
pub struct PerformanceThresholds {
    pub min_confidence: f32,
    pub max_retries: usize,
    pub timeout_seconds: u64,
    pub progress_stall_threshold: f32, // percentage points
}

#[derive(Debug, Clone)]
pub struct StrategyRecord {
    pub strategy: String,
    pub start_time: std::time::Instant,
    pub evaluations: Vec<ProgressEvaluation>,
    pub success: Option<bool>,
}

impl ProgressEvaluator {
    pub fn new(
        llm_client: std::sync::Arc<crate::agents::GroqLlmClient>,
        min_confidence: f32,
        eval_interval: u32,
    ) -> Self {
        Self {
            evaluation_history: Vec::new(),
            performance_thresholds: PerformanceThresholds {
                min_confidence: 0.3,
                max_retries: 3,
                timeout_seconds: 300, // 5 minutes
                progress_stall_threshold: 0.1, // 10% progress threshold
            },
            strategy_history: Vec::new(),
            min_confidence,
            eval_interval,
            last_eval_iteration: 0,
            llm_client: Some(llm_client),
        }
    }

    /// Check if progress evaluation should be triggered
    pub fn should_evaluate(&mut self, current_iteration: u32) -> bool {
        if current_iteration - self.last_eval_iteration >= self.eval_interval {
            self.last_eval_iteration = current_iteration;
            true
        } else {
            false
        }
    }

    /// Evaluate progress with enhanced assessment
    pub async fn evaluate_progress(
        &mut self,
        summary: &ToolCallSummary,
    ) -> Result<ProgressEvaluation> {
        // Analyze current progress based on tool call summary
        let completion_percentage = self.assess_completion_from_summary(summary)?;
        let confidence = self.assess_confidence_from_summary(summary)?;
        let risk_level = self.assess_risk_from_summary(summary)?;

        // Determine if we should continue or change strategy
        let (should_continue, should_change_strategy, next_action, reasoning) =
            self.determine_next_action_from_summary(&completion_percentage, &confidence, summary)?;

        // Estimate time remaining
        let estimated_time_remaining = self.estimate_time_remaining_from_summary(&completion_percentage, summary);

        // Suggest improvements
        let improvements = self.suggest_improvements_from_summary(summary)?;

        let evaluation = ProgressEvaluation {
            should_continue,
            should_change_strategy,
            confidence,
            completion_percentage,
            next_action,
            reasoning,
            improvements: improvements.clone(),
            risk_level,
            estimated_time_remaining,
            // Compatibility fields
            progress_percentage: completion_percentage,
            recommendations: improvements,
            change_strategy: should_change_strategy,
        };

        self.evaluation_history.push(evaluation.clone());

        Ok(evaluation)
    }

    /// Get progress percentage (alias for completion_percentage for compatibility)
    pub fn progress_percentage(&self) -> f32 {
        self.evaluation_history
            .last()
            .map(|e| e.completion_percentage)
            .unwrap_or(0.0)
    }

    /// Get recommendations (alias for improvements for compatibility)
    pub fn recommendations(&self) -> Vec<String> {
        self.evaluation_history
            .last()
            .map(|e| e.improvements.clone())
            .unwrap_or_default()
    }

    /// Get change_strategy flag (alias for should_change_strategy for compatibility)
    pub fn change_strategy(&self) -> bool {
        self.evaluation_history
            .last()
            .map(|e| e.should_change_strategy)
            .unwrap_or(false)
    }

    /// Assess completion percentage from tool call summary
    fn assess_completion_from_summary(&self, summary: &ToolCallSummary) -> Result<f32> {
        // Simple heuristic-based assessment
        let time_factor = (summary.elapsed_seconds as f32 / 300.0).min(1.0); // 5 minutes as full time
        let call_factor = (summary.total_calls as f32 / 50.0).min(1.0); // 50 calls as high usage
        let error_factor = (summary.errors.len() as f32 / 10.0).min(1.0); // 10 errors as problematic
        let file_factor = (summary.files_changed.len() as f32 / 5.0).min(1.0); // 5 files as good progress

        // Combine factors with weights
        let completion = (call_factor * 0.4 + file_factor * 0.3 + (1.0 - error_factor) * 0.2 + (1.0 - time_factor) * 0.1).min(1.0);

        Ok(completion.max(0.0))
    }

    /// Assess confidence from tool call summary
    fn assess_confidence_from_summary(&self, summary: &ToolCallSummary) -> Result<f32> {
        let mut confidence = 0.5; // Start with neutral confidence

        // Factor 1: Success rate
        let successful_calls = summary.recent_calls.iter().filter(|c| c.success).count();
        let total_calls = summary.recent_calls.len().max(1);
        let success_rate = successful_calls as f32 / total_calls as f32;
        confidence = confidence * 0.6 + success_rate * 0.4;

        // Factor 2: Error rate
        let error_rate = (summary.errors.len() as f32 / summary.total_calls as f32).min(1.0);
        confidence = confidence * (1.0 - error_rate * 0.3);

        // Factor 3: Tool diversity
        let tool_diversity = summary.tool_usage.len() as f32 / 10.0; // Assume 10 tools as max diversity
        confidence = confidence * 0.8 + tool_diversity.min(1.0) * 0.2;

        Ok(confidence.min(1.0).max(0.0))
    }

    /// Assess risk level from tool call summary
    fn assess_risk_from_summary(&self, summary: &ToolCallSummary) -> Result<RiskLevel> {
        let mut risk_score = 0.0;

        // Risk factor 1: High error rate
        let error_rate = summary.errors.len() as f32 / summary.total_calls.max(1) as f32;
        risk_score += error_rate * 0.4;

        // Risk factor 2: Long execution time
        if summary.elapsed_seconds > 600 {
            risk_score += 0.2; // 10 minutes
        }

        // Risk factor 3: Many repeated tool calls
        if summary.total_calls > 100 {
            risk_score += 0.2;
        }

        // Risk factor 4: Low success rate in recent calls
        let recent_success_rate = summary.recent_calls.iter()
            .filter(|c| c.success)
            .count() as f32 / summary.recent_calls.len().max(1) as f32;
        if recent_success_rate < 0.5 {
            risk_score += 0.2;
        }

        // Map risk score to risk level
        Ok(if risk_score >= 0.6 {
            RiskLevel::Critical
        } else if risk_score >= 0.4 {
            RiskLevel::High
        } else if risk_score >= 0.2 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        })
    }

    /// Determine next action based on summary
    fn determine_next_action_from_summary(
        &mut self,
        completion_percentage: &f32,
        confidence: &f32,
        summary: &ToolCallSummary,
    ) -> Result<(bool, bool, String, String)> {
        let should_continue = confidence >= &self.min_confidence;
        let should_change_strategy = *confidence < 0.3 && summary.total_calls > 30;

        let (next_action, reasoning) = if should_change_strategy {
            (
                "Consider alternative approach based on tool usage patterns".to_string(),
                format!("Low confidence ({:.1}%) after {} tool calls suggests strategy change needed",
                    confidence * 100.0, summary.total_calls)
            )
        } else if completion_percentage >= &0.8 {
            (
                "Final validation and completion".to_string(),
                "High completion percentage indicates task is nearly finished".to_string()
            )
        } else if completion_percentage >= &0.5 {
            (
                "Continue with current approach, monitor progress".to_string(),
                "Moderate progress suggests current strategy is working".to_string()
            )
        } else {
            (
                "Continue execution, may need refinement".to_string(),
                "Early stage progress, more time needed for assessment".to_string()
            )
        };

        Ok((should_continue, should_change_strategy, next_action, reasoning))
    }

    /// Estimate time remaining from summary
    fn estimate_time_remaining_from_summary(
        &self,
        completion_percentage: &f32,
        summary: &ToolCallSummary,
    ) -> Option<u64> {
        if *completion_percentage <= 0.0 {
            return None;
        }

        let avg_time_per_call = summary.elapsed_seconds as f32 / summary.total_calls.max(1) as f32;
        let estimated_remaining_calls = (1.0 - completion_percentage) * 100.0; // Assume 100 calls total for full task
        let estimated_remaining_seconds = estimated_remaining_calls * avg_time_per_call;

        Some(estimated_remaining_seconds as u64)
    }

    /// Suggest improvements from summary
    fn suggest_improvements_from_summary(&self, summary: &ToolCallSummary) -> Result<Vec<String>> {
        let mut suggestions = Vec::new();

        // Check for common improvement opportunities
        if summary.errors.len() > 5 {
            suggestions.push("High error rate detected - consider reviewing tool usage and parameters".to_string());
        }

        if summary.elapsed_seconds > 300 {
            suggestions.push("Task is taking longer than expected - consider breaking into smaller subtasks".to_string());
        }

        if summary.files_changed.is_empty() && summary.total_calls > 20 {
            suggestions.push("Many tool calls but no files modified - consider if current approach is effective".to_string());
        }

        // Check for tool-specific suggestions
        if let Some((tool, count)) = summary.tool_usage.iter().max_by_key(|(_, &count)| count) {
            if *count > 10 {
                suggestions.push(format!("Heavy usage of '{}' tool - consider if there's a more efficient approach", tool));
            }
        }

        if suggestions.is_empty() {
            suggestions.push("Current approach looks good, continue execution".to_string());
        }

        Ok(suggestions)
    }

    /// Assess completion percentage based on task and current output
    fn assess_completion_percentage(
        &self,
        task_description: &str,
        current_output: &str,
    ) -> Result<f32> {
        // Simple heuristic-based assessment
        let output_length = current_output.len() as f32;
        let expected_length = self.estimate_expected_output_length(task_description) as f32;
        
        let length_ratio = (output_length / expected_length).min(1.0);
        
        // Check for completion indicators
        let completion_indicators = self.count_completion_indicators(task_description, current_output);
        let indicator_ratio = (completion_indicators as f32 / 5.0).min(1.0); // Assume 5 indicators for full completion
        
        // Combine factors
        let completion = (length_ratio * 0.6 + indicator_ratio * 0.4).min(1.0);
        
        Ok(completion)
    }
    
    /// Estimate expected output length for a task
    fn estimate_expected_output_length(&self, task_description: &str) -> usize {
        // Simple estimation based on task type
        if task_description.contains("implement") || task_description.contains("create") {
            2000 // Assume ~2000 characters for implementation tasks
        } else if task_description.contains("analyze") || task_description.contains("review") {
            1000 // Assume ~1000 characters for analysis tasks
        } else if task_description.contains("fix") || task_description.contains("debug") {
            500 // Assume ~500 characters for fixes
        } else {
            800 // Default
        }
    }
    
    /// Count completion indicators in output
    fn count_completion_indicators(
        &self,
        task_description: &str,
        current_output: &str,
    ) -> usize {
        let mut indicators = 0;
        
        // Check for code completion indicators
        if task_description.contains("code") || task_description.contains("implement") {
            if current_output.contains("```") { indicators += 1; }
            if current_output.contains("fn ") || current_output.contains("function") { indicators += 1; }
            if current_output.contains("return") { indicators += 1; }
        }
        
        // Check for analysis completion indicators
        if task_description.contains("analyze") || task_description.contains("review") {
            if current_output.contains("conclusion") || current_output.contains("summary") { indicators += 1; }
            if current_output.contains("issue") || current_output.contains("problem") { indicators += 1; }
            if current_output.contains("recommendation") || current_output.contains("suggestion") { indicators += 1; }
        }
        
        // Check for fix completion indicators
        if task_description.contains("fix") || task_description.contains("debug") {
            if current_output.contains("fixed") || current_output.contains("resolved") { indicators += 1; }
            if current_output.contains("error") || current_output.contains("bug") { indicators += 1; }
        }
        
        indicators
    }
    
    /// Assess confidence in current approach
    fn assess_confidence(
        &self,
        task_description: &str,
        current_output: &str,
        context: &str,
    ) -> Result<f32> {
        // Simple confidence assessment based on various factors
        let mut confidence = 0.5; // Start with neutral confidence
        
        // Factor 1: Output relevance to task
        let relevance = self.calculate_relevance(task_description, current_output);
        confidence = confidence * 0.6 + relevance * 0.4;
        
        // Factor 2: Context alignment
        let context_alignment = self.calculate_context_alignment(context, current_output);
        confidence = confidence * 0.7 + context_alignment * 0.3;
        
        // Factor 3: Progress momentum (if we have history)
        if !self.evaluation_history.is_empty() {
            let recent_confidence: f32 = self.evaluation_history
                .iter()
                .rev()
                .take(3)
                .map(|e| e.confidence)
                .sum::<f32>() / self.evaluation_history.len().min(3) as f32;
            confidence = confidence * 0.8 + recent_confidence * 0.2;
        }
        
        Ok(confidence.min(1.0).max(0.0))
    }
    
    /// Calculate relevance of output to task
    fn calculate_relevance(&self, task_description: &str, current_output: &str) -> f32 {
        // Simple keyword-based relevance
        let task_keywords = self.extract_keywords(task_description);
        let output_keywords = self.extract_keywords(current_output);
        
        let common_keywords: Vec<String> = task_keywords
            .iter()
            .filter(|kw| output_keywords.iter().any(|ow| ow.to_lowercase().contains(&kw.to_lowercase())))
            .cloned()
            .collect();
        
        if task_keywords.is_empty() {
            0.5
        } else {
            (common_keywords.len() as f32 / task_keywords.len() as f32).min(1.0)
        }
    }
    
    /// Extract keywords from text
    fn extract_keywords(&self, text: &str) -> Vec<String> {
        let important_words = ["implement", "create", "fix", "analyze", "review", "debug", "optimize", "refactor"];
        let words: Vec<&str> = text.split_whitespace().collect();
        
        words.iter()
            .filter(|w| important_words.iter().any(|iw| w.to_lowercase().contains(&iw.to_lowercase())))
            .map(|w| w.to_string())
            .collect()
    }
    
    /// Calculate context alignment
    fn calculate_context_alignment(&self, context: &str, current_output: &str) -> f32 {
        if context.is_empty() {
            return 0.5;
        }
        
        let context_keywords = self.extract_keywords(context);
        let output_keywords = self.extract_keywords(current_output);
        
        let alignment = context_keywords.iter()
            .filter(|ck| output_keywords.iter().any(|ok| ok.to_lowercase().contains(&ck.to_lowercase())))
            .count();
        
        if context_keywords.is_empty() {
            0.5
        } else {
            (alignment as f32 / context_keywords.len() as f32).min(1.0)
        }
    }
    
    /// Assess risk level
    fn assess_risk(
        &self,
        task_description: &str,
        current_output: &str,
        previous_attempts: &[ProgressEvaluation],
    ) -> Result<RiskLevel> {
        let mut risk_score = 0.0;
        
        // Risk factor 1: Multiple failures
        let recent_failures = previous_attempts.iter()
            .rev()
            .take(3)
            .filter(|e| e.confidence < 0.3)
            .count();
        risk_score += recent_failures as f32 * 0.3;
        
        // Risk factor 2: Low completion progress
        if previous_attempts.last().map(|e| e.completion_percentage).unwrap_or(0.0) < 0.2 {
            risk_score += 0.2;
        }
        
        // Risk factor 3: Complex task
        if task_description.contains("complex") || task_description.contains("multiple") {
            risk_score += 0.1;
        }
        
        // Map risk score to risk level
        Ok(if risk_score >= 0.6 {
            RiskLevel::Critical
        } else if risk_score >= 0.4 {
            RiskLevel::High
        } else if risk_score >= 0.2 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        })
    }
    
    /// Determine next action based on current state
    fn determine_next_action(
        &mut self,
        completion_percentage: &f32,
        confidence: &f32,
        previous_attempts: &[ProgressEvaluation],
        context: &str,
    ) -> Result<(bool, bool, String, String)> {
        let should_continue = confidence >= &self.performance_thresholds.min_confidence;
        let should_change_strategy = *confidence < 0.3 && previous_attempts.len() >= 2;
        
        let (next_action, reasoning) = if should_change_strategy {
            (
                "Consider alternative approach based on previous attempts".to_string(),
                format!("Low confidence ({:.1}%) after {} attempts suggests strategy change needed", 
                    confidence * 100.0, previous_attempts.len())
            )
        } else if completion_percentage >= &0.8 {
            (
                "Final validation and completion".to_string(),
                "High completion percentage indicates task is nearly finished".to_string()
            )
        } else if completion_percentage >= &0.5 {
            (
                "Continue with current approach, monitor progress".to_string(),
                "Moderate progress suggests current strategy is working".to_string()
            )
        } else {
            (
                "Continue execution, may need refinement".to_string(),
                "Early stage progress, more time needed for assessment".to_string()
            )
        };
        
        Ok((should_continue, should_change_strategy, next_action, reasoning))
    }
    
    /// Estimate time remaining
    fn estimate_time_remaining(
        &self,
        completion_percentage: &f32,
        previous_attempts: &[ProgressEvaluation],
    ) -> Option<u64> {
        if *completion_percentage <= 0.0 || previous_attempts.is_empty() {
            return None;
        }
        
        // Calculate average time per percentage point
        let total_time_spent: u64 = previous_attempts.iter()
            .filter_map(|e| e.estimated_time_remaining)
            .sum();
        
        let avg_time_per_percent = total_time_spent as f32 / previous_attempts.len() as f32;
        let remaining_percentage = (1.0 - completion_percentage).max(0.0);
        let estimated_remaining = (remaining_percentage * avg_time_per_percent * 60.0) as u64; // Convert to seconds
        
        Some(estimated_remaining.max(30)) // Minimum 30 seconds
    }
    
    /// Suggest improvements
    fn suggest_improvements(
        &self,
        task_description: &str,
        current_output: &str,
        context: &str,
    ) -> Result<Vec<String>> {
        let mut suggestions = Vec::new();
        
        // Check for common improvement opportunities
        if task_description.contains("code") && !current_output.contains("```") {
            suggestions.push("Consider adding code examples or implementation details".to_string());
        }
        
        if task_description.contains("analyze") && !current_output.contains("conclusion") {
            suggestions.push("Add a conclusion or summary of findings".to_string());
        }
        
        if current_output.len() < 100 {
            suggestions.push("Output seems quite short, consider providing more detailed explanation".to_string());
        }
        
        if context.contains("urgent") || context.contains("critical") {
            suggestions.push("Given the urgent context, focus on the most important aspects first".to_string());
        }
        
        // Add more specific suggestions based on task type
        if task_description.contains("refactor") {
            suggestions.push("Ensure the refactored code maintains the same functionality".to_string());
            suggestions.push("Consider adding tests to verify the refactoring".to_string());
        }
        
        if suggestions.is_empty() {
            suggestions.push("Current approach looks good, continue execution".to_string());
        }
        
        Ok(suggestions)
    }
    
    /// Get evaluation history
    pub fn get_evaluation_history(&self) -> &Vec<ProgressEvaluation> {
        &self.evaluation_history
    }
    
    /// Get recent evaluations
    pub fn get_recent_evaluations(&self, count: usize) -> Vec<&ProgressEvaluation> {
        let start_index = self.evaluation_history.len().saturating_sub(count);
        self.evaluation_history
            .iter()
            .skip(start_index)
            .collect()
    }
    
    /// Clear evaluation history
    pub fn clear_history(&mut self) {
        self.evaluation_history.clear();
    }
    
    /// Update performance thresholds
    pub fn update_thresholds(&mut self, min_confidence: Option<f32>, max_retries: Option<usize>, timeout_seconds: Option<u64>) {
        if let Some(confidence) = min_confidence {
            self.performance_thresholds.min_confidence = confidence;
        }
        if let Some(retries) = max_retries {
            self.performance_thresholds.max_retries = retries;
        }
        if let Some(timeout) = timeout_seconds {
            self.performance_thresholds.timeout_seconds = timeout;
        }
    }
    
    /// Display current evaluation
    pub fn display_evaluation(&self, evaluation: &ProgressEvaluation) {
        println!("{}", "─".repeat(50).bright_cyan());
        println!("{}", "📊 PROGRESS EVALUATION".bright_cyan().bold());
        println!("{}", "─".repeat(50).bright_cyan());
        
        let status_icon = if evaluation.should_continue { "✅" } else { "❌" };
        let status_color = if evaluation.should_continue { "Continue".green() } else { "Stop".red() };
        
        println!("{} {} {}", 
            status_icon,
            "Decision:".bright_white(),
            status_color
        );
        
        println!("{} {:.1}%", "Confidence:".bright_cyan(), evaluation.confidence * 100.0);
        println!("{} {:.1}%", "Completion:".bright_cyan(), evaluation.completion_percentage * 100.0);
        println!("{} {:?}", "Risk Level:".bright_cyan(), evaluation.risk_level);
        
        if let Some(time_remaining) = evaluation.estimated_time_remaining {
            println!("{} {}s", "Est. Time Remaining:".bright_cyan(), time_remaining);
        }
        
        println!("{} {}", "Next Action:".bright_cyan(), evaluation.next_action.yellow());
        println!("{} {}", "Reasoning:".bright_cyan(), evaluation.reasoning.white());
        
        if !evaluation.improvements.is_empty() {
            println!("{}", "Suggestions:".bright_cyan());
            for (i, suggestion) in evaluation.improvements.iter().enumerate() {
                println!("  {} {}", format!("{}.", i + 1).bright_blue(), suggestion.bright_white());
            }
        }
        
        println!("{}", "─".repeat(50).bright_cyan());
        println!();
    }
}

// Removed Default implementation since ProgressEvaluator now requires parameters