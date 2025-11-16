use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};

// Embedding support for semantic skill search
pub mod embeddings;
// Embedded skills compiled into the binary
pub mod embedded;

#[cfg(feature = "fastembed")]
use embeddings::fastembed_backend::FastEmbedBackend;
use embeddings::EmbeddingBackend;

/// Represents a skill loaded from a SKILL.md file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub file_path: PathBuf,
}

/// Manages the skill library
#[derive(Clone)]
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
    skills_dir: PathBuf,
    // Optional embedding backend for semantic search
    embedding_backend: Option<Arc<dyn EmbeddingBackend>>,
    // Precomputed embeddings for each skill (name -> embedding)
    skill_embeddings: HashMap<String, Vec<f32>>,
}

impl std::fmt::Debug for SkillRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkillRegistry")
            .field("skills", &self.skills)
            .field("skills_dir", &self.skills_dir)
            .field("has_embeddings", &self.embedding_backend.is_some())
            .field("num_embeddings", &self.skill_embeddings.len())
            .finish()
    }
}

impl SkillRegistry {
    /// Create a new skill registry from a skills directory
    pub fn new(skills_dir: PathBuf) -> Result<Self> {
        let mut registry = Self {
            skills: HashMap::new(),
            skills_dir: skills_dir.clone(),
            embedding_backend: None,
            skill_embeddings: HashMap::new(),
        };

        registry.load_all_skills()?;
        registry.initialize_embeddings();
        Ok(registry)
    }

    /// Initialize embedding backend and precompute skill embeddings
    /// This is called automatically during registry creation
    /// Falls back gracefully to keyword-only mode if embeddings fail
    fn initialize_embeddings(&mut self) {
        #[cfg(feature = "fastembed")]
        {
            match FastEmbedBackend::new() {
                Ok(backend) => {
                    eprintln!("Initializing skill embeddings...");
                    let backend = Arc::new(backend);

                    // Precompute embeddings for all skills
                    let mut embeddings = HashMap::new();
                    for (name, skill) in &self.skills {
                        // Combine name and description for embedding
                        let text = format!("{} {}", skill.name, skill.description);

                        match backend.embed(&text) {
                            Ok(embedding) => {
                                embeddings.insert(name.clone(), embedding);
                            }
                            Err(e) => {
                                eprintln!("Warning: Failed to embed skill '{}': {}", name, e);
                            }
                        }
                    }

                    if !embeddings.is_empty() {
                        eprintln!("Successfully embedded {} skills", embeddings.len());
                        self.skill_embeddings = embeddings;
                        self.embedding_backend = Some(backend);
                    } else {
                        eprintln!("Warning: No skill embeddings generated, falling back to keyword search");
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Failed to initialize embedding backend: {}", e);
                    eprintln!("Falling back to keyword-only skill search");
                }
            }
        }

        #[cfg(not(feature = "fastembed"))]
        {
            // Embeddings feature not enabled, using keyword-only search
            eprintln!("Embedding feature not enabled, using keyword-only skill search");
        }
    }

    /// Load all skills from embedded data and filesystem
    /// Embedded skills are loaded first, then filesystem skills can override or complement them
    fn load_all_skills(&mut self) -> Result<()> {
        // First, load embedded skills (always available)
        let embedded = embedded::get_embedded_skills();
        for (skill_name, content) in embedded {
            match self.parse_frontmatter(content) {
                Ok((name, description)) => {
                    let skill = Skill {
                        name: name.clone(),
                        description,
                        content: content.to_string(),
                        file_path: PathBuf::from(format!("<embedded:{}>", skill_name)),
                    };
                    self.skills.insert(name, skill);
                }
                Err(e) => {
                    eprintln!("Warning: Failed to parse embedded skill '{}': {}", skill_name, e);
                }
            }
        }
        eprintln!("Loaded {} embedded skills", self.skills.len());

        // Then, load from filesystem (can override embedded skills)
        if self.skills_dir.exists() {
            let initial_count = self.skills.len();

            for entry in fs::read_dir(&self.skills_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    // Look for SKILL.md in each subdirectory
                    let skill_file = path.join("SKILL.md");
                    if skill_file.exists() {
                        match self.load_skill(&skill_file) {
                            Ok(skill) => {
                                let is_override = self.skills.contains_key(&skill.name);
                                let skill_name = skill.name.clone();
                                self.skills.insert(skill_name.clone(), skill);
                                if is_override {
                                    eprintln!("  ↳ Overriding embedded skill with filesystem version: {}", skill_name);
                                }
                            }
                            Err(e) => {
                                eprintln!("Warning: Failed to load skill from {:?}: {}", skill_file, e);
                            }
                        }
                    }
                }
            }

            let filesystem_count = self.skills.len() - initial_count;
            if filesystem_count > 0 {
                eprintln!("Loaded {} additional skills from filesystem", filesystem_count);
            }
        } else {
            eprintln!("Skills directory does not exist: {:?} (using embedded skills only)", self.skills_dir);
        }

        eprintln!("Total skills available: {}", self.skills.len());
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
    /// Uses hybrid scoring: combines keyword matching with semantic embeddings (if available)
    pub fn find_relevant_skills(&self, task_description: &str) -> Vec<String> {
        // Get keyword scores
        let keyword_scores = self.compute_keyword_scores(task_description);

        // Get embedding scores if available
        let embedding_scores = self.compute_embedding_scores(task_description);

        // Combine scores
        let combined_scores = self.combine_scores(keyword_scores, embedding_scores);

        // Sort by score (descending) and return top 5 skill names
        let mut scored_skills: Vec<_> = combined_scores.into_iter().collect();
        scored_skills.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scored_skills.into_iter()
            .take(5)
            .map(|(name, _)| name)
            .collect()
    }

    /// Compute keyword-based relevance scores for all skills
    fn compute_keyword_scores(&self, task_description: &str) -> HashMap<String, f32> {
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
            return HashMap::new();
        }

        let mut scores = HashMap::new();

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

            if score > 0.0 {
                scores.insert(name.clone(), score);
            }
        }

