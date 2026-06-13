# OpenCode Memory System — AI Agent Implementation Guide

> This document is an AGENTS.md for the memory system itself, describing how to build,
> modify, and debug the system. It is designed for AI coding agents working on this codebase.

## Architecture Overview

```
memory-mcp-server (bin)          → MCP stdio server, entry point
  └── memory-core (lib)          → All core logic
        ├── extraction/          → LLM Single-Pass memory extraction
        ├── consolidation/       → ADD-only dedup, entity linking, decay
        ├── retrieval/           → Hybrid semantic+BM25+temporal search
        ├── storage/             → SQLite, USearch (HNSW), Tantivy (BM25)
        └── models/              → Data types (Memory, SearchQuery, etc.)
memory-cli (bin)                 → Debug CLI tool
```

## Key Design Decisions

### ADD-only Immutability
Memory content is never mutated after creation. Only access statistics (`access_count`, `last_accessed_at`)
and decay parameters (`importance_score`, `retention_factor`, `metadata.archived`) are updated.
This ensures traceability and simplifies concurrency.

### Single-Pass LLM Extraction
One LLM call extracts all memories from a conversation turn. The extraction prompt
returns structured JSON array. Quality filters (`min_confidence >= 0.6`, `min_importance >= 2`)
remove low-signal memories before storage.

### Dedup Strategy
- **cosine > 0.92**: Exact duplicate → skip, increment access_count on existing
- **0.75 to 0.92**: Near duplicate → entity overlap check → if >50% overlap, treat as synonym
- **< 0.75**: New memory → insert

### Ebbinghaus Decay
Formula: `R(t) = e^(-t/S)` where `S = importance_score * 30` days.
Accessed memories get `S *= 1.2` (reinforcement). `retention_factor < 0.1` → archived.

## Common Tasks

### Adding a New SQLite Migration
1. Create file in `crates/memory-core/src/storage/migrations/V{N}__{description}.sql`
2. `sqlx::migrate!()` in `SqliteStore::new()` auto-runs pending migrations
3. Follow existing naming convention: idempotent (`IF NOT EXISTS` / `OR IGNORE`)

### Adding a New MCP Tool
1. Define input struct with `#[derive(Deserialize, JsonSchema)]` in `server.rs`
2. Add `#[tool(name = "...", description = "...")]` async method on `MemoryMcpServer`
3. Delegate to `MemoryService` method
4. Tools are auto-discovered via `#[tool(tool_box)]` macro

### Testing with Mock LLM
Set `LLM_API_KEY=mock` or `LLM_API_BASE=mock` — the `LlmClient` returns canned responses
for both chat completions and embeddings. No real API needed.

## Debugging

- Log output goes to **stderr** (MCP stdio protocol requires stdout clean for JSON-RPC)
- CLI tool: `cargo run -p memory-cli -- stats` (quick health check)
- MCP Server: `cargo run -p memory-mcp-server -- health` (test initialization)
- Use `MEMORY_LOG_LEVEL=debug` for verbose tracing

## Configuration Reference

| Env Var | Default | Purpose |
|---------|---------|---------|
| `MEMORY_DB_PATH` | `.opencode/memory.db` | SQLite path |
| `MEMORY_VECTOR_PATH` | `.opencode/vectors.usearch` | USearch HNSW path |
| `MEMORY_TANTIVY_PATH` | `.opencode/tantivy` | Tantivy index dir |
| `LLM_API_BASE` | `https://api.anthropic.com/v1` | OpenAI-compatible API |
| `LLM_API_KEY` | `local` | API key (`mock` for testing) |
| `EMBEDDING_DIM` | `1536` | Vector dimensions |

## Project Structure Map

```
crates/memory-core/src/
├── lib.rs              — Public API exports
├── config.rs           — MemoryConfig from env
├── error.rs            — MemoryError enum
├── service.rs          — MemoryService orchestrator
├── models/
│   ├── mod.rs
│   ├── memory.rs       — Memory, MemoryCategory, MemoryScope
│   └── query.rs        — SearchQuery, HybridWeights, SearchResult
├── extraction/
│   ├── mod.rs
│   ├── engine.rs       — ExtractionEngine (LLM call + JSON parse)
│   ├── prompt.rs       — System prompt templates
│   └── llm_client.rs   — HTTP client for LLM/embedding APIs
├── consolidation/
│   ├── mod.rs
│   ├── engine.rs       — ConsolidationEngine (ADD-only insert)
│   ├── dedup.rs        — Duplicate detection logic
│   ├── entity.rs       — Entity linking (entities table)
│   ├── decay.rs        — Ebbinghaus decay math
│   └── scheduler.rs    — Background decay loop
├── retrieval/
│   ├── mod.rs
│   ├── engine.rs       — RetrievalEngine (orchestration)
│   ├── semantic.rs     — SemanticRetriever (USearch HNSW)
│   ├── bm25.rs         — Bm25Retriever (Tantivy)
│   └── hybrid.rs       — BM25 score normalization
└── storage/
    ├── mod.rs
    ├── sqlite.rs       — SqliteStore (CRUD + entity + stats)
    ├── vector.rs       — VectorStore (USearch wrapper)
    ├── text_index.rs   — TextIndex (Tantivy wrapper)
    └── migrations/
        └── 1_init.sql  — Schema init
```

## Version Compatibility

| Dependency | Version | Notes |
|-----------|---------|-------|
| Rust edition | 2021 | Stable |
| sqlx | 0.8 | SQLite + Tokio |
| tantivy | 0.22 | BM25 full-text |
| usearch | 2.x | HNSW vectors |
| rmcp | 0.1 | MCP protocol |
