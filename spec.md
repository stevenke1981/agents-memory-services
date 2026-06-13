# Technical Specification: OpenCode Agent Long-Term Memory System (spec.md)

This specification defines the data models, database schema, extraction guidelines, consolidation rules, retrieval metrics, and API protocols for the OpenCode Agent Long-Term Memory System.

## 1. Directory Structure

The system is structured as a Cargo workspace:

```
opencode-memory/
├── Cargo.toml                          # workspace root
├── Cargo.lock
│
├── crates/
│   ├── memory-core/                    # Core library crate
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # Entry point
│   │       ├── error.rs                # Error definitions
│   │       ├── config.rs               # Environment & configuration parameters
│   │       ├── service.rs              # High-level memory service orchestrator
│   │       ├── models/                 # Data schemas
│   │       │   ├── mod.rs
│   │       │   ├── memory.rs           # Memory & Category schemas
│   │       │   └── query.rs            # Search & weights schemas
│   │       ├── extraction/             # LLM Extraction logic
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs           # Extraction engine
│   │       │   ├── prompt.rs           # Prompt strings
│   │       │   └── llm_client.rs       # HTTP LLM Client
│   │       ├── consolidation/          # Consolidation & decay logic
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs           # Consolidation engine
│   │       │   ├── dedup.rs            # Vector & Entity deduplication
│   │       │   ├── entity.rs           # Entity linking
│   │       │   └── decay.rs            # Ebbinghaus decay & stability
│   │       ├── retrieval/              # Hybrid retrieval orchestrator
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs           # Search orchestrator
│   │       │   ├── semantic.rs         # USearch HNSW retriever
│   │       │   ├── bm25.rs             # Tantivy BM25 retriever
│   │       │   └── hybrid.rs           # Reciprocal Rank Fusion / Score Fusion
│   │       └── storage/                # Database and Indexes
│   │           ├── mod.rs
│   │           ├── sqlite.rs           # sqlx Sqlite connection pool
│   │           ├── vector.rs           # USearch adapter
│   │           ├── text_index.rs       # Tantivy adapter
│   │           └── migrations/
│   │               └── V1__init.sql    # Database schema
│   │
│   ├── memory-mcp-server/              # MCP stdio executable
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── server.rs               # MCP JSON-RPC Server
│   │       └── tools/                  # MCP Tool definitions
│   │
│   └── memory-cli/                     # Debug CLI executable
│       ├── Cargo.toml
│       └── src/
│           └── main.rs
│
├── plugin/                             # TypeScript shim for OpenCode hook life-cycles
│   ├── package.json
│   ├── tsconfig.json
│   └── src/
│       └── index.ts
│
└── tests/
    └── integration/
        ├── lifecycle_test.rs
        ├── dedup_test.rs
        └── retrieval_test.rs
```

---

## 2. Core Data Models

### 2.1 Memory Record
Stored in SQLite and mapped to HNSW/BM25 indexes:
```rust
pub struct Memory {
    pub id: String,                  // UUID v4
    pub content: String,             // Self-contained third-person statement
    pub category: String,            // Fact | Preference | Decision | etc.
    pub scope: String,               // Global | Project | Session | Agent
    pub project_id: Option<String>,  // Path or ID if scope = Project
    pub agent_id: Option<String>,    // ID if scope = Agent
    pub source_session: String,      // Session ID
    pub created_at: i64,             // UNIX timestamp (ms)
    pub updated_at: i64,             // UNIX timestamp (ms)
    pub last_accessed_at: i64,       // Last hit timestamp (ms)
    pub access_count: i32,           // Number of retrieval hits
    pub importance_score: f64,       // Derived score [0.0, 1.0]
    pub retention_factor: f64,       // Ebbinghaus decay percentage [0.0, 1.0]
    pub entities: String,            // JSON array of strings
    pub vector_id: i64,              // USearch internal index ID
    pub metadata: String,            // JSON metadata map
}
```

### 2.2 Category & Scope Types
- **MemoryCategory**: `Fact`, `Preference`, `Decision`, `ProjectKnowledge`, `CodePattern`, `ErrorLesson`, `Workflow`
- **MemoryScope**: `Global`, `Project`, `Session`, `Agent`

---

## 3. Database Schema (SQLite WAL Mode)

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS memories (
    id                  TEXT    PRIMARY KEY,
    content             TEXT    NOT NULL,
    category            TEXT    NOT NULL,
    scope               TEXT    NOT NULL DEFAULT 'Global',
    project_id          TEXT,
    agent_id            TEXT,
    source_session      TEXT    NOT NULL,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    last_accessed_at    INTEGER NOT NULL,
    access_count        INTEGER NOT NULL DEFAULT 0,
    importance_score    REAL    NOT NULL DEFAULT 0.5,
    retention_factor    REAL    NOT NULL DEFAULT 1.0,
    entities            TEXT    NOT NULL DEFAULT '[]',
    vector_id           INTEGER NOT NULL,
    metadata            TEXT    NOT NULL DEFAULT '{}'
) STRICT;
```

---

## 4. Consolidation & Decay Formulas

### 4.1 ADD-only Consolidation Thresholds
- **$\ge 0.92$ Cosine Similarity**: Skip (exact duplicate), increment `access_count` on the existing memory, update `last_accessed_at`.
- **$0.75$ to $0.92$ Cosine Similarity**: Compare entity overlap. If overlap ratio is $> 0.5$, treat as synonym (increment `access_count` and skip insertion). Otherwise, insert as a new memory.
- **$< 0.75$ Cosine Similarity**: Treat as new memory and insert.

### 4.2 Importance Score Formula
$$importance\_score = 0.5 \cdot s_{llm} + 0.3 \cdot s_{access} + 0.2 \cdot s_{recency}$$
- $s_{llm} = \frac{importance}{5.0}$ (where importance is $1-5$ from extraction)
- $s_{access} = \min(1.0, \frac{access\_count}{10})$
- $s_{recency} = e^{-0.001 \cdot \Delta t_{days}}$

### 4.3 Memory Decay (Ebbinghaus Model)
$$R(t) = e^{-t / S}$$
- $R(t)$ is `retention_factor` after $t$ days.
- Stability $S$ initializes to $importance\_score \times 30.0$ days.
- S is reinforced: $S_{new} = S_{old} \times 1.2$ on each memory access.
- Memories with $R(t) < 0.1$ are archived.

---

## 5. Hybrid Retrieval

The final score is a weighted combination:
$$score\_final = \alpha \cdot s_{sem} + \beta \cdot s_{bm25} + \gamma \cdot s_{temp}$$
- **Default Weights**: $\alpha = 0.60$, $\beta = 0.30$, $\gamma = 0.10$.
- **Semantic Score**: Normalized cosine similarity from HNSW index.
- **BM25 Score**: Tantivy text relevance score, min-max normalized.
- **Temporal Score**: $e^{-0.05 \cdot \Delta t_{days}}$ where $\Delta t$ is days since `last_accessed_at`.

---

## 6. MCP Server Tools API

The server implements the Model Context Protocol over stdio:
1. `add_memory(content: string, scope?: string, project_id?: string, session_id?: string, metadata?: object)`
2. `search_memories(query: string, top_k?: number, scope?: string, project_id?: string, categories?: string[], min_importance?: number, weights?: object)`
3. `get_memories(ids?: string[], scope?: string, project_id?: string, limit?: number)`
4. `delete_memory(id: string)`
5. `consolidate_memories(scope?: string, project_id?: string)`
6. `get_memory_stats()`
