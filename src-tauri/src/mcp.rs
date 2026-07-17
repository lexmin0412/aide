use std::collections::HashMap;
use std::path::PathBuf;
use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

// ── MCP Adapter ──

#[derive(Clone, Debug)]
pub struct McpAdapter {
    pub key: &'static str,
    pub name: &'static str,
    /// Config file path (relative to ~)
    pub config_path: &'static str,
    /// JSON/TOML key where MCP servers are stored. Empty means root object.
    pub mcp_key: &'static str,
    pub format: McpFormat,
}

#[derive(Clone, Debug, PartialEq)]
pub enum McpFormat {
    Json,
    Toml,
}

impl McpAdapter {
    fn resolve_path(&self) -> PathBuf {
        let home = dirs::home_dir().expect("home dir");
        home.join(self.config_path)
    }
}

pub fn all_mcp_adapters() -> Vec<McpAdapter> {
    vec![
        McpAdapter { key: "opencode", name: "OpenCode", config_path: ".config/opencode/opencode.jsonc", mcp_key: "mcp", format: McpFormat::Json },
        McpAdapter { key: "claude_code", name: "Claude Code", config_path: ".claude.json", mcp_key: "mcpServers", format: McpFormat::Json },
        McpAdapter { key: "trae", name: "Trae", config_path: "Library/Application Support/Trae/User/mcp.json", mcp_key: "mcpServers", format: McpFormat::Json },
        McpAdapter { key: "cursor", name: "Cursor", config_path: ".cursor/mcp.json", mcp_key: "mcpServers", format: McpFormat::Json },
        McpAdapter { key: "warp", name: "Warp", config_path: ".warp/.mcp.json", mcp_key: "", format: McpFormat::Json },
        McpAdapter { key: "vscode", name: "VS Code", config_path: "Library/Application Support/Code/User/mcp.json", mcp_key: "servers", format: McpFormat::Json },
        McpAdapter { key: "codex", name: "Codex", config_path: ".codex/config.toml", mcp_key: "mcp_servers", format: McpFormat::Toml },
        McpAdapter { key: "mimocode", name: "MiMo Code", config_path: ".config/mimocode/mimocode.jsonc", mcp_key: "mcp", format: McpFormat::Json },
    ]
}

// ── Central Config Model ──

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct McpServerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub targets: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpCentralConfig {
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}

impl Default for McpCentralConfig {
    fn default() -> Self {
        McpCentralConfig { servers: HashMap::new() }
    }
}

fn central_path() -> PathBuf {
    let home = dirs::home_dir().expect("home dir");
    home.join(".aide").join("mcp.json")
}

