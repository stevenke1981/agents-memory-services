# memlong 改善任務清單 todos.md

## Gate 0：開始前

- [x] 建立分支：`chore/memlong-compat-plan`。
- [x] 執行 baseline：`scripts/verify.sh` 或 `scripts/verify.ps1`。
- [x] 將 baseline 結果寫入 `docs/baseline.md`。
- [x] 確認目前 `LLM_API_KEY=mock` 測試可跑。

## P0：文件與命名

- [x] 決定正式 server id：`memlong-memory`（MCP server id）。
- [x] README 統一 server id / 三層架構說明 / 多 client 支援。
- [x] 新增 `docs/opencode.md`。
- [x] 新增 `docs/codex.md`。
- [x] 新增 `docs/claude-code.md`。
- [x] 新增 `docs/config-reference.md`。
- [x] 補上「release 尚未發布時請使用 source build」說明。
- [x] 補上 Windows PowerShell 安裝範例。
- [x] 補上 Linux/macOS 安裝範例。
- [x] 補上 uninstall / remove MCP config 說明。

## P0：Agent instructions

- [x] 將 `AGENTS.md` 放到 repo root，供 OpenCode / Codex 讀取。
- [x] 將 `CLAUDE.md` 放到 repo root，供 Claude Code 讀取。
- [x] 指示 agent：任務開始先 `search_memories`。
- [x] 指示 agent：任務完成後只保存 durable memories。
- [x] 指示 agent：不得保存 secrets。
- [x] 指示 agent：stdout clean for MCP。

## P1：Config snippets

- [x] 新增 `examples/opencode/opencode.jsonc`。
- [x] 新增 `examples/codex/config.toml`。
- [x] 新增 `examples/claude/.mcp.json`。
- [x] 新增 `examples/env/local-llm.env.example`。
- [x] 新增 `examples/env/openai-compatible.env.example`。
- [x] 文件說明 `PROJECT_ROOT` 對 `.opencode/` 資料位置的影響（在 docs/config-reference.md 中）。

## P1：Install / Doctor

- [x] 建立 `install.sh`（Unix：download + source-build 兩種路徑，支援 `--client`/`--dry-run`/`--print-config`）。
- [x] `install.sh` 新增 `--client opencode|codex|claude|all`。
- [x] `install.ps1` 新增 `-Client opencode|codex|claude|all`。
- [x] 新增 `--dry-run` / `-DryRun`（script & binary 雙層支援）。
- [x] 新增 `--print-config` / `-PrintConfig`（script & binary 雙層支援）。
- [x] release asset 下載失敗時提示 `--from-source`（已在 install.ps1 實作）。
- [x] `--fallback-source` 自動 source build（已在 install.ps1 實作）。
- [x] `memory-mcp-server install --json` 輸出 warnings 與 skipped clients。
- [x] 新增 `doctor` command。
- [x] 新增 `doctor --json`。
- [x] `doctor` 檢查 OpenCode config。
- [x] `doctor` 檢查 Codex config。
- [x] `doctor` 提供 Claude Code 設定建議。

## P1：Mock / Test stability

- [x] Mock embedding 依 `EMBEDDING_DIM` 回傳向量（`LlmClient` 新增 `embedding_dim` 欄位）。
- [x] 增加 test：`EMBEDDING_DIM=8` / 64 / 1536 時 mock 向量長度正確。
- [x] 增加 test：vector index dimension mismatch 回傳 actionable error（`dimension_mismatch_returns_actionable_error`）。
- [x] 增加 test：`LLM_API_BASE=mock` + `LLM_API_KEY=local` 可通過（`mock_mode_with_local_key_and_mock_base`）。
- [x] 增加 test：extraction parse failed 不會中斷 session（`extraction_parse_failure_degrades_gracefully`）。

## P2：MCP schema 與輸出控制

- [x] `search_memories` 新增 `output_mode`（brief：content+category+scores，省略 metadata）。
- [x] `search_memories` 新增 `max_output_chars`（截斷總輸出字數）。
- [ ] `get_memories` 新增 pagination / cursor。
- [x] MCP tool descriptions 標明何時使用。
- [x] 回傳結果加入 `score_breakdown`（score_final/score_semantic/score_bm25/score_temporal），brief 模式可省略 metadata。

## P2：資料一致性與修復

