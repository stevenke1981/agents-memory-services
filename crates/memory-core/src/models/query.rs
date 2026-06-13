use serde::{Deserialize, Serialize};
use crate::models::memory::{Memory, MemoryCategory, MemoryScope};

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

fn default_top_k() -> usize { 10 }
fn default_semantic_weight() -> f64 { 0.60 }
fn default_bm25_weight() -> f64 { 0.30 }
fn default_temporal_weight() -> f64 { 0.10 }

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
            weights: None,
        }
    }
}
