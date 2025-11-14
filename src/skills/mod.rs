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
    /// Returns list of skill names that might apply, sorted by relevance score
    pub fn find_relevant_skills(&self, task_description: &str) -> Vec<String> {
        let task_lower = task_description.to_lowercase();

        // Extract meaningful words from task (filter out common stop words)
        let stop_words = ["the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for",
                          "of", "with", "by", "from", "as", "is", "was", "are", "were", "be",
                          "have", "has", "had", "do", "does", "did", "will", "would", "should",
                          "could", "may", "might", "must", "can", "this", "that", "these", "those",
                          "i", "you", "we", "they", "it", "he", "she", "my", "your", "our", "their"];

        let task_words: Vec<&str> = task_lower
            .split(|c: char| !c.is_alphanumeric() && c != '-')
            .filter(|w| w.len() > 2 && !stop_words.contains(w))
            .collect();

        if task_words.is_empty() {
            return Vec::new();
        }

        // Score each skill based on relevance
        let mut scored_skills: Vec<(String, f32)> = Vec::new();

        for (name, skill) in &self.skills {
            let mut score = 0.0;

            let name_lower = name.to_lowercase();
            let desc_lower = skill.description.to_lowercase();

            // Extract words from skill name and description
            let skill_words: Vec<String> = format!("{} {}", name_lower, desc_lower)
                .split(|c: char| !c.is_alphanumeric() && c != '-')
                .filter(|w| w.len() > 2 && !stop_words.contains(w))
                .map(|s| s.to_string())
                .collect();

            // Score based on word overlap
            for task_word in &task_words {
                for skill_word in &skill_words {
                    if task_word == skill_word {
                        // Exact match
                        score += 10.0;
                    } else if task_word.contains(skill_word) || skill_word.contains(task_word) {
                        // Partial match (e.g., "debugging" contains "debug")
                        score += 5.0;
                    } else if Self::similar_words(task_word, skill_word) {
                        // Similar words (common root/stem)
                        score += 3.0;
                    }
                }
            }

            // Bonus for name matches (skill name is more important than description)
            for task_word in &task_words {
                if name_lower.contains(task_word) {
                    score += 15.0;
                }
            }

            // Only include skills with a minimum relevance score
            if score > 8.0 {
                scored_skills.push((name.clone(), score));
            }
        }

        // Sort by score (descending) and return skill names
        scored_skills.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top 5 most relevant skills
        scored_skills.into_iter()
            .take(5)
            .map(|(name, _)| name)
            .collect()
    }

    /// Check if two words are similar (common patterns for related words)
    fn similar_words(word1: &str, word2: &str) -> bool {
        // Check for common verb forms (e.g., "test" and "testing")
        if word1.len() > 4 && word2.len() > 4 {
            let min_len = word1.len().min(word2.len());
            let common_prefix = word1.chars().zip(word2.chars())
                .take_while(|(a, b)| a == b)
                .count();

            // If they share at least 70% of the shorter word, consider them similar
            if common_prefix >= (min_len * 7) / 10 {
                return true;
            }
        }

        // Check for common word endings that indicate related concepts
        let endings = [
            ("ing", ""), ("ed", ""), ("s", ""),
            ("tion", "te"), ("ment", ""), ("ness", ""),
        ];

        for (ending1, ending2) in &endings {
            if word1.ends_with(ending1) && word2.ends_with(ending2) {
                let stem1 = &word1[..word1.len() - ending1.len()];
                let stem2 = &word2[..word2.len() - ending2.len()];
                if stem1 == stem2 {
                    return true;
                }
            }
        }

        false
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
