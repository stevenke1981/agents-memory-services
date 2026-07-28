"use strict";
// TypeScript Thin Shim — lifecycle adapter for the Rust MCP memory service.
// The adapter is deliberately selective and fail-open so memory never blocks the host app.
Object.defineProperty(exports, "__esModule", { value: true });
exports.shouldRecall = shouldRecall;
exports.shouldCaptureTurn = shouldCaptureTurn;
exports.redactSecrets = redactSecrets;
exports.selectMemories = selectMemories;
const response_1 = require("./response");
const env = globalThis.process?.env ?? {};
const config = {
    enabled: readBoolean("AMS_ENABLED", true),
    recallTopK: readInteger("AMS_RECALL_TOP_K", 6, 1, 20),
    globalTopK: readInteger("AMS_GLOBAL_TOP_K", 3, 0, 10),
    maxInjectedMemories: readInteger("AMS_MAX_INJECTED_MEMORIES", 6, 1, 20),
    minRecallScore: readNumber("AMS_MIN_RECALL_SCORE", 0.3, 0, 1),
    maxPromptCharacters: readInteger("AMS_MAX_PROMPT_CHARS", 3600, 512, 16000),
    minQueryCharacters: readInteger("AMS_MIN_QUERY_CHARS", 8, 1, 200),
    minCaptureCharacters: readInteger("AMS_MIN_CAPTURE_CHARS", 24, 1, 1000),
    maxCaptureCharacters: readInteger("AMS_MAX_CAPTURE_CHARS", 12000, 1000, 100000),
    recallTimeoutMs: readInteger("AMS_RECALL_TIMEOUT_MS", 1500, 100, 30000),
    writeTimeoutMs: readInteger("AMS_WRITE_TIMEOUT_MS", 2500, 100, 60000),
    consolidationTimeoutMs: readInteger("AMS_CONSOLIDATION_TIMEOUT_MS", 2000, 100, 60000),
    maxConcurrentCalls: readInteger("AMS_MAX_CONCURRENT_CALLS", 2, 1, 8),
    failureThreshold: readInteger("AMS_FAILURE_THRESHOLD", 3, 1, 20),
    cooldownMs: readInteger("AMS_COOLDOWN_MS", 60000, 1000, 3600000),
};
const LOW_SIGNAL_QUERY = /^(?:hi|hello|hey|thanks|thank you|ok(?:ay)?|continue|好的?|謝謝|嗨|哈囉|在嗎|繼續|收到|嗯+|喔+)[\s!！?？。.]*$/iu;
const MEMORY_SIGNAL = /(?:remember|from now on|always|never|prefer|preference|decid(?:e|ed|ing)|choose|chosen|architecture|workflow|convention|pattern|root cause|fix(?:ed)?|bug|error|deploy|database|repository|project|must|should|記住|記得|從現在|以後|一律|永遠|偏好|決定|選擇|架構|流程|慣例|模式|根本原因|修復|錯誤|部署|資料庫|儲存庫|專案|必須|應該)/iu;
const GLOBAL_PREFERENCE_SIGNAL = /(?:from now on|always|for every project|across projects|my default|my preference|globally|以後都|從現在|所有專案|全部專案|我的偏好|預設都|一律|永遠)/iu;
let activeCalls = 0;
let consecutiveFailures = 0;
let disabledUntil = 0;
function readBoolean(name, fallback) {
    const value = env[name]?.trim().toLowerCase();
    if (value === undefined || value === "")
        return fallback;
    return !["0", "false", "off", "no"].includes(value);
}
function readNumber(name, fallback, min, max) {
    const value = Number(env[name]);
    if (!Number.isFinite(value))
        return fallback;
    return Math.min(max, Math.max(min, value));
}
function readInteger(name, fallback, min, max) {
    return Math.round(readNumber(name, fallback, min, max));
}
function normalizedText(value) {
    return value.trim().replace(/\s+/g, " ");
}
function shouldRecall(query) {
    const normalized = normalizedText(query);
    return (config.enabled &&
        normalized.length >= config.minQueryCharacters &&
        !LOW_SIGNAL_QUERY.test(normalized));
}
function shouldCaptureTurn(userMessage, assistantMessage) {
    if (!config.enabled)
        return false;
    const user = normalizedText(userMessage);
    const assistant = normalizedText(assistantMessage);
    const totalCharacters = user.length + assistant.length;
    if (totalCharacters < config.minCaptureCharacters)
        return false;
    if (LOW_SIGNAL_QUERY.test(user) && assistant.length < 160)
        return false;
    if (MEMORY_SIGNAL.test(`${user}\n${assistant}`))
        return true;
    // Longer turns usually contain durable implementation context, while short casual turns do not.
    return totalCharacters >= 500;
}
function inferScope(userMessage, projectId) {
    if (!projectId || GLOBAL_PREFERENCE_SIGNAL.test(userMessage))
        return "Global";
    return "Project";
}
function redactSecrets(value) {
    return value
        .replace(/-----BEGIN [A-Z ]+ PRIVATE KEY-----[\s\S]*?-----END [A-Z ]+ PRIVATE KEY-----/g, "[REDACTED_PRIVATE_KEY]")
        .replace(/\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}\b/g, "[REDACTED_GITHUB_TOKEN]")
        .replace(/\bAKIA[0-9A-Z]{16}\b/g, "[REDACTED_AWS_ACCESS_KEY]")
        .replace(/\b(?:sk|rk|pk)-[A-Za-z0-9_-]{16,}\b/g, "[REDACTED_API_KEY]")
        .replace(/((?:api[_ -]?key|access[_ -]?token|secret|password)\s*[:=]\s*)["']?[^\s"']{8,}["']?/giu, "$1[REDACTED]");
}
function clipText(value, maxCharacters) {
    if (value.length <= maxCharacters)
        return value;
    const marker = "\n…[truncated by AMS capture policy]…\n";
    const available = Math.max(0, maxCharacters - marker.length);
    const head = Math.ceil(available * 0.7);
    const tail = available - head;
    return `${value.slice(0, head)}${marker}${value.slice(-tail)}`;
}
function buildCaptureContent(userMessage, assistantMessage) {
    if (!shouldCaptureTurn(userMessage, assistantMessage))
        return undefined;
    const userBudget = Math.floor(config.maxCaptureCharacters * 0.35);
    const assistantBudget = config.maxCaptureCharacters - userBudget;
    const user = clipText(redactSecrets(userMessage), userBudget);
    const assistant = clipText(redactSecrets(assistantMessage), assistantBudget);
    return [`User: ${user}`, `Assistant: ${assistant}`].join("\n\n");
}
function memoryRank(memory) {
    return (memory.score_final ?? 0) * 0.8 + memory.importance_score * 0.2;
}
function selectMemories(memories) {
    const deduplicated = new Map();
    for (const memory of memories) {
        const score = memory.score_final ?? 0;
        const importantFallback = memory.importance_score >= 0.75 && score >= config.minRecallScore * 0.6;
        if (score < config.minRecallScore && !importantFallback)
            continue;
        const contentKey = normalizedText(memory.content).toLowerCase();
        const key = contentKey || memory.id;
        const existing = deduplicated.get(key);
        if (!existing || memoryRank(memory) > memoryRank(existing)) {
            deduplicated.set(key, memory);
        }
    }
    return [...deduplicated.values()]
        .sort((left, right) => memoryRank(right) - memoryRank(left))
        .slice(0, config.maxInjectedMemories);
}
function withTimeout(promise, timeoutMs, operation) {
    return new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error(`${operation} exceeded ${timeoutMs}ms`)), timeoutMs);
        promise.then((value) => {
            clearTimeout(timer);
            resolve(value);
        }, (error) => {
            clearTimeout(timer);
            reject(error);
        });
    });
}
async function safeMcpCall(client, tool, params, timeoutMs) {
    if (!config.enabled || Date.now() < disabledUntil)
        return undefined;
    if (activeCalls >= config.maxConcurrentCalls)
        return undefined;
    activeCalls += 1;
    try {
        const result = await withTimeout(client.call(tool, params), timeoutMs, tool);
        consecutiveFailures = 0;
        return result;
    }
    catch {
        consecutiveFailures += 1;
        if (consecutiveFailures >= config.failureThreshold) {
            disabledUntil = Date.now() + config.cooldownMs;
            consecutiveFailures = 0;
            console.warn(`[ams] memory calls paused for ${config.cooldownMs}ms after repeated failures`);
        }
        return undefined;
    }
    finally {
        activeCalls -= 1;
    }
}
async function searchScope(ctx, query, scope, topK) {
    if (topK <= 0)
        return [];
    const result = await safeMcpCall(ctx.mcp, "search_memories", {
        query,
        top_k: topK,
        scope,
        project_id: scope === "Project" ? ctx.projectId : undefined,
        min_importance: scope === "Global" ? 0.4 : 0.25,
        // Relevance should dominate recency to avoid recently-used but unrelated memories.
        weights: { semantic: 0.68, bm25: 0.3, temporal: 0.02 },
    }, config.recallTimeoutMs);
    return (0, response_1.parseMemoriesResponse)(result);
}
function runDetached(task) {
    queueMicrotask(() => {
        void task().catch(() => {
            // Lifecycle memory is best-effort and must never surface as an app failure.
        });
    });
}
// ─────────────────────────────────────────────────────────────
// OpenCode Plugin Core
// ─────────────────────────────────────────────────────────────
exports.default = {
    name: "ams",
    version: "1.1.0",
    hooks: {
        /**
         * Session Start: selectively retrieve relevant project and global memories.
         */
        onChatStart: async (ctx) => {
            const queryText = ctx.initialQuery?.trim() ?? "";
            if (!shouldRecall(queryText))
                return;
            const searches = [
                searchScope(ctx, queryText, "Global", config.globalTopK),
            ];
            if (ctx.projectId) {
                searches.unshift(searchScope(ctx, queryText, "Project", config.recallTopK));
            }
            const memories = selectMemories((await Promise.all(searches)).flat());
            if (memories.length === 0)
                return;
            const prompt = (0, response_1.formatMemoriesForInjection)(memories, {
                maxCharacters: config.maxPromptCharacters,
                maxItems: config.maxInjectedMemories,
            });
            if (prompt)
                ctx.injectSystemPrompt(prompt);
        },
        /**
         * Turn Complete: selectively capture durable information without blocking the host app.
         */
        onMessageComplete: async (ctx) => {
            const content = buildCaptureContent(ctx.userMessage, ctx.assistantMessage);
            if (!content)
                return;
            runDetached(async () => {
                await safeMcpCall(ctx.mcp, "add_memory", {
                    content,
                    scope: inferScope(ctx.userMessage, ctx.projectId),
                    project_id: inferScope(ctx.userMessage, ctx.projectId) === "Project"
                        ? ctx.projectId
                        : undefined,
                    session_id: ctx.sessionId,
                    metadata: {
                        source: "opencode-lifecycle",
                        capture_policy: "selective-v2",
                        secrets_redacted: true,
                    },
                }, config.writeTimeoutMs);
            });
        },
        /**
         * Session End: mark the session and consolidate in detached best-effort tasks.
         */
        onSessionEnd: async (ctx) => {
            runDetached(async () => {
                await safeMcpCall(ctx.mcp, "end_session", { session_id: ctx.sessionId }, config.writeTimeoutMs);
                await safeMcpCall(ctx.mcp, "consolidate_memories", {
                    scope: ctx.projectId ? "Project" : "Global",
                    project_id: ctx.projectId,
                }, config.consolidationTimeoutMs);
            });
        },
    },
};
