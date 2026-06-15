# Memlong

<div align="right">

[English](README.md) | **繁體中文**

</div>

Memlong 是一個以本地端優先（local-first）的長期記憶系統，專為程式碼代理人（coding agents）設計。它能跨工作階段儲存持久的事實、偏好設定、決策、程式碼模式及專案知識，並透過混合語意（hybrid semantic）、關鍵字（keyword）與時間排序（temporal ranking）來檢索相關記憶。

核心以 Rust 實作，並以 MCP 伺服器（Model Context Protocol server）形式提供服務。另有一個輕量的 TypeScript 適配層，可為 OpenCode 提供自動化生命週期鉤子，用於自動檢索與捕獲記憶。

---

## 功能特色

- **單次 LLM 記憶提取**：附帶信心度與重要性過濾
- **僅新增（ADD-only）合併策略**：支援精確重複與近似重複偵測
- **多層級記憶範圍**：專案（project）、全域（global）、工作階段（session）、代理人（agent）
- **USearch HNSW 向量搜尋**：搭配持久化的本地索引
- **Tantivy BM25 全文檢索**：高效關鍵字匹配
- **艾賓豪斯遺忘曲線啟發**：記憶衰減與存取強化機制
- **MCP 工具**：提供新增、搜尋、列出、刪除、合併與檢查記憶的完整工具組
- **OpenCode 外掛**：輕量的工作階段注入與自動捕獲

---

## 系統架構

```text
OpenCode / Codex / MCP Client
          |
          | JSON-RPC over stdio
          v
memory-mcp-server
          |
          v
memory-core
  |-- SQLite 元資料與實體
  |-- USearch HNSW 向量
  |-- Tantivy BM25 索引
  `-- 提取、合併、檢索、衰減
```

預設的專案資料儲存於 `.opencode/` 目錄下：

```text
.opencode/
|-- memory.db          # SQLite 資料庫
|-- vectors.usearch    # USearch 向量索引
`-- tantivy/           # Tantivy 全文檢索索引
```

---

## 快速開始（一般使用者）

### 系統需求

- **Windows:** Windows 10 或 11，Rust stable（含 MSVC 工具鏈），Visual Studio Build Tools
- **Linux:** glibc 2.31+，Rust stable（僅原始碼編譯時需要）
- **macOS:** macOS 12+，Rust stable（僅原始碼編譯時需要）
- Node.js 18+（僅在編譯或測試 OpenCode 外掛時需要）
- OpenAI 相容的聊天補全（chat completions）與嵌入式（embeddings）端點

### 從原始碼編譯

```bash
git clone https://github.com/stevenke1981/memlong.git
cd memlong
cargo build --release
```

編譯完成後，MCP 伺服器位於：

| 平台 | 路徑 |
|------|------|
| Windows | `target\release\memory-mcp-server.exe` |
| Linux / macOS | `target/release/memory-mcp-server` |

### 在 Windows 上安裝

從原始碼建置並安裝：

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1 -FromSource
```

安裝已發行的版本：

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1 -Version v0.1.0
```

### 在 Linux 上安裝

從原始碼建置並安裝：

```bash
./install.sh --from-source
```

安裝已發行的版本：

```bash
./install.sh --version v0.1.0
```

### 在 macOS 上安裝

從原始碼建置並安裝：

```bash
./install.sh --from-source
```

安裝已發行的版本：

```bash
./install.sh --version v0.1.0
```

安裝程式會將執行檔放置於 `~/.config/opencode-memory/bin`（Linux/macOS）或 `%USERPROFILE%\.config\opencode-memory\bin`（Windows），並自動執行 `install` 指令來設定支援的 MCP 用戶端。完成後請重新啟動您的 MCP 用戶端。

### 設定模型

啟動 MCP 伺服器前，請先設定 OpenAI 相容的端點：

**Windows (PowerShell):**
```powershell
$env:LLM_API_BASE = "http://localhost:8080/v1"
$env:LLM_API_KEY = "local"
$env:EXTRACTION_MODEL = "your-chat-model"
$env:EMBEDDING_MODEL = "your-embedding-model"
$env:EMBEDDING_DIM = "1536"
```

**Linux / macOS (bash):**
```bash
export LLM_API_BASE="http://localhost:8080/v1"
export LLM_API_KEY="local"
export EXTRACTION_MODEL="your-chat-model"
export EMBEDDING_MODEL="your-embedding-model"
export EMBEDDING_DIM="1536"
```

重要選用設定：

| 變數 | 預設值 | 用途 |
| --- | --- | --- |
| `PROJECT_ROOT` | 目前目錄 | 用於 `.opencode` 資料目錄的根路徑 |
| `MEMORY_DB_PATH` | `.opencode/memory.db` | SQLite 資料庫路徑 |
| `MEMORY_VECTOR_PATH` | `.opencode/vectors.usearch` | USearch 索引路徑 |
| `MEMORY_TANTIVY_PATH` | `.opencode/tantivy` | Tantivy 索引目錄 |
| `MEMORY_DEDUP_THRESHOLD` | `0.92` | 精確重複的餘弦閾值 |
| `MEMORY_NEAR_DEDUP_THRESHOLD` | `0.75` | 近似重複的餘弦閾值 |
| `MEMORY_MIN_CONFIDENCE` | `0.60` | 最低提取信心度 |
| `MEMORY_MIN_IMPORTANCE` | `2` | LLM 重要性評分（1 至 5） |
| `MEMORY_DECAY_LAMBDA` | `0.001` | 重要性隨時間衰減率 |
| `MEMORY_DECAY_MU` | `0.05` | 檢索時間衰減率 |

