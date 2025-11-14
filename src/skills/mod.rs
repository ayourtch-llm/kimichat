use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};

/// Represents a skill loaded from a SKILL.md file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub file_path: PathBuf,
}

/// Manages the skill library
#[derive(Debug, Clone)]
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
    skills_dir: PathBuf,
}

impl SkillRegistry {
    /// Create a new skill registry from a skills directory
    pub fn new(skills_dir: PathBuf) -> Result<Self> {
        let mut registry = Self {
            skills: HashMap::new(),
            skills_dir: skills_dir.clone(),
        };

        registry.load_all_skills()?;
        Ok(registry)
    }

    /// Load all skills from the skills directory
    fn load_all_skills(&mut self) -> Result<()> {
        if !self.skills_dir.exists() {
            eprintln!("Skills directory does not exist: {:?}", self.skills_dir);
            return Ok(());
        }

        // Iterate through all subdirectories in skills/
        for entry in fs::read_dir(&self.skills_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Look for SKILL.md in each subdirectory
                let skill_file = path.join("SKILL.md");
                if skill_file.exists() {
                    match self.load_skill(&skill_file) {
                        Ok(skill) => {
                            self.skills.insert(skill.name.clone(), skill);
                        }
                        Err(e) => {
                            eprintln!("Warning: Failed to load skill from {:?}: {}", skill_file, e);
                        }
                    }
                }
            }
        }

        eprintln!("Loaded {} skills", self.skills.len());
        Ok(())
    }

    /// Load a single skill from a SKILL.md file
    fn load_skill(&self, path: &Path) -> Result<Skill> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read skill file: {:?}", path))?;

        // Parse YAML frontmatter
        let (name, description) = self.parse_frontmatter(&content)?;

        Ok(Skill {
            name,
            description,
            content,
            file_path: path.to_path_buf(),
        })
    }

    /// Parse YAML frontmatter from skill content
    /// Returns (name, description)
    fn parse_frontmatter(&self, content: &str) -> Result<(String, String)> {
        // Check if content starts with ---
        if !content.starts_with("---") {
            anyhow::bail!("Skill file does not start with YAML frontmatter");
        }

        // Find the closing ---
        let lines: Vec<&str> = content.lines().collect();
        let mut frontmatter_end = None;
        for (i, line) in lines.iter().enumerate().skip(1) {
            if line.trim() == "---" {
                frontmatter_end = Some(i);
                break;
            }
        }

        let frontmatter_end = frontmatter_end
            .ok_or_else(|| anyhow::anyhow!("Could not find closing --- for YAML frontmatter"))?;

        // Parse the frontmatter
        let frontmatter_lines = &lines[1..frontmatter_end];
        let mut name = None;
        let mut description = None;

        for line in frontmatter_lines {
            let line = line.trim();
            if line.starts_with("name:") {
                name = Some(line.strip_prefix("name:").unwrap().trim().to_string());
            } else if line.starts_with("description:") {
                // Description might span multiple lines
                let desc_start = line.strip_prefix("description:").unwrap().trim().to_string();
                description = Some(desc_start);
            }
        }

        let name = name.ok_or_else(|| anyhow::anyhow!("Skill frontmatter missing 'name' field"))?;
        let description = description.ok_or_else(|| anyhow::anyhow!("Skill frontmatter missing 'description' field"))?;

        Ok((name, description))
    }

    /// Get a skill by name
    pub fn get_skill(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    /// Get all skill names
    pub fn list_skills(&self) -> Vec<String> {
        let mut names: Vec<_> = self.skills.keys().cloned().collect();
        names.sort();
        names
    }

    /// Get all skills
    pub fn get_all_skills(&self) -> &HashMap<String, Skill> {
        &self.skills
    }

    /// Find skills relevant to a given task description
    /// Returns list of skill names that might apply
    pub fn find_relevant_skills(&self, task_description: &str) -> Vec<String> {
        let task_lower = task_description.to_lowercase();
        let mut relevant = Vec::new();

        for (name, skill) in &self.skills {
            let desc_lower = skill.description.to_lowercase();

            // Check for keyword matches
            if task_lower.contains("test") && (name.contains("test") || desc_lower.contains("test")) {
                relevant.push(name.clone());
            } else if task_lower.contains("debug") && (name.contains("debug") || desc_lower.contains("debug")) {
                relevant.push(name.clone());
            } else if task_lower.contains("plan") && (name.contains("plan") || desc_lower.contains("plan")) {
                relevant.push(name.clone());
            } else if task_lower.contains("review") && (name.contains("review") || desc_lower.contains("review")) {
                relevant.push(name.clone());
            }
            // Add more heuristics as needed
        }

        relevant
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter() {
        let content = r#"---
name: test-skill
description: A test skill for testing
---

# Test Skill

This is the content.
"#;

        let registry = SkillRegistry {
            skills: HashMap::new(),
            skills_dir: PathBuf::from("skills"),
        };

        let (name, description) = registry.parse_frontmatter(content).unwrap();
        assert_eq!(name, "test-skill");
        assert_eq!(description, "A test skill for testing");
    }
}
