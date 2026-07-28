import { formatMemoriesForInjection, parseMemoriesResponse } from "./response";

export interface Memory {
  id: string;
  content: string;
  category: string;
  importance_score: number;
  score_final?: number;
  scope?: string;
  project_id?: string;
}

interface McpClient {
  call(tool: string, params: Record<string, unknown>): Promise<unknown>;
}
interface ChatContext {
  projectId?: string;
  sessionId?: string;
  initialQuery?: string;
  mcp: McpClient;
  injectSystemPrompt: (text: string) => void;
}
interface MessageContext {
  userMessage: string;
  assistantMessage: string;
  projectId?: string;
  sessionId: string;
  mcp: McpClient;
}
interface SessionContext {
  projectId?: string;
  sessionId: string;
  mcp: McpClient;
}
interface PendingCapture {
  client: McpClient;
  sessionId: string;
  scope: "Global" | "Project";
  projectId?: string;
  content: string;
  priority: number;
  attempts: number;
  enqueuedAt: number;
}
interface RecallEntry {
  expiresAt: number;
  memories: Memory[];
}
type Outcome = { status: "ok" | "unavailable" | "timeout" | "error"; value?: unknown };
type EnvMap = Record<string, string | undefined>;

const env: EnvMap =
  (globalThis as unknown as { process?: { env?: EnvMap } }).process?.env ?? {};
const numberEnv = (name: string, fallback: number, min: number, max: number): number => {
  const value = Number(env[name]);
  return Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : fallback;
};
const integerEnv = (name: string, fallback: number, min: number, max: number): number =>
  Math.round(numberEnv(name, fallback, min, max));
const boolEnv = (name: string, fallback: boolean): boolean => {
  const value = env[name]?.trim().toLowerCase();
  return value ? !["0", "false", "off", "no"].includes(value) : fallback;
};

const config = {
  enabled: boolEnv("AMS_ENABLED", true),
  recallTopK: integerEnv("AMS_RECALL_TOP_K", 6, 1, 20),
  globalTopK: integerEnv("AMS_GLOBAL_TOP_K", 3, 0, 10),
  maxInjected: integerEnv("AMS_MAX_INJECTED_MEMORIES", 6, 1, 20),
  minRecallScore: numberEnv("AMS_MIN_RECALL_SCORE", 0.3, 0, 1),
  maxPromptChars: integerEnv("AMS_MAX_PROMPT_CHARS", 3600, 512, 16000),
  minQueryChars: integerEnv("AMS_MIN_QUERY_CHARS", 8, 1, 200),
  minCaptureChars: integerEnv("AMS_MIN_CAPTURE_CHARS", 24, 1, 1000),
  maxCaptureChars: integerEnv("AMS_MAX_CAPTURE_CHARS", 12000, 1000, 100000),
  recallTimeout: integerEnv("AMS_RECALL_TIMEOUT_MS", 1500, 100, 30000),
  writeTimeout: integerEnv("AMS_WRITE_TIMEOUT_MS", 2500, 100, 60000),
  consolidationTimeout: integerEnv("AMS_CONSOLIDATION_TIMEOUT_MS", 2000, 100, 60000),
  maxCalls: integerEnv("AMS_MAX_CONCURRENT_CALLS", 2, 1, 8),
  failureThreshold: integerEnv("AMS_FAILURE_THRESHOLD", 3, 1, 20),
  cooldownMs: integerEnv("AMS_COOLDOWN_MS", 60000, 1000, 3600000),
  cacheTtl: integerEnv("AMS_RECALL_CACHE_TTL_MS", 60000, 0, 3600000),
  cacheSize: integerEnv("AMS_RECALL_CACHE_SIZE", 64, 0, 1000),
  queueSize: integerEnv("AMS_CAPTURE_QUEUE_SIZE", 64, 1, 1000),
  batchSize: integerEnv("AMS_CAPTURE_BATCH_SIZE", 3, 1, 10),
  debounceMs: integerEnv("AMS_CAPTURE_DEBOUNCE_MS", 250, 0, 10000),
  consolidationInterval: integerEnv(
    "AMS_CONSOLIDATION_INTERVAL_MS",
    21600000,
    0,
    604800000,
  ),
};

