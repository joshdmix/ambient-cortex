use chrono::Utc;
use cortex_common::models::Insight;

/// Scores and filters insights based on relevance.
pub struct InsightRanker {
    threshold: f64,
}

impl InsightRanker {
    pub fn new(threshold: f64) -> Self {
        Self { threshold }
    }

    /// Score an insight from 0.0 to 1.0 based on recency, severity, and frequency.
    pub fn score(&self, insight: &Insight, occurrence_count: u64) -> f64 {
        let recency = self.recency_score(&insight.created_at);
        let severity = self.severity_score(insight);
        let frequency = self.frequency_score(occurrence_count);

        // Weighted combination
        let score = recency * 0.4 + severity * 0.4 + frequency * 0.2;
        score.clamp(0.0, 1.0)
    }

    /// Returns true if the insight passes the relevance threshold.
    pub fn passes_threshold(&self, relevance: f64) -> bool {
        relevance >= self.threshold
    }

    /// Exponential decay from event time. Recent events score higher.
    fn recency_score(&self, created_at: &chrono::DateTime<Utc>) -> f64 {
        let age_secs = (Utc::now() - *created_at).num_seconds().max(0) as f64;
        let half_life_secs = 3600.0; // 1 hour half-life
        (-age_secs / half_life_secs * std::f64::consts::LN_2).exp()
    }

    /// Severity score based on insight type.
    fn severity_score(&self, insight: &Insight) -> f64 {
        use cortex_common::models::InsightType;
        match insight.insight_type {
            InsightType::Warning => 0.8,
            InsightType::Reminder => 0.5,
            InsightType::Suggestion => 0.3,
            InsightType::History => 0.4,
        }
    }

    /// Frequency score using log scale.
    fn frequency_score(&self, count: u64) -> f64 {
        if count == 0 {
            return 0.0;
        }
        let score = (count as f64).ln() / 10.0_f64.ln(); // log base 10
        score.clamp(0.0, 1.0)
    }
}
