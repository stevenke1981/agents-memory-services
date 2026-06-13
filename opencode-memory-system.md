# OpenCode Agent 長期記憶系統
## 計劃書 & 技術規格書 v1.0

> **技術棧：** Rust 2021 (全核心) + TypeScript shim (Plugin 薄層)  
> **架構：** MCP Server (Rust binary) + OpenCode Plugin (TS lifecycle hooks)  
> **演算法對齊：** Mem0 — Single-Pass Extraction + ADD-only Consolidation + Hybrid Retrieval

---

## 目錄

1. [Part I：計劃書](#part-i計劃書)
   - 專案背景與目標
   - 核心演算法對齊矩陣
   - 技術選型
   - 系統架構圖
   - 資料流程
   - 開發階段規劃
   - 風險評估
2. [Part II：技術規格書](#part-ii技術規格書)
   - Cargo Workspace 結構
   - 資料模型
   - SQLite Schema
   - Extraction Engine
   - Consolidation Engine
   - Hybrid Retrieval Engine
   - MCP Server 工具規格
   - Plugin TypeScript Shim
   - OpenCode 配置規格
   - Cargo.toml 規格
   - Skills 規格
   - 測試規格
3. [Part III：AGENTS.md（AI 實現指南）](#part-iiiagentsmd)

---

## Part I：計劃書

### 1. 專案背景與目標

OpenCode Agent 目前每次會話均從零開始，缺乏跨會話的持久上下文。本專案以 Mem0 演算法為藍本，以 Rust 語言實現完整的長期記憶系統，整合至 OpenCode 生態。

**四大核心目標：**

| # | 目標 | 驗收標準 |
|---|------|---------|
| 1 | 跨會話持久記憶 | Agent 重啟後仍知道過去決策/偏好 |
| 2 | 自動提取與注入 | 無需人工干預，lifecycle hooks 全自動 |
| 3 | 本地優先，零外部依賴 | 離線可用，無 SaaS lock-in |
| 4 | 高精準度檢索 | Hybrid score 優於純語義 |

---

### 2. 核心演算法對齊矩陣

| Mem0 演算法 | 本系統實現 | 技術元件 |
|-------------|------------|---------|
| Single-Pass Hierarchical Extraction | LLM 單次呼叫，結構化 JSON 輸出，一次提取全部記憶 | `ExtractionEngine` (Rust) |
| ADD-only Consolidation | 只新增不覆蓋，dedup + entity linking + importance score | `ConsolidationEngine` (Rust) |
| Multi-Signal Hybrid Retrieval | Semantic + BM25 + Temporal，加權融合 Top-K | `RetrievalEngine` (Rust) |
| Lifecycle Auto-injection | onChatStart 自動注入 System Prompt | Plugin TS shim |
| Memory Decay | Ebbinghaus 衰減模型，定期 compact | `DecayScheduler` (Rust) |

---

### 3. 技術選型

| 組件 | 選用技術 | 版本 | 選型理由 |
|------|---------|------|---------|
| 核心語言 | Rust | 2021 edition | 零GC、記憶體安全、高效能 |
| 非同步運行時 | Tokio | 1.x | Rust 標準 async runtime |
| 關聯式儲存 | SQLite via sqlx | 0.8 | 本地優先、零依賴、WAL 模式 |
| 向量索引 | USearch | 2.x | 純 Rust HNSW，低記憶體佔用 |
| 全文搜索 | Tantivy | 0.22 | 純 Rust BM25，生產就緒 |
| MCP 協議 | rmcp | 0.1 | 官方 Rust MCP SDK |
| HTTP 客戶端 | reqwest | 0.12 | 非同步、OpenAI-compatible API |
| 序列化 | serde + serde_json | 1.x | Rust 標準 |
| 錯誤處理 | anyhow + thiserror | 1.x | 分層錯誤類型 |
| 記錄/追蹤 | tracing | 0.1 | 結構化日誌 (輸出至 stderr) |
| Plugin 薄層 | TypeScript | 5.x | OpenCode 強制要求，≤100 行 |

> **注意：** Plugin 層因 OpenCode 架構限制必須使用 TypeScript。設計原則為**最小化 TS 程式碼**，所有業務邏輯均在 Rust MCP Server 實現。TypeScript 僅作薄包裝層（lifecycle hooks 委派 MCP 調用）。

---

### 4. 系統架構圖

```
┌──────────────────────────────────────────────────────────────────────┐
│                          OpenCode IDE                                │
│                                                                      │
│  ┌─────────────────────────┐   ┌────────────────────────────────┐   │
│  │   Plugin (TS shim ~80行) │   │         LLM Agent              │   │
│  │  ┌─────────────────────┐│   │  (自主呼叫 MCP tools)           │   │
│  │  │ onChatStart         ││   └───────────────┬────────────────┘   │
│  │  │  → search + inject  ││                   │ MCP tool calls     │
│  │  │ onMessageComplete   ││                   │                    │
│  │  │  → add_memory (async││                   │                    │
│  │  │ onSessionEnd        ││                   │                    │
│  │  │  → consolidate      ││                   │                    │
│  │  └─────────────────────┘│                   │                    │
│  └───────────┬─────────────┘                   │                    │
│              │ spawn/IPC                        │                    │
└──────────────┼──────────────────────────────────┼────────────────────┘
               │                                  │
               ▼                                  ▼
┌──────────────────────────────────────────────────────────────────────┐
│              Memory MCP Server  (Rust binary)                        │
│                                                                      │
│  JSON-RPC 2.0 over stdio  |  MCP Protocol via rmcp crate            │
│                                                                      │
│  Tools:  add_memory  |  search_memories  |  get_memories             │
│          delete_memory  |  consolidate_memories  |  get_stats        │
└──────────────────────────────────────────────────────────────────────┘
                               │
                               │ Rust library call
                               ▼
┌──────────────────────────────────────────────────────────────────────┐
│                    Memory Core  (Rust library crate)                 │
│                                                                      │
│  ┌──────────────────┐  ┌─────────────────────┐  ┌────────────────┐  │
│  │  ExtractionEngine│  │ ConsolidationEngine  │  │RetrievalEngine │  │
│  │  ───────────────  │  │ ──────────────────  │  │──────────────  │  │
│  │  Single-Pass LLM  │  │ Dedup (cosine >0.92)│  │ Semantic       │  │
│  │  Structured JSON  │  │ Entity Linking      │  │ + BM25         │  │
│  │  Filtering        │  │ Importance Scoring  │  │ + Temporal     │  │
│  │  Categorization   │  │ ADD-only only       │  │ Score Fusion   │  │
│  └────────┬─────────┘  └──────────┬──────────┘  └──────┬─────────┘  │
│           └────────────────────────┼─────────────────────┘           │
│                                    ▼                                  │
│           ┌────────────────────────────────────────────┐            │
│           │              Storage Layer                  │            │
│           │                                             │            │
│           │  ┌──────────────┐  ┌──────────────────────┐│            │
│           │  │   SQLite     │  │  USearch (HNSW)       ││            │
│           │  │  (metadata   │  │  (dense vectors,      ││            │
│           │  │   + facts    │  │   cosine similarity)  ││            │
│           │  │   + entities)│  └──────────────────────┘│            │
│           │  └──────────────┘  ┌──────────────────────┐│            │
│           │                    │  Tantivy (BM25 index) ││            │
│           │                    │  (full-text search)   ││            │
│           │                    └──────────────────────┘│            │
│           └────────────────────────────────────────────┘            │
└──────────────────────────────────────────────────────────────────────┘

儲存路徑：${PROJECT_ROOT}/.opencode/
  ├── memory.db          # SQLite
  ├── vectors.usearch    # USearch HNSW index
  └── tantivy/           # Tantivy index directory
```

---

### 5. 資料流程

#### 5.1 會話開始（自動記憶注入）

```
Plugin.onChatStart(context)
  │
  ├─→ MCP: search_memories(query=context.projectPath, top_k=10, scope=Project)
  │         │
  │         └─→ RetrievalEngine.search()
  │               ├── USearch cosine similarity (semantic)
  │               ├── Tantivy BM25 (keyword)
  │               └── Temporal recency score
  │                   → Hybrid score fusion → Top-K results
  │
  └─→ inject_to_system_prompt(formatted_memories)
       Agent 開始對話，已注入歷史上下文
```

#### 5.2 對話中（非阻塞記憶提取）

```
Plugin.onMessageComplete(msg)  [非阻塞 queueMicrotask]
  │
  └─→ MCP: add_memory(content=conversation_turn, scope=Project)
            │
            └─→ ExtractionEngine.extract(content)
                  │  LLM Single-Pass call
                  │  → [{content, category, entities, importance, confidence}, ...]
                  │
                  └─→ ConsolidationEngine.consolidate(extracted_memories)
                        ├── Embed each memory → vector
                        ├── USearch search: top-5 similar
                        ├── If cosine > 0.92 → SKIP (duplicate)
                        ├── If 0.75 < cosine < 0.92 → entity link check
                        └── If novel → INSERT SQLite + USearch + Tantivy
```

#### 5.3 會話結束（批量鞏固）

```
Plugin.onSessionEnd(context)
  │
  └─→ MCP: consolidate_memories(scope=Project, project_id=...)
            │
            └─→ ConsolidationEngine.batch_consolidate()
                  ├── Dedup pass: 移除餘下重複項
                  ├── Decay update: 更新 retention_factor
                  ├── Importance recalculation
                  └── Index compaction (Tantivy + USearch)
```

---

### 6. 開發階段規劃

#### Phase 1：Memory Core 基礎層（Week 1–2）

| 任務 | 說明 |
|------|------|
| Cargo workspace 建立 | 三個 crate：memory-core / memory-mcp-server / memory-cli |
| SQLite schema + migration | `sqlx migrate` V1__init.sql，WAL mode |
| 基礎 Storage Layer | CRUD: insert/get/delete/list for memories + entities |
| USearch 整合 | 初始化 HNSW index，insert/search vector operations |
| Tantivy 整合 | IndexWriter/IndexReader，BM25 search |
| ExtractionEngine skeleton | LLM API client + extraction prompt + JSON parsing |
| 單元測試 | storage/extraction 各覆蓋率 ≥ 85% |

#### Phase 2：MCP Server（Week 3）

| 任務 | 說明 |
|------|------|
| rmcp server setup | JSON-RPC 2.0 over stdio，stderr 日誌 |
| 6 個 MCP tools 實作 | add/search/get/delete/consolidate/stats |
| Tool schema 驗證 | inputSchema JSON Schema，嚴格型別 |
| MCP 整合測試 | 模擬 OpenCode MCP client，驗證協議相容性 |

#### Phase 3：Plugin + 整合（Week 4）

| 任務 | 說明 |
|------|------|
| TypeScript shim 撰寫 | onChatStart / onMessageComplete / onSessionEnd |
| opencode.json 配置 | MCP server 路徑、環境變數 |
| 端對端測試 | 完整 lifecycle 模擬測試 |

#### Phase 4：Hybrid Retrieval + Consolidation 完整版（Week 5）

| 任務 | 說明 |
|------|------|
| Hybrid score fusion | α·semantic + β·BM25 + γ·temporal |
| ADD-only Consolidation 完整版 | entity linking + near-dedup merge |
| Ebbinghaus decay 排程器 | tokio::spawn 背景任務，每日執行 |
| 性能 benchmark | 10K records 下 latency 測試 |
| Recall accuracy 測試 | 對比純 semantic baseline |

**總計：5 週**

---

### 7. 風險評估

| 風險 | 嚴重度 | 可能性 | 緩解策略 |
|------|-------|-------|---------|
| OpenCode Plugin API 變更 | 中 | 低 | TS shim ≤100行，最小化耦合；核心邏輯全在 Rust |
| LLM extraction 品質不穩 | 高 | 中 | Few-shot prompt + JSON schema validation + confidence 過濾 |
| rmcp crate API 不穩定 | 中 | 中 | Pin 版本；若不穩定可直接實作 JSON-RPC 2.0 over stdio (200行) |
| USearch 向量維度變更 | 中 | 低 | 維度寫入 DB metadata，版本遷移腳本 |
| SQLite 並發寫入 | 低 | 低 | WAL mode + single writer pattern (Arc<Mutex<Pool>>) |
| 本地 embedding 延遲 | 低 | 中 | 非同步非阻塞提取；extraction 在 onMessage 後臺執行 |

---

## Part II：技術規格書

### 1. Cargo Workspace 目錄結構

```
opencode-memory/
├── Cargo.toml                          # workspace root
├── Cargo.lock
│
├── crates/
│   ├── memory-core/                    # 核心庫 (lib crate)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # 公開 API 入口
│   │       ├── error.rs                # MemoryError enum (thiserror)
│   │       ├── config.rs               # MemoryConfig (env + file)
│   │       ├── service.rs              # MemoryService (orchestrator)
│   │       │
│   │       ├── models/
│   │       │   ├── mod.rs
│   │       │   ├── memory.rs           # Memory, MemoryCategory, MemoryScope
│   │       │   └── query.rs            # SearchQuery, HybridWeights
│   │       │
│   │       ├── extraction/
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs           # ExtractionEngine
│   │       │   ├── prompt.rs           # Prompt templates (const str)
│   │       │   └── llm_client.rs       # OpenAI-compatible HTTP client
│   │       │
│   │       ├── consolidation/
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs           # ConsolidationEngine
│   │       │   ├── dedup.rs            # Deduplication logic
│   │       │   ├── entity.rs           # Entity linking
│   │       │   └── decay.rs            # Ebbinghaus decay + scheduler
│   │       │
│   │       ├── retrieval/
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs           # RetrievalEngine (top-level)
│   │       │   ├── semantic.rs         # USearch HNSW retriever
│   │       │   ├── bm25.rs             # Tantivy BM25 retriever
│   │       │   └── hybrid.rs           # Score fusion algorithm
│   │       │
│   │       └── storage/
│   │           ├── mod.rs
│   │           ├── sqlite.rs           # SqliteStore (sqlx Pool)
│   │           ├── vector.rs           # VectorStore (USearch wrapper)
│   │           ├── text_index.rs       # TextIndex (Tantivy wrapper)
│   │           └── migrations/
│   │               └── V1__init.sql
│   │
│   ├── memory-mcp-server/              # MCP Server (bin crate)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs                 # Entry, tokio::main
│   │       ├── server.rs               # McpServer setup (rmcp)
│   │       └── tools/
│   │           ├── mod.rs
│   │           ├── add_memory.rs
│   │           ├── search_memories.rs
│   │           ├── get_memories.rs
│   │           ├── delete_memory.rs
│   │           ├── consolidate.rs
│   │           └── get_stats.rs
│   │
│   └── memory-cli/                     # Debug CLI (bin crate)
│       ├── Cargo.toml
│       └── src/
│           └── main.rs                 # clap commands: add/search/list/stats
│
├── plugin/                             # TypeScript shim
│   ├── package.json
│   ├── tsconfig.json
│   └── src/
│       └── index.ts                    # ≤100 行，lifecycle hooks only
│
├── skills/
│   └── memory-extraction.md            # OpenCode Skill 定義
│
├── tests/
│   └── integration/
│       ├── lifecycle_test.rs           # 完整 lifecycle e2e
│       ├── dedup_test.rs               # ADD-only dedup 驗證
│       └── retrieval_test.rs           # Hybrid ranking 驗證
│
└── docs/
    └── AGENTS.md                       # AI Coding Agent 實現指南
```

---

### 2. 資料模型規格

#### 2.1 Memory（核心實體）

```rust
// crates/memory-core/src/models/memory.rs

use chrono::{DateTime, Utc};
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
}

/// 記憶作用域
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryScope {
    Global,   // 跨所有專案共用
    Project,  // 特定專案隔離
    Session,  // 僅當前會話 (短暫)
    Agent,    // 特定 Agent 實例
}
```

#### 2.2 SearchQuery & HybridWeights

```rust
// crates/memory-core/src/models/query.rs

use serde::{Deserialize, Serialize};
use crate::models::memory::{MemoryCategory, MemoryScope};

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
```

#### 2.3 ExtractionResult（LLM 輸出契約）

```rust
// crates/memory-core/src/extraction/engine.rs

/// LLM Single-Pass 提取結果（嚴格對應 LLM JSON 輸出格式）
#[derive(Debug, Serialize, Deserialize)]
pub struct ExtractionResponse {
    pub memories: Vec<ExtractedMemory>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExtractedMemory {
    /// 原子性事實（自包含、第三人稱）
    pub content: String,
    /// 類別
    pub category: MemoryCategory,
    /// 命名實體列表
    pub entities: Vec<String>,
    /// LLM 評估重要性 [1, 5]
    pub importance: u8,
    /// LLM 信心度 [0.0, 1.0]
    pub confidence: f64,
}
```

---

### 3. SQLite Schema（V1__init.sql）

```sql
-- crates/memory-core/src/storage/migrations/V1__init.sql

-- 高效能配置
PRAGMA journal_mode = WAL;       -- Write-Ahead Logging: 允許並發讀
PRAGMA synchronous = NORMAL;     -- WAL 模式下安全
PRAGMA foreign_keys = ON;
PRAGMA temp_store = MEMORY;
PRAGMA mmap_size = 268435456;    -- 256MB memory-mapped I/O

-- ─────────────────────────────────
-- 核心記憶表
-- ─────────────────────────────────
CREATE TABLE IF NOT EXISTS memories (
    id                  TEXT    PRIMARY KEY,          -- UUID v4
    content             TEXT    NOT NULL,
    category            TEXT    NOT NULL,             -- MemoryCategory as TEXT
    scope               TEXT    NOT NULL DEFAULT 'Global',
    project_id          TEXT,
    agent_id            TEXT,
    source_session      TEXT    NOT NULL,
    created_at          INTEGER NOT NULL,             -- Unix timestamp ms
    updated_at          INTEGER NOT NULL,
    last_accessed_at    INTEGER NOT NULL,
    access_count        INTEGER NOT NULL DEFAULT 0,
    importance_score    REAL    NOT NULL DEFAULT 0.5, -- [0.0, 1.0]
    retention_factor    REAL    NOT NULL DEFAULT 1.0, -- [0.0, 1.0]
    entities            TEXT    NOT NULL DEFAULT '[]',-- JSON array
    vector_id           INTEGER NOT NULL,             -- USearch index ID
    metadata            TEXT    NOT NULL DEFAULT '{}'-- JSON object
) STRICT;

-- 查詢最佳化索引
CREATE INDEX IF NOT EXISTS idx_mem_scope_project
    ON memories (scope, project_id);
CREATE INDEX IF NOT EXISTS idx_mem_category
    ON memories (category);
CREATE INDEX IF NOT EXISTS idx_mem_created_at
    ON memories (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_mem_importance
    ON memories (importance_score DESC);
CREATE INDEX IF NOT EXISTS idx_mem_retention
    ON memories (retention_factor DESC);
CREATE INDEX IF NOT EXISTS idx_mem_vector_id
    ON memories (vector_id);

-- ─────────────────────────────────
-- 實體索引表（Entity Linking 用）
-- ─────────────────────────────────
CREATE TABLE IF NOT EXISTS entities (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    aliases     TEXT    NOT NULL DEFAULT '[]',  -- JSON array of alias strings
    memory_ids  TEXT    NOT NULL DEFAULT '[]',  -- JSON array of memory UUIDs
    frequency   INTEGER NOT NULL DEFAULT 1,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_entity_name ON entities (name);

-- ─────────────────────────────────
-- 會話統計表
-- ─────────────────────────────────
CREATE TABLE IF NOT EXISTS session_stats (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id          TEXT    NOT NULL UNIQUE,
    project_id          TEXT,
    memories_extracted  INTEGER NOT NULL DEFAULT 0,
    memories_added      INTEGER NOT NULL DEFAULT 0,
    memories_deduplicated INTEGER NOT NULL DEFAULT 0,
    memories_retrieved  INTEGER NOT NULL DEFAULT 0,
    started_at          INTEGER NOT NULL,
    ended_at            INTEGER,
    total_tokens_used   INTEGER NOT NULL DEFAULT 0
) STRICT;

-- ─────────────────────────────────
-- 系統配置表（版本/維度等）
-- ─────────────────────────────────
CREATE TABLE IF NOT EXISTS system_config (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

-- 初始化向量維度記錄（首次寫入 embedding 時更新）
INSERT OR IGNORE INTO system_config (key, value)
VALUES
    ('schema_version', '1'),
    ('vector_dimensions', '1536'),   -- text-embedding-3-small
    ('embedding_model', 'unknown');
```

---

### 4. Extraction Engine 規格

#### 4.1 Single-Pass Extraction Prompt（System）

```rust
// crates/memory-core/src/extraction/prompt.rs

/// Single-Pass Extraction System Prompt
/// 設計原則：一次 LLM 呼叫提取所有記憶，嚴格 JSON 輸出
pub const EXTRACTION_SYSTEM_PROMPT: &str = r#"
You are a precision memory extraction system for an AI coding assistant.
Extract discrete, atomic memories from the provided conversation.

OUTPUT RULES (CRITICAL):
- Respond ONLY with valid JSON. No markdown, no preamble.
- Schema: {"memories": [{...}, ...]}

MEMORY OBJECT SCHEMA:
{
  "content": "<atomic, self-contained fact in third person>",
  "category": "<Fact|Preference|Decision|ProjectKnowledge|CodePattern|ErrorLesson|Workflow>",
  "entities": ["<entity1>", "<entity2>"],
  "importance": <integer 1-5>,
  "confidence": <float 0.0-1.0>
}

EXTRACTION RULES:
1. ATOMIC: Each memory = exactly one fact/preference/decision
2. SELF-CONTAINED: Understandable without the conversation context
3. THIRD PERSON: "User prefers X" not "I prefer X"
4. DECISIONS include rationale: "Decided to use X instead of Y because Z"
5. CODE PATTERNS include language/framework: "In Rust/tokio, user uses..."
6. SKIP: Greetings, trivial exchanges, temporary debugging steps
7. IMPORTANCE scoring:
   - 5: Critical architecture/irreversible decisions
   - 4: Strong preferences, key project facts
   - 3: Useful patterns and conventions
   - 2: Minor preferences
   - 1: Low-value ephemeral facts (usually skip)
8. Extract ALL qualifying memories in ONE pass
9. If nothing worth remembering, return: {"memories": []}
"#;

/// User prompt template
pub fn extraction_user_prompt(conversation: &str) -> String {
    format!(
        "Extract memories from this conversation:\n\n---\n{}\n---",
        conversation
    )
}
```

#### 4.2 ExtractionEngine 實作骨架

```rust
// crates/memory-core/src/extraction/engine.rs

pub struct ExtractionConfig {
    pub model: String,          // 預設: "claude-sonnet-4-6" 或 local model
    pub max_tokens: u32,        // 預設: 2048
    pub temperature: f32,       // 預設: 0.1 (低溫以保持一致性)
    pub min_confidence: f64,    // 過濾閾值，預設 0.60
    pub min_importance: u8,     // 過濾閾值，預設 2
}

pub struct ExtractionEngine {
    llm_client: Arc<LlmClient>,  // OpenAI-compatible HTTP client
    embedder: Arc<Embedder>,     // Embedding client
    config: ExtractionConfig,
}

impl ExtractionEngine {
    /// 單次 LLM 呼叫提取所有記憶（Single-Pass）
    pub async fn extract(&self, conversation: &str) -> Result<Vec<ExtractedMemory>> {
        let user_prompt = extraction_user_prompt(conversation);
        
        // LLM 單次呼叫
        let raw_json = self.llm_client
            .complete(EXTRACTION_SYSTEM_PROMPT, &user_prompt, &self.config)
            .await?;
        
        // 解析 JSON，容錯處理 markdown fence
        let cleaned = raw_json.trim()
            .trim_start_matches("```json")
            .trim_end_matches("```")
            .trim();
        
        let response: ExtractionResponse = serde_json::from_str(cleaned)
            .map_err(|e| MemoryError::ExtractionParseFailed(e.to_string()))?;
        
        // 品質過濾
        let filtered: Vec<_> = response.memories
            .into_iter()
            .filter(|m| m.confidence >= self.config.min_confidence)
            .filter(|m| m.importance >= self.config.min_importance)
            .filter(|m| !m.content.trim().is_empty())
            .collect();
        
        Ok(filtered)
    }

    /// 非同步產生 embedding 向量
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.embedder.embed(text).await
    }
}
```

---

### 5. Consolidation Engine 規格

#### 5.1 ADD-only 鞏固演算法

```
ALGORITHM: ADD-only Consolidation
─────────────────────────────────────────────────────
INPUT:  extracted: Vec<ExtractedMemory>
        已有向量索引 (USearch) + SQLite

