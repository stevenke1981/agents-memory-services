# Memlong

<div align="right">

**English** | [繁體中文](README.zh-TW.md)

</div>

Memlong is a local-first long-term memory system for coding agents. It stores durable facts, preferences, decisions, code patterns, and project knowledge across sessions, then retrieves relevant memories through hybrid semantic, keyword, and temporal ranking.

The core is implemented in Rust and exposed as an MCP server. A small TypeScript shim provides optional OpenCode lifecycle hooks for automatic retrieval and capture.

## What It Provides

- Single-pass LLM memory extraction with confidence and importance filtering
- ADD-only consolidation with exact and near-duplicate detection
- Project, global, session, and agent memory scopes
- USearch HNSW vector search with persistent local indexes
- Tantivy BM25 full-text search
- Ebbinghaus-inspired retention decay and access reinforcement
- MCP tools for adding, searching, listing, deleting, consolidating, and inspecting memories
- A thin OpenCode plugin for automatic session injection and capture

## Architecture

```text
OpenCode / Codex / MCP Client
          |
          | JSON-RPC over stdio
          v
memory-mcp-server
          |
          v
memory-core
  |-- SQLite metadata and entities
  |-- USearch HNSW vectors
  |-- Tantivy BM25 index
  `-- extraction, consolidation, retrieval, decay
```

Default project-local data is stored under `.opencode/`:

```text
.opencode/
|-- memory.db
|-- vectors.usearch
`-- tantivy/
```

## Human Quick Start

### Requirements

- **Windows:** Windows 10 or 11, Rust stable with MSVC toolchain, Visual Studio Build Tools
- **Linux:** glibc 2.31+, Rust stable (only for source builds)
- **macOS:** macOS 12+, Rust stable (only for source builds)
- Node.js 18+ only when building or testing the OpenCode plugin
- An OpenAI-compatible chat completions and embeddings endpoint

### Build From Source

```bash
git clone https://github.com/stevenke1981/memlong.git
cd memlong
cargo build --release
```

The MCP server is created at:

| Platform | Path |
|----------|------|
| Windows  | `target\release\memory-mcp-server.exe` |
| Linux / macOS | `target/release/memory-mcp-server` |

### Install On Windows

Build and install from the checkout:

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1 -FromSource
```

Install a published release when available:

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1 -Version v0.1.0
```

### Install On Linux

Build and install from the checkout:

```bash
./install.sh --from-source
```

Install a published release when available:

```bash
./install.sh --version v0.1.0
```

### Install On macOS

Build and install from the checkout:

```bash
./install.sh --from-source
```

Install a published release when available:

```bash
./install.sh --version v0.1.0
```

The installer places the executable under `~/.config/opencode-memory/bin` (Linux/macOS) or `%USERPROFILE%\.config\opencode-memory\bin` (Windows) and invokes its `install` command to configure supported MCP clients. Restart the client after installation.

### Configure The Models

Set an OpenAI-compatible endpoint before starting the MCP server:

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

Important optional settings:

| Variable | Default | Purpose |
| --- | --- | --- |
| `PROJECT_ROOT` | current directory | Root used for the `.opencode` data directory |
| `MEMORY_DB_PATH` | `.opencode/memory.db` | SQLite database path |
| `MEMORY_VECTOR_PATH` | `.opencode/vectors.usearch` | USearch index path |
| `MEMORY_TANTIVY_PATH` | `.opencode/tantivy` | Tantivy index directory |
| `MEMORY_DEDUP_THRESHOLD` | `0.92` | Exact duplicate cosine threshold |
| `MEMORY_NEAR_DEDUP_THRESHOLD` | `0.75` | Near-duplicate cosine threshold |
| `MEMORY_MIN_CONFIDENCE` | `0.60` | Minimum extraction confidence |
| `MEMORY_MIN_IMPORTANCE` | `2` | Minimum LLM importance from 1 to 5 |
| `MEMORY_DECAY_LAMBDA` | `0.001` | Importance recency decay rate |
| `MEMORY_DECAY_MU` | `0.05` | Retrieval temporal decay rate |

