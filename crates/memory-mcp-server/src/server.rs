use memory_core::{
    models::{HybridWeights, MemoryCategory, MemoryScope, SearchQuery},
    service::MemoryService,
};
use rmcp::{
    model::{CallToolResult, Content, ServerInfo},
    tool, Error as McpError, ServerHandler,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
pub struct MemoryMcpServer {
    service: Arc<MemoryService>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tool Input Schemas
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize, JsonSchema)]
pub struct AddMemoryInput {
    #[schemars(description = "Conversation text or fact to extract memories from")]
    pub content: String,
    #[schemars(description = "Scope of the memory (Global, Project, Session, Agent)")]
    pub scope: Option<String>,
    #[schemars(description = "Project path or ID (required when scope=Project)")]
    pub project_id: Option<String>,
    #[schemars(description = "Agent instance ID (required when scope=Agent)")]
    pub agent_id: Option<String>,
    #[schemars(description = "Session ID")]
    pub session_id: Option<String>,
    #[schemars(description = "Additional metadata key-value pairs")]
    pub metadata: Option<Value>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchWeightsInput {
    pub semantic: Option<f64>,
    pub bm25: Option<f64>,
    pub temporal: Option<f64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchMemoriesInput {
    #[schemars(description = "Natural language search query")]
    pub query: String,
    #[schemars(description = "Number of memories to return")]
    pub top_k: Option<usize>,
    #[schemars(description = "Filter by scope")]
    pub scope: Option<String>,
    #[schemars(description = "Filter by project ID")]
    pub project_id: Option<String>,
    #[schemars(description = "Filter by categories")]
    pub categories: Option<Vec<String>>,
    #[schemars(description = "Minimum importance score threshold")]
    pub min_importance: Option<f64>,
    #[schemars(description = "Weights for semantic, BM25, and temporal scores")]
    pub weights: Option<SearchWeightsInput>,
    #[schemars(
        description = "Output mode: 'brief' (content+category+scores) or 'full' (complete memory with all fields). Default: 'full'"
    )]
    pub output_mode: Option<String>,
    #[schemars(
        description = "Maximum total output characters. Longer output is truncated to fit within this limit"
    )]
    pub max_output_chars: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetMemoriesInput {
    #[schemars(description = "List of memory IDs to fetch")]
    pub ids: Option<Vec<String>>,
    #[schemars(description = "Filter by scope")]
    pub scope: Option<String>,
    #[schemars(description = "Filter by project ID")]
    pub project_id: Option<String>,
    #[schemars(description = "Limit of results")]
    pub limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct DeleteMemoryInput {
    #[schemars(description = "Memory UUID to delete")]
    pub id: String,
}

#[derive(Deserialize, JsonSchema)]
#[allow(dead_code)]
pub struct ConsolidateMemoriesInput {
    #[schemars(description = "Filter consolidation by scope")]
    pub scope: Option<String>,
    #[schemars(description = "Filter consolidation by project ID")]
    pub project_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct EmptyInput {}

#[derive(Deserialize, JsonSchema)]
pub struct RepairIndexesInput {
    #[schemars(
        description = "When true, only report issues without making changes. Default: false"
    )]
    pub dry_run: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UndeleteMemoryInput {
    #[schemars(description = "Memory UUID to restore from soft-deleted status")]
    pub id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct CompactDeletedInput {
    #[schemars(
        description = "Confirm permanent deletion of all soft-deleted memories. Must be true."
    )]
    pub confirm: Option<bool>,
}

// ─────────────────────────────────────────────────────────────────────────────
// MemoryMcpServer Implementation
// ─────────────────────────────────────────────────────────────────────────────

#[tool(tool_box)]
impl MemoryMcpServer {
    pub fn new(service: Arc<MemoryService>) -> Self {
        Self { service }
    }

    pub async fn serve_stdio(self) -> anyhow::Result<()> {
        let transport = (tokio::io::stdin(), tokio::io::stdout());
        let running = rmcp::serve_server(self, transport).await?;
        running.waiting().await?;
        Ok(())
    }