FOR each mem IN extracted:

  1. embed(mem.content) → vec_new

  2. candidates = usearch.search(vec_new, k=5)

  3. FOR each candidate IN candidates:
       cosine = cosine_similarity(vec_new, candidate.vector)

       CASE cosine > DEDUP_THRESHOLD (0.92):
         → 完全重複 (Exact Duplicate)
         → UPDATE memories SET
             access_count = access_count + 1,
             last_accessed_at = now()
           WHERE id = candidate.id
         → CONTINUE to next mem (不新增)

       CASE 0.75 < cosine <= 0.92:
         → 近似重複 (Near Duplicate)
         → 比較 entities 重疊度
         → IF entity_overlap > 0.5:
             → 視為同義，執行 access_count++ 同上
             → CONTINUE
         → ELSE:
             → 視為相關但不重複，繼續 step 4

       CASE cosine <= 0.75:
         → 視為新記憶，繼續 step 4

  4. 計算 importance_score:
       llm_score = mem.importance / 5.0
       importance_score = clamp(
         0.5 * llm_score + 0.3 * 0.0 + 0.2 * 1.0,  // 新記憶 access=0, recency=1
         0.0, 1.0
       )

  5. 取得下一個 vector_id (USearch append)

  6. INSERT INTO memories (...) VALUES (...)
     usearch.add(vector_id, vec_new)
     tantivy.add_document(id, content, category, entities)

  7. UPDATE entities table (entity linking)

