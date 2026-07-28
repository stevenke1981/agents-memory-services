use memory_core::{config::MemoryConfig, service::MemoryService};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::signal;
use tracing_subscriber::fmt::format::FmtSpan;

mod server;

const SERVER_NAME: &str = "ams";
const LEGACY_SERVER_NAMES: &[&str] = &[
    "opencode-memory",
    "memory-mcp-server",
    "memory-mcp",
    "memlong-memory",
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        let cmd = args[0].as_str();
        match cmd {
            "--version" | "-V" | "version" => {
                println!("ams v0.1.0");
                return Ok(());
            }
            "health" => {
                let result = run_health_check().await;
                println!("{}", serde_json::to_string_pretty(&result)?);
                if result.get("status").and_then(|v| v.as_str()) != Some("ok") {
                    std::process::exit(1);
                }
                return Ok(());
            }
            "install" => {
                let json_output = args.iter().skip(1).any(|arg| arg == "--json");
                run_install(json_output).await?;
                return Ok(());
            }
            _ => {
                eprintln!("Unknown command: {}", cmd);
                eprintln!("Available commands: --version, health, install [--json]");
                std::process::exit(1);
            }
        }
    }

    // Log to stderr (MCP requires stdout to be clean for JSON-RPC messages)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    tracing::info!("Memory MCP Server starting...");

    // Read config from env
    let config = MemoryConfig::from_env()?;
    tracing::info!("DB path: {}", config.db_path);

    // Initialize Memory Service (creates DB, HNSW vector index, Tantivy index)
    let service = Arc::new(MemoryService::new(config).await?);
    tracing::info!("Memory service initialized");

    // Spawn background decay scheduler (runs every 24 hours)
    let scheduler = memory_core::consolidation::DecayScheduler::new(
        service.consolidation_engine(),
        std::time::Duration::from_secs(24 * 60 * 60),
    );
    let scheduler_handle = tokio::spawn(async move {
        scheduler.run().await;
    });
    tracing::info!("Decay scheduler spawned (24h interval)");

    // Keep a clone for the shutdown handler
    let shutdown_service = service.clone();

    // Launch MCP server with graceful shutdown
    let server = server::MemoryMcpServer::new(service);

    tokio::select! {
        result = server.serve_stdio() => {
            if let Err(e) = result {
                tracing::error!("MCP server error: {e}");
            }
        }
        _ = signal::ctrl_c() => {
            tracing::info!("Received SIGINT, starting graceful shutdown...");
        }
    }

    // Cancel background scheduler
    scheduler_handle.abort();

    // Run final consolidation before exit
    tracing::info!("Running final batch consolidation...");
    if let Err(e) = shutdown_service.consolidate_memories(None, None).await {
        tracing::error!("Final consolidation failed: {e}");
    }
    tracing::info!("Memory server shut down gracefully");

    Ok(())
}

async fn run_health_check() -> serde_json::Value {
    let config_res = MemoryConfig::from_env();
    let config = match config_res {
        Ok(c) => c,
        Err(e) => {
            return serde_json::json!({
                "status": "error",
                "reason": format!("Failed to load config: {}", e)
            })
        }
    };

    match MemoryService::new(config).await {
        Ok(_) => serde_json::json!({
            "status": "ok",
            "database": "connected",
            "vector_store": "ready",
            "text_index": "ready"
        }),
        Err(e) => serde_json::json!({
            "status": "error",
            "reason": format!("Failed to initialize MemoryService: {}", e)
        }),
    }
}

#[derive(Debug, Serialize)]
struct InstallReport {
    server_name: &'static str,
    binary_path: String,
    configured_clients: Vec<ClientConfigReport>,
    restart_required: bool,
}

#[derive(Debug, Serialize)]
struct ClientConfigReport {
    client: &'static str,
    path: String,
    status: &'static str,
    message: String,
}