const LOW_SIGNAL =
  /^(?:hi|hello|hey|thanks|thank you|ok(?:ay)?|continue|好的?|謝謝|嗨|哈囉|在嗎|繼續|收到|嗯+|喔+)[\s!！?？。.]*$/iu;
const EXPLICIT_MEMORY =
  /(?:remember|from now on|always|never|prefer|preference|my default|keep in mind|記住|記得|從現在|以後都|一律|永遠|偏好|我的預設|不要忘記)/iu;
const DURABLE =
  /(?:decid(?:e|ed|ing)|chose|implemented|added|removed|renamed|migrat(?:e|ed|ion)|architecture|workflow|convention|pattern|root cause|fix(?:ed|ing)?|resolved|regression|deploy(?:ed|ment)?|release|database|schema|repository|project|configuration|benchmark|test(?:s)? pass|決定|選擇|實作|新增|移除|重新命名|遷移|架構|流程|慣例|模式|根本原因|修復|解決|回歸|部署|發布|資料庫|結構|儲存庫|專案|設定|基準|測試通過)/iu;
const TECHNICAL =
  /(?:\b(?:rust|typescript|javascript|python|swift|cargo|npm|mcp|api|sql|sqlite|tantivy|usearch|llama\.cpp|embedding|token|branch|commit|pull request|ci)\b|[\\/][\w.-]+|\.[a-z0-9]{1,8}\b|錯誤|例外|編譯|分支|提交|記憶|檢索)/iu;
const EPHEMERAL =
  /(?:just for now|temporary|temporarily|try this|maybe|perhaps|for this test only|先試試|暫時|臨時|也許|可能|只用於測試)/iu;
const GLOBAL_PREFERENCE =
  /(?:from now on|always|for every project|across projects|my default|my preference|globally|以後都|從現在|所有專案|全部專案|我的偏好|預設都|一律|永遠)/iu;

let activeCalls = 0;
let failures = 0;
let disabledUntil = 0;
let lastConsolidation = Date.now();
let drainTimer: ReturnType<typeof setTimeout> | undefined;
let drainPromise: Promise<void> | undefined;
const queue: PendingCapture[] = [];
const recallCache = new Map<string, RecallEntry>();
const recallInFlight = new Map<string, Promise<Memory[]>>();
const normalized = (value: string): string => value.trim().replace(/\s+/g, " ");

export function shouldRecall(query: string): boolean {
  const value = normalized(query);
  return config.enabled && value.length >= config.minQueryChars && !LOW_SIGNAL.test(value);
}

export function capturePriority(userMessage: string, assistantMessage: string): number {
  if (!config.enabled) return 0;
  const user = normalized(userMessage);
  const assistant = normalized(assistantMessage);
  const length = user.length + assistant.length;
  if (length < config.minCaptureChars || (LOW_SIGNAL.test(user) && assistant.length < 200)) return 0;
  if (EXPLICIT_MEMORY.test(user)) return 3;

  let score = 0;
  if (DURABLE.test(user)) score += 2;
  if (DURABLE.test(assistant)) score += 1;
  if (TECHNICAL.test(user)) score += 1;
  if (length >= 700) score += 1;
  if (length >= 1800) score += 1;
  if (EPHEMERAL.test(user) && !DURABLE.test(assistant)) score -= 1;
  return score >= 3 ? 3 : score >= 2 ? 2 : score >= 1 && length >= 700 ? 1 : 0;
}

export const shouldCaptureTurn = (user: string, assistant: string): boolean =>
  capturePriority(user, assistant) > 0;

