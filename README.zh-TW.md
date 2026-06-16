# Memlong

<div align="right">

[English](README.md) | **繁體中文**

</div>

本地端優先（local-first）的長期記憶系統，專為程式碼代理人（coding agents）設計。跨工作階段儲存事實、偏好、決策、程式碼模式與專案知識。採用混合語意 + BM25 + 時間排序檢索。

核心以 Rust 實作，以 MCP 伺服器形式提供服務。另有 TypeScript 適配層可為 OpenCode 提供自動化生命週期鉤子。

## 快速開始

```bash
# 編譯
git clone https://github.com/stevenke1981/memlong.git
cd memlong
cargo build --release

# 安裝
./install.sh --from-source

# 設定環境變數
export LLM_API_BASE="http://localhost:8080/v1"
export LLM_API_KEY="local"
export EXTRACTION_MODEL="your-chat-model"
export EMBEDDING_MODEL="your-embedding-model"
export EMBEDDING_DIM="1536"

# 驗證
./target/release/memory-mcp-server health
```

### CLI 偵錯

```bash
cargo run -p memory-cli -- add --content "使用者偏好使用 Rust 開發核心服務"
cargo run -p memory-cli -- search --query "偏好的實作語言"
cargo run -p memory-cli -- list
cargo run -p memory-cli -- stats
cargo run -p memory-cli -- consolidate
```

## 設定參數

| 變數 | 預設值 | 說明 |
|------|--------|------|
| `LLM_API_BASE` | `http://localhost:8080/v1` | OpenAI 相容端點 |
| `LLM_API_KEY` | `local` | API 金鑰 |
| `EXTRACTION_MODEL` | `llama-3-8b` | 提取用聊天模型 |
| `EMBEDDING_MODEL` | `text-embedding-3-small` | 嵌入模型 |
| `EMBEDDING_DIM` | `1536` | 嵌入維度（須與模型一致） |
| `PROJECT_ROOT` | 目前目錄 | `.opencode/` 資料目錄的根路徑 |
| `MEMORY_DB_PATH` | `.opencode/memory.db` | SQLite 路徑 |
| `MEMORY_VECTOR_PATH` | `.opencode/vectors.usearch` | USearch 索引路徑 |
| `MEMORY_TANTIVY_PATH` | `.opencode/tantivy` | Tantivy 索引目錄 |
| `MEMORY_DEDUP_THRESHOLD` | `0.92` | 精確重複餘弦閾值 |
| `MEMORY_NEAR_DEDUP_THRESHOLD` | `0.75` | 近似重複餘弦閾值 |
| `MEMORY_MAX_RECORDS` | `50000` | 記憶上限 |
| `MEMORY_DECAY_LAMBDA` | `0.001` | 重要性隨時間衰減率 |
| `MEMORY_TEMPORAL_MU` | `0.05` | 檢索時間衰減率 |

## MCP 工具

| 工具 | 用途 |
|------|------|
| `add_memory` | 從文字提取並儲存記憶 |
| `search_memories` | 混合語意 + BM25 + 時間檢索 |
| `get_memories` | 依 ID 或過濾條件查詢 |
| `delete_memory` | 刪除記憶並清除所有索引 |
| `consolidate_memories` | 執行衰減、去重與壓縮 |
| `get_memory_stats` | 統計資料 |
| `end_session` | 標記工作階段結束 |

## 代理人開發指南

### 協定

1. **僅新增（ADD-only）**：記憶內容不可變，僅更新存取統計、保留率、重要性與歸檔標記。
2. **Rust 核心，TypeScript 僅適配**：記憶邏輯在 `memory-core`，`plugin/` 僅做生命週期橋接。
3. **索引一致性**：SQLite、USearch、Tantivy 與實體連結必須在每次插入與刪除後保持一致。
4. **MCP 協定**：stdout 保留給 JSON-RPC，診斷資訊走 stderr。
5. **範圍隔離**：重複偵測須同時檢查 scope 與 project 邊界。
6. **測試禁用真實 LLM**：所有測試使用 `api_key = "mock"`。

### 架構

```
MCP Client → memory-mcp-server → memory-core
                                    ├── SQLite（元資料、實體、統計）
                                    ├── USearch HNSW（向量索引）
                                    ├── Tantivy BM25（全文索引）
                                    ├── extraction/（LLM 提取 + 嵌入）
                                    ├── consolidation/（去重、實體連結、艾賓豪斯衰減）
                                    └── retrieval/（混合排序、過濾）
```

預設資料位置：`.opencode/`

### 主要程式碼路徑

| 路徑 | 職責 |
|------|------|
| `crates/memory-core/src/service.rs` | 高階流程編排 |
| `crates/memory-core/src/extraction/` | LLM 提取與嵌入 |
| `crates/memory-core/src/consolidation/` | 去重、實體連結、艾賓豪斯衰減 |
| `crates/memory-core/src/retrieval/` | 混合排序與過濾 |
| `crates/memory-core/src/storage/` | SQLite、USearch、Tantivy 適配層 |
| `crates/memory-mcp-server/src/server.rs` | MCP 工具結構與處理器 |
| `plugin/src/index.ts` | OpenCode 生命週期橋接 |

### 驗證

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
cargo bench -p memory-core
cd plugin && npm ci && npm test
```

## 文件

- [產品規格](opencode-memory-system.md)
- [技術規格](spec.md)
- [實作狀態](task.md)
- [經驗教訓](lessons.md)

## 解除安裝

```bash
# 移除二進位檔與 MCP 設定（保留記憶資料）
./uninstall.sh

# 移除全部（包含已儲存的記憶）
./uninstall.sh --remove-data
```

## 授權條款

MIT
