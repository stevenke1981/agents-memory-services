import type { Memory } from "./index";
export interface MemoryFormatOptions {
    maxCharacters?: number;
    maxItems?: number;
}
/**
 * Normalise an MCP tool call response into a `Memory[]`.
 * Handles direct arrays, `{ results: [...] }`, and MCP text-content responses.
 */
export declare function parseMemoriesResponse(value: unknown): Memory[];
/**
 * Format memories as a bounded, explicitly untrusted context block.
 * Memory content is flattened and XML-escaped to reduce prompt-injection risk.
 */
export declare function formatMemoriesForInjection(memories: unknown[], options?: MemoryFormatOptions): string;