export function redactSecrets(value: string): string {
  return value
    .replace(
      /-----BEGIN (?:[A-Z0-9 ]+ )?PRIVATE KEY-----[\s\S]*?-----END (?:[A-Z0-9 ]+ )?PRIVATE KEY-----/g,
      "[REDACTED_PRIVATE_KEY]",
    )
    .replace(/\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}\b/g, "[REDACTED_GITHUB_TOKEN]")
    .replace(/\bAKIA[0-9A-Z]{16}\b/g, "[REDACTED_AWS_ACCESS_KEY]")
    .replace(/\b(?:sk|rk|pk)-[A-Za-z0-9_-]{16,}\b/g, "[REDACTED_API_KEY]")
    .replace(/\bBearer\s+[A-Za-z0-9._~+/=-]{16,}\b/giu, "Bearer [REDACTED]")
    .replace(/\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b/g, "[REDACTED_JWT]")
    .replace(/((?:postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis):\/\/)[^@\s/]+@/giu, "$1[REDACTED]@")
    .replace(
      /((?:api[_ -]?key|access[_ -]?token|secret|password)\s*[:=]\s*)["']?[^\s"']{8,}["']?/giu,
      "$1[REDACTED]",
    );
}

function clip(value: string, limit: number): string {
  if (value.length <= limit) return value;
  const marker = "\n…[truncated by AMS capture policy]…\n";
  const available = Math.max(0, limit - marker.length);
  const head = Math.ceil(available * 0.7);
  return `${value.slice(0, head)}${marker}${value.slice(-(available - head))}`;
}

function buildCapture(userMessage: string, assistantMessage: string) {
  const priority = capturePriority(userMessage, assistantMessage);
  if (!priority) return undefined;
  const userBudget = Math.floor(config.maxCaptureChars * 0.35);
  return {
    priority,
    content: [
      `User: ${clip(redactSecrets(userMessage), userBudget)}`,
      `Assistant: ${clip(redactSecrets(assistantMessage), config.maxCaptureChars - userBudget)}`,
    ].join("\n\n"),
  };
}

const rank = (memory: Memory): number =>
  (memory.score_final ?? 0) * 0.8 + memory.importance_score * 0.2;
export function selectMemories(memories: Memory[]): Memory[] {
  const unique = new Map<string, Memory>();
  for (const memory of memories) {
    const score = memory.score_final ?? 0;
    if (
      score < config.minRecallScore &&
      !(memory.importance_score >= 0.75 && score >= config.minRecallScore * 0.6)
    ) continue;
    const key = normalized(memory.content).toLowerCase() || memory.id;
    const existing = unique.get(key);
    if (!existing || rank(memory) > rank(existing)) unique.set(key, memory);
  }
  return [...unique.values()].sort((a, b) => rank(b) - rank(a)).slice(0, config.maxInjected);
}

function timeout<T>(promise: Promise<T>, milliseconds: number, operation: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`${operation} exceeded ${milliseconds}ms`)), milliseconds);
    promise.then(
      (value) => { clearTimeout(timer); resolve(value); },
      (error: unknown) => { clearTimeout(timer); reject(error); },
    );
  });
}

async function callOutcome(
  client: McpClient,
  tool: string,
  params: Record<string, unknown>,
  milliseconds: number,
): Promise<Outcome> {
  if (!config.enabled || Date.now() < disabledUntil || activeCalls >= config.maxCalls) {
    return { status: "unavailable" };
  }
  activeCalls += 1;
  const operation = Promise.resolve().then(() => client.call(tool, params));
  void operation.then(
    () => { activeCalls = Math.max(0, activeCalls - 1); failures = 0; },
    () => { activeCalls = Math.max(0, activeCalls - 1); },
  );

  try {
    const value = await timeout(operation, milliseconds, tool);
    failures = 0;
    return { status: "ok", value };
  } catch (error) {
    failures += 1;
    if (failures >= config.failureThreshold) {
      disabledUntil = Date.now() + config.cooldownMs;
      failures = 0;
      console.warn(`[ams] memory calls paused for ${config.cooldownMs}ms after repeated failures`);
    }
    const message = error instanceof Error ? error.message : String(error);
    return { status: message.includes(" exceeded ") ? "timeout" : "error" };
  }
}

