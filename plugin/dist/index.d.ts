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
export declare function shouldRecall(query: string): boolean;
export declare function capturePriority(userMessage: string, assistantMessage: string): number;
export declare const shouldCaptureTurn: (user: string, assistant: string) => boolean;
export declare function redactSecrets(value: string): string;
export declare function selectMemories(memories: Memory[]): Memory[];
declare const _default: {
    name: string;
    version: string;
    hooks: {
        onChatStart: (ctx: ChatContext) => Promise<void>;
        onMessageComplete: (ctx: MessageContext) => Promise<void>;
        onSessionEnd: (ctx: SessionContext) => Promise<void>;
    };
};
export default _default;
