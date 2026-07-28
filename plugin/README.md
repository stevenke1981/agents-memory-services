# AMS OpenCode Plugin

The OpenCode adapter is intentionally **selective, bounded, and fail-open**. Memory improves the agent when relevant, but MCP latency or failures must never become an application failure.

## Behavior

- Recalls both project-scoped and global memories for meaningful initial queries.
- Filters low-relevance results and removes duplicate content before injection.
- Gives semantic and BM25 relevance priority over recency.
- Limits injected memory count and prompt size.
- Marks retrieved memories as untrusted background facts, not instructions.
- Skips short casual turns and stores only turns likely to contain durable information.
- Redacts common API keys, access tokens, passwords, and private-key blocks before capture.
- Runs capture, session close, and consolidation as detached best-effort tasks.
- Uses timeouts, concurrency limits, and a circuit breaker so memory cannot stall the host app.
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
| `AMS_MAX_CONCURRENT_CALLS` | `2` | Maximum active lifecycle MCP calls. |
| `AMS_FAILURE_THRESHOLD` | `3` | Consecutive failures before the circuit opens. |
| `AMS_COOLDOWN_MS` | `60000` | Circuit-breaker cooldown duration. |

## Recommended conservative profile

```bash
export AMS_RECALL_TOP_K=4
export AMS_GLOBAL_TOP_K=2
export AMS_MAX_INJECTED_MEMORIES=4
export AMS_MIN_RECALL_SCORE=0.35
export AMS_MAX_PROMPT_CHARS=2400
export AMS_RECALL_TIMEOUT_MS=1000
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
