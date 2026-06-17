use std::io::{BufRead, BufReader, Read, Write};
use std::process::Command;
use std::time::Duration;

/// Locate the server binary relative to the current test executable.
fn server_binary() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_memory-mcp-server"))
}

/// Create a temporary directory for the test database/indexes and return (dir, env_overrides).
struct TestEnv {
    _dir: tempfile::TempDir,
    envs: Vec<(String, String)>,
}

impl TestEnv {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let db = dir.path().join("test.db");
        let vec_path = dir.path().join("test.usearch");
        let idx_path = dir.path().join("tantivy");

        let envs = vec![
            (
                "MEMORY_DB_PATH".to_string(),
                db.to_string_lossy().to_string(),
            ),
            (
                "MEMORY_VECTOR_PATH".to_string(),
                vec_path.to_string_lossy().to_string(),
            ),
            (
                "MEMORY_TANTIVY_PATH".to_string(),
                idx_path.to_string_lossy().to_string(),
            ),
            ("LLM_API_KEY".to_string(), "mock".to_string()),
            ("LLM_API_BASE".to_string(), "mock".to_string()),
        ];
        Self { _dir: dir, envs }
    }
}

/// Read one JSON-RPC line from the reader (with timeout).
fn read_response<R: BufRead>(reader: &mut R, label: &str) -> serde_json::Value {
    let mut line = String::new();
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > Duration::from_secs(10) {
            panic!("Timeout waiting for {label} — no response in 10s");
        }
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                panic!("Server closed stdout while waiting for {label}");
            }
            Ok(_) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    return serde_json::from_str(trimmed).unwrap_or_else(|e| {
                        panic!("Invalid JSON in {label}: {e}\nRaw: {trimmed}")
                    });
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                panic!("IO error reading {label}: {e}");
            }
        }
    }
}

/// Send a JSON-RPC message.
fn send_message(stdin: &mut impl Write, msg: &serde_json::Value) {
    let line = serde_json::to_string(msg).expect("serialize msg") + "\n";
    stdin.write_all(line.as_bytes()).expect("write to stdin");
    stdin.flush().expect("flush stdin");
}