OUTPUT: Vec<Memory> (新增的記憶)
─────────────────────────────────────────────────────
```

#### 5.2 Importance Score 公式

$$\text{importance\_score}(m) = \text{clamp}\!\left(w_1 \cdot s_{\text{llm}} + w_2 \cdot s_{\text{access}} + w_3 \cdot s_{\text{recency}},\; 0, 1\right)$$

| 符號 | 計算方式 | 說明 |
|------|---------|------|
| $w_1 = 0.5$ | 固定 | LLM 評估權重 |
| $w_2 = 0.3$ | 固定 | 存取頻率權重 |
| $w_3 = 0.2$ | 固定 | 近期性權重 |
| $s_{\text{llm}}$ | `importance / 5.0` | LLM 評分歸一化 |
| $s_{\text{access}}$ | $\min(1.0,\; \text{access\_count} / 10)$ | 存取次數歸一化 |
| $s_{\text{recency}}$ | $e^{-\lambda \cdot \Delta t_{\text{days}}}$，$\lambda = 0.001$ | 時間衰減 |

#### 5.3 Ebbinghaus 記憶衰減

$$R(t) = e^{-t \,/\, S}$$

| 符號 | 說明 |
|------|------|
| $R(t)$ | $t$ 天後的記憶保留率 `retention_factor` |
| $S$ | 穩定性係數；初始 $S_0 = \text{importance\_score} \times 30$（天） |
| 每次存取後 | $S \leftarrow S \times 1.2$（記憶強化） |
| 衰減任務 | 背景 Tokio task，每 24 小時執行一次 |
| 封存閾值 | `retention_factor < 0.1` → 標記 `metadata.archived = true` |

```rust
// crates/memory-core/src/consolidation/decay.rs

