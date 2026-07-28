# Non-intrusive memory runtime

AMS is designed as an optional enhancement. The host application must continue normally when extraction, embeddings, indexes, or the MCP transport are slow or unavailable.

## Retrieval

`search_memories` combines semantic, BM25, and temporal scores. If either semantic or BM25 retrieval is unavailable, AMS re-normalizes the configured weights over the paths that remain. This prevents a healthy fallback path from being artificially capped by its original partial weight.

The MCP tool accepts these optional controls:

- `min_score`: discard results below a final relevance threshold in `[0.0, 1.0]`.
- `compact`: return prompt-oriented objects instead of the complete persisted memory record.
- `created_after`: filter by creation timestamp in Unix milliseconds.
- `include_decayed`: include archived/decayed memories.
- `top_k`: bounded to `1..=100`.

Queries are rejected when blank, over 16,384 characters, or otherwise outside validated limits.

## Extraction and embedding HTTP limits

| Variable | Default | Purpose |
|---|---:|---|
| `LLM_CONNECT_TIMEOUT_MS` | `3000` | Connection establishment timeout for both endpoints. |
| `LLM_CHAT_TIMEOUT_MS` | `60000` | Complete extraction request timeout. |
| `EMBEDDING_TIMEOUT_MS` | `5000` | Embedding request timeout. |

Values are clamped to safe ranges. The OpenCode lifecycle adapter also applies shorter caller-side budgets; a caller-side timeout does not release the concurrency slot until the underlying MCP promise settles.

## Capture queue

The OpenCode adapter scores completed turns before enqueueing them. Explicit preferences and durable project decisions receive priority; greetings and transient troubleshooting chatter are ignored. Pending captures are bounded, grouped by session/scope, redacted, and batched before `add_memory` calls.

A full queue retains the most useful durable turns by allowing higher-priority entries to displace lower-priority entries. One retry is permitted after a timeout or error. Writes remain detached from the host application's response path.

See `plugin/README.md` for all adapter controls.

## Storage path safety

`PROJECT_ROOT` ignores empty and unexpanded template values such as `${PROJECT_ROOT}`, `$PROJECT_ROOT`, `$(pwd)`, or `%PROJECT_ROOT%`. AMS falls back to the current working directory instead of creating literal placeholder directories inside the host application.