/// ─────────────────────────────────────────────────────────────
/// Test: health command
/// ─────────────────────────────────────────────────────────────
#[test]
fn health_check_returns_ok() {
    let env = TestEnv::new();
    let bin = server_binary();

    assert!(bin.exists(), "server binary not found at {}", bin.display());

    let output = Command::new(&bin)
        .arg("health")
        .envs(env.envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .output()
        .expect("failed to run health command");

    assert!(output.status.success(), "health exited with failure");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value =
        serde_json::from_str(&stdout).expect("health output must be valid JSON");

    assert_eq!(
        result["status"], "ok",
        "Health status should be 'ok', got: {stdout}"
    );
}

/// ─────────────────────────────────────────────────────────────
/// Test: full MCP protocol lifecycle
/// ─────────────────────────────────────────────────────────────
#[test]
fn mcp_protocol_full_lifecycle() {
    let env = TestEnv::new();
    let bin = server_binary();

    assert!(bin.exists(), "server binary not found at {}", bin.display());

    // Spawn the server with env overrides
    let mut child = Command::new(&bin)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .envs(env.envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .spawn()
        .expect("failed to spawn server");

    let mut stdin = child.stdin.take().expect("stdin");
    let mut reader = BufReader::new(child.stdout.take().expect("stdout"));

    // ── Phase 1: initialize ──
    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {
                "name": "mcp-smoke-test",
                "version": "1.0"
            }
        }
    });
    send_message(&mut stdin, &init_req);

    let init_resp = read_response(&mut reader, "initialize response");
    assert_eq!(init_resp["jsonrpc"], "2.0", "initialize: jsonrpc version");
    assert_eq!(
        init_resp["id"], 1,
        "initialize: response id must match request id"
    );
    assert!(
        init_resp["result"].is_object(),
        "initialize: must have 'result' object, got: {init_resp}"
    );
    eprintln!(
        "  ✓ initialize: protocolVersion={}",
        init_resp["result"]["protocolVersion"]
            .as_str()
            .unwrap_or("unknown")
    );

    // ── Phase 2: initialized notification ──
    let notif = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
        "params": {}
    });
    send_message(&mut stdin, &notif);
    std::thread::sleep(Duration::from_millis(200));

    // ── Phase 3: tools/list ──
    let tools_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });
    send_message(&mut stdin, &tools_req);

    let tools_resp = read_response(&mut reader, "tools/list response");
    assert_eq!(tools_resp["id"], 2, "tools/list: id mismatch");

    let tools = tools_resp["result"]["tools"]
        .as_array()
        .expect("tools must be an array");
    assert!(
        !tools.is_empty(),
        "tools/list should return at least one tool"
    );

    let tool_names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    eprintln!("  ✓ tools/list returned {} tools:", tool_names.len());
    for name in &tool_names {
        eprintln!("    - {name}");
    }

    // Verify essential tools exist
    assert!(
        tool_names.contains(&"add_memory"),
        "tools must include 'add_memory', got: {tool_names:?}"
    );
    assert!(
        tool_names.contains(&"search_memories"),
        "tools must include 'search_memories', got: {tool_names:?}"
    );
    assert!(
        tool_names.contains(&"get_memory_stats"),
        "tools must include 'get_memory_stats', got: {tool_names:?}"
    );

    // ── Phase 4: add_memory ──
    let add_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "add_memory",
            "arguments": {
                "content": "The user prefers Rust for backend services and TypeScript for frontend work.",
                "scope": "Session",
                "session_id": "smoke-test-session"
            }
        }
    });
    send_message(&mut stdin, &add_req);

    let add_resp = read_response(&mut reader, "add_memory response");
    assert_eq!(add_resp["id"], 3, "add_memory: id mismatch");
    assert!(
        add_resp.get("error").is_none(),
        "add_memory should not error, got: {:?}",
        add_resp.get("error")
    );

    let add_content = add_resp["result"]["content"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|c| c["text"].as_str())
        .unwrap_or("[]");
    let added: serde_json::Value =
        serde_json::from_str(add_content).unwrap_or(serde_json::Value::Null);
    eprintln!(
        "  ✓ add_memory returned {} memories",
        added.as_array().map(|a| a.len()).unwrap_or(0)
    );

    // ── Phase 5: search_memories ──
    let search_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "search_memories",
            "arguments": {
                "query": "Rust backend TypeScript frontend",
                "top_k": 5
            }
        }
    });
    send_message(&mut stdin, &search_req);

    let search_resp = read_response(&mut reader, "search_memories response");
    assert_eq!(search_resp["id"], 4, "search_memories: id mismatch");
    assert!(
        search_resp.get("error").is_none(),
        "search_memories should not error, got: {:?}",
        search_resp.get("error")
    );

    // ── Phase 6: get_memory_stats ──
    let stats_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "get_memory_stats",
            "arguments": {}
        }
    });
    send_message(&mut stdin, &stats_req);

    let stats_resp = read_response(&mut reader, "get_memory_stats response");
    assert_eq!(stats_resp["id"], 5, "stats: id mismatch");
    assert!(
        stats_resp.get("error").is_none(),
        "get_memory_stats should not error, got: {:?}",
        stats_resp.get("error")
    );
    eprintln!("  ✓ get_memory_stats returned successfully");

    // ── Cleanup ──
    drop(stdin);
    let status = child.wait().expect("wait for server");
    if !status.success() {
        let stderr = child.stderr.take().map(|s| {
            let mut buf = String::new();
            BufReader::new(s).read_to_string(&mut buf).ok();
            buf
        });
        eprintln!("  ⚠ Server exited with status: {status}");
        if let Some(err) = stderr {
            eprintln!("  Server stderr:\n{err}");
        }
    } else {
        eprintln!("  ✓ Server exited cleanly");
    }
}