async function safeCall(
  client: McpClient,
  tool: string,
  params: Record<string, unknown>,
  milliseconds: number,
): Promise<unknown | undefined> {
  const result = await callOutcome(client, tool, params, milliseconds);
  return result.status === "ok" ? result.value : undefined;
}

function cacheKey(query: string, scope: "Global" | "Project", projectId?: string): string {
  return `${scope}\u0000${projectId ?? ""}\u0000${normalized(query).toLowerCase()}`;
}
function cached(key: string): Memory[] | undefined {
  if (!config.cacheTtl || !config.cacheSize) return undefined;
  const entry = recallCache.get(key);
  if (!entry || entry.expiresAt <= Date.now()) {
    recallCache.delete(key);
    return undefined;
  }
  recallCache.delete(key);
  recallCache.set(key, entry);
  return entry.memories.map((memory) => ({ ...memory }));
}
function cache(key: string, memories: Memory[]): void {
  if (!config.cacheTtl || !config.cacheSize) return;
  recallCache.delete(key);
  recallCache.set(key, {
    expiresAt: Date.now() + config.cacheTtl,
    memories: memories.map((memory) => ({ ...memory })),
  });
  while (recallCache.size > config.cacheSize) {
    const oldest = recallCache.keys().next().value as string | undefined;
    if (!oldest) break;
    recallCache.delete(oldest);
  }
}

async function searchScope(
  ctx: ChatContext,
  query: string,
  scope: "Global" | "Project",
  topK: number,
): Promise<Memory[]> {
  if (!topK) return [];
  const key = cacheKey(query, scope, scope === "Project" ? ctx.projectId : undefined);
  const hit = cached(key);
  if (hit) return hit;
  const running = recallInFlight.get(key);
  if (running) return running;

  const pending = (async () => {
    const result = await safeCall(
      ctx.mcp,
      "search_memories",
      {
        query,
        top_k: topK,
        scope,
        project_id: scope === "Project" ? ctx.projectId : undefined,
        session_id: ctx.sessionId,
        min_importance: scope === "Global" ? 0.4 : 0.25,
        min_score: config.minRecallScore * 0.5,
        compact: true,
        weights: { semantic: 0.68, bm25: 0.3, temporal: 0.02 },
      },
      config.recallTimeout,
    );
    const memories = result === undefined ? [] : parseMemoriesResponse(result);
    if (result !== undefined) cache(key, memories);
    return memories;
  })().finally(() => recallInFlight.delete(key));
  recallInFlight.set(key, pending);
  return pending;
}

const sameGroup = (a: PendingCapture, b: PendingCapture): boolean =>
  a.client === b.client && a.sessionId === b.sessionId && a.scope === b.scope && a.projectId === b.projectId;
function scheduleDrain(delay = config.debounceMs): void {
  if (drainTimer !== undefined || drainPromise !== undefined) return;
  drainTimer = setTimeout(() => { drainTimer = undefined; void drain(); }, delay);
}
function enqueue(item: PendingCapture): void {
  if (queue.length >= config.queueSize) {
    let lowest = 0;
    for (let index = 1; index < queue.length; index += 1) {
      if (
        queue[index].priority < queue[lowest].priority ||
        (queue[index].priority === queue[lowest].priority && queue[index].enqueuedAt < queue[lowest].enqueuedAt)
      ) lowest = index;
    }
    if (item.priority < queue[lowest].priority) return;
    queue.splice(lowest, 1);
  }
  queue.push(item);
  scheduleDrain();
}
function takeBatch(): PendingCapture[] {
  const first = queue.shift();
  if (!first) return [];
  const batch = [first];
  for (let index = 0; index < queue.length && batch.length < config.batchSize; ) {
    if (sameGroup(first, queue[index])) batch.push(...queue.splice(index, 1));
    else index += 1;
  }
  return batch;
}
function combine(batch: PendingCapture[]): string {
  if (batch.length === 1) return batch[0].content;
  const budget = Math.max(256, Math.floor(config.maxCaptureChars / batch.length));
  return batch
    .map((item, index) => `--- Durable turn ${index + 1} ---\n${clip(item.content, budget)}`)
    .join("\n\n");
}

