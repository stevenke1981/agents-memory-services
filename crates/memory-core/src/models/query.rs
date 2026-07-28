use crate::models::memory::{Memory, MemoryCategory, MemoryScope};
use crate::{error::Result, MemoryError};
use serde::{Deserialize, Serialize};

const MAX_QUERY_CHARACTERS: usize = 16_384;
const MAX_TOP_K: usize = 100;

/// 混合檢索查詢參數
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// 查詢文字
    pub query: String,

    /// 返回數量上限 (預設 10)
    #[serde(default = "default_top_k")]
    pub top_k: usize,

    /// 作用域過濾 (None = 所有)
    pub scope: Option<MemoryScope>,

    /// 專案 ID 過濾
    pub project_id: Option<String>,

    /// 類別過濾 (None = 所有)
    pub categories: Option<Vec<MemoryCategory>>,

    /// 時間範圍：起始 (Unix ms)
    pub created_after: Option<i64>,

    /// 最低重要性分數 (預設無下限)
    pub min_importance: Option<f64>,

    /// 是否包含 retention_factor < 0.1 的衰減記憶 (預設 false)
    #[serde(default)]
    pub include_decayed: bool,

    /// Session ID (for session_stats tracking)
    pub session_id: Option<String>,

    /// Hybrid 評分權重 (None = 使用配置預設)
    pub weights: Option<HybridWeights>,
}

/// Hybrid Retrieval 三路評分權重
/// 約束：semantic + bm25 + temporal = 1.0
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridWeights {
    /// 語義相似度權重 (預設 0.60)
    #[serde(default = "default_semantic_weight")]
    pub semantic: f64,
    /// BM25 關鍵字權重 (預設 0.30)
    #[serde(default = "default_bm25_weight")]
    pub bm25: f64,
    /// 時間近似度權重 (預設 0.10)
    #[serde(default = "default_temporal_weight")]
    pub temporal: f64,
}

fn default_top_k() -> usize {
    10
}
fn default_semantic_weight() -> f64 {
    0.60
}
fn default_bm25_weight() -> f64 {
    0.30
}
fn default_temporal_weight() -> f64 {
    0.10
}

impl Default for HybridWeights {
    fn default() -> Self {
        Self {
            semantic: default_semantic_weight(),
            bm25: default_bm25_weight(),
            temporal: default_temporal_weight(),
        }
    }
}

/// 搜尋結果（含分數明細，供調試）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub memory: Memory,
    pub score_final: f64,
    pub score_semantic: f64,
    pub score_bm25: f64,
    pub score_temporal: f64,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            top_k: default_top_k(),
            scope: None,
            project_id: None,
            categories: None,
            created_after: None,
            min_importance: None,
            include_decayed: false,
            session_id: None,
            weights: None,
        }
    }
}

impl SearchQuery {
    pub fn validate(&self) -> Result<()> {
        let query_length = self.query.trim().chars().count();
        if query_length == 0 {
            return Err(MemoryError::Config(
                "Search query must not be empty".to_string(),
            ));
        }
        if query_length > MAX_QUERY_CHARACTERS {
            return Err(MemoryError::Config(format!(
                "Search query must be at most {MAX_QUERY_CHARACTERS} characters"
            )));
        }
        if self.top_k == 0 || self.top_k > MAX_TOP_K {
            return Err(MemoryError::Config(format!(
                "Search top_k must be between 1 and {MAX_TOP_K}"
            )));
        }

        if let Some(min_importance) = self.min_importance {
            if !min_importance.is_finite() || !(0.0..=1.0).contains(&min_importance) {
                return Err(MemoryError::Config(
                    "Search min_importance must be finite and between 0.0 and 1.0".to_string(),
                ));
            }
        }

        if let Some(weights) = &self.weights {
            let values = [weights.semantic, weights.bm25, weights.temporal];
            if values
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
            {
                return Err(MemoryError::Config(
                    "Hybrid weights must be finite and non-negative".to_string(),
                ));
            }
            let sum: f64 = values.iter().sum();
            if (sum - 1.0).abs() > 1e-6 {
                return Err(MemoryError::Config(format!(
                    "Hybrid weights must sum to 1.0, got {sum}"
                )));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_and_oversized_queries() {
        let blank = SearchQuery {
            query: "   ".to_string(),
            ..Default::default()
        };
        assert!(blank.validate().is_err());

        let oversized = SearchQuery {
            query: "x".repeat(MAX_QUERY_CHARACTERS + 1),
            ..Default::default()
        };
        assert!(oversized.validate().is_err());
    }

    #[test]
    fn rejects_unbounded_result_requests() {
        let query = SearchQuery {
            query: "memory".to_string(),
            top_k: MAX_TOP_K + 1,
            ..Default::default()
        };
        assert!(query.validate().is_err());
    }

    #[test]
    fn rejects_invalid_importance_thresholds() {
        for value in [-0.1, 1.1, f64::NAN] {
            let query = SearchQuery {
                query: "memory".to_string(),
                min_importance: Some(value),
                ..Default::default()
            };
            assert!(query.validate().is_err());
        }
    }
}