async fn run_install(json_output: bool) -> anyhow::Result<()> {
    let current_exe = std::env::current_exe()?;
    let exe_path_str = current_exe.to_string_lossy().to_string();
    if !json_output {
        println!(
            "Installing agent configurations using binary path: {}",
            exe_path_str
        );
    }

    let user_profile = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| anyhow::anyhow!("Could not find user profile directory"))?;

    let mut configured_clients = Vec::new();

    for path in opencode_config_paths(&user_profile) {
        match update_opencode_config(&path, &exe_path_str) {
            Ok(()) => {
                if !json_output {
                    println!("Successfully configured OpenCode at {}", path.display());
                }
                configured_clients.push(ClientConfigReport {
                    client: "opencode",
                    path: path.to_string_lossy().to_string(),
                    status: "configured",
                    message: format!("registered {SERVER_NAME} MCP server"),
                });
            }
            Err(e) => {
                eprintln!(
                    "Warning: Failed to update OpenCode config at {}: {}",
                    path.display(),
                    e
                );
                configured_clients.push(ClientConfigReport {
                    client: "opencode",
                    path: path.to_string_lossy().to_string(),
                    status: "error",
                    message: e.to_string(),
                });
            }
        }
    }

    for path in codex_config_paths(&user_profile) {
        match update_codex_config(&path, &exe_path_str) {
            Ok(()) => {
                if !json_output {
                    println!("Successfully configured Codex at {}", path.display());
                }
                configured_clients.push(ClientConfigReport {
                    client: "codex",
                    path: path.to_string_lossy().to_string(),
                    status: "configured",
                    message: format!("registered {SERVER_NAME} MCP server"),
                });
            }
            Err(e) => {
                eprintln!(
                    "Warning: Failed to update Codex config at {}: {}",
                    path.display(),
                    e
                );
                configured_clients.push(ClientConfigReport {
                    client: "codex",
                    path: path.to_string_lossy().to_string(),
                    status: "error",
                    message: e.to_string(),
                });
            }
        }
    }

    let report = InstallReport {
        server_name: SERVER_NAME,
        binary_path: exe_path_str,
        restart_required: configured_clients
            .iter()
            .any(|entry| entry.status == "configured"),
        configured_clients,
    };

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if report.restart_required {
        println!(
            "Installation complete! Please restart your OpenCode/Codex agent to apply changes."
        );
    } else {
        println!("No OpenCode or Codex configuration was updated.");
    }

    Ok(())
}

fn opencode_config_paths(user_profile: &str) -> Vec<PathBuf> {
    let dir = Path::new(user_profile).join(".config").join("opencode");
    let jsonc_path = dir.join("opencode.jsonc");
    let json_path = dir.join("opencode.json");

    let mut paths = Vec::new();
    if jsonc_path.exists() {
        paths.push(jsonc_path);
    }
    if json_path.exists() {
        paths.push(json_path.clone());
    }
    if paths.is_empty() {
        paths.push(json_path);
    }
    paths
}

fn codex_config_paths(user_profile: &str) -> Vec<PathBuf> {
    let codex_path = Path::new(user_profile).join(".codex").join("config.toml");
    let claude_codex_path = Path::new(user_profile)
        .join(".claude")
        .join(".codex")
        .join("config.toml");
    let mut paths = vec![codex_path];
    if claude_codex_path.exists() {
        paths.push(claude_codex_path);
    }
    paths
}

fn update_opencode_config(path: &Path, exe_path: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        "{}".to_string()
    };

    let mut config: serde_json::Value = serde_json::from_str(&content)
        .or_else(|_| serde_json::from_str(&strip_jsonc_comments(&content)))
        .unwrap_or(serde_json::json!({}));
    if !config.is_object() {
        config = serde_json::json!({});
    }

    let mcp = config
        .as_object_mut()
        .unwrap()
        .entry("mcp".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !mcp.is_object() {
        *mcp = serde_json::json!({});
    }

    let mcp_object = mcp.as_object_mut().unwrap();
    for legacy_name in LEGACY_SERVER_NAMES {
        mcp_object.remove(*legacy_name);
    }
    mcp_object.insert(
        SERVER_NAME.to_string(),
        serde_json::json!({
            "type": "local",
            "command": [exe_path],
            "enabled": true,
            "timeout": 120000,
            "environment": {}
        }),
    );

    let new_content = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, new_content)?;
    Ok(())
}

fn strip_jsonc_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }

        if ch == '/' {
            match chars.peek().copied() {
                Some('/') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if next == '\n' {
                            output.push('\n');
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    chars.next();
                    let mut previous = '\0';
                    for next in chars.by_ref() {
                        if previous == '*' && next == '/' {
                            break;
                        }
                        previous = next;
                    }
                    continue;
                }
                _ => {}
            }
        }

        output.push(ch);
    }

    output
}

fn update_codex_config(path: &Path, exe_path: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        "".to_string()
    };

    let mut lines = remove_codex_mcp_blocks(
        &content,
        &[SERVER_NAME]
            .iter()
            .chain(LEGACY_SERVER_NAMES.iter())
            .copied()
            .collect::<Vec<_>>(),
    );
    let clean_exe_path = exe_path.replace("\\", "/");

    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    if !lines.is_empty() {
        lines.push(String::new());
    }
    lines.push(format!("[mcp_servers.{SERVER_NAME}]"));
    lines.push(format!(
        "command = \"{}\"",
        toml_escape_string(&clean_exe_path)
    ));
    lines.push("args = []".to_string());

    let new_content = lines.join("\n");
    std::fs::write(path, new_content)?;
    Ok(())
}