`EMBEDDING_DIM` 必須與嵌入式模型一致。現有的向量索引與維度綁定。

### 健康檢查

```bash
# Windows (PowerShell)
.\target\release\memory-mcp-server.exe health

# Linux / macOS
./target/release/memory-mcp-server health
```

### 偵錯 CLI

```bash
# Windows (PowerShell) 或 Linux/macOS
cargo run -p memory-cli -- add --content "使用者偏好使用 Rust 開發核心服務"
cargo run -p memory-cli -- search --query "偏好的實作語言"
cargo run -p memory-cli -- list
cargo run -p memory-cli -- stats
cargo run -p memory-cli -- consolidate
```

### OpenCode 外掛

此外掛為輕量的生命週期適配層，記憶行為仍由 Rust 核心負責。

```bash
cd plugin
npm ci
npm run build
```

建置後的入口點為 `plugin/dist/index.js`。支援直接陣列、`{ results: [...] }` 以及 MCP text-content 回應格式。

---

## MCP 工具列表

| 工具 | 用途 |
| --- | --- |
| `add_memory` | 從文字中提取並儲存記憶 |
| `search_memories` | 混合語意、BM25 與時間檢索 |
| `get_memories` | 依 ID 或過濾條件取得記憶 |
| `delete_memory` | 刪除記憶並清除所有索引 |
| `consolidate_memories` | 執行範圍衰減與合併 |
| `get_memory_stats` | 回傳計數與索引健康狀態 |
| `end_session` | 標記工作階段結束（設定 ended_at 時間戳） |

搜尋權重必須為有限數值、不可為負數，且總和為 `1.0`。

---

## 代理人開發指南

在此儲存庫中工作的代理人應將 `opencode-memory-system.md` 視為權威產品規格，並遵守以下約定：

1. 核心記憶行為屬於 Rust。TypeScript 僅作為輕量的生命週期適配層。
2. 記憶內容為「僅新增」（ADD-only）。存取統計、保留策略、重要性與歸檔元資料可更新。
3. 重複偵測必須尊重範圍（scope）與專案邊界。
4. SQLite、USearch、Tantivy 與實體連結在插入或刪除後必須保持一致。
5. MCP stdout 保留給協定訊息；診斷資訊請使用 stderr。
6. 測試應使用臨時隔離的資料庫與索引，不得呼叫真實的 LLM 端點。

### 程式碼探索

此儲存庫已由 `codebase-memory-mcp` 以 `cbrlm+D-memlong` 為名建立索引。探索程式碼時，建議優先使用圖形工具再使用文字搜尋：

1. `search_graph` 或 `rlm_filter`
2. `trace_path`
3. `rlm_read_symbol` 或 `get_code_snippet`
4. `query_graph`
5. `get_architecture`

純文字搜尋或 grep 則用於設定檔、文件、字面錯誤訊息及其他非程式碼內容。在結構大幅變更後，請重新執行 `index_repository`。

### 主要程式碼路徑

| 路徑 | 職責 |
| --- | --- |
| `crates/memory-core/src/service.rs` | 高階流程編排 |
| `crates/memory-core/src/extraction/` | LLM 提取與嵌入式 |
| `crates/memory-core/src/consolidation/` | 重複偵測、實體連結、衰減 |
| `crates/memory-core/src/retrieval/` | 混合排序與過濾 |
| `crates/memory-core/src/storage/` | SQLite、USearch 與 Tantivy 適配層 |
| `crates/memory-mcp-server/src/server.rs` | MCP 結構定義與處理器 |
| `plugin/src/index.ts` | OpenCode 生命週期橋接 |

### 必要驗證指令

```bash
cargo fmt --all -- --check
cargo test --workspace          # 15+ 項測試，包含 MCP 協定煙霧測試
cargo bench -p memory-core      # Criterion 基準測試（add_memory、search_memories）
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
cd plugin
npm ci
npm test
```

發行版伺服器檔案大小應保持在規格文件中所述的 20 MB 目標以下。

---

## 打包

建立發行版壓縮檔與 SHA256 校驗檔：

**Windows：**
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -Version 0.1.0
```

**Linux：**
```bash
./scripts/package-release.sh --version 0.1.0
```

產出物位於 `target/` 目錄下。

---

## 文件

- 完整產品與技術規格：[`opencode-memory-system.md`](opencode-memory-system.md)
- 精簡技術規格：[`spec.md`](spec.md)
- 實作狀態：[`task.md`](task.md)
- 記憶提取技能：[`skills/memory-extraction.md`](skills/memory-extraction.md)

---

## 授權條款

MIT

---

## 發行版本

此專案使用 GitHub Actions 自動編譯與發行。當推送符合 `v*` 模式的標籤（如 `v0.1.0`）時，CI 會自動建置二進位檔、打包為 ZIP 壓縮檔並上傳至 GitHub Releases。

您也可以在 GitHub 頁面上手動觸發 `Release` workflow，輸入版本號即可產生發行版。

壓縮檔命名規則：`opencode-memory-v{版本}-x86_64-pc-windows-msvc.zip`
