// TypeScript Thin Shim — Delegate all memory orchestration to the Rust MCP Server

interface ChatContext {
  projectPath?: string;
  projectId?: string;
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

interface Memory {
  id: string;
  content: string;
  category: string;
  importance_score: number;
  score_final?: number;
}

interface McpClient {
  call(tool: string, params: Record<string, unknown>): Promise<unknown>;
}

// ─────────────────────────────────────────────────────────────
// OpenCode Plugin Core
// ─────────────────────────────────────────────────────────────
export default {
  name: "opencode-memory",
  version: "1.0.0",

  hooks: {
    /**
     * Session Start: retrieve relevant memories and inject into System Prompt
     */
    onChatStart: async (ctx: ChatContext): Promise<void> => {
      try {
        const queryText = ctx.initialQuery ?? ctx.projectPath ?? "";
        if (!queryText) return;

        const result = await ctx.mcp.call("search_memories", {
          query: queryText,
          top_k: 10,
          scope: ctx.projectId ? "Project" : "Global",
          project_id: ctx.projectId,
          min_importance: 0.3,
        }) as Memory[];

        const memories = result ?? [];
        if (memories.length === 0) return;

        ctx.injectSystemPrompt(formatMemoriesForInjection(memories));
      } catch (err) {
        console.error("[opencode-memory] onChatStart error:", err);
      }
    },

    /**
     * Turn Complete: extract and save new memories asynchronously
     */
    onMessageComplete: async (ctx: MessageContext): Promise<void> => {
      // Non-blocking: background run
      queueMicrotask(async () => {
        try {
          const conversationTurn = [
            `User: ${ctx.userMessage}`,
            `Assistant: ${ctx.assistantMessage}`,
          ].join("\n\n");

          await ctx.mcp.call("add_memory", {
            content: conversationTurn,
            scope: ctx.projectId ? "Project" : "Global",
            project_id: ctx.projectId,
            session_id: ctx.sessionId,
          });
        } catch (err) {
          console.error("[opencode-memory] onMessageComplete error:", err);
        }
      });
    },

    /**
     * Session End: trigger batch consolidation (decay and deduplication checks)
     */
    onSessionEnd: async (ctx: SessionContext): Promise<void> => {
      try {
        await ctx.mcp.call("consolidate_memories", {
          scope: ctx.projectId ? "Project" : "Global",
          project_id: ctx.projectId,
        });
      } catch (err) {
        console.error("[opencode-memory] onSessionEnd error:", err);
      }
    },
  },
};

/**
 * Formats memory context block for insertion into System Prompt
 */
function formatMemoriesForInjection(memories: Memory[]): string {
  const lines = memories.map((m, i) => {
    // If wrapped in SearchResult, the memory is nested
    const category = m.category || (m as any).memory?.category;
    const content = m.content || (m as any).memory?.content;
    return `${i + 1}. [${category}] ${content}`;
  });
  return [
    "## Relevant Memory Context",
    "(From past sessions — use as background context)",
    ...lines,
    "",
  ].join("\n");
}