/// 計算 Ebbinghaus 衰減後的保留率
pub fn calculate_retention(
    stability_days: f64, // S: 穩定性係數
    elapsed_days: f64,   // t: 距最後存取天數
) -> f64 {
    // R(t) = e^(-t/S)
    (-elapsed_days / stability_days).exp().clamp(0.0, 1.0)
}

/// 每次存取後強化記憶 (stability 增加)
pub fn reinforce_stability(current_stability: f64) -> f64 {
    current_stability * 1.2
}

/// 初始穩定性係數
pub fn initial_stability(importance_score: f64) -> f64 {
    // 重要記憶有更長的穩定週期 (最長 30 天)
    importance_score * 30.0
}
```

---

### 6. Hybrid Retrieval Engine 規格

#### 6.1 評分融合公式

$$\text{score\_final}(d) = \alpha \cdot \hat{s}_{\text{sem}}(d) + \beta \cdot \hat{s}_{\text{bm25}}(d) + \gamma \cdot s_{\text{temp}}(d)$$

預設：$\alpha = 0.60$，$\beta = 0.30$，$\gamma = 0.10$

**Semantic Score（USearch HNSW）：**

$$\hat{s}_{\text{sem}}(d) = \frac{\vec{q} \cdot \vec{d}}{\|\vec{q}\|\,\|\vec{d}\|}$$（餘弦相似度，歸一化至 $[0,1]$）

**BM25 Score（Tantivy）：**

$$\text{BM25}(d, q) = \sum_{t \in q} \text{IDF}(t) \cdot \frac{f_{t,d} \cdot (k_1 + 1)}{f_{t,d} + k_1 \cdot \left(1 - b + b \cdot \frac{|d|}{\text{avgdl}}\right)}$$

參數：$k_1 = 1.2$，$b = 0.75$。Tantivy 原生 BM25 實作，最終分數 min-max 歸一化。

**Temporal Score（時間近似度）：**

$$s_{\text{temp}}(d) = e^{-\mu \cdot \Delta t_{\text{days}}}，\quad \mu = 0.05$$

$\Delta t$：查詢時刻距該記憶 `last_accessed_at` 的天數。

#### 6.2 RetrievalEngine 實作骨架

```rust
// crates/memory-core/src/retrieval/engine.rs

