# Agents Memory Service (AMS)

<div align="right">

**English** | [繁體中文](README.zh-TW.md)

</div>

**Agents Memory Service (AMS)** — A local-first long-term memory system for AI coding agents. Durable facts, preferences, decisions, code patterns, and project knowledge across sessions. Hybrid semantic + BM25 + temporal retrieval.

Core in Rust, exposed as an MCP server (`ams` binary). Optional TypeScript shim for OpenCode lifecycle hooks. Successor to the original `memlong` / `opencode-memory` project.

## Prerequisites

AMS expects a local OpenAI-compatible endpoint. With **llama.cpp**:

```bash
# Start extraction server (Phi-4)
llama-server -m phi-4-Q4_K_M.gguf --port 8080 --ctx-size 4096

# Start embedding server (qwen3-embedding:0.6b Q4)
llama-server -m qwen3-embedding-0.6b-Q4_K_M.gguf --port 8081 \
  --embedding --pooling mean --no-context --cont-batching
```

Set `LLM_API_BASE` to the extraction server and `EMBEDDING_API_BASE` to the embedding server if they differ; by default AMS uses `LLM_API_BASE` for both.

## Quick Start

```bash
# Build
git clone https://github.com/stevenke1981/agents-memory-services.git
cd agents-memory-services
cargo build --release

# Install
./install.sh --from-source

# Configure (set before starting)
export LLM_API_BASE="http://localhost:8080/v1"
export LLM_API_KEY="local"
export EXTRACTION_MODEL="phi-4"
export EMBEDDING_MODEL="qwen3-embedding:0.6b"
export EMBEDDING_DIM="1024"

# Verify
./target/release/ams health
```

### CLI Debug

```bash
cargo run -p agents-memory-cli -- add --content "User prefers Rust for core services"
cargo run -p agents-memory-cli -- search --query "preferred implementation language"
cargo run -p agents-memory-cli -- list
cargo run -p agents-memory-cli -- stats
cargo run -p agents-memory-cli -- consolidate
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `LLM_API_BASE` | `http://localhost:8080/v1` | OpenAI-compatible endpoint (phi-4) |
| `EMBEDDING_API_BASE` | (same as LLM_API_BASE) | Separate endpoint for embeddings |
| `LLM_API_KEY` | `local` | API key |
| `EXTRACTION_MODEL` | `phi-4` | Chat model for extraction (llama.cpp) |
| `EMBEDDING_MODEL` | `qwen3-embedding:0.6b` | Embedding model (Q4, llama.cpp) |
| `EMBEDDING_DIM` | `1024` | Embedding dimensions (must match model) |
| `PROJECT_ROOT` | current dir | Root for `.opencode/` data directory |
| `MEMORY_DB_PATH` | `.opencode/memory.db` | SQLite path |
| `MEMORY_VECTOR_PATH` | `.opencode/vectors.usearch` | USearch index path |
| `MEMORY_TANTIVY_PATH` | `.opencode/tantivy` | Tantivy index directory |
| `MEMORY_DEDUP_THRESHOLD` | `0.92` | Exact duplicate cosine threshold |
| `MEMORY_NEAR_DEDUP_THRESHOLD` | `0.75` | Near-duplicate cosine threshold |
| `MEMORY_MAX_RECORDS` | `50000` | Maximum memory capacity |
| `MEMORY_DECAY_LAMBDA` | `0.001` | Importance recency decay rate |
| `MEMORY_TEMPORAL_MU` | `0.05` | Retrieval temporal decay rate |

## MCP Tools

| Tool | Purpose |
|------|---------|
| `add_memory` | Extract and store memories from text |
| `search_memories` | Hybrid semantic + BM25 + temporal search |
| `get_memories` | Fetch by ID or filter (scope, project) |
| `delete_memory` | Delete and clean all indexes |
| `consolidate_memories` | Apply decay, dedup, and compaction |
| `get_memory_stats` | Counts, categories, scope breakdown |
| `end_session` | Mark a session ended (`ended_at` timestamp) |

## Agent Guide

### Contracts

1. **ADD-only**: memory content is immutable. Only access stats, retention, importance, and archival metadata may be updated.
2. **Rust core, TS thin**: memory logic in `agents-memory-core`. TypeScript (`plugin/`) is a lifecycle adapter only.
3. **Index consistency**: SQLite, USearch, Tantivy, and entity links must remain consistent through every insert and delete.
4. **MCP protocol**: stdout reserved for JSON-RPC; diagnostics to stderr.
5. **Scope isolation**: duplicate detection respects scope + project boundaries.
6. **No real LLM in tests**: all tests use `api_key = "mock"`.

### Architecture

```
MCP Client → agents-memory-servics → agents-memory-core
                                    ├── SQLite (metadata, entities, stats)
                                    ├── USearch HNSW (vector index)
                                    ├── Tantivy BM25 (text index)
                                    ├── extraction/  (LLM + embedding)
                                    ├── consolidation/  (dedup, entity linking, Ebbinghaus decay)
                                    └── retrieval/  (hybrid ranking, filtering)
```

Default data location: `.opencode/`

### Code Paths

| Path | Responsibility |
|------|---------------|
| `crates/agents-memory-core/src/service.rs` | High-level orchestration |
| `crates/agents-memory-core/src/extraction/` | LLM extraction and embeddings |
| `crates/agents-memory-core/src/consolidation/` | Dedup, entity linking, Ebbinghaus decay |
| `crates/agents-memory-core/src/retrieval/` | Hybrid ranking and filtering |
| `crates/agents-memory-core/src/storage/` | SQLite, USearch, Tantivy adapters |
| `crates/agents-memory-servics/src/server.rs` | MCP tool schemas and handlers |
| `plugin/src/index.ts` | OpenCode lifecycle bridge |

### Verification

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
cargo bench -p agents-memory-core
cd plugin && npm ci && npm test
```

### Data Flow

```
add_memory(content):
  1. extraction::engine.extract(content) → Vec<ExtractedMemory>
  2. extraction::engine.embed(chunk) → Vec<f32>
  3. consolidation::engine.consolidate_single(chunk, vector, scope, ...)
     a. vector_store.search(top 5 candidates)
     b. dedup check (exact 0.92, near 0.75 + entity overlap)
     c. insert: vector_store → text_index → entity linking → SQLite
  4. text_index.flush()

search_memories(query):
  1. tokio::join!(llm_client.embed(query), bm25.search_normalized(query))
  2. semantic.search(query_vec) → vector_ids
  3. sqlite.get_memories_by_vector_ids() + get_by_ids()
  4. fusion: 0.6*semantic + 0.3*bm25 + 0.1*temporal
  5. filter by scope/project/category/importance/decay
  6. tokio::spawn(update_access_stats)

consolidate_memories():
  1. paginated (1000/page) load from SQLite
  2. per memory: Ebbinghaus decay R(t) = e^(-t/S)
  3. archive if R(t) < 0.1
  4. compact tantivy + usearch indexes
```

### Decay Formulas

```
Importance = 0.5 * llm_score + 0.3 * access_factor + 0.2 * recency_factor
Retention  R(t) = e^(-t / S)        # Ebbinghaus
Stability  S = importance * 30 days  # reinforced ×1.2 per access
Recency    = e^(-0.001 * Δt_days)
```

## Docs

- [Product spec](opencode-memory-system.md)
- [Tech spec](spec.md)
- [Task status](task.md)
- [Lessons](lessons.md)

## Uninstall

```bash
# Remove binaries and MCP config (keeps memory data)
./uninstall.sh

# Remove everything including stored memories
./uninstall.sh --remove-data
```

## License

MIT