        scores
    }

    /// Compute embedding-based similarity scores for all skills
    /// Returns empty map if embeddings are not available
    fn compute_embedding_scores(&self, task_description: &str) -> HashMap<String, f32> {
        let backend = match &self.embedding_backend {
            Some(backend) => backend,
            None => return HashMap::new(),
        };

        // Embed the task description
        let task_embedding = match backend.embed(task_description) {
            Ok(embedding) => embedding,
            Err(e) => {
                eprintln!("Warning: Failed to embed task description: {}", e);
                return HashMap::new();
            }
        };

        // Compute cosine similarity with all skill embeddings
        let mut scores = HashMap::new();
        for (name, skill_embedding) in &self.skill_embeddings {
            let similarity = embeddings::cosine_similarity(&task_embedding, skill_embedding);
            // Scale similarity (0-1) to match keyword score range
            // Typical high similarity is 0.7-0.9, so multiply by 100 to get comparable scores
            scores.insert(name.clone(), similarity * 100.0);
        }

        scores
    }

    /// Combine keyword and embedding scores with configurable weights
    /// Falls back to keyword-only if embeddings are not available
    fn combine_scores(
        &self,
        keyword_scores: HashMap<String, f32>,
        embedding_scores: HashMap<String, f32>,
    ) -> HashMap<String, f32> {
        let mut combined = HashMap::new();

        // If no embedding scores, use keyword-only (100% weight)
        if embedding_scores.is_empty() {
            for (name, score) in keyword_scores {
                if score > 8.0 {
                    combined.insert(name, score);
                }
            }
            return combined;
        }

        // Hybrid scoring: 40% keyword, 60% embedding
        // Embedding is weighted higher as it captures semantic meaning better
        let keyword_weight = 0.4;
        let embedding_weight = 0.6;

        // Collect all skill names
        let mut all_skills: std::collections::HashSet<String> = keyword_scores.keys().cloned().collect();
        all_skills.extend(embedding_scores.keys().cloned());

        for skill_name in all_skills {
            let keyword_score = keyword_scores.get(&skill_name).copied().unwrap_or(0.0);
            let embedding_score = embedding_scores.get(&skill_name).copied().unwrap_or(0.0);

            let combined_score = keyword_score * keyword_weight + embedding_score * embedding_weight;

            // Lower threshold for hybrid scoring (embeddings help find relevant skills)
            if combined_score > 5.0 {
                combined.insert(skill_name, combined_score);
            }
        }

        combined
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
            embedding_backend: None,
            skill_embeddings: HashMap::new(),
        };

        let (name, description) = registry.parse_frontmatter(content).unwrap();
        assert_eq!(name, "test-skill");
        assert_eq!(description, "A test skill for testing");
    }
}