pub struct RetrievalEngine {
    semantic: SemanticRetriever,   // USearch HNSW
    bm25: Bm25Retriever,           // Tantivy
    embedder: Arc<Embedder>,
    sqlite: Arc<SqliteStore>,
    default_weights: HybridWeights,
}

impl RetrievalEngine {
    /// Hybrid 混合檢索主入口
    pub async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let weights = query.weights.clone().unwrap_or_else(|| self.default_weights.clone());

        // 1. 平行執行 Semantic + BM25 兩路檢索
        let fetch_k = query.top_k * 3; // 取較多候選，fusion 後再截取
        let query_vec = self.embedder.embed(&query.query).await?;

        let (sem_results, bm25_results) = tokio::try_join!(
            self.semantic.search(&query_vec, fetch_k),
            self.bm25.search(&query.query, fetch_k),
        )?;

        // 2. 從 SQLite 取得所有候選的完整 Memory 資料
        let candidate_ids: Vec<String> = sem_results.iter()
            .map(|(id, _)| id.clone())
            .chain(bm25_results.iter().map(|(id, _)| id.clone()))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let memories = self.sqlite.get_by_ids(&candidate_ids).await?;

        // 3. 融合評分
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut scored: Vec<SearchResult> = memories.into_iter().filter_map(|mem| {
            // 應用過濾器
            if !self.passes_filters(&mem, query) { return None; }

            let s_sem = sem_results.iter()
                .find(|(id, _)| id == &mem.id)
                .map(|(_, s)| *s).unwrap_or(0.0);

            let s_bm25 = bm25_results.iter()
                .find(|(id, _)| id == &mem.id)
                .map(|(_, s)| *s).unwrap_or(0.0);

            // Temporal score
            let days_since_access = (now_ms - mem.last_accessed_at) as f64 / 86_400_000.0;
            let s_temp = (-0.05 * days_since_access).exp();

            let score_final = weights.semantic * s_sem
                + weights.bm25 * s_bm25
                + weights.temporal * s_temp;

            Some(SearchResult {
                memory: mem,
                score_final,
                score_semantic: s_sem,
                score_bm25: s_bm25,
                score_temporal: s_temp,
            })
        }).collect();

        // 4. 降序排序，取 Top-K
        scored.sort_by(|a, b| b.score_final.partial_cmp(&a.score_final).unwrap());
        scored.truncate(query.top_k);

        // 5. 更新命中記憶的存取統計（非阻塞）
        let hit_ids: Vec<String> = scored.iter().map(|r| r.memory.id.clone()).collect();
        let sqlite = self.sqlite.clone();
        tokio::spawn(async move {
            let _ = sqlite.update_access_stats(&hit_ids).await;
        });

        Ok(scored)
    }
}
```

---

### 7. MCP Server 工具規格

#### 7.1 工具清單

| Tool Name | 說明 | Required Params |
|-----------|------|----------------|
| `add_memory` | 提取並儲存記憶（觸發 ExtractionEngine） | `content` |
| `search_memories` | Hybrid 混合檢索 | `query` |
| `get_memories` | 批量獲取記憶詳情 | 無（可選 filters） |
| `delete_memory` | 刪除指定記憶 | `id` |
| `consolidate_memories` | 觸發批量鞏固與 decay 更新 | 無 |
| `get_memory_stats` | 返回統計資訊 | 無 |

#### 7.2 完整 Tool Schema（JSON Schema）

```json
[
  {
    "name": "add_memory",
    "description": "Extract and store memories from conversation text using Single-Pass LLM extraction. Automatically deduplicates via ADD-only consolidation.",
    "inputSchema": {
      "type": "object",
      "required": ["content"],
      "properties": {
        "content": {
          "type": "string",
          "description": "Conversation text or fact to extract memories from"
        },
        "scope": {
          "type": "string",
          "enum": ["Global", "Project", "Session", "Agent"],
          "default": "Global"
        },
        "project_id": {
          "type": "string",
          "description": "Project path or ID (required when scope=Project)"
        },
        "session_id": {
          "type": "string"
        },
        "metadata": {
          "type": "object",
          "description": "Additional metadata key-value pairs"
        }
      }
    }
  },
  {
    "name": "search_memories",
    "description": "Hybrid semantic+BM25+temporal retrieval of relevant memories. Returns ranked results with score breakdown.",
    "inputSchema": {
      "type": "object",
      "required": ["query"],
      "properties": {
        "query": {
          "type": "string",
          "description": "Natural language search query"
        },
        "top_k": {
          "type": "integer",
          "default": 10,
          "minimum": 1,
          "maximum": 50
        },
        "scope": {
          "type": "string",
          "enum": ["Global", "Project", "Session", "Agent"]
        },
        "project_id": { "type": "string" },
        "categories": {
          "type": "array",
          "items": {
            "type": "string",
            "enum": ["Fact","Preference","Decision","ProjectKnowledge","CodePattern","ErrorLesson","Workflow"]
          }
        },
        "min_importance": {
          "type": "number",
          "minimum": 0.0,
          "maximum": 1.0
        },
        "weights": {
          "type": "object",
          "properties": {
            "semantic": { "type": "number" },
            "bm25": { "type": "number" },
            "temporal": { "type": "number" }
          }
        }
      }
    }
  },
  {
    "name": "get_memories",
    "description": "Retrieve memory records by IDs or list recent memories.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "ids": {
          "type": "array",
          "items": { "type": "string" }
        },
        "scope": { "type": "string" },
        "project_id": { "type": "string" },
        "limit": { "type": "integer", "default": 20 }
      }
    }
  },
  {
    "name": "delete_memory",
    "description": "Delete a memory by ID. Use with caution — prefer decay archival for most cases.",
    "inputSchema": {
      "type": "object",
      "required": ["id"],
      "properties": {
        "id": { "type": "string", "description": "Memory UUID to delete" }
      }
    }
  },
  {
    "name": "consolidate_memories",
    "description": "Trigger batch consolidation: deduplication, decay update, and index compaction.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "scope": { "type": "string" },
        "project_id": { "type": "string" }
      }
    }
  },
  {
    "name": "get_memory_stats",
    "description": "Return memory system statistics: total count, category breakdown, index health.",
    "inputSchema": {
      "type": "object",
      "properties": {}
    }
  }
]
```

#### 7.3 MCP Server main.rs 骨架

```rust
// crates/memory-mcp-server/src/main.rs

