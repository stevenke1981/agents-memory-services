# AMS OpenCode Plugin

The OpenCode adapter is intentionally **selective, bounded, queued, and fail-open**. Memory improves the agent when relevant, but memory latency or failure must never become an application failure.

## Behavior

- Recalls both project-scoped and global memories for meaningful initial queries.
- Uses a short-lived LRU recall cache and single-flight queries to avoid duplicate MCP work.
- Filters low-relevance results and removes duplicate content before injection.
- Gives semantic and BM25 relevance priority over recency.
- Limits injected memory count and prompt size.
- Marks retrieved memories as untrusted background facts, not instructions.
- Scores completed turns and stores only durable preferences, decisions, fixes, workflows, and project knowledge.
- Redacts common API keys, bearer tokens, JWTs, database credentials, passwords, and private-key blocks before capture.
- Queues and batches writes so slow extraction never blocks the host app or silently drops every busy-period turn.
- Keeps timed-out MCP calls in the concurrency budget until the underlying promise settles.
- Uses timeouts, bounded concurrency, retries, and a circuit breaker so memory cannot stall the host app.
- Flushes queued captures at session end and rate-limits expensive consolidation work.
- Falls back to BM25 when embeddings are unavailable; falls back to semantic retrieval when BM25 is unavailable.

## Configuration

All settings are optional environment variables.

| Variable | Default | Purpose |
|---|---:|---|
| `AMS_ENABLED` | `true` | Disable all lifecycle memory behavior with `false` or `0`. |
| `AMS_RECALL_TOP_K` | `6` | Project memories requested per recall. |
| `AMS_GLOBAL_TOP_K` | `3` | Global memories requested per recall. |
| `AMS_MAX_INJECTED_MEMORIES` | `6` | Maximum memories injected into the prompt. |
| `AMS_MIN_RECALL_SCORE` | `0.30` | Minimum hybrid relevance score. |
| `AMS_MAX_PROMPT_CHARS` | `3600` | Maximum injected memory-context characters. |
| `AMS_MIN_QUERY_CHARS` | `8` | Ignore shorter initial queries. |
| `AMS_MIN_CAPTURE_CHARS` | `24` | Ignore shorter completed turns. |
| `AMS_MAX_CAPTURE_CHARS` | `12000` | Maximum captured turn size before clipping. |
| `AMS_RECALL_TIMEOUT_MS` | `1500` | Maximum time the start hook waits for recall. |
| `AMS_WRITE_TIMEOUT_MS` | `2500` | Best-effort capture/session-write timeout. |
| `AMS_CONSOLIDATION_TIMEOUT_MS` | `2000` | Best-effort consolidation timeout. |
| `AMS_MAX_CONCURRENT_CALLS` | `2` | Maximum active lifecycle MCP calls, including timed-out calls still running underneath. |
| `AMS_FAILURE_THRESHOLD` | `3` | Consecutive failures before the circuit opens. |
| `AMS_COOLDOWN_MS` | `60000` | Circuit-breaker cooldown duration. |
| `AMS_RECALL_CACHE_TTL_MS` | `60000` | Recall-cache lifetime; set `0` to disable. |
| `AMS_RECALL_CACHE_SIZE` | `64` | Maximum cached scope/query combinations; set `0` to disable. |
| `AMS_CAPTURE_QUEUE_SIZE` | `64` | Maximum pending durable turns. Higher-priority turns displace lower-priority entries when full. |
| `AMS_CAPTURE_BATCH_SIZE` | `3` | Maximum compatible turns combined into one extraction call. |
| `AMS_CAPTURE_DEBOUNCE_MS` | `250` | Delay used to batch nearby completed turns. |
| `AMS_CONSOLIDATION_INTERVAL_MS` | `21600000` | Minimum interval between lifecycle consolidations (six hours); set `0` to disable lifecycle consolidation. |

## Recommended conservative profile

```bash
export AMS_RECALL_TOP_K=4
export AMS_GLOBAL_TOP_K=2
export AMS_MAX_INJECTED_MEMORIES=4
export AMS_MIN_RECALL_SCORE=0.35
export AMS_MAX_PROMPT_CHARS=2400
export AMS_RECALL_TIMEOUT_MS=1000
export AMS_CAPTURE_QUEUE_SIZE=32
export AMS_CAPTURE_BATCH_SIZE=3
export AMS_CONSOLIDATION_INTERVAL_MS=43200000
```

## Validation

```bash
cd plugin
npm ci
npm test

cd ..
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
