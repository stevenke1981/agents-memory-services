export interface Memory {
    id: string;
    content: string;
    category: string;
    importance_score: number;
    score_final?: number;
}
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
interface McpClient {
    call(tool: string, params: Record<string, unknown>): Promise<unknown>;
}
export declare function shouldRecall(query: string): boolean;
export declare function shouldCaptureTurn(userMessage: string, assistantMessage: string): boolean;
export declare function redactSecrets(value: string): string;
export declare function selectMemories(memories: Memory[]): Memory[];
declare const _default: {
    name: string;
    version: string;
    hooks: {
        /**
         * Session Start: selectively retrieve relevant project and global memories.
         */
        onChatStart: (ctx: ChatContext) => Promise<void>;
        /**
         * Turn Complete: selectively capture durable information without blocking the host app.
         */
        onMessageComplete: (ctx: MessageContext) => Promise<void>;
        /**
         * Session End: mark the session and consolidate in detached best-effort tasks.
         */
        onSessionEnd: (ctx: SessionContext) => Promise<void>;
    };
};
export default _default;