use memory_core::{config::MemoryConfig, service::MemoryService};
use std::sync::Arc;
use tracing_subscriber::fmt::format::FmtSpan;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 日誌輸出至 stderr（MCP stdio 協議要求 stdout 乾淨）
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    tracing::info!("Memory MCP Server starting...");

    // 從環境變數讀取配置
    let config = MemoryConfig::from_env()?;
    tracing::info!("DB path: {}", config.db_path);

    // 初始化 Memory Service（含 SQLite migrate + USearch init + Tantivy init）
    let service = Arc::new(MemoryService::new(config).await?);
    tracing::info!("Memory service initialized");

    // 啟動 MCP Server，監聽 stdin/stdout
    let server = server::MemoryMcpServer::new(service);
    server.serve_stdio().await?;

    Ok(())
}
```

---

### 8. Plugin TypeScript Shim 規格

```typescript
// plugin/src/index.ts
// TypeScript 薄包裝層 — 所有業務邏輯委派至 Rust MCP Server
// 原則：此檔案不包含任何記憶邏輯，僅作 lifecycle event → MCP tool call 橋接

interface ChatContext {
  projectPath?: string;
  projectId?: string;
  initialQuery?: string;
  mcp: McpClient;
  injectSystemPrompt: (text: string) => void;
}

interface MessageContext {
  userMessage: string;
  assistantMessage: string;
  projectId?: string;
  sessionId: string;
  mcp: McpClient;
}

interface SessionContext {
  projectId?: string;
  sessionId: string;
  mcp: McpClient;
}

interface Memory {
  id: string;
  content: string;
  category: string;
  importance_score: number;
  score_final?: number;
}

interface McpClient {
  call(tool: string, params: Record<string, unknown>): Promise<unknown>;
}

// ─────────────────────────────────────────────────────────────
// OpenCode Plugin 主體
// ─────────────────────────────────────────────────────────────
export default {
  name: "opencode-memory",
  version: "1.0.0",

  hooks: {
    /**
     * 會話開始時：檢索相關記憶並注入 System Prompt
     * 目的：讓 Agent 從一開始就具備歷史上下文
     */
    onChatStart: async (ctx: ChatContext): Promise<void> => {
      try {
        const queryText = ctx.initialQuery ?? ctx.projectPath ?? "";
        if (!queryText) return;

        const result = await ctx.mcp.call("search_memories", {
          query: queryText,
          top_k: 10,
          scope: ctx.projectId ? "Project" : "Global",
          project_id: ctx.projectId,
          min_importance: 0.3,
        }) as { results?: Memory[] };

        const memories = result?.results ?? [];
        if (memories.length === 0) return;

        ctx.injectSystemPrompt(formatMemoriesForInjection(memories));
      } catch (err) {
        // 記憶注入失敗不應阻斷對話
        console.error("[opencode-memory] onChatStart error:", err);
      }
    },

    /**
     * 對話輪次完成後：非阻塞地提取並儲存新記憶
     * 使用 queueMicrotask 確保不阻塞 UI
     */
    onMessageComplete: async (ctx: MessageContext): Promise<void> => {
      // 非阻塞：背景執行，不等待結果
      queueMicrotask(async () => {
        try {
          const conversationTurn = [
            `User: ${ctx.userMessage}`,
            `Assistant: ${ctx.assistantMessage}`,
          ].join("\n\n");

          await ctx.mcp.call("add_memory", {
            content: conversationTurn,
            scope: ctx.projectId ? "Project" : "Global",
            project_id: ctx.projectId,
            session_id: ctx.sessionId,
          });
        } catch (err) {
          console.error("[opencode-memory] onMessageComplete error:", err);
        }
      });
    },

    /**
     * 會話結束時：觸發批量鞏固（去重 + decay 更新）
     */
    onSessionEnd: async (ctx: SessionContext): Promise<void> => {
      try {
        await ctx.mcp.call("consolidate_memories", {
          scope: ctx.projectId ? "Project" : "Global",
          project_id: ctx.projectId,
        });
      } catch (err) {
        console.error("[opencode-memory] onSessionEnd error:", err);
      }
    },
  },
};

/**
 * 格式化記憶列表為 System Prompt 注入格式
 * 精簡清晰，避免佔用過多 token
 */
