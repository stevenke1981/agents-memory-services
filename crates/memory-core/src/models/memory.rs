use serde::{Deserialize, Serialize};

/// 記憶單元 — 系統核心資料結構
/// ADD-only: 建立後 content 不可修改，只更新存取統計與 decay 參數
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Memory {
    /// UUID v4 — 主鍵
    pub id: String,

    /// 提取的原子性事實/偏好/洞見
    /// 語意上自包含 (self-contained)，不依賴對話上下文即可理解
    pub content: String,

    /// 記憶類別 (存為 TEXT，SQLite 無 enum 型別)
    pub category: String,

    /// 記憶作用域
    pub scope: String,

    /// 所屬專案路徑或 ID (scope=Project 時有值)
    pub project_id: Option<String>,

    /// 所屬 Agent 實例 ID (scope=Agent 時有值)
    pub agent_id: Option<String>,

    /// 來源會話 ID
    pub source_session: String,

    /// 建立時間 (Unix timestamp milliseconds)
    pub created_at: i64,

    /// 最後更新時間 (importance/retention 更新時)
    pub updated_at: i64,

    /// 最後存取時間 (retrieval 命中時更新，用於 decay)
    pub last_accessed_at: i64,

    /// 被檢索命中次數 (強化重要性)
    pub access_count: i32,

    /// 重要性評分 [0.0, 1.0]
    /// = 0.5 * llm_score + 0.3 * access_factor + 0.2 * recency_factor
    pub importance_score: f64,

    /// Ebbinghaus 記憶保留率 [0.0, 1.0]
    /// 初始 1.0，每日根據穩定性係數 S 計算衰減
    pub retention_factor: f64,

    /// 提取到的命名實體 (JSON array of strings)
    /// 範例: '["Rust", "tokio", "RTX 3070 Ti"]'
    pub entities: String,

    /// 對應 USearch 向量索引的 ID
    pub vector_id: i64,

    /// 額外元資料 (JSON object)
    /// 範例: '{"language": "rust", "framework": "tokio"}'
    pub metadata: String,
}

/// 記憶類別
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryCategory {
    Fact,              // 一般事實知識
    Preference,        // 使用者偏好與習慣
    Decision,          // 架構/技術決策及其理由
    ProjectKnowledge,  // 專案特定知識 (結構、慣例)
    CodePattern,       // 程式碼模式與最佳實踐
    ErrorLesson,       // 錯誤教訓 (RSI: 不重蹈覆轍)
    Workflow,          // 工作流程與 SOP
}

impl MemoryCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fact => "Fact",
            Self::Preference => "Preference",
            Self::Decision => "Decision",
            Self::ProjectKnowledge => "ProjectKnowledge",
            Self::CodePattern => "CodePattern",
            Self::ErrorLesson => "ErrorLesson",
            Self::Workflow => "Workflow",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Fact" => Some(Self::Fact),
            "Preference" => Some(Self::Preference),
            "Decision" => Some(Self::Decision),
            "ProjectKnowledge" => Some(Self::ProjectKnowledge),
            "CodePattern" => Some(Self::CodePattern),
            "ErrorLesson" => Some(Self::ErrorLesson),
            "Workflow" => Some(Self::Workflow),
            _ => None,
        }
    }
}

/// 記憶作用域
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryScope {
    Global,   // 跨所有專案共用
    Project,  // 特定專案隔離
    Session,  // 僅當前會話 (短暫)
    Agent,    // 特定 Agent 實例
}

impl MemoryScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Global => "Global",
            Self::Project => "Project",
            Self::Session => "Session",
            Self::Agent => "Agent",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Global" => Some(Self::Global),
            "Project" => Some(Self::Project),
            "Session" => Some(Self::Session),
            "Agent" => Some(Self::Agent),
            _ => None,
        }
    }
}