fn remove_codex_mcp_blocks(content: &str, server_names: &[&str]) -> Vec<String> {
    let mut result = Vec::new();
    let mut skip_current_block = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            skip_current_block = server_names.iter().any(|name| {
                trimmed == format!("[mcp_servers.{name}]")
                    || trimmed.starts_with(&format!("[mcp_servers.{name}."))
            });
        }

        if !skip_current_block {
            result.push(line.to_string());
        }
    }

    result
}

fn toml_escape_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_config_paths_use_existing_files_or_create_json() {
        let temp_dir = std::env::temp_dir().join(format!("ams-path-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let user_profile = temp_dir.join("user");
        let config_dir = user_profile.join(".config").join("opencode");
        std::fs::create_dir_all(&config_dir).unwrap();

        let default_paths = opencode_config_paths(user_profile.to_str().unwrap());
        assert_eq!(default_paths, vec![config_dir.join("opencode.json")]);

        std::fs::write(config_dir.join("opencode.jsonc"), "{}").unwrap();
        std::fs::write(config_dir.join("opencode.json"), "{}").unwrap();
        let existing_paths = opencode_config_paths(user_profile.to_str().unwrap());
        assert_eq!(
            existing_paths,
            vec![
                config_dir.join("opencode.jsonc"),
                config_dir.join("opencode.json")
            ]
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn update_opencode_config_preserves_existing_mcp_entries_and_removes_legacy_names() {
        let temp_dir = std::env::temp_dir().join(format!("ams-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("opencode.jsonc");
        std::fs::write(
            &config_path,
            r#"{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "existing": {
      "type": "local",
      "command": ["existing.exe"],
      "enabled": true
    },
    "memory-mcp-server": {
      "type": "local",
      "command": ["old.exe"],
      "enabled": true
    }
  }
}"#,
        )
        .unwrap();

        update_opencode_config(&config_path, "C:\\tools\\memory.exe").unwrap();
        let updated: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();

        assert_eq!(
            updated["mcp"]["existing"]["command"][0],
            serde_json::json!("existing.exe")
        );
        assert_eq!(
            updated["mcp"]["ams"]["command"][0],
            serde_json::json!("C:\\tools\\memory.exe")
        );
        assert!(updated["mcp"].get("memory-mcp-server").is_none());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn update_opencode_config_accepts_jsonc_comments() {
        let temp_dir = std::env::temp_dir().join(format!("ams-jsonc-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("opencode.jsonc");
        std::fs::write(
            &config_path,
            r#"{
  // OpenCode allows JSONC here.
  "plugin": [
    "~/.config/opencode/plugins/example.ts"
  ],
  "mcp": {}
}"#,
        )
        .unwrap();

        update_opencode_config(&config_path, "memory.exe").unwrap();
        let updated: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();

        assert_eq!(
            updated["plugin"][0],
            serde_json::json!("~/.config/opencode/plugins/example.ts")
        );
        assert_eq!(
            updated["mcp"]["ams"]["command"][0],
            serde_json::json!("memory.exe")
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn update_codex_config_replaces_current_and_legacy_blocks() {
        let temp_dir = std::env::temp_dir().join(format!("ams-codex-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("config.toml");
        std::fs::write(
            &config_path,
            r#"model = "gpt-5"

[mcp_servers.other]
command = "other.exe"
args = ["--stdio"]

[mcp_servers.opencode-memory]
command = "old-memory.exe"
args = ["old"]

[mcp_servers.opencode-memory.env]
MEMORY_DB_PATH = "old.db"

[mcp_servers.memory-mcp-server]
command = "legacy-memory.exe"
args = []

[profiles.default]
approval_policy = "never"
"#,
        )
        .unwrap();

        update_codex_config(&config_path, "C:\\tools\\opencode-memory.exe").unwrap();
        let updated = std::fs::read_to_string(&config_path).unwrap();

        assert!(updated.contains(r#"model = "gpt-5""#));
        assert!(updated.contains("[mcp_servers.other]"));
        assert!(updated.contains("[profiles.default]"));
        assert!(!updated.contains("old-memory.exe"));
        assert!(!updated.contains("[mcp_servers.opencode-memory.env]"));
        assert!(!updated.contains("old.db"));
        assert!(!updated.contains("[mcp_servers.memory-mcp-server]"));
        assert!(updated.contains("[mcp_servers.ams]"));
        assert!(updated.contains(r#"command = "C:/tools/opencode-memory.exe""#));
        assert!(updated.contains("args = []"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn update_codex_config_creates_parent_directory() {
        let temp_dir =
            std::env::temp_dir().join(format!("ams-codex-create-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let config_path = temp_dir.join(".codex").join("config.toml");

        update_codex_config(&config_path, "memory.exe").unwrap();
        let updated = std::fs::read_to_string(&config_path).unwrap();

        assert!(updated.contains("[mcp_servers.ams]"));
        assert!(updated.contains(r#"command = "memory.exe""#));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