    #[tool(
        name = "add_memory",
        description = "Extract and store memories from conversation text using Single-Pass LLM extraction. Call this when the user shares personal info, preferences, project context, or any durable fact you want to recall later. Automatically deduplicates via ADD-only consolidation."
    )]
    async fn add_memory(
        &self,
        #[tool(aggr)] input: AddMemoryInput,
    ) -> Result<CallToolResult, McpError> {
        let scope_raw = input.scope.as_deref().unwrap_or("Global");
        let scope = MemoryScope::from_str(scope_raw).ok_or_else(|| {
            McpError::invalid_params(format!("Invalid scope: {}", scope_raw), None)
        })?;

        // Validate scope requirements
        match &scope {
            MemoryScope::Project => {
                if input.project_id.is_none() {
                    return Err(McpError::invalid_params(
                        "project_id is required when scope=Project",
                        None,
                    ));
                }
            }
            MemoryScope::Agent => {
                if input.agent_id.is_none() {
                    return Err(McpError::invalid_params(
                        "agent_id is required when scope=Agent",
                        None,
                    ));
                }
            }
            _ => {}
        }

        let session_id = input.session_id.unwrap_or_else(|| "default".to_string());

        let memories = self
            .service
            .add_memory(
                &input.content,
                scope,
                input.project_id,
                input.agent_id,
                session_id,
                input.metadata,
            )
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to add memory: {}", e), None))?;

        let text = serde_json::to_string_pretty(&memories).map_err(|e| {
            McpError::internal_error(format!("Failed to serialize result: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "search_memories",
        description = "Hybrid semantic+BM25+temporal retrieval of relevant memories. Call this at the start of every task to recall relevant past context. Supports output_mode 'brief' (compact) or 'full' (complete metadata). Returns ranked results with score breakdown."
    )]
    async fn search_memories(
        &self,
        #[tool(aggr)] input: SearchMemoriesInput,
    ) -> Result<CallToolResult, McpError> {
        let scope = input
            .scope
            .as_deref()
            .map(|s| {
                MemoryScope::from_str(s)
                    .ok_or_else(|| McpError::invalid_params(format!("Invalid scope: {}", s), None))
            })
            .transpose()?;

        let categories = input.categories.map(|arr| {
            arr.iter()
                .filter_map(|val| MemoryCategory::from_str(val))
                .collect::<Vec<_>>()
        });

        let weights = input.weights.map(|w| {
            let semantic = w.semantic.unwrap_or(0.6);
            let bm25 = w.bm25.unwrap_or(0.3);
            let temporal = w.temporal.unwrap_or(0.1);
            HybridWeights {
                semantic,
                bm25,
                temporal,
            }
        });

        let query = SearchQuery {
            query: input.query,
            top_k: input.top_k.unwrap_or(10),
            scope,
            project_id: input.project_id,
            categories,
            created_after: None,
            min_importance: input.min_importance,
            include_decayed: false,
            weights,
        };

        query
            .validate()
            .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

        let results = self.service.search_memories(&query).await.map_err(|e| {
            McpError::internal_error(format!("Failed to search memories: {}", e), None)
        })?;

        // Apply output_mode
        let output_is_brief = input
            .output_mode
            .as_deref()
            .map(|m| m.eq_ignore_ascii_case("brief"))
            .unwrap_or(false);

        let json_value: serde_json::Value = if output_is_brief {
            // Brief mode: only content, category, and scores
            let brief: Vec<serde_json::Value> = results
                .into_iter()
                .map(|r| {
                    serde_json::json!({
                        "content": r.memory.content,
                        "category": r.memory.category,
                        "score_final": r.score_final,
                        "score_semantic": r.score_semantic,
                        "score_bm25": r.score_bm25,
                        "score_temporal": r.score_temporal,
                    })
                })
                .collect();
            serde_json::Value::Array(brief)
        } else {
            // Full mode: complete SearchResult with Memory and score breakdown
            serde_json::to_value(&results).map_err(|e| {
                McpError::internal_error(format!("Failed to serialize result: {}", e), None)
            })?
        };

        let mut text = serde_json::to_string_pretty(&json_value).map_err(|e| {
            McpError::internal_error(format!("Failed to serialize result: {}", e), None)
        })?;

        // Apply max_output_chars limit
        if let Some(max_chars) = input.max_output_chars {
            if text.len() > max_chars {
                text = text.chars().take(max_chars).collect();
                // Ensure we close cleanly; append truncation indicator
                text.push_str("\n...\n\"truncated\": true");
            }
        }

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "get_memories",
        description = "Retrieve memory records by specific IDs or list recent memories by scope/project. Use this for direct lookup when you already have memory IDs, not for relevance search."
    )]
    async fn get_memories(
        &self,
        #[tool(aggr)] input: GetMemoriesInput,
    ) -> Result<CallToolResult, McpError> {
        let scope = input
            .scope
            .as_deref()
            .map(|s| {
                MemoryScope::from_str(s)
                    .ok_or_else(|| McpError::invalid_params(format!("Invalid scope: {}", s), None))
            })
            .transpose()?;

        let limit = input.limit.unwrap_or(20);

        let memories = self
            .service
            .get_memories(input.ids, scope, input.project_id, limit)
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to retrieve memories: {}", e), None)
            })?;

        let text = serde_json::to_string_pretty(&memories).map_err(|e| {
            McpError::internal_error(format!("Failed to serialize result: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "delete_memory",
        description = "Delete a memory by ID permanently. Only use when the user explicitly requests deletion of specific information. For routine cleanup, use consolidate_memories which respects decay thresholds."
    )]
    async fn delete_memory(
        &self,
        #[tool(aggr)] input: DeleteMemoryInput,
    ) -> Result<CallToolResult, McpError> {
        let deleted = self.service.delete_memory(&input.id).await.map_err(|e| {
            McpError::internal_error(format!("Failed to delete memory: {}", e), None)
        })?;

        let text = serde_json::to_string_pretty(&deleted).map_err(|e| {
            McpError::internal_error(format!("Failed to serialize result: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "consolidate_memories",
        description = "Trigger batch maintenance: deduplication, decay calculation, and index compaction. Call periodically (e.g., every 10-20 add_memory calls) to keep storage efficient and remove low-importance memories that have fallen below decay threshold."
    )]
    async fn consolidate_memories(
        &self,
        #[tool(aggr)] input: ConsolidateMemoriesInput,
    ) -> Result<CallToolResult, McpError> {
        let scope = input
            .scope
            .as_deref()
            .map(|scope| {
                MemoryScope::from_str(scope).ok_or_else(|| {
                    McpError::invalid_params(format!("Invalid scope: {scope}"), None)
                })
            })
            .transpose()?;
        self.service
            .consolidate_memories(scope, input.project_id.as_deref())
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to consolidate memories: {}", e), None)
            })?;

        let result = serde_json::json!({ "status": "success" });
        let text = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Failed to serialize result: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "get_memory_stats",
        description = "Return memory system statistics: total count, category breakdown, and index health. Use this for monitoring and debugging — call it to check whether consolidate_memories is needed or to verify the system is functioning."
    )]
    async fn get_memory_stats(
        &self,
        #[tool(aggr)] _input: EmptyInput,
    ) -> Result<CallToolResult, McpError> {
        let stats =
            self.service.get_stats().await.map_err(|e| {
                McpError::internal_error(format!("Failed to get stats: {}", e), None)
            })?;

        let text = serde_json::to_string_pretty(&stats).map_err(|e| {
            McpError::internal_error(format!("Failed to serialize result: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "repair_indexes",
        description = "Diagnose and repair index inconsistencies: orphaned entity references and vector store vs SQLite count mismatches. Run this periodically or when get_memory_stats shows discrepancies. Embedding metadata backfill runs automatically at startup."
    )]
    async fn repair_indexes(
        &self,
        #[tool(aggr)] input: RepairIndexesInput,
    ) -> Result<CallToolResult, McpError> {
        let dry_run = input.dry_run.unwrap_or(false);

        if dry_run {
            // In dry-run mode, run diagnostics without fixing
            let stats = self.service.get_stats().await.map_err(|e| {
                McpError::internal_error(format!("Failed to get stats: {}", e), None)
            })?;
            let text = serde_json::to_string_pretty(&serde_json::json!({
                "dry_run": true,
                "message": "Dry-run mode: no changes made. Run without dry_run to apply repairs.",
                "stats": stats,
            }))
            .map_err(|e| McpError::internal_error(format!("Failed to serialize: {}", e), None))?;
            return Ok(CallToolResult::success(vec![Content::text(text)]));
        }

        let result = self.service.repair_indexes().await.map_err(|e| {
            McpError::internal_error(format!("Failed to repair indexes: {}", e), None)
        })?;

        let text = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Failed to serialize result: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "undelete_memory",
        description = "Restore a previously soft-deleted memory to active status. Note: the memory will be found by ID lookup and listing, but will not appear in hybrid search results until re-embedding occurs (via consolidate_memories)."
    )]
    async fn undelete_memory(
        &self,
        #[tool(aggr)] input: UndeleteMemoryInput,
    ) -> Result<CallToolResult, McpError> {
        let restored = self.service.undelete_memory(&input.id).await.map_err(|e| {
            McpError::internal_error(format!("Failed to undelete memory: {}", e), None)
        })?;

        let text = serde_json::to_string_pretty(&serde_json::json!({
            "restored": restored,
            "id": input.id,
        }))
        .map_err(|e| McpError::internal_error(format!("Failed to serialize: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "compact_deleted",
        description = "Permanently remove all soft-deleted memories from SQLite and indexes. This is irreversible. Use confirm=true to execute."
    )]
    async fn compact_deleted(
        &self,
        #[tool(aggr)] input: CompactDeletedInput,
    ) -> Result<CallToolResult, McpError> {
        if !input.confirm.unwrap_or(false) {
            let deleted_count: i64 = self.service.count_deleted().await.map_err(|e| {
                McpError::internal_error(format!("Failed to count deleted: {}", e), None)
            })?;
            let text = serde_json::to_string_pretty(&serde_json::json!({
                "confirm_required": true,
                "deleted_memories_count": deleted_count,
                "message": "Pass confirm=true to permanently remove all soft-deleted memories. This is irreversible."
            }))
            .map_err(|e| McpError::internal_error(format!("Failed to serialize: {}", e), None))?;
            return Ok(CallToolResult::success(vec![Content::text(text)]));
        }

        let purged = self.service.compact_deleted().await.map_err(|e| {
            McpError::internal_error(format!("Failed to compact deleted: {}", e), None)
        })?;

        let text = serde_json::to_string_pretty(&serde_json::json!({
            "purged": purged,
            "status": "completed"
        }))
        .map_err(|e| McpError::internal_error(format!("Failed to serialize: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ServerHandler implementation with Schema Normalization
// ─────────────────────────────────────────────────────────────────────────────

impl ServerHandler for MemoryMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: rmcp::model::Implementation {
                name: "opencode-memory".into(),
                version: "0.1.0".into(),
            },
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _: rmcp::model::PaginatedRequestParam,
        _: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, McpError> {
        let mut tools = Self::tool_box().list();
        for tool in &mut tools {
            let input_schema = std::sync::Arc::make_mut(&mut tool.input_schema);
            let mut val = Value::Object(std::mem::take(input_schema));
            normalize_schema(&mut val);
            if let Value::Object(normalized_map) = val {
                *input_schema = normalized_map;
            }
        }
        Ok(rmcp::model::ListToolsResult {
            next_cursor: None,
            tools,
        })
    }

    async fn call_tool(
        &self,
        call_tool_request_param: rmcp::model::CallToolRequestParam,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        let context = rmcp::handler::server::tool::ToolCallContext::new(
            self,
            call_tool_request_param,
            context,
        );
        Self::tool_box().call(context).await
    }
}

// Recursive helper to clean boolean schemas into empty objects
fn normalize_schema(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(properties) = map.get_mut("properties") {
                if let Some(prop_map) = properties.as_object_mut() {
                    for v in prop_map.values_mut() {
                        if v.is_boolean() {
                            *v = serde_json::json!({});
                        } else {
                            normalize_schema(v);
                        }
                    }
                }
            }
            if let Some(items) = map.get_mut("items") {
                if items.is_boolean() {
                    *items = serde_json::json!({});
                } else {
                    normalize_schema(items);
                }
            }
            for def_key in &["definitions", "$defs"] {
                if let Some(definitions) = map.get_mut(*def_key) {
                    if let Some(def_map) = definitions.as_object_mut() {
                        for v in def_map.values_mut() {
                            if v.is_boolean() {
                                *v = serde_json::json!({});
                            } else {
                                normalize_schema(v);
                            }
                        }
                    }
                }
            }
            for (k, v) in map.iter_mut() {
                if k != "properties" && k != "items" && k != "definitions" && k != "$defs" {
                    normalize_schema(v);
                }
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                normalize_schema(v);
            }
        }
        _ => {}
    }
}