function drain(): Promise<void> {
  if (drainTimer !== undefined) { clearTimeout(drainTimer); drainTimer = undefined; }
  if (drainPromise) return drainPromise;
  let nextDelay = config.debounceMs;
  drainPromise = (async () => {
    while (queue.length) {
      if (Date.now() < disabledUntil || activeCalls >= config.maxCalls) {
        nextDelay = Math.max(100, disabledUntil - Date.now());
        break;
      }
      const batch = takeBatch();
      if (!batch.length) break;
      const first = batch[0];
      const outcome = await callOutcome(
        first.client,
        "add_memory",
        {
          content: combine(batch),
          scope: first.scope,
          project_id: first.scope === "Project" ? first.projectId : undefined,
          session_id: first.sessionId,
          metadata: {
            source: "opencode-lifecycle",
            capture_policy: "selective-queued-v3",
            capture_priority: Math.max(...batch.map((item) => item.priority)),
            batch_size: batch.length,
            secrets_redacted: true,
          },
        },
        config.writeTimeout,
      );
      if (outcome.status === "unavailable") {
        queue.unshift(...batch);
        nextDelay = Math.max(100, disabledUntil - Date.now());
        break;
      }
      if (
        (outcome.status === "timeout" || outcome.status === "error") &&
        Math.max(...batch.map((item) => item.attempts)) < 1
      ) {
        for (const item of batch) { item.attempts += 1; queue.push(item); }
      }
      recallCache.clear();
    }
  })().finally(() => {
    drainPromise = undefined;
    if (queue.length) scheduleDrain(nextDelay);
  });
  return drainPromise;
}

function shouldConsolidate(): boolean {
  if (!config.consolidationInterval || Date.now() - lastConsolidation < config.consolidationInterval) return false;
  lastConsolidation = Date.now();
  return true;
}
function detached(task: () => Promise<void>): void {
  queueMicrotask(() => { void task().catch(() => undefined); });
}

export default {
  name: "ams",
  version: "1.2.0",
  hooks: {
    onChatStart: async (ctx: ChatContext): Promise<void> => {
      const query = ctx.initialQuery?.trim() ?? "";
      if (!shouldRecall(query)) return;
      const searches = [searchScope(ctx, query, "Global", config.globalTopK)];
      if (ctx.projectId) searches.unshift(searchScope(ctx, query, "Project", config.recallTopK));
      const memories = selectMemories((await Promise.all(searches)).flat());
      if (!memories.length) return;
      const prompt = formatMemoriesForInjection(memories, {
        maxCharacters: config.maxPromptChars,
        maxItems: config.maxInjected,
      });
      if (prompt) ctx.injectSystemPrompt(prompt);
    },
    onMessageComplete: async (ctx: MessageContext): Promise<void> => {
      const capture = buildCapture(ctx.userMessage, ctx.assistantMessage);
      if (!capture) return;
      const scope = !ctx.projectId || GLOBAL_PREFERENCE.test(ctx.userMessage) ? "Global" : "Project";
      enqueue({
        client: ctx.mcp,
        sessionId: ctx.sessionId,
        scope,
        projectId: scope === "Project" ? ctx.projectId : undefined,
        content: capture.content,
        priority: capture.priority,
        attempts: 0,
        enqueuedAt: Date.now(),
      });
    },
    onSessionEnd: async (ctx: SessionContext): Promise<void> => {
      detached(async () => {
        await drain();
        await safeCall(ctx.mcp, "end_session", { session_id: ctx.sessionId }, config.writeTimeout);
        if (shouldConsolidate()) {
          await safeCall(
            ctx.mcp,
            "consolidate_memories",
            { scope: ctx.projectId ? "Project" : "Global", project_id: ctx.projectId },
            config.consolidationTimeout,
          );
        }
      });
    },
  },
};