pub fn read_central() -> Result<McpCentralConfig, String> {
    let path = central_path();
    if !path.exists() {
        return Ok(McpCentralConfig::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("read mcp config: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("parse mcp config: {}", e))
}

pub fn save_central(config: &McpCentralConfig) -> Result<(), String> {
    let path = central_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, &content).map_err(|e| e.to_string())
}

// ── Sync ──

pub fn sync_to_tool(adapter: &McpAdapter, central: &McpCentralConfig) -> Result<SyncMcpResult, String> {
    let config_path = adapter.resolve_path();
    let enabled_servers: HashMap<&String, &McpServerConfig> = central
        .servers
        .iter()
        .filter(|(_, s)| !s.disabled.unwrap_or(false) && (s.targets.is_empty() || s.targets.iter().any(|t| t == adapter.key)))
        .collect();

    if enabled_servers.is_empty() && !config_path.exists() {
        return Ok(SyncMcpResult { skipped: true, message: "No servers to sync".into() });
    }

    let is_adapter_format = adapter.key == "opencode" || adapter.key == "mimocode";
    let server_map: HashMap<&String, serde_json::Value> = enabled_servers
        .iter()
        .map(|(name, cfg)| {
            let mut map = serde_json::Map::new();
            if is_adapter_format {
                let mut cmd_parts = vec![];
                if let Some(cmd) = &cfg.command {
                    cmd_parts.push(serde_json::Value::String(cmd.clone()));
                }
                if let Some(args) = &cfg.args {
                    cmd_parts.extend(args.iter().map(|a| serde_json::Value::String(a.clone())));
                }
                if !cmd_parts.is_empty() {
                    map.insert("command".into(), serde_json::Value::Array(cmd_parts));
                    map.insert("type".into(), serde_json::Value::String("local".into()));
                }
                if let Some(url) = &cfg.url {
                    map.insert("url".into(), serde_json::Value::String(url.clone()));
                    map.insert("type".into(), serde_json::Value::String("remote".into()));
                }
                if let Some(env) = &cfg.env {
                    let env_map: serde_json::Map<String, serde_json::Value> = env.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect();
                    map.insert("environment".into(), serde_json::Value::Object(env_map));
                }
            } else {
                if let Some(cmd) = &cfg.command {
                    map.insert("command".into(), serde_json::Value::String(cmd.clone()));
                }
                if let Some(args) = &cfg.args {
                    map.insert("args".into(), serde_json::Value::Array(args.iter().map(|a| serde_json::Value::String(a.clone())).collect()));
                }
                if let Some(url) = &cfg.url {
                    map.insert("url".into(), serde_json::Value::String(url.clone()));
                }
                if let Some(env) = &cfg.env {
                    let env_map: serde_json::Map<String, serde_json::Value> = env.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect();
                    map.insert("env".into(), serde_json::Value::Object(env_map));
                }
            }
            (*name, serde_json::Value::Object(map))
        })
        .collect();

    let mcp_value = serde_json::Value::Object(serde_json::Map::from_iter(
        server_map.into_iter().map(|(k, v)| (k.clone(), v)),
    ));

    match adapter.format {
        McpFormat::Json => sync_json(adapter, &config_path, &mcp_value),
        McpFormat::Toml => sync_toml(adapter, &config_path, &mcp_value),
    }
}

fn sync_json(adapter: &McpAdapter, path: &Path, mcp_value: &serde_json::Value) -> Result<SyncMcpResult, String> {
    let mut root: serde_json::Value = if path.exists() {
        let content = strip_jsonc_comments(&fs::read_to_string(path).map_err(|e| e.to_string())?);
        serde_json::from_str(&content).unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    };

    if adapter.mcp_key.is_empty() {
        // Root object IS the MCP servers (e.g. standalone mcp.json)
        root = mcp_value.clone();
    } else {
        root[&adapter.mcp_key] = mcp_value.clone();
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    fs::write(path, &content).map_err(|e| e.to_string())?;

    let count = mcp_value.as_object().map(|o| o.len()).unwrap_or(0);
    Ok(SyncMcpResult { skipped: false, message: format!("Synced {} server(s)", count) })
}

fn sync_toml(adapter: &McpAdapter, path: &Path, mcp_value: &serde_json::Value) -> Result<SyncMcpResult, String> {
    let mcp_key = &adapter.mcp_key;
    let mut content = if path.exists() {
        fs::read_to_string(path).map_err(|e| e.to_string())?
    } else {
        String::new()
    };

    let toml_str = json_value_to_toml_table(mcp_value, mcp_key);

    // Find and replace the mcp_servers section, or append
    if content.contains(&format!("[{}]", mcp_key)) {
        // Simple approach: rebuild entire file
        let existing: toml::Value = content.parse().unwrap_or(toml::Value::Table(toml::value::Table::new()));
        let mut table = existing.as_table().cloned().unwrap_or_default();
        // Remove old mcp_servers entries
        let key = mcp_key.to_string();
        table.remove(&key);
        // Parse new entries and insert
        let new_table: toml::Value = toml_str.parse().map_err(|e| format!("toml parse error: {}", e))?;
        if let Some(new_val) = new_table.get(mcp_key) {
            table.insert(mcp_key.to_string(), new_val.clone());
        }
        let new_content = toml::to_string_pretty(&toml::Value::Table(table)).map_err(|e| e.to_string())?;
        content = new_content;
    } else {
        // Append section
        content.push_str(&format!("\n{}", toml_str));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, &content).map_err(|e| e.to_string())?;

    let count = mcp_value.as_object().map(|o| o.len()).unwrap_or(0);
    Ok(SyncMcpResult { skipped: false, message: format!("Synced {} server(s)", count) })
}

fn json_value_to_toml_table(value: &serde_json::Value, root_key: &str) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    if let Some(obj) = value.as_object() {
        for (name, server) in obj {
            let _ = writeln!(out, "[{}.{}]", root_key, name);
            if let Some(obj) = server.as_object() {
                for (k, v) in obj {
                    match v {
                        serde_json::Value::String(s) => {
                            if s.contains(' ') || s.contains('#') {
                                let _ = writeln!(out, "{} = \"{}\"", k, s);
                            } else {
                                let _ = writeln!(out, "{} = {}", k, s);
                            }
                        }
                        serde_json::Value::Array(arr) => {
                            let items: Vec<String> = arr.iter().filter_map(|a| a.as_str().map(|s| format!("\"{}\"", s))).collect();
                            let _ = writeln!(out, "{} = [{}]", k, items.join(", "));
                        }
                        serde_json::Value::Bool(b) => { let _ = writeln!(out, "{} = {}", k, b); }
                        _ => {}
                    }
                }
            }
        }
    }
    out
}

pub fn import_from_adapter(adapter: &McpAdapter, central: &mut McpCentralConfig) -> Result<ImportMcpResult, String> {
    let config_path = adapter.resolve_path();
    if !config_path.exists() {
        return Ok(ImportMcpResult { imported: vec![], skipped: vec![], source: adapter.name.to_string() });
    }

    let raw = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    let cleaned = strip_jsonc_comments(&raw);

    let servers_value: serde_json::Value = serde_json::from_str(&cleaned).map_err(|e| format!("parse {}: {}", adapter.config_path, e))?;

    let servers_obj = if adapter.mcp_key.is_empty() {
        &servers_value
    } else {
        servers_value.get(&adapter.mcp_key).unwrap_or(&serde_json::Value::Null)
    };

    let servers_map = match servers_obj {
        serde_json::Value::Object(m) => m,
        _ => return Ok(ImportMcpResult { imported: vec![], skipped: vec![], source: adapter.name.to_string() }),
    };

    let mut imported = Vec::new();
    let mut skipped = Vec::new();

    for (name, value) in servers_map {
        if central.servers.contains_key(name) {
            skipped.push(name.clone());
            continue;
        }

        let obj = match value.as_object() {
            Some(o) => o,
            None => { skipped.push(name.clone()); continue; }
        };

        let config = if adapter.key == "opencode" || adapter.key == "mimocode" {
            let command_arr = obj.get("command").and_then(|v| v.as_array());
            let (cmd, args) = if let Some(arr) = command_arr {
                let cmd = arr.first().and_then(|v| v.as_str().map(|s| s.to_string()));
                let args = if arr.len() > 1 {
                    Some(arr[1..].iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                } else {
                    None
                };
                (cmd, args)
            } else {
                (obj.get("command").and_then(|v| v.as_str().map(|s| s.to_string())),
                 obj.get("args").and_then(|v| v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())))
            };
            let env = obj.get("environment").or_else(|| obj.get("env"))
                .and_then(|v| v.as_object().map(|o| o.iter().map(|(k, val)| (k.clone(), val.as_str().unwrap_or("").to_string())).collect()));
            let disabled = obj.get("enabled").and_then(|v| v.as_bool()).map(|b| !b);
            McpServerConfig {
                command: cmd,
                args,
                url: obj.get("url").and_then(|v| v.as_str().map(|s| s.to_string())),
                env,
                disabled,
                description: None,
                targets: vec![adapter.key.to_string()],
            }
        } else {
            McpServerConfig {
                command: obj.get("command").and_then(|v| v.as_str().map(|s| s.to_string())),
                args: obj.get("args").and_then(|v| v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())),
                url: obj.get("url").and_then(|v| v.as_str().map(|s| s.to_string())),
                env: obj.get("env").and_then(|v| v.as_object().map(|o| o.iter().map(|(k, val)| (k.clone(), val.as_str().unwrap_or("").to_string())).collect())),
                disabled: None,
                description: None,
                targets: vec![adapter.key.to_string()],
            }
        };

        central.servers.insert(name.clone(), config);
        imported.push(name.clone());
    }

    Ok(ImportMcpResult { imported, skipped, source: adapter.name.to_string() })
}

#[derive(Debug, Serialize)]
pub struct ImportMcpResult {
    pub source: String,
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
}

fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            in_string = !in_string;
            out.push(c);
        } else if !in_string && c == '/' {
            match chars.peek() {
                Some('/') => {
                    chars.next();
                    while let Some(&n) = chars.peek() {
                        if n == '\n' { break; }
                        chars.next();
                    }
                }
                Some('*') => {
                    chars.next();
                    while let Some(&n) = chars.peek() {
                        if n == '*' {
                            chars.next();
                            if chars.peek() == Some(&'/') {
                                chars.next();
                                break;
                            }
                        } else {
                            chars.next();
                        }
                    }
                }
                _ => out.push(c),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[derive(Debug, Serialize)]
pub struct SyncMcpResult {
    pub skipped: bool,
    pub message: String,
}
