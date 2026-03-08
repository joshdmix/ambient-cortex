use anyhow::Result;
use cortex_common::models::Insight;

/// Claude API client for generating insights (Phase 2).
pub struct ClaudeClient {
    #[allow(dead_code)]
    api_key: String,
    #[allow(dead_code)]
    calls_this_hour: u32,
    #[allow(dead_code)]
    max_calls_per_hour: u32,
}

impl ClaudeClient {
    pub fn new(api_key: String, max_calls_per_hour: u32) -> Self {
        Self {
            api_key,
            calls_this_hour: 0,
            max_calls_per_hour,
        }
    }

    /// Generate an insight using Claude API. Returns Ok(None) for now (Phase 2).
    pub async fn generate_insight(
        &mut self,
        _context: &str,
    ) -> Result<Option<Insight>> {
        // Phase 2: will call Claude API here
        tracing::debug!("claude insight generation not yet implemented");
        Ok(None)
    }
}
