use anyhow::Result;
use fastembed::{InitOptions, TextEmbedding};

pub struct EmbeddingEngine {
    model: TextEmbedding,
}

impl EmbeddingEngine {
    pub fn new() -> Result<Self> {
        tracing::info!("loading BGE-small-en-v1.5 embedding model...");
        let model = TextEmbedding::try_new(InitOptions::default())?;
        tracing::info!("embedding model loaded successfully");
        Ok(Self { model })
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let results = self.model.embed(vec![text.to_string()], None)?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("embedding returned no results"))
    }

    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let results = self.model.embed(texts.to_vec(), None)?;
        Ok(results)
    }

    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        dot / (norm_a * norm_b)
    }
}
