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
- Local uncommitted installer fix now targets `%USERPROFILE%\.config\opencode\opencode.jsonc` and supports JSONC comments.

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

- [ ] Enforce `MEMORY_MAX_RECORDS`.
  - Spec/config evidence: `MEMORY_MAX_RECORDS` is parsed as part of runtime config.
  - Current evidence: `MemoryConfig.max_records` is stored but not used by insertion, retrieval, decay, or archival logic.
  - Suggested files: `crates/memory-core/src/config.rs`, `crates/memory-core/src/service.rs`, `crates/memory-core/src/storage/sqlite.rs`.

- [ ] Populate and maintain `session_stats`.
  - Spec evidence: schema defines per-session extracted/added/deduplicated/retrieved counters.
  - Current evidence: migration creates `session_stats`, but there are no read/write helpers or service updates.
  - Suggested files: `crates/memory-core/src/storage/sqlite.rs`, `crates/memory-core/src/service.rs`, `crates/memory-core/tests/lifecycle_test.rs`.

- [ ] Persist actual embedding metadata in `system_config`.
  - Spec evidence: vector dimensions and embedding model are meant to be tracked for dimension migration safety.
  - Current evidence: migration inserts `vector_dimensions = 1536` and `embedding_model = unknown`; runtime config does not update them.
  - Suggested files: `crates/memory-core/src/storage/sqlite.rs`, `crates/memory-core/src/service.rs`, `crates/memory-core/src/storage/migrations/1_init.sql`.

- [ ] Add USearch compaction/rebuild path during batch consolidation.
  - Spec evidence: session-end consolidation includes index compaction for Tantivy and USearch.
  - Current evidence: `batch_consolidate()` compacts only Tantivy.
  - Suggested files: `crates/memory-core/src/storage/vector.rs`, `crates/memory-core/src/consolidation/engine.rs`.

- [ ] Add MCP protocol smoke test for OpenCode compatibility.
  - Spec evidence: completion requires MCP Server to be loadable by OpenCode and respond to tool list.
  - Current evidence: Rust tests cover service lifecycle, but there is no root integration test that starts the binary and verifies MCP `tools/list`.
  - Suggested files: `tests/integration/`, `crates/memory-mcp-server/tests/`, `scripts/`.

- [ ] Decide whether to implement or remove the startup `initialized` stderr event requirement.
  - Spec evidence: AGENTS section says startup should immediately emit an initialized JSON-RPC event to stderr.
  - Current evidence: server logs normal tracing messages to stderr and uses rmcp over stdout/stdin; no explicit initialized event is emitted.
  - Suggested files: `crates/memory-mcp-server/src/main.rs`, `opencode-memory-system.md`, `docs/AGENTS.md`.

- [ ] Add benchmark and recall test suite.
  - Spec evidence: 10K/100K search latency, add latency, memory usage, disk usage, and recall accuracy are explicit targets.
  - Current evidence: no `benches/` directory or Criterion/performance harness exists.
  - Suggested files: `benches/`, `crates/memory-core/Cargo.toml`, `docs/benchmarks.md`.

- [ ] Add coverage measurement workflow for the stated module targets.
  - Spec evidence: extraction, consolidation, retrieval, storage, and MCP server each have coverage goals.
  - Current evidence: tests exist, but there is no coverage command, CI target, or report artifact.
  - Suggested files: `README.md`, `docs/AGENTS.md`, CI or script files.

## P2 Todo

- [ ] Reconcile the TypeScript shim size rule with the current robust parser.
  - Spec evidence: plugin shim should be <=100 lines and lifecycle-only.
  - Current evidence: `plugin/src/index.ts` is about 170 physical lines and includes response normalization helpers.
  - Suggested options: split helper parsing into a separate file, or update the spec to allow minimal compatibility helpers in TS.
  - Suggested files: `plugin/src/index.ts`, `plugin/src/response.ts`, `plugin/test/index.test.cjs`, `opencode-memory-system.md`.

- [ ] Update OpenCode config documentation to the current JSONC path and key shape.
  - Spec evidence: section 9 still says `~/.config/opencode/config.json` and uses `env`.
  - Current evidence: installer writes `~/.config/opencode/opencode.jsonc` and uses `environment`.
  - Suggested files: `opencode-memory-system.md`, `README.md`, `docs/AGENTS.md`.

- [ ] Fix stale milestone wording in `task.md`.
  - Current evidence: `task.md` says custom JSON-RPC stdio loop, but the implementation uses `rmcp::serve_server`; it also mentions a flat-scan vector fallback even though USearch is now the active store.
  - Suggested files: `task.md`.

- [ ] Align `README.md` with install behavior and verification commands.
  - Current evidence: README describes the installer generally but does not clearly state the OpenCode JSONC destination or the exact MCP entry generated by `install`.
  - Suggested files: `README.md`, `install.ps1`.

- [ ] Decide whether `opencode-memory-system.md` remains the source spec or becomes an archival planning doc.
  - Current evidence: implementation has evolved beyond some original details, especially config file naming, plugin helper size, and rmcp behavior.
  - Suggested outcome: add a short "current deviations" section or move current contract into `spec.md`/`docs/AGENTS.md`.
  - Suggested files: `opencode-memory-system.md`, `spec.md`, `docs/AGENTS.md`.

## Recommended Next Order

1. Commit/push the OpenCode JSONC installer fix so users get the correct MCP config immediately.
2. Implement P0 scope validation and partial-insert rollback tests.
3. Update docs/spec paths so future agents do not reinstall to the legacy config location.