function formatMemoriesForInjection(memories: Memory[]): string {
  const lines = memories.map((m, i) =>
    `${i + 1}. [${m.category}] ${m.content}`
  );
  return [
    "## Relevant Memory Context",
    "(From past sessions — use as background context)",
    ...lines,
    "",
  ].join("\n");
}
```

---

### 9. OpenCode 配置規格

#### 9.1 ~/.config/opencode/config.json

```json
{
  "mcp": {
    "opencode-memory": {
      "type": "local",
      "command": [
        "${HOME}/.cargo/bin/memory-mcp-server"
      ],
      "env": {
        "MEMORY_DB_PATH": "${PROJECT_ROOT}/.opencode/memory.db",
        "MEMORY_VECTOR_PATH": "${PROJECT_ROOT}/.opencode/vectors.usearch",
        "MEMORY_TANTIVY_PATH": "${PROJECT_ROOT}/.opencode/tantivy",
        "LLM_API_BASE": "http://localhost:8080/v1",
        "LLM_API_KEY": "local",
        "EMBEDDING_MODEL": "text-embedding-3-small",
        "EMBEDDING_DIM": "1536",
        "EXTRACTION_MODEL": "claude-sonnet-4-6",
        "EXTRACTION_MAX_TOKENS": "2048",
        "MEMORY_DEDUP_THRESHOLD": "0.92",
        "MEMORY_NEAR_DEDUP_THRESHOLD": "0.75",
        "MEMORY_TOP_K": "10",
        "MEMORY_DECAY_LAMBDA": "0.001",
        "MEMORY_DECAY_MU": "0.05",
        "MEMORY_MAX_RECORDS": "50000",
        "MEMORY_MIN_CONFIDENCE": "0.60",
        "MEMORY_MIN_IMPORTANCE": "2",
        "MEMORY_LOG_LEVEL": "info"
      }
    }
  },
  "plugins": [
    "${HOME}/.config/opencode/plugins/opencode-memory"
  ]
}
```

#### 9.2 完整環境變數規格

| 環境變數 | 預設值 | 說明 |
|---------|-------|------|
| `MEMORY_DB_PATH` | `.opencode/memory.db` | SQLite 資料庫路徑 |
| `MEMORY_VECTOR_PATH` | `.opencode/vectors.usearch` | USearch HNSW index 路徑 |
| `MEMORY_TANTIVY_PATH` | `.opencode/tantivy` | Tantivy index 目錄 |
| `LLM_API_BASE` | `https://api.anthropic.com/v1` | LLM API endpoint（相容 OpenAI format） |
| `LLM_API_KEY` | `"local"` | API 金鑰 |
| `EMBEDDING_MODEL` | `text-embedding-3-small` | Embedding 模型名稱 |
| `EMBEDDING_DIM` | `1536` | 向量維度（必須與模型一致） |
| `EXTRACTION_MODEL` | `claude-sonnet-4-6` | 記憶提取 LLM |
| `EXTRACTION_MAX_TOKENS` | `2048` | 提取最大 token 數 |
| `MEMORY_DEDUP_THRESHOLD` | `0.92` | 完全重複閾值 |
| `MEMORY_NEAR_DEDUP_THRESHOLD` | `0.75` | 近似重複閾值 |
| `MEMORY_TOP_K` | `10` | 預設檢索 Top-K |
| `MEMORY_DECAY_LAMBDA` | `0.001` | Importance decay 率（$\lambda$） |
| `MEMORY_DECAY_MU` | `0.05` | Temporal score decay 率（$\mu$） |
| `MEMORY_MAX_RECORDS` | `50000` | DB 最大記憶數量 |
| `MEMORY_MIN_CONFIDENCE` | `0.60` | 提取最低信心度 |
| `MEMORY_MIN_IMPORTANCE` | `2` | 提取最低重要性分數 |
| `MEMORY_LOG_LEVEL` | `info` | 日誌等級（輸出至 stderr） |

---

### 10. Cargo.toml 規格

#### workspace Cargo.toml

```toml
[workspace]
resolver = "2"
members = [
    "crates/memory-core",
    "crates/memory-mcp-server",
    "crates/memory-cli",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
authors = ["OpenCode Memory System"]
license = "MIT"

[workspace.dependencies]
# ── Async Runtime ─────────────────────────────────────────────
tokio            = { version = "1", features = ["full"] }

# ── Serialization ─────────────────────────────────────────────
serde            = { version = "1", features = ["derive"] }
serde_json       = "1"

# ── Database ──────────────────────────────────────────────────
# sqlx: async SQL toolkit, SQLite feature + WAL support
sqlx             = { version = "0.8", features = [
                     "sqlite", "runtime-tokio", "macros", "migrate"
                   ] }

# ── Vector Search (HNSW) ──────────────────────────────────────
# usearch: Unum Cloud HNSW, 純 Rust binding
usearch          = { version = "2", features = [] }

# ── Full-Text Search (BM25) ───────────────────────────────────
tantivy          = { version = "0.22", default-features = true }

# ── MCP Protocol ──────────────────────────────────────────────
# rmcp: 官方 Model Context Protocol Rust SDK
rmcp             = { version = "0.1", features = ["server", "transport-io"] }

# ── HTTP Client (LLM API) ─────────────────────────────────────
reqwest          = { version = "0.12", features = ["json"] }

# ── Error Handling ────────────────────────────────────────────
anyhow           = "1"
thiserror        = "1"

# ── Logging / Tracing ─────────────────────────────────────────
tracing          = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# ── Utilities ─────────────────────────────────────────────────
uuid             = { version = "1", features = ["v4"] }
chrono           = { version = "0.4", features = ["serde"] }

[profile.release]
opt-level = 3
lto = "fat"         # Link-Time Optimization: 最大化二進制性能
codegen-units = 1
strip = true        # 移除 debug symbols，縮小二進制
```

#### crates/memory-core/Cargo.toml

```toml
[package]
name = "memory-core"
version.workspace = true
edition.workspace = true

[dependencies]
tokio        = { workspace = true }
serde        = { workspace = true }
serde_json   = { workspace = true }
sqlx         = { workspace = true }
usearch      = { workspace = true }
tantivy      = { workspace = true }
reqwest      = { workspace = true }
anyhow       = { workspace = true }
thiserror    = { workspace = true }
tracing      = { workspace = true }
uuid         = { workspace = true }
chrono       = { workspace = true }

[dev-dependencies]
tokio-test   = "0.4"
tempfile     = "3"   # 測試用暫存 SQLite DB
```

#### crates/memory-mcp-server/Cargo.toml

```toml
[package]
name = "memory-mcp-server"
version.workspace = true
edition.workspace = true

[[bin]]
name = "memory-mcp-server"
path = "src/main.rs"

[dependencies]
memory-core         = { path = "../memory-core" }
rmcp                = { workspace = true }
tokio               = { workspace = true }
serde               = { workspace = true }
serde_json          = { workspace = true }
anyhow              = { workspace = true }
tracing             = { workspace = true }
tracing-subscriber  = { workspace = true }
```

---

### 11. Skills 規格

#### skills/memory-extraction.md

```markdown
# Memory Extraction Skill

## 用途
當需要手動觸發高品質記憶提取或查詢歷史上下文時使用。

## 觸發條件
- 用戶說「記住這個」/ 「remember this」/ 「add to memory」
- 複雜架構決策討論完成後
- 重要代碼模式確認後
- 需要查詢「我們之前決定...」類問題

## 工作流程

### A. 手動提取（add）
1. 先用 `search_memories` 確認無重複
2. 用 `add_memory` 儲存
3. 回報已儲存的記憶條數

### B. 查詢歷史（search）
1. 分析用戶意圖，構建精確查詢
2. 用 `search_memories` 檢索
3. 整理呈現相關記憶

## 提示模板

\`\`\`
[MEMORY_EXTRACTION_TASK]
從以下內容提取重要記憶，依序執行：

1. 先用 search_memories 確認無重複：
   query: <核心事實關鍵詞>

2. 若無重複，用 add_memory 儲存：
   content: <完整對話或事實描述>
   scope: Project (若有 project_id) 或 Global

提取標準：
- 每條記憶必須原子性（單一事實）
- 偏好用第三人稱（"User prefers..."）
- 決策包含理由（"Decided to use X because Y"）
- 代碼模式包含語言和框架
- 最多提取 5 條最重要的記憶
\`\`\`

## 輸出說明
- 呼叫 MCP tools: search_memories → add_memory
- 回報：「已儲存 N 條新記憶」或「與現有記憶重複，跳過」
```

---

### 12. 測試規格

#### 12.1 單元測試覆蓋率目標

