
-- Core Memory Table
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
    metadata            TEXT    NOT NULL DEFAULT '{}' -- JSON object
) STRICT;

-- Indices for query optimization
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

-- Entity Index Table for Entity Linking
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

-- Session Stats Table
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

-- System Config Table
CREATE TABLE IF NOT EXISTS system_config (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

-- Initial configurations
INSERT OR IGNORE INTO system_config (key, value)
VALUES
    ('schema_version', '1'),
    ('vector_dimensions', '1536'),
    ('embedding_model', 'unknown');