`EMBEDDING_DIM` must match the embedding model. Existing vector indexes are dimension-specific.

### Health Check

```bash
# Windows (PowerShell)
.\target\release\memory-mcp-server.exe health

# Linux / macOS
./target/release/memory-mcp-server health
```

### Debug CLI

```bash
# Windows (PowerShell) or Linux/macOS
cargo run -p memory-cli -- add --content "User prefers Rust for core services"
cargo run -p memory-cli -- search --query "preferred implementation language"
cargo run -p memory-cli -- list
cargo run -p memory-cli -- stats
cargo run -p memory-cli -- consolidate
```

### OpenCode Plugin

The plugin is a thin lifecycle adapter; memory behavior remains in Rust.

```bash
cd plugin
npm ci
npm run build
```

The built entry point is `plugin/dist/index.js`. It supports direct arrays, `{ results: [...] }`, and MCP text-content responses.

## MCP Tools

| Tool | Purpose |
| --- | --- |
| `add_memory` | Extract and store memories from text |
| `search_memories` | Hybrid semantic, BM25, and temporal retrieval |
| `get_memories` | Fetch memories by ID or filters |
| `delete_memory` | Delete a memory and clean all indexes |
| `consolidate_memories` | Apply scoped decay and consolidation |
| `get_memory_stats` | Return counts and index health data |
| `end_session` | Mark a session as ended (sets ended_at timestamp) |

Search weights must be finite, non-negative, and sum to `1.0`.

## Agent Guide

Agents working in this repository should treat `opencode-memory-system.md` as the authoritative product specification and preserve these contracts:

1. Core memory behavior belongs in Rust. TypeScript remains a thin lifecycle adapter.
2. Memory content is ADD-only. Access statistics, retention, importance, and archival metadata may be updated.
3. Duplicate detection must respect scope and project boundaries.
4. SQLite, USearch, Tantivy, and entity links must remain consistent after insertion or deletion.
5. MCP stdout is reserved for protocol messages; diagnostics go to stderr.
6. Tests use temporary isolated databases and indexes. They must not call real LLM endpoints.

### Code Discovery

This repository is indexed by `codebase-memory-mcp` as `cbrlm+D-memlong`. Prefer graph tools before text search when exploring code:

1. `search_graph` or `rlm_filter`
2. `trace_path`
3. `rlm_read_symbol` or `get_code_snippet`
4. `query_graph`
5. `get_architecture`

Use grep or file search for configuration, documentation, literal error messages, and other non-code content. Re-run `index_repository` after structural changes when the graph is stale.

### Main Code Paths

| Path | Responsibility |
| --- | --- |
| `crates/memory-core/src/service.rs` | High-level orchestration |
| `crates/memory-core/src/extraction/` | LLM extraction and embeddings |
| `crates/memory-core/src/consolidation/` | Deduplication, entity linking, decay |
| `crates/memory-core/src/retrieval/` | Hybrid ranking and filtering |
| `crates/memory-core/src/storage/` | SQLite, USearch, and Tantivy adapters |
| `crates/memory-mcp-server/src/server.rs` | MCP schemas and handlers |
| `plugin/src/index.ts` | OpenCode lifecycle bridge |

### Required Verification

```bash
cargo fmt --all -- --check
cargo test --workspace          # 15+ tests including MCP protocol smoke test
cargo bench -p memory-core      # Criterion benchmarks (add_memory, search_memories)
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
cd plugin
npm ci
npm test
```

The release server should remain below the 20 MB target documented in the specification.

## Packaging

Create the release archive and SHA256 file:

**Windows:**
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -Version 0.1.0
```

**Linux:**
```bash
./scripts/package-release.sh --version 0.1.0
```

Artifacts are written under `target/`.

## Documentation

- Full product and technical specification: [`opencode-memory-system.md`](opencode-memory-system.md)
- Condensed technical specification: [`spec.md`](spec.md)
- Implementation status: [`task.md`](task.md)
- Memory extraction skill: [`skills/memory-extraction.md`](skills/memory-extraction.md)

## License

MIT