| 模組 | 核心測試項目 | 目標覆蓋率 |
|------|------------|---------|
| `extraction` | prompt 格式、JSON 解析、confidence 過濾、edge cases | ≥ 90% |
| `consolidation` | dedup 閾值、entity linking、decay 計算、ADD-only 約束 | ≥ 85% |
| `retrieval` | semantic/BM25/temporal 各分項、融合算法、過濾器 | ≥ 85% |
| `storage` | CRUD 完整性、索引一致性、migration | ≥ 95% |
| `mcp-server` | tool schema 驗證、輸入邊界、錯誤回應 | ≥ 80% |

#### 12.2 整合測試場景

```rust
// tests/integration/lifecycle_test.rs

/// 完整記憶生命週期 e2e 測試
#[tokio::test]
async fn test_full_memory_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let service = MemoryService::new_test(tmp.path()).await.unwrap();

    // 1. 模擬對話輸入
    let conversation = "User: I prefer using tokio::spawn for background tasks in Rust.\n\
                        Assistant: Good practice. I'll remember that preference.";

    // 2. 提取並儲存
    let added = service.add_memory(conversation, Default::default()).await.unwrap();
    assert!(!added.is_empty(), "Should extract at least one memory");

    // 3. 驗證 ADD-only：相同內容不新增
    let added2 = service.add_memory(conversation, Default::default()).await.unwrap();
    assert!(added2.is_empty(), "Duplicate should be deduplicated");

    // 4. 驗證 Hybrid 檢索
    let results = service.search_memories(&SearchQuery {
        query: "Rust async background task preference".to_string(),
        top_k: 5,
        ..Default::default()
    }).await.unwrap();
    assert!(!results.is_empty(), "Should find the stored memory");
    assert!(results[0].score_final > 0.5, "Score should be significant");

    // 5. 驗證鞏固
    service.consolidate_memories(None, None).await.unwrap();
}

/// ADD-only 去重精確測試
#[tokio::test]
async fn test_dedup_threshold_precision() {
    // 語義完全相同 → 去重
    // 語義相關但不同 → 保留
    // ...
}

/// Hybrid 排名驗證
#[tokio::test]
async fn test_hybrid_retrieval_ranking() {
    // 插入多條記憶（semantic 相關 vs keyword 相關 vs recent）
    // 驗證 hybrid score 排名符合預期優先序
    // ...
}
```

#### 12.3 性能 Benchmark 目標

| 指標 | 目標 | 測試條件 |
|------|------|---------|
| `add_memory` 端對端延遲（含 LLM 提取） | < 500ms | 不含網路延遲 |
| `add_memory` 純儲存延遲（無 LLM） | < 10ms | 本地 USearch + SQLite |
| `search_memories` 延遲 (10K records) | < 50ms | Hybrid 三路並行 |
| `search_memories` 延遲 (100K records) | < 200ms | HNSW 對數複雜度 |
| `consolidate_memories` batch (100 records) | < 5s | 含 dedup + decay |
| 記憶體佔用 / 10K records | < 100MB | SQLite + USearch |
| DB 磁碟佔用 / 1K records | < 5MB | 含向量索引 |

---

## Part III：AGENTS.md（AI 實現指南）

```markdown
# OpenCode Memory System — AGENTS.md
# AI Coding Agent 實現指南 v1.0

## 專案概覽
實現一套 Rust 長期記憶系統，架構為 Cargo workspace + MCP Server + TypeScript Plugin shim。
詳細規格見 `docs/spec.md`（本文件）。

## 執行優先順序
Phase 1 (memory-core 基礎) → Phase 2 (MCP server) → Phase 3 (plugin) → Phase 4 (hybrid retrieval)
每個 Phase 完成後必須通過對應測試才能進入下一個。

## 🚫 禁止事項（CRITICAL）
- 不得使用 Python 或 Node.js 實現任何核心邏輯
- 不得引入無 Rust 版本的 C/C++ 依賴（除 usearch 已知 C++ 核心外）
- 不得 UPDATE/DELETE memories.content（ADD-only 原則，違反即為 bug）
- MCP Server 的 stdout 必須乾淨（JSON-RPC only），日誌一律輸出 stderr
- 不得在 Plugin TS shim 中寫入任何記憶邏輯（純委派 MCP）

## 實現注意事項

### Storage
1. SQLite: 建立連接後第一件事執行 `PRAGMA journal_mode = WAL`
2. USearch: 初始化時必須指定 `dimensions`，從 env `EMBEDDING_DIM` 讀取
3. Tantivy: IndexWriter 必須有 commit 後才能 search

### Extraction
4. LLM response 可能包含 ```json fence，解析前必須 strip
5. extraction 失敗（LLM timeout/parse error）需 graceful degradation，不中斷會話

### Consolidation
6. Dedup 閾值從 env `MEMORY_DEDUP_THRESHOLD` 讀取，不硬編碼
7. access_count / last_accessed_at 的 UPDATE 不算違反 ADD-only 原則

### MCP Server
8. rmcp crate tool handler 錯誤回應格式：`{"error": {"code": -32603, "message": "..."}}`
9. 啟動時立即輸出 `{"jsonrpc":"2.0","method":"initialized",...}` 至 stderr

### Tests
10. 所有測試使用 `tempfile::tempdir()` 產生獨立 DB，禁止共享全域狀態
11. integration test 需 mock LLM client（不發真實 HTTP 請求）

## Git 提交規範
遵循 Conventional Commits:
- feat(extraction): add single-pass LLM extraction
- fix(consolidation): correct dedup threshold comparison
- test(retrieval): add hybrid score fusion test
- refactor(storage): extract SqliteStore trait

## 完成標準
- [ ] `cargo test --workspace` 全過
- [ ] `cargo clippy --workspace -- -D warnings` 零警告
- [ ] `cargo build --release` 成功，binary < 20MB
- [ ] MCP Server 能被 OpenCode 載入並回應 tool list
- [ ] Plugin lifecycle hooks 能正確委派至 MCP
```

---

## 附錄：快速啟動指令

```bash
# 1. 建立 workspace
cargo new --lib opencode-memory
cd opencode-memory

# 2. 初始化子 crates
cargo new --lib crates/memory-core
cargo new --bin crates/memory-mcp-server
cargo new --bin crates/memory-cli

# 3. 建置
cargo build --release

# 4. 安裝 MCP Server 二進制
cargo install --path crates/memory-mcp-server

# 5. 初始化 Plugin（TypeScript）
cd plugin && npm install && npm run build

# 6. 執行測試
cargo test --workspace

# 7. 啟動 MCP Server（測試用）
MEMORY_DB_PATH=./test.db \
MEMORY_VECTOR_PATH=./test.usearch \
MEMORY_TANTIVY_PATH=./test-tantivy \
LLM_API_BASE=http://localhost:8080/v1 \
LLM_API_KEY=local \
./target/release/memory-mcp-server

# 8. CLI 測試
./target/release/memory-cli search "Rust async preference"
./target/release/memory-cli stats
```

---

*文件版本：v1.0 | 語言：Rust 2021 + TypeScript 5 | 最後更新：2026-06*
