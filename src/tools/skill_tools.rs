use crate::{param, core::tool::{Tool, ToolParameters, ToolResult, ParameterDefinition}};
use crate::core::tool_context::ToolContext;
use async_trait::async_trait;
use std::collections::HashMap;

/// Tool for loading and using a skill
pub struct LoadSkillTool;

#[async_trait]
impl Tool for LoadSkillTool {
    fn name(&self) -> &str {
        "load_skill"
    }

    fn description(&self) -> &str {
        "Load a skill by name and return its full content for following. Skills are proven workflows and techniques that MUST be followed when applicable. Use this BEFORE starting any task to check if a relevant skill exists. Examples: 'test-driven-development', 'systematic-debugging', 'writing-plans', 'code-review'."
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("skill_name", "string", "Name of the skill to load (e.g., 'test-driven-development', 'systematic-debugging')", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let skill_name = match params.get_required::<String>("skill_name") {
            Ok(name) => name,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        // Get skill registry from context
        let skill_registry = match &context.skill_registry {
            Some(registry) => registry,
            None => return ToolResult::error("Skill registry not available".to_string()),
        };

        // Get the skill
        match skill_registry.get_skill(&skill_name) {
            Some(skill) => {
                let result = format!(
                    "# Skill: {}\n\n**Description:** {}\n\n---\n\n{}\n\n---\n\n**IMPORTANT:** You MUST follow this skill exactly as written. Announce that you are using this skill before proceeding.",
                    skill.name,
                    skill.description,
                    skill.content
                );
                ToolResult::success(result)
            }
            None => {
                // List available skills as a hint
                let available = skill_registry.list_skills();
                ToolResult::error(format!(
                    "Skill '{}' not found. Available skills:\n{}",
                    skill_name,
                    available.join("\n")
                ))
            }
        }
    }
}

/// Tool for listing all available skills
pub struct ListSkillsTool;

#[async_trait]
impl Tool for ListSkillsTool {
    fn name(&self) -> &str {
        "list_skills"
    }

    fn description(&self) -> &str {
        "List all available skills with their descriptions. Use this to discover which skills exist and might be relevant to your current task."
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::new()
    }

    async fn execute(&self, _params: ToolParameters, context: &ToolContext) -> ToolResult {
        // Get skill registry from context
        let skill_registry = match &context.skill_registry {
            Some(registry) => registry,
            None => return ToolResult::error("Skill registry not available".to_string()),
        };

        let skills = skill_registry.get_all_skills();
        if skills.is_empty() {
            return ToolResult::success("No skills available".to_string());
        }

        let mut result = String::from("Available Skills:\n\n");
        let mut skill_list: Vec<_> = skills.values().collect();
        skill_list.sort_by_key(|s| &s.name);

        for skill in skill_list {
            result.push_str(&format!("• **{}**\n  {}\n\n", skill.name, skill.description));
        }

        result.push_str("\nUse load_skill tool to get the full content of any skill.");

        ToolResult::success(result)
    }
}

/// Tool for finding skills relevant to a task
pub struct FindRelevantSkillsTool;

#[async_trait]
impl Tool for FindRelevantSkillsTool {
    fn name(&self) -> &str {
        "find_relevant_skills"
    }

    fn description(&self) -> &str {
        "Find skills that might be relevant to a given task description. Use this at the start of any task to ensure you're following proven workflows."
    }

    fn parameters(&self) -> HashMap<String, ParameterDefinition> {
        HashMap::from([
            param!("task_description", "string", "Description of the task you're about to perform", required),
        ])
    }

    async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
        let task_description = match params.get_required::<String>("task_description") {
            Ok(desc) => desc,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        // Get skill registry from context
        let skill_registry = match &context.skill_registry {
            Some(registry) => registry,
            None => return ToolResult::error("Skill registry not available".to_string()),
        };

        let relevant = skill_registry.find_relevant_skills(&task_description);

        if relevant.is_empty() {
            ToolResult::success("No obviously relevant skills found. Consider using list_skills to browse all available skills.".to_string())
        } else {
            let mut result = String::from("Potentially relevant skills:\n\n");
            for skill_name in &relevant {
                if let Some(skill) = skill_registry.get_skill(skill_name) {
                    result.push_str(&format!("• **{}**\n  {}\n\n", skill.name, skill.description));
                }
            }
            result.push_str("\nYou MUST use load_skill to read and follow any applicable skills before proceeding.");
            ToolResult::success(result)
        }
    }
}
