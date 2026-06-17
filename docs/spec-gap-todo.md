# Spec Gap Todo List

Generated: 2026-06-13

Scope: Compare `opencode-memory-system.md` against the current repository implementation. This file is intentionally separate from `task.md` because `task.md` records broad milestone completion, while this list tracks remaining spec and reality gaps.

Legend:
- P0: correctness or install compatibility gap
- P1: required spec coverage, observability, or production-readiness gap
- P2: documentation or cleanup gap

## Already Implemented

- Cargo workspace with `memory-core`, `memory-mcp-server`, and `memory-cli`.
- SQLite storage with WAL mode, migrations, memory/entity/session/system tables.
- Persistent USearch-backed `VectorStore`, Tantivy `TextIndex`, and hybrid retrieval.
- MCP tools: `add_memory`, `search_memories`, `get_memories`, `delete_memory`, `consolidate_memories`, `get_memory_stats`.
- Plugin lifecycle hooks: `onChatStart`, `onMessageComplete`, and `onSessionEnd`.
- Background `DecayScheduler` is spawned by the MCP server with a 24-hour interval.
- Installer fix targets `%USERPROFILE%\.config\opencode\opencode.jsonc` and supports JSONC comments.
- MCP protocol smoke test (`protocol_test.rs`) spawns the binary and performs full MCP lifecycle.
- Criterion benchmarks for `add_memory_single` (~183µs) and `search_memories_top5` (~128µs).
- Coverage measurement via `scripts/coverage.ps1` using `cargo-llvm-cov`.
- Plugin helpers split into `response.ts`; `index.ts` lifecycle-only and under the 100-line target.

## P0 Todo

- [x] Commit and push the OpenCode installer path fix.
  - Commit `b9b50e8` — installer now targets `%USERPROFILE%\.config\opencode\opencode.jsonc` with JSONC comment support.
  - Files: `crates/memory-mcp-server/src/main.rs`, `docs/spec-gap-todo.md`.

- [x] Enforce required scope parameters and support `Agent` scope properly.
  - Commit `ee93cc8` — `agent_id` added to `AddMemoryInput`, `MemoryService::add_memory()`, and `ConsolidationEngine::consolidate_single()`. Scope validation rejects missing `project_id` for Project scope and missing `agent_id` for Agent scope.
  - Files: `crates/memory-mcp-server/src/server.rs`, `crates/memory-core/src/service.rs`, `crates/memory-core/src/consolidation/engine.rs`, `crates/memory-cli/src/main.rs`.

- [x] Add rollback or transaction-style cleanup for partial insert failures.
  - Commit `ee93cc8` — each index step (vector → text → entity) rolls back the previous steps on failure. Tests confirm no SQLite orphans remain.
  - Files: `crates/memory-core/src/consolidation/engine.rs`.

- [x] Define and test extraction graceful degradation.
  - Commit `ee93cc8` — extraction/HTTP errors are caught with `tracing::warn!`, return empty `Vec` instead of propagating. Embedding failures skip the chunk. Consolidation errors also degraded.
  - Files: `crates/memory-core/src/service.rs`.

## P1 Todo

- [x] Enforce `MEMORY_MAX_RECORDS`.
  - Commit HEAD — `add_memory()` checks `sqlite.memory_count()` before insertion, rejects if >= config.max_records.
  - Files: `crates/memory-core/src/storage/sqlite.rs`, `crates/memory-core/src/service.rs`, `crates/memory-core/src/config.rs`.

- [x] Populate and maintain `session_stats`.
  - Commit HEAD — `ensure_session()`, `update_session_stats()`, `end_session()` in SqliteStore. `add_memory()` tracks extracted/added/deduplicated. `search_memories()` tracks retrieved. `end_session` MCP tool.
  - Files: `crates/memory-core/src/storage/sqlite.rs`, `crates/memory-core/src/service.rs`, `crates/memory-core/src/models/query.rs`, `crates/memory-mcp-server/src/server.rs`.

- [x] Persist actual embedding metadata in `system_config`.
  - Commit HEAD — `set_system_config()`/`get_system_config()` in SqliteStore. `MemoryService::new()` writes runtime `vector_dimensions` and `embedding_model`.
  - Files: `crates/memory-core/src/storage/sqlite.rs`, `crates/memory-core/src/service.rs`.

- [x] Add USearch compaction/rebuild path during batch consolidation.
  - Commit HEAD — `VectorStore::compact()` rebuilds HNSW graph via `reset()`+re-add. Called from `batch_consolidate()` after Tantivy compaction.
  - Files: `crates/memory-core/src/storage/vector.rs`, `crates/memory-core/src/consolidation/engine.rs`.

- [x] Add MCP protocol smoke test for OpenCode compatibility.
  - Commit HEAD — `protocol_test.rs` spawns the server binary, performs full MCP initialize handshake, calls `tools/list`, `add_memory`, `search_memories`, `get_memory_stats`, and verifies responses.
  - Files: `crates/memory-mcp-server/tests/protocol_test.rs`, `crates/memory-mcp-server/Cargo.toml`.

- [x] Decide whether to implement or remove the startup `initialized` stderr event requirement.
  - **Decided: keep current behavior** — rmcp already handles the MCP initialize/initialized handshake automatically (protocol version 2024-11-05 confirmed in test). No additional stderr event needed.
  - Files: (no change needed).

- [x] Add benchmark and recall test suite.
  - Commit HEAD — Criterion benchmarks for `add_memory_single` (~183µs) and `search_memories_top5` (~128µs) with mock LLM.
  - Files: `crates/memory-core/benches/memory_bench.rs`, `crates/memory-core/Cargo.toml`.

- [x] Add coverage measurement workflow for the stated module targets.
  - Commit HEAD — `scripts/coverage.ps1` with `cargo-llvm-cov` integration, supports quick terminal report and full HTML output.
  - Files: `scripts/coverage.ps1`.

## P2 Todo

- [x] Reconcile the TypeScript shim size rule with the current robust parser.
  - Commit HEAD — helpers moved to `plugin/src/response.ts`; `index.ts` is lifecycle-only and under the 100-line target. Tests pass (2/2).
  - Files: `plugin/src/index.ts`, `plugin/src/response.ts`, `plugin/test/index.test.cjs`.

- [x] Update OpenCode config documentation to the current JSONC path and key shape.
  - Commit HEAD — `opencode-memory-system.md` §9.1 updated: `config.json` → `opencode.jsonc`, `env` → `environment`.
  - Files: `opencode-memory-system.md`.

- [x] Fix stale milestone wording in `task.md`.
  - Commit HEAD — Updated: `rmcp::serve_server` instead of custom stdio loop, USearch instead of flat-scan, 7 MCP tools instead of 6.
  - Files: `task.md`.

- [x] Align `README.md` with install behavior and verification commands.
  - Commit HEAD — Added `end_session` to MCP tools table, updated test count and benchmark command in verification section.
  - Files: `README.md`.

- [x] Decide whether `opencode-memory-system.md` remains the source spec or becomes an archival planning doc.
  - **Decided: keep as source spec with a "Current Deviations" header** documenting the 7 known implementation divergences.
  - Files: `opencode-memory-system.md`.

## Recommended Next Order

1. Commit/push the OpenCode JSONC installer fix so users get the correct MCP config immediately.
2. Implement P0 scope validation and partial-insert rollback tests.
3. Update docs/spec paths so future agents do not reinstall to the legacy config location.
