use anyhow::Result;
use chrono::{DateTime, Utc};
use cortex_common::models::{Insight, InsightType};
use serde::{Deserialize, Serialize};

/// Prompt type determines the system prompt template used for Claude API calls.
#[derive(Debug, Clone)]
pub enum PromptType {
    /// Context about a specific file and its edit history.
    FileContext,
    /// Correlating errors across commands and files.
    ErrorCorrelation,
    /// Summarizing a development session.
    SessionSummary,
}

/// Claude API client for generating insights (Tier 2).
pub struct ClaudeClient {
    api_key: String,
    http: reqwest::Client,
    calls_this_hour: u32,
    max_calls_per_hour: u32,
    hour_start: DateTime<Utc>,
}

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<ApiMessage>,
}

#[derive(Serialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

impl ClaudeClient {
    pub fn new(api_key: String, max_calls_per_hour: u32) -> Self {
        Self {
            api_key,
            http: reqwest::Client::new(),
            calls_this_hour: 0,
            max_calls_per_hour,
            hour_start: Utc::now(),
        }
    }

    /// Returns true if the Claude client has a valid API key and is enabled.
    pub fn is_enabled(&self) -> bool {
        !self.api_key.is_empty()
    }

    /// Generate an insight using Claude API.
    /// Returns Ok(None) if rate limited or if Claude returns no actionable insight.
    pub async fn generate_insight(
        &mut self,
        context: &str,
        prompt_type: PromptType,
    ) -> Result<Option<Insight>> {
        if !self.is_enabled() {
            return Ok(None);
        }

        // Reset hourly counter if the hour has rolled over
        self.maybe_reset_hourly_counter();

        // Rate limit check
        if self.calls_this_hour >= self.max_calls_per_hour {
            tracing::debug!(
                "claude rate limited: {}/{} calls this hour",
                self.calls_this_hour,
                self.max_calls_per_hour
            );
            return Ok(None);
        }

        let system_prompt = self.system_prompt_for(&prompt_type);

        let request = ApiRequest {
            model: "claude-sonnet-4-6".to_string(),
            max_tokens: 1024,
            system: system_prompt,
            messages: vec![ApiMessage {
                role: "user".to_string(),
                content: context.to_string(),
            }],
        };

        let response = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?;

        self.calls_this_hour += 1;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::warn!("claude API error {}: {}", status, body);
            return Ok(None);
        }

        let api_response: ApiResponse = response.json().await?;

        let text = api_response
            .content
            .into_iter()
            .filter_map(|block| block.text)
            .collect::<Vec<_>>()
            .join("\n");

        if text.is_empty() {
            return Ok(None);
        }

        let (insight_type, title) = match prompt_type {
            PromptType::FileContext => (InsightType::Suggestion, "File context insight".to_string()),
            PromptType::ErrorCorrelation => {
                (InsightType::Warning, "Error correlation insight".to_string())
            }
            PromptType::SessionSummary => {
                (InsightType::History, "Session summary".to_string())
            }
        };

        Ok(Some(Insight {
            id: None,
            created_at: Utc::now(),
            trigger_event: None,
            insight_type,
            title,
            body: text,
            relevance: 0.8,
            surfaced: false,
            dismissed: false,
            file_path: None,
            project: None,
        }))
    }

    /// Reset the hourly counter if more than an hour has passed.
    fn maybe_reset_hourly_counter(&mut self) {
        let elapsed = (Utc::now() - self.hour_start).num_seconds();
        if elapsed >= 3600 {
            self.calls_this_hour = 0;
            self.hour_start = Utc::now();
        }
    }

    /// Build the system prompt for a given prompt type.
    fn system_prompt_for(&self, prompt_type: &PromptType) -> String {
        match prompt_type {
            PromptType::FileContext => {
                "You are an expert development assistant analyzing file edit patterns. \
                 Given a summary of recent file activity, identify potential issues, \
                 suggest improvements, or highlight patterns the developer may not notice. \
                 Be concise and actionable. Never include raw file contents — only work \
                 with the summaries provided. Respond with a single paragraph."
                    .to_string()
            }
            PromptType::ErrorCorrelation => {
                "You are an expert development assistant analyzing error patterns. \
                 Given a summary of recent command failures and file changes, identify \
                 correlations between errors and recent code changes. Suggest the most \
                 likely root cause and which files to investigate. Be concise and actionable. \
                 Never include raw file contents — only work with the summaries provided. \
                 Respond with a single paragraph."
                    .to_string()
            }
            PromptType::SessionSummary => {
                "You are an expert development assistant summarizing a coding session. \
                 Given a summary of events from a development session, provide a brief \
                 recap of what was accomplished, any unresolved issues, and suggested \
                 next steps. Be concise. Never include raw file contents — only work \
                 with the summaries provided. Respond with a single paragraph."
                    .to_string()
            }
        }
    }
}
