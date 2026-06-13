use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use memory_core::{
    service::MemoryService,
    models::{MemoryScope, SearchQuery, MemoryCategory, HybridWeights},
};

pub struct MemoryMcpServer {
    service: Arc<MemoryService>,
}

#[derive(Deserialize, Debug)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Serialize, Debug)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize, Debug)]
struct JsonRpcError {
    code: i32,
    message: String,
}

impl MemoryMcpServer {
    pub fn new(service: Arc<MemoryService>) -> Self {
        Self { service }
    }

    pub async fn serve_stdio(&self) -> anyhow::Result<()> {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin).lines();
        let mut stdout = io::stdout();

        // Print initialized notification to stderr as required
        eprintln!("{}", r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#);

        while let Some(line) = reader.next_line().await? {
            let line_trimmed = line.trim();
            if line_trimmed.is_empty() {
                continue;
            }

            match serde_json::from_str::<JsonRpcRequest>(line_trimmed) {
                Ok(req) => {
                    if let Some(id) = req.id {
                        let res = self.handle_request(req.method, req.params, id).await;
                        let response_str = serde_json::to_string(&res)?;
                        stdout.write_all(response_str.as_bytes()).await?;
                        stdout.write_all(b"\n").await?;
                        stdout.flush().await?;
                    } else {
                        // Notification
                        self.handle_notification(req.method, req.params).await;
                    }
                }
                Err(err) => {
                    let res = JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: Value::Null,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("Parse error: {}", err),
                        }),
                    };
                    let response_str = serde_json::to_string(&res)?;
                    stdout.write_all(response_str.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                    stdout.flush().await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_notification(&self, method: String, _params: Option<Value>) {
        tracing::debug!("Received notification: {}", method);
    }

    async fn handle_request(&self, method: String, params: Option<Value>, id: Value) -> JsonRpcResponse {
        match method.as_str() {
            "initialize" => {
                let result = serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "opencode-memory",
                        "version": "1.0.0"
                    }
                });
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(result),
                    error: None,
                }
            }
            "tools/list" => {
                let result = serde_json::json!({
                    "tools": self.get_tools_schema()
                });
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(result),
                    error: None,
                }
            }
            "tools/call" => {
                let params_val = params.unwrap_or(Value::Null);
                let tool_name = params_val.get("name").and_then(|v| v.as_str()).unwrap_or_default();
                let arguments = params_val.get("arguments").cloned().unwrap_or(Value::Null);

                match self.call_tool(tool_name, arguments).await {
                    Ok(res_val) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(serde_json::json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&res_val).unwrap_or_default()
                                }
                            ]
                        })),
                        error: None,
                    },
                    Err(err) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32603,
                            message: format!("Tool execution error: {}", err),
                        }),
                    },
                }
            }
            _ => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", method),
                }),
            },
        }
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        match name {
            "add_memory" => {
                let content = args.get("content").and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: content".to_string())?;
                
                let scope_raw = args.get("scope").and_then(|v| v.as_str()).unwrap_or("Global");
                let scope = MemoryScope::from_str(scope_raw)
                    .ok_or_else(|| format!("Invalid scope: {}", scope_raw))?;
                
                let project_id = args.get("project_id").and_then(|v| v.as_str()).map(|s| s.to_string());
                let session_id = args.get("session_id").and_then(|v| v.as_str()).unwrap_or("default").to_string();
                let metadata = args.get("metadata").cloned();

                let memories = self.service.add_memory(content, scope, project_id, session_id, metadata)
                    .await
                    .map_err(|e| format!("Failed to add memory: {}", e))?;

                Ok(serde_json::to_value(memories).unwrap())
            }
            "search_memories" => {
                let query_str = args.get("query").and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: query".to_string())?;

                let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
                
                let scope = args.get("scope").and_then(|v| v.as_str())
                    .and_then(|s| MemoryScope::from_str(s));

                let project_id = args.get("project_id").and_then(|v| v.as_str()).map(|s| s.to_string());

                let categories = args.get("categories").and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|val| val.as_str().and_then(|s| MemoryCategory::from_str(s)))
                            .collect::<Vec<_>>()
                    });

                let min_importance = args.get("min_importance").and_then(|v| v.as_f64());

                let weights = args.get("weights").and_then(|w| {
                    let semantic = w.get("semantic").and_then(|v| v.as_f64()).unwrap_or(0.6);
                    let bm25 = w.get("bm25").and_then(|v| v.as_f64()).unwrap_or(0.3);
                    let temporal = w.get("temporal").and_then(|v| v.as_f64()).unwrap_or(0.1);
                    Some(HybridWeights { semantic, bm25, temporal })
                });

                let query = SearchQuery {
                    query: query_str.to_string(),
                    top_k,
                    scope,
                    project_id,
                    categories,
                    created_after: None,
                    min_importance,
                    include_decayed: false,
                    weights,
                };

                let results = self.service.search_memories(&query)
                    .await
                    .map_err(|e| format!("Failed to search memories: {}", e))?;

                Ok(serde_json::to_value(results).unwrap())
            }
            "get_memories" => {
                let ids = args.get("ids").and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|val| val.as_str().map(|s| s.to_string())).collect::<Vec<_>>());

                let scope = args.get("scope").and_then(|v| v.as_str())
                    .and_then(|s| MemoryScope::from_str(s));

                let project_id = args.get("project_id").and_then(|v| v.as_str()).map(|s| s.to_string());
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

                let memories = self.service.get_memories(ids, scope, project_id, limit)
                    .await
                    .map_err(|e| format!("Failed to retrieve memories: {}", e))?;

                Ok(serde_json::to_value(memories).unwrap())
            }
            "delete_memory" => {
                let id = args.get("id").and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: id".to_string())?;

                let deleted = self.service.delete_memory(id)
                    .await
                    .map_err(|e| format!("Failed to delete memory: {}", e))?;

                Ok(serde_json::to_value(deleted).unwrap())
            }
            "consolidate_memories" => {
                self.service.consolidate_memories()
                    .await
                    .map_err(|e| format!("Failed to consolidate memories: {}", e))?;

                Ok(serde_json::json!({ "status": "success" }))
            }
            "get_memory_stats" => {
                let stats = self.service.get_stats()
                    .await
                    .map_err(|e| format!("Failed to get stats: {}", e))?;

                Ok(stats)
            }
            _ => Err(format!("Unknown tool: {}", name)),
        }
    }

    fn get_tools_schema(&self) -> Value {
        serde_json::json!([
          {
            "name": "add_memory",
            "description": "Extract and store memories from conversation text using Single-Pass LLM extraction. Automatically deduplicates via ADD-only consolidation.",
            "inputSchema": {
              "type": "object",
              "required": ["content"],
              "properties": {
                "content": {
                  "type": "string",
                  "description": "Conversation text or fact to extract memories from"
                },
                "scope": {
                  "type": "string",
                  "enum": ["Global", "Project", "Session", "Agent"],
                  "default": "Global"
                },
                "project_id": {
                  "type": "string",
                  "description": "Project path or ID (required when scope=Project)"
                },
                "session_id": {
                  "type": "string"
                },
                "metadata": {
                  "type": "object",
                  "description": "Additional metadata key-value pairs"
                }
              }
            }
          },
          {
            "name": "search_memories",
            "description": "Hybrid semantic+BM25+temporal retrieval of relevant memories. Returns ranked results with score breakdown.",
            "inputSchema": {
              "type": "object",
              "required": ["query"],
              "properties": {
                "query": {
                  "type": "string",
                  "description": "Natural language search query"
                },
                "top_k": {
                  "type": "integer",
                  "default": 10,
                  "minimum": 1,
                  "maximum": 50
                },
                "scope": {
                  "type": "string",
                  "enum": ["Global", "Project", "Session", "Agent"]
                },
                "project_id": { "type": "string" },
                "categories": {
                  "type": "array",
                  "items": {
                    "type": "string",
                    "enum": ["Fact","Preference","Decision","ProjectKnowledge","CodePattern","ErrorLesson","Workflow"]
                  }
                },
                "min_importance": {
                  "type": "number",
                  "minimum": 0.0,
                  "maximum": 1.0
                },
                "weights": {
                  "type": "object",
                  "properties": {
                    "semantic": { "type": "number" },
                    "bm25": { "type": "number" },
                    "temporal": { "type": "number" }
                  }
                }
              }
            }
          },
          {
            "name": "get_memories",
            "description": "Retrieve memory records by IDs or list recent memories.",
            "inputSchema": {
              "type": "object",
              "properties": {
                "ids": {
                  "type": "array",
                  "items": { "type": "string" }
                },
                "scope": { "type": "string" },
                "project_id": { "type": "string" },
                "limit": { "type": "integer", "default": 20 }
              }
            }
          },
          {
            "name": "delete_memory",
            "description": "Delete a memory by ID. Use with caution — prefer decay archival for most cases.",
            "inputSchema": {
              "type": "object",
              "required": ["id"],
              "properties": {
                "id": { "type": "string", "description": "Memory UUID to delete" }
              }
            }
          },
          {
            "name": "consolidate_memories",
            "description": "Trigger batch consolidation: deduplication, decay update, and index compaction.",
            "inputSchema": {
              "type": "object",
              "properties": {
                "scope": { "type": "string" },
                "project_id": { "type": "string" }
              }
            }
          },
          {
            "name": "get_memory_stats",
            "description": "Return memory system statistics: total count, category breakdown, index health.",
            "inputSchema": {
              "type": "object",
              "properties": {}
            }
          }
        ])
    }
}