- [x] 建立 migration `2_data_consistency.sql`（ALTER TABLE + index_repair_queue）。
- [x] Memory struct 新增 `status`、`embedding_model`、`embedding_dim`、`content_hash`。
- [x] Insert 流程寫入 status='active'、embedding_model、embedding_dim、SHA256 content_hash。
- [x] 新增 `index_repair_queue` table + `enqueue_repair_issue` helper。
- [x] 新增 `repair_indexes` MCP tool（支援 `dry_run`）。
- [x] `repair_indexes` 含 entity reference cleanup、embedding metadata backfill、vector store consistency check。
- [x] `repair_indexes` 自動記入 repair queue。
- [x] `get_memory_stats` 回傳 active_memories、unresolved_repairs、vector_count。
- [x] 持久化 migration：`SqliteStore::new()` startup 自動 backfill pre-v2 memories。
- [x] `backfill_embedding_metadata` 從 `repair_indexes` 移至 startup（不再每次 repair 都掃描）。
- [x] Soft-delete / tombstone：`delete_memory` SET status='deleted'，保留 SQLite row。
- [x] `undelete_memory` MCP tool：恢復已 soft-deleted 的記憶。
- [x] `compact_deleted` MCP tool：永久清除已刪除記憶（需 confirm=true）。
- [x] 所有查詢方法統一過濾 `WHERE status = 'active'`（`get_by_ids`、`list_memories`、`get_memory_by_vector_id`、`get_memories_for_decay`）。
- [x] `get_memory_stats` 回傳 `active_memories`、`deleted_memories`。

## P2：OpenCode plugin

- [ ] plugin build 輸出清楚。
- [ ] README 說明 plugin 如何安裝到 `.opencode/plugins/` 或 global plugins。
- [ ] plugin onSessionEnd 同時呼叫 `end_session`。
- [ ] plugin 對 `projectPath` / `projectId` 使用一致化。
- [ ] plugin test 覆蓋 MCP text content response。
- [ ] plugin test 覆蓋 empty memory response。

## P3：Retrieval eval / benchmark

- [ ] 建立 `tests/fixtures/memories.jsonl`。
- [ ] 建立 retrieval gold queries。
- [ ] benchmark 1K records。
- [ ] benchmark 10K records。
- [ ] benchmark 50K records。
- [ ] 比較 hybrid vs semantic-only vs BM25-only。
- [ ] 將結果寫入 `docs/retrieval-eval.md`。

## P3：Release

- [ ] GitHub Actions release workflow。
- [ ] 產出 Linux x86_64 tar.gz。
- [ ] 產出 Linux aarch64 tar.gz。
- [ ] 產出 Windows x86_64 zip。
- [ ] release artifact name 與 install scripts 一致。
- [ ] `install.sh` 下載 release asset smoke test。
- [ ] `install.ps1` 下載 release asset smoke test。
- [ ] 更新 `CHANGELOG.md`。

## 修復項目 (Phase 0/P1 + 設定修復)

- [x] `${PROJECT_ROOT}` placeholder 防護 — `config.rs` 新增 `is_placeholder()` + fallback
- [x] 清除 `${PROJECT_ROOT}` 字面目錄殘留
- [x] 新增 `from_env` sanitization 測試（4 個新 unit tests）
- [x] Mock embedding 維度固定 1536 bug — `LlmClient` 新增 `embedding_dim` 欄位
- [x] 新增 `mock_embedding_respects_dimension` 測試（8/64/1536 dim）
- [x] 新增 `mock_mode_with_local_key_and_mock_base` 測試
- [x] 新增 `dimension_mismatch_returns_actionable_error` 測試
- [x] 新增 `extraction_parse_failure_degrades_gracefully` 測試
- [x] MCP server 重構：新增 `doctor` command（支援 `--json`）
- [x] MCP server 新增 `install --json` 輸出 warnings / skipped / restart_required
- [x] MCP server 新增 `install --client opencode|codex|claude|all`
- [x] MCP server 新增 `install --dry-run` / `--print-config`
- [x] 新增 Claude MCP config 安裝支援 (`update_claude_config`)
- [x] commands.rs 模組化：install / doctor / config helpers 獨立模組
- [x] `install.ps1` 新增 `-Client`, `-DryRun`, `-PrintConfig` 參數
- [x] 建立 `install.sh`（Unix install script，支援 `--client`/`--dry-run`/`--print-config`/`--from-source`）
- [x] P2 資料一致性 migration `2_data_consistency.sql`
- [x] P2 Memory struct 新增 status/embedding_model/embedding_dim/content_hash
- [x] P2 ConsolidationEngine 寫入 embedding metadata + content_hash
- [x] P2 `repair_indexes` MCP tool（entity cleanup + metadata backfill + vector count check）
- [x] P2 `get_memory_stats` 回傳 active_memories + unresolved_repairs
- [x] P2 Soft-delete: delete_memory SET status='deleted' + undelete_memory + compact_deleted
- [x] P2 `backfill_embedding_metadata` 移至 SqliteStore::new() startup（不再重複掃描）
- [x] P2 查詢方法統一過濾 `status = 'active'`

## Final gate

- [x] `cargo fmt --all -- --check`
- [x] `cargo test --workspace`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo build --release`
- [x] `cd plugin && npm ci && npm test`
- [x] MCP server `health` returns ok。
- [x] OpenCode config parse smoke test（透過 `update_opencode_config` 兩項 unit tests 涵蓋）。
- [x] Codex config parse smoke test（透過 `update_codex_config` coverage + doctor 檢查）。
- [x] Claude `.mcp.json` parse smoke test（透過 `update_claude_config` 兩項 unit tests 涵蓋）。
- [x] `doctor --json` reports ok or actionable warning（已驗證）。
- [ ] `final.md` 更新實際完成項目。
