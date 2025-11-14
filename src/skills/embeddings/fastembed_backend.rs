/// FastEmbed backend implementation for embeddings
use anyhow::{Result, Context};
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};
use std::sync::Mutex;
use std::path::PathBuf;
use super::EmbeddingBackend;

/// FastEmbed-based embedding backend
/// Uses ONNX runtime with small, efficient models
/// Model is wrapped in Mutex for interior mutability (required by fastembed v5 API)
pub struct FastEmbedBackend {
    model: Mutex<TextEmbedding>,
    dimension: usize,
}

impl FastEmbedBackend {
    /// Get or create the cache directory for FastEmbed models
    /// Returns ~/.okaychat/fastembed
    fn get_cache_dir() -> Result<PathBuf> {
        let home_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .context("Failed to get home directory")?;

        let cache_dir = PathBuf::from(home_dir)
            .join(".okaychat")
            .join("fastembed");

        // Create directory if it doesn't exist
        if !cache_dir.exists() {
            std::fs::create_dir_all(&cache_dir)
                .context("Failed to create cache directory")?;
            eprintln!("Created FastEmbed cache directory: {:?}", cache_dir);
        }

        Ok(cache_dir)
    }

    /// Create a new FastEmbed backend with default settings
    /// Uses all-MiniLM-L6-v2 model (~25MB, 384 dimensions)
    pub fn new() -> Result<Self> {
        Self::with_model(EmbeddingModel::AllMiniLML6V2)
    }

    /// Create a new FastEmbed backend with a specific model
    pub fn with_model(model: EmbeddingModel) -> Result<Self> {
        eprintln!("Loading FastEmbed model: {:?}", model);

        // Get cache directory
        let cache_dir = Self::get_cache_dir()?;
        eprintln!("Using cache directory: {:?}", cache_dir);

        let embedding_model = TextEmbedding::try_new(
            InitOptions::new(model.clone())
                .with_cache_dir(cache_dir)
                .with_show_download_progress(true)
        ).context("Failed to initialize FastEmbed model")?;

        // Get dimension from model
        let dimension = match model {
            EmbeddingModel::AllMiniLML6V2 => 384,
            EmbeddingModel::AllMiniLML12V2 => 384,
            EmbeddingModel::BGESmallENV15 => 384,
            EmbeddingModel::BGEBaseENV15 => 768,
            EmbeddingModel::BGELargeENV15 => 1024,
            _ => 384, // Default fallback
        };

        eprintln!("FastEmbed model loaded successfully (dimension: {})", dimension);

        Ok(Self {
            model: Mutex::new(embedding_model),
            dimension,
        })
    }
}

impl EmbeddingBackend for FastEmbedBackend {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut model = self.model.lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock model: {}", e))?;

        let embeddings = model.embed(vec![text], None)
            .context("Failed to generate embedding")?;

        embeddings.into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No embedding generated"))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut model = self.model.lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock model: {}", e))?;

        let text_strings: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        model.embed(text_strings, None)
            .context("Failed to generate batch embeddings")
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn backend_name(&self) -> &str {
        "fastembed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Ignore by default as it downloads model
    fn test_fastembed_backend() {
        let backend = FastEmbedBackend::new().unwrap();

        let embedding = backend.embed("test query").unwrap();
        assert_eq!(embedding.len(), backend.dimension());

        // Test that similar texts have similar embeddings
        let emb1 = backend.embed("debug a problem").unwrap();
        let emb2 = backend.embed("fix a bug").unwrap();
        let emb3 = backend.embed("cook a meal").unwrap();

        let sim_similar = crate::skills::embeddings::cosine_similarity(&emb1, &emb2);
        let sim_different = crate::skills::embeddings::cosine_similarity(&emb1, &emb3);

        assert!(sim_similar > sim_different,
            "Similar texts should have higher similarity");
    }
}
