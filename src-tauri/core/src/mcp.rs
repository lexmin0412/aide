use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::storage;

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

/// Trae's per-user MCP config. macOS and Windows keep application support data
/// in different roots, and Linux uses the XDG config dir; a single hardcoded
/// macOS path silently skipped Trae for every other platform.
const TRAE_CONFIG_PATH: &str = if cfg!(target_os = "macos") {
    "Library/Application Support/Trae/User/mcp.json"
} else if cfg!(target_os = "windows") {
    "AppData/Roaming/Trae/User/mcp.json"
} else {
    ".config/Trae/User/mcp.json"
};

/// VS Code's per-user MCP config (see `TRAE_CONFIG_PATH` for the OS split).
const VSCODE_CONFIG_PATH: &str = if cfg!(target_os = "macos") {
    "Library/Application Support/Code/User/mcp.json"
} else if cfg!(target_os = "windows") {
    "AppData/Roaming/Code/User/mcp.json"
} else {
    ".config/Code/User/mcp.json"
};

/// OpenCode accepts both `opencode.json` and `opencode.jsonc`. One resolver is
/// shared by MCP sync and model-profile sync: prefer an existing `.jsonc`, then
/// an existing `.json`, and create `.jsonc` for a fresh install. A split
/// resolver would make the two features write different files.
pub fn opencode_config_path(home: &Path) -> PathBuf {
    let dir = home.join(".config").join("opencode");
    let jsonc = dir.join("opencode.jsonc");
    if jsonc.exists() {
        return jsonc;
    }
    let json = dir.join("opencode.json");
    if json.exists() {
        return json;
    }
    jsonc
}

impl McpAdapter {
    /// Absolute config path in the real home directory.
    pub fn resolve_path(&self) -> Result<PathBuf, String> {
        let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
        Ok(self.resolve_path_in(&home))
    }

    /// Absolute config path in an explicit home directory (used by sync/import
    /// and unit tests). Adapters whose file name is dynamic go through their
    /// shared resolver instead of the static `config_path`.
    pub fn resolve_path_in(&self, home: &Path) -> PathBuf {
        if self.key == "opencode" {
            opencode_config_path(home)
        } else {
            home.join(self.config_path)
        }
    }
}

pub fn all_mcp_adapters() -> Vec<McpAdapter> {
    vec![
        McpAdapter {
            key: "opencode",
            name: "OpenCode",
            // Placeholder: `resolve_path_in` uses `opencode_config_path`.
            config_path: ".config/opencode/opencode.jsonc",
            mcp_key: "mcp",
            format: McpFormat::Json,
        },
        McpAdapter {
            key: "claude_code",
            name: "Claude Code",
            config_path: ".claude.json",
            mcp_key: "mcpServers",
            format: McpFormat::Json,
        },
        McpAdapter {
            key: "trae",
            name: "Trae",
            config_path: TRAE_CONFIG_PATH,
            mcp_key: "mcpServers",
            format: McpFormat::Json,
        },
        McpAdapter {
            key: "cursor",
            name: "Cursor",
            config_path: ".cursor/mcp.json",
            mcp_key: "mcpServers",
            format: McpFormat::Json,
        },
        McpAdapter {
            key: "warp",
            name: "Warp",
            config_path: ".warp/.mcp.json",
            mcp_key: "",
            format: McpFormat::Json,
        },
        McpAdapter {
            key: "vscode",
            name: "VS Code",
            config_path: VSCODE_CONFIG_PATH,
            mcp_key: "servers",
            format: McpFormat::Json,
        },
        McpAdapter {
            key: "codex",
            name: "Codex",
            config_path: ".codex/config.toml",
            mcp_key: "mcp_servers",
            format: McpFormat::Toml,
        },
        McpAdapter {
            key: "mimocode",
            name: "MiMo Code",
            config_path: ".config/mimocode/mimocode.jsonc",
            mcp_key: "mcp",
            format: McpFormat::Json,
        },
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
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub targets: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct McpCentralConfig {
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}

fn central_path() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|h| h.join(".aide").join("mcp.json"))
        .ok_or_else(|| "Cannot find home directory".to_string())
}

pub fn read_central() -> Result<McpCentralConfig, String> {
    read_central_at(&central_path()?)
}

/// Read a central MCP config from an explicit path (used by doctor and tests).
pub fn read_central_at(path: &Path) -> Result<McpCentralConfig, String> {
    Ok(storage::read_json(path)?.unwrap_or_default())
}

/// Locate the bundled `aide-mcp` sidecar binary next to the running executable.
/// Same location in packaged builds and in target/debug during development.
pub fn sidecar_path() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    ["aide-mcp", "aide-mcp.exe"]
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.exists())
}

pub fn save_central(config: &McpCentralConfig) -> Result<(), String> {
    let path = central_path()?;
    let backups = backup_dir()?;
    storage::write_json(&path, config, Some(&backups))?;
    // The file can hold env vars with credentials.
    storage::restrict_permissions(&path)
}

fn backup_dir() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|h| h.join(".aide").join("backups"))
        .ok_or_else(|| "Cannot find home directory".to_string())
}

// ── Managed manifest ──

/// Records which server names aide wrote into each tool config. The manifest is
/// the only source of truth for un-sync: a name is removed from a tool config
/// only when aide wrote it before, so servers created by the user or the tool
/// itself are never deleted.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpManagedState {
    #[serde(default)]
    pub tools: HashMap<String, Vec<String>>,
}

fn managed_path() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|h| h.join(".aide").join("mcp-managed.json"))
        .ok_or_else(|| "Cannot find home directory".to_string())
}

/// Read the managed manifest. A missing file is empty state; a corrupt one is an
/// error so drift/cleanup never overwrite what the user could still repair.
pub fn read_managed() -> Result<McpManagedState, String> {
    read_managed_at(&managed_path()?)
}

pub(crate) fn read_managed_at(path: &Path) -> Result<McpManagedState, String> {
    Ok(storage::read_json(path)?.unwrap_or_default())
}

pub fn save_managed(state: &McpManagedState) -> Result<(), String> {
    write_managed_at(&managed_path()?, state, backup_dir().ok().as_deref())
}

fn write_managed_at(
    path: &Path,
    state: &McpManagedState,
    backups: Option<&Path>,
) -> Result<(), String> {
    storage::write_json(path, state, backups)
}

/// Central servers that should be present in the given tool config.
fn enabled_server_names(central: &McpCentralConfig, tool_key: &str) -> Vec<String> {
    let mut names: Vec<String> = central
        .servers
        .iter()
        .filter(|(_, server)| {
            !server.disabled.unwrap_or(false)
                && (server.targets.is_empty() || server.targets.iter().any(|t| t == tool_key))
        })
        .map(|(name, _)| name.clone())
        .collect();
    names.sort();
    names
}

/// Names recorded in the manifest that are no longer enabled for this tool.
fn stale_server_names(previously_managed: &[String], expected: &[String]) -> Vec<String> {
    let mut stale: Vec<String> = previously_managed
        .iter()
        .filter(|name| !expected.contains(name))
        .cloned()
        .collect();
    stale.sort();
    stale.dedup();
    stale
}

fn sync_message(synced: usize, removed: usize) -> String {
    match (synced, removed) {
        (0, 0) => "No servers to sync".to_string(),
        (0, removed) => format!("Removed {removed} managed server(s) no longer enabled"),
        (synced, 0) => format!("Synced {synced} server(s)"),
        (synced, removed) => {
            format!("Synced {synced} server(s), removed {removed} managed server(s)")
        }
    }
}

// ── Sync ──

/// Build the JSON representation aide writes into a tool config.
fn server_map(central: &McpCentralConfig, adapter: &McpAdapter) -> serde_json::Value {
    let is_adapter_format = adapter.key == "opencode" || adapter.key == "mimocode";
    let server_map: HashMap<String, serde_json::Value> = central
        .servers
        .iter()
        .filter(|(_, s)| {
            !s.disabled.unwrap_or(false)
                && (s.targets.is_empty() || s.targets.iter().any(|t| t == adapter.key))
        })
        .map(|(name, cfg)| {
            if !is_adapter_format {
                return (name.clone(), standard_server_json(cfg));
            }
            let mut map = serde_json::Map::new();
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
                let env_map: serde_json::Map<String, serde_json::Value> = env
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect();
                map.insert("environment".into(), serde_json::Value::Object(env_map));
            }
            if let Some(headers) = &cfg.headers {
                let headers_map: serde_json::Map<String, serde_json::Value> = headers
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect();
                map.insert("headers".into(), serde_json::Value::Object(headers_map));
            }
            (name.clone(), serde_json::Value::Object(map))
        })
        .collect();
    serde_json::Value::Object(serde_json::Map::from_iter(server_map))
}

/// JSON shape aide writes for standard MCP configs (Claude/Codex-style
/// `command`/`args`/`env`/`headers`). Shared by global sync and project sync.
pub(crate) fn standard_server_json(server: &McpServerConfig) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    if let Some(cmd) = &server.command {
        map.insert("command".into(), serde_json::Value::String(cmd.clone()));
    }
    if let Some(args) = &server.args {
        map.insert(
            "args".into(),
            serde_json::Value::Array(
                args.iter()
                    .map(|arg| serde_json::Value::String(arg.clone()))
                    .collect(),
            ),
        );
    }
    if let Some(url) = &server.url {
        map.insert("url".into(), serde_json::Value::String(url.clone()));
    }
    if let Some(env) = &server.env {
        let env_map: serde_json::Map<String, serde_json::Value> = env
            .iter()
            .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
            .collect();
        map.insert("env".into(), serde_json::Value::Object(env_map));
    }
    if let Some(headers) = &server.headers {
        let headers_map: serde_json::Map<String, serde_json::Value> = headers
            .iter()
            .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
            .collect();
        map.insert("headers".into(), serde_json::Value::Object(headers_map));
    }
    serde_json::Value::Object(map)
}

/// Sync central servers into a tool and return the names aide now owns for it.
/// `previously_managed` are the names aide wrote on the last sync; names in that
/// list which are no longer enabled are deleted from this tool's config.
pub fn sync_to_tool(
    adapter: &McpAdapter,
    central: &McpCentralConfig,
) -> Result<SyncMcpResult, String> {
    let config_path = adapter.resolve_path()?;
    let mut managed = read_managed()?;
    let previous = managed.tools.get(adapter.key).cloned().unwrap_or_default();
    let backups = backup_dir().ok();
    let (result, expected) = sync_to_tool_at(
        adapter,
        &config_path,
        central,
        &previous,
        backups.as_deref(),
    )?;
    let changed = managed.tools.get(adapter.key) != Some(&expected);
    if changed {
        if expected.is_empty() {
            managed.tools.remove(adapter.key);
        } else {
            managed.tools.insert(adapter.key.to_string(), expected);
        }
        save_managed(&managed)?;
    }
    Ok(result)
}

fn sync_to_tool_at(
    adapter: &McpAdapter,
    config_path: &Path,
    central: &McpCentralConfig,
    previously_managed: &[String],
    backups: Option<&Path>,
) -> Result<(SyncMcpResult, Vec<String>), String> {
    let expected = enabled_server_names(central, adapter.key);
    let stale = stale_server_names(previously_managed, &expected);
    let config_exists = config_path.exists();

    // Nothing to add and nothing recorded can be removed: leave the tool alone
    // instead of creating a config file for an uninstalled tool.
    if expected.is_empty() && (stale.is_empty() || !config_exists) {
        return Ok((
            SyncMcpResult {
                skipped: true,
                message: "No servers to sync".into(),
            },
            expected,
        ));
    }

    let mcp_value = server_map(central, adapter);
    let result = match adapter.format {
        McpFormat::Json => sync_json(adapter, config_path, &mcp_value, &stale, backups),
        McpFormat::Toml => sync_toml(adapter, config_path, &mcp_value, &stale, backups),
    }?;
    Ok((result, expected))
}

/// Upsert entries from `source` into the object `target`, preserving entries
/// that only exist on the tool side (added outside aide).
fn merge_server_map(target: &mut serde_json::Value, source: &serde_json::Value) {
    if !target.is_object() {
        *target = serde_json::Value::Object(serde_json::Map::new());
    }
    if let (Some(dst), Some(src)) = (target.as_object_mut(), source.as_object()) {
        for (name, cfg) in src {
            dst.insert(name.clone(), cfg.clone());
        }
    }
}

/// Remove `names` from an object and return how many were actually present.
fn remove_server_names(target: &mut serde_json::Value, names: &[String]) -> usize {
    let Some(obj) = target.as_object_mut() else {
        return 0;
    };
    names
        .iter()
        .filter(|name| obj.remove(*name).is_some())
        .count()
}

/// Read a JSON/JSONC configuration file. A missing file is an empty object; an
/// existing but unparsable file is an error, because "repairing" it by writing
/// defaults would destroy whatever the user (or the tool) had configured.
fn read_jsonc_file(path: &Path) -> Result<serde_json::Value, String> {
    let empty = || serde_json::Value::Object(serde_json::Map::new());
    if !path.exists() {
        return Ok(empty());
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if raw.trim().is_empty() {
        // An empty file holds no data; treat it as an empty object.
        return Ok(empty());
    }
    serde_json::from_str(&strip_jsonc_comments(&raw)).map_err(|e| {
        format!(
            "Cannot parse {}: {e}. Fix or move the file before syncing; it was left untouched.",
            path.display()
        )
    })
}

fn read_toml_file(path: &Path) -> Result<toml::Table, String> {
    if !path.exists() {
        return Ok(toml::Table::new());
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(toml::Table::new());
    }
    raw.parse().map_err(|e| {
        format!(
            "Cannot parse {}: {e}. Fix or move the file before syncing; it was left untouched.",
            path.display()
        )
    })
}

fn write_tool_config(path: &Path, content: &str, backups: Option<&Path>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    storage::write_bytes(path, content.as_bytes(), backups)?;
    // MCP server definitions routinely carry env vars with API keys, so the
    // tool's own config must not stay world-readable after aide writes it.
    storage::restrict_permissions(path)
}

fn sync_json(
    adapter: &McpAdapter,
    path: &Path,
    mcp_value: &serde_json::Value,
    stale: &[String],
    backups: Option<&Path>,
) -> Result<SyncMcpResult, String> {
    let mut root = read_jsonc_file(path)?;

    let removed = if adapter.mcp_key.is_empty() {
        // Root object IS the MCP servers (e.g. standalone mcp.json).
        merge_server_map(&mut root, mcp_value);
        remove_server_names(&mut root, stale)
    } else {
        if !root.is_object() {
            root = serde_json::Value::Object(serde_json::Map::new());
        }
        let existing = root
            .get(adapter.mcp_key)
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        let mut merged = existing;
        merge_server_map(&mut merged, mcp_value);
        let removed = remove_server_names(&mut merged, stale);
        let expected_empty = mcp_value.as_object().is_none_or(|o| o.is_empty());
        let section_empty = merged.as_object().is_none_or(|o| o.is_empty());
        if expected_empty && section_empty {
            // Aide-owned section emptied out; drop it so stale empty keys do not
            // linger in the user's config.
            if let Some(obj) = root.as_object_mut() {
                obj.remove(adapter.mcp_key);
            }
        } else {
            root[adapter.mcp_key] = merged;
        }
        removed
    };

    let content = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    write_tool_config(path, &content, backups)?;

    let count = mcp_value.as_object().map(|o| o.len()).unwrap_or(0);
    Ok(SyncMcpResult {
        skipped: false,
        message: sync_message(count, removed),
    })
}

fn json_to_toml_value(value: &serde_json::Value) -> toml::Value {
    match value {
        serde_json::Value::Null => toml::Value::String(String::new()),
        serde_json::Value::Bool(b) => toml::Value::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml::Value::Integer(i)
            } else {
                toml::Value::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => toml::Value::String(s.clone()),
        serde_json::Value::Array(arr) => {
            toml::Value::Array(arr.iter().map(json_to_toml_value).collect())
        }
        serde_json::Value::Object(obj) => toml::Value::Table(
            obj.iter()
                .map(|(k, v)| (k.clone(), json_to_toml_value(v)))
                .collect(),
        ),
    }
}

fn sync_toml(
    adapter: &McpAdapter,
    path: &Path,
    mcp_value: &serde_json::Value,
    stale: &[String],
    backups: Option<&Path>,
) -> Result<SyncMcpResult, String> {
    let mut table = read_toml_file(path)?;

    let mut section = table
        .remove(adapter.mcp_key)
        .map(|value| match value {
            toml::Value::Table(table) => table,
            _ => toml::Table::new(),
        })
        .unwrap_or_default();
    if let Some(servers) = mcp_value.as_object() {
        for (name, server) in servers {
            section.insert(name.clone(), json_to_toml_value(server));
        }
    }
    let mut removed = 0;
    for name in stale {
        if section.remove(name).is_some() {
            removed += 1;
        }
    }
    if !section.is_empty() {
        table.insert(adapter.mcp_key.to_string(), toml::Value::Table(section));
    }

    let content = toml::to_string_pretty(&toml::Value::Table(table)).map_err(|e| e.to_string())?;
    write_tool_config(path, &content, backups)?;

    let count = mcp_value.as_object().map(|o| o.len()).unwrap_or(0);
    Ok(SyncMcpResult {
        skipped: false,
        message: sync_message(count, removed),
    })
}

fn toml_to_json_value(value: &toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::Value::Number((*i).into()),
        toml::Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Datetime(d) => serde_json::Value::String(d.to_string()),
        toml::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(toml_to_json_value).collect())
        }
        toml::Value::Table(table) => serde_json::Value::Object(
            table
                .iter()
                .map(|(k, v)| (k.clone(), toml_to_json_value(v)))
                .collect(),
        ),
    }
}

// ── Drift ──

/// Difference between what aide should have written into a tool and what the
/// tool config actually contains.
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterDrift {
    /// Enabled central servers expected for the tool but absent from its config.
    pub missing: Vec<String>,
    /// Names aide previously wrote that are no longer enabled but still present.
    pub leftover: Vec<String>,
}

/// Server names currently present in a tool config (top-level object when the
/// adapter stores servers at the root).
fn server_names_in_config(adapter: &McpAdapter, config_path: &Path) -> Result<Vec<String>, String> {
    if !config_path.exists() {
        return Ok(Vec::new());
    }
    let mut names: Vec<String> = match adapter.format {
        McpFormat::Json => {
            let root = read_jsonc_file(config_path)?;
            let section = if adapter.mcp_key.is_empty() {
                Some(&root)
            } else {
                root.get(adapter.mcp_key)
            };
            section
                .and_then(|v| v.as_object())
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default()
        }
        McpFormat::Toml => {
            let table = read_toml_file(config_path)?;
            table
                .get(adapter.mcp_key)
                .and_then(|v| v.as_table())
                .map(|t| t.keys().cloned().collect())
                .unwrap_or_default()
        }
    };
    names.sort();
    names.dedup();
    Ok(names)
}

/// Compute the drift for one adapter. `Ok(None)` means the adapter is not
/// installed (no config file) and has neither expected nor managed entries, or
/// that there is nothing to report.
pub fn compute_drift(
    adapter: &McpAdapter,
    config_path: &Path,
    central: &McpCentralConfig,
    managed: &McpManagedState,
) -> Result<Option<AdapterDrift>, String> {
    let expected = enabled_server_names(central, adapter.key);
    let previous = managed.tools.get(adapter.key).cloned().unwrap_or_default();
    if !config_path.exists() && expected.is_empty() && previous.is_empty() {
        return Ok(None);
    }
    let present = server_names_in_config(adapter, config_path)?;
    let missing: Vec<String> = expected
        .iter()
        .filter(|name| !present.contains(name))
        .cloned()
        .collect();
    let leftover: Vec<String> = previous
        .iter()
        .filter(|name| !expected.contains(name) && present.contains(name))
        .cloned()
        .collect();
    if missing.is_empty() && leftover.is_empty() {
        return Ok(None);
    }
    Ok(Some(AdapterDrift { missing, leftover }))
}

pub fn import_from_adapter(
    adapter: &McpAdapter,
    central: &mut McpCentralConfig,
) -> Result<ImportMcpResult, String> {
    let config_path = adapter.resolve_path()?;
    import_from_file(adapter, &config_path, central)
}

/// Import one tool's MCP entries into the central config. `config_path` is a
/// parameter so the parsing/merge logic can be unit-tested with temp files.
pub(crate) fn import_from_file(
    adapter: &McpAdapter,
    config_path: &Path,
    central: &mut McpCentralConfig,
) -> Result<ImportMcpResult, String> {
    if !config_path.exists() {
        return Ok(ImportMcpResult {
            imported: vec![],
            skipped: vec![],
            source: adapter.name.to_string(),
        });
    }

    let servers_value = match adapter.format {
        McpFormat::Json => read_jsonc_file(config_path)?,
        McpFormat::Toml => {
            let table = read_toml_file(config_path)?;
            toml_to_json_value(&toml::Value::Table(table))
        }
    };

    let servers_obj = if adapter.mcp_key.is_empty() {
        &servers_value
    } else {
        servers_value
            .get(adapter.mcp_key)
            .unwrap_or(&serde_json::Value::Null)
    };

    let servers_map = match servers_obj {
        serde_json::Value::Object(m) => m,
        _ => {
            return Ok(ImportMcpResult {
                imported: vec![],
                skipped: vec![],
                source: adapter.name.to_string(),
            })
        }
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
            None => {
                skipped.push(name.clone());
                continue;
            }
        };

        let config = if adapter.key == "opencode" || adapter.key == "mimocode" {
            let command_arr = obj.get("command").and_then(|v| v.as_array());
            let (cmd, args) = if let Some(arr) = command_arr {
                let cmd = arr.first().and_then(|v| v.as_str().map(|s| s.to_string()));
                let args = if arr.len() > 1 {
                    Some(
                        arr[1..]
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect(),
                    )
                } else {
                    None
                };
                (cmd, args)
            } else {
                (
                    obj.get("command")
                        .and_then(|v| v.as_str().map(|s| s.to_string())),
                    obj.get("args").and_then(|v| {
                        v.as_array().map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                    }),
                )
            };
            let env = obj
                .get("environment")
                .or_else(|| obj.get("env"))
                .and_then(|v| {
                    v.as_object().map(|o| {
                        o.iter()
                            .map(|(k, val)| (k.clone(), val.as_str().unwrap_or("").to_string()))
                            .collect()
                    })
                });
            let headers = obj.get("headers").and_then(|v| {
                v.as_object().map(|o| {
                    o.iter()
                        .map(|(k, val)| (k.clone(), val.as_str().unwrap_or("").to_string()))
                        .collect()
                })
            });
            let disabled = obj.get("enabled").and_then(|v| v.as_bool()).map(|b| !b);
            McpServerConfig {
                command: cmd,
                args,
                url: obj
                    .get("url")
                    .and_then(|v| v.as_str().map(|s| s.to_string())),
                env,
                headers,
                disabled,
                description: None,
                targets: vec![adapter.key.to_string()],
            }
        } else {
            let headers = obj.get("headers").and_then(|v| {
                v.as_object().map(|o| {
                    o.iter()
                        .map(|(k, val)| (k.clone(), val.as_str().unwrap_or("").to_string()))
                        .collect()
                })
            });
            McpServerConfig {
                command: obj
                    .get("command")
                    .and_then(|v| v.as_str().map(|s| s.to_string())),
                args: obj.get("args").and_then(|v| {
                    v.as_array().map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                }),
                url: obj
                    .get("url")
                    .and_then(|v| v.as_str().map(|s| s.to_string())),
                env: obj.get("env").and_then(|v| {
                    v.as_object().map(|o| {
                        o.iter()
                            .map(|(k, val)| (k.clone(), val.as_str().unwrap_or("").to_string()))
                            .collect()
                    })
                }),
                headers,
                disabled: obj.get("enabled").and_then(|v| v.as_bool()).map(|b| !b),
                description: None,
                targets: vec![adapter.key.to_string()],
            }
        };

        central.servers.insert(name.clone(), config);
        imported.push(name.clone());
    }

    Ok(ImportMcpResult {
        imported,
        skipped,
        source: adapter.name.to_string(),
    })
}

#[derive(Debug, Serialize)]
pub struct ImportMcpResult {
    pub source: String,
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
}

/// Strip `//` and `/* */` comments from JSONC while leaving string literals
/// alone, then drop trailing commas so the result is plain JSON. Escaped quotes
/// (`\"`) inside strings no longer terminate the string, and `//` inside a URL
/// or `/*` inside a description are preserved.
pub(crate) fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' => match chars.peek() {
                Some('/') => {
                    chars.next();
                    while let Some(&n) = chars.peek() {
                        if n == '\n' {
                            break;
                        }
                        chars.next();
                    }
                }
                Some('*') => {
                    chars.next();
                    while let Some(n) = chars.next() {
                        if n == '*' && chars.peek() == Some(&'/') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => out.push(c),
            },
            _ => out.push(c),
        }
    }
    remove_trailing_commas(&out)
}

fn remove_trailing_commas(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == ']' || chars[j] == '}') {
                i += 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

#[derive(Debug, Serialize)]
pub struct SyncMcpResult {
    pub skipped: bool,
    pub message: String,
}

// ── Command-facing views and helpers ──

/// Name of the MCP server entry that exposes aide's own sidecar.
pub const AGENT_SERVER_NAME: &str = "aide";

/// Central server in the shape the GUI edits (round-trips through `save`).
#[derive(Debug, Serialize, Deserialize)]
pub struct McpServerView {
    pub name: String,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub headers: Option<HashMap<String, String>>,
    pub disabled: Option<bool>,
    pub description: Option<String>,
    pub targets: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct McpToolView {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct McpSyncResult {
    pub key: String,
    pub name: String,
    pub skipped: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub source: String,
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct McpDrift {
    pub tool_key: String,
    pub tool_name: String,
    pub missing: Vec<String>,
    pub leftover: Vec<String>,
}

fn to_sync_view(adapter: &McpAdapter, result: SyncMcpResult) -> McpSyncResult {
    McpSyncResult {
        key: adapter.key.to_string(),
        name: adapter.name.to_string(),
        skipped: result.skipped,
        message: result.message,
    }
}

/// Central servers in GUI shape.
pub fn server_views() -> Result<Vec<McpServerView>, String> {
    let central = read_central()?;
    Ok(central
        .servers
        .into_iter()
        .map(|(name, server)| McpServerView {
            name,
            command: server.command,
            args: server.args,
            url: server.url,
            env: server.env,
            headers: server.headers,
            disabled: server.disabled,
            description: server.description,
            targets: server.targets,
        })
        .collect())
}

/// Replace the central server set from GUI shape.
pub fn save_server_views(servers: Vec<McpServerView>) -> Result<(), String> {
    let config = McpCentralConfig {
        servers: servers
            .into_iter()
            .map(|server| {
                (
                    server.name,
                    McpServerConfig {
                        command: server.command,
                        args: server.args,
                        url: server.url,
                        env: server.env,
                        headers: server.headers,
                        disabled: server.disabled,
                        description: server.description,
                        targets: server.targets,
                    },
                )
            })
            .collect(),
    };
    save_central(&config)
}

/// MCP-capable adapters in GUI shape.
pub fn tool_views() -> Vec<McpToolView> {
    all_mcp_adapters()
        .into_iter()
        .map(|adapter| McpToolView {
            key: adapter.key.to_string(),
            name: adapter.name.to_string(),
        })
        .collect()
}

/// Sync the central config into one tool.
pub fn sync_tool_view(tool_key: &str) -> Result<McpSyncResult, String> {
    let adapter = all_mcp_adapters()
        .into_iter()
        .find(|adapter| adapter.key == tool_key)
        .ok_or_else(|| format!("Unknown tool: {tool_key}"))?;
    let config = read_central()?;
    let result = sync_to_tool(&adapter, &config)?;
    Ok(to_sync_view(&adapter, result))
}

/// Sync the central config into every tool, reporting per-tool failures.
pub fn sync_all_views() -> Result<Vec<McpSyncResult>, String> {
    let config = read_central()?;
    Ok(all_mcp_adapters()
        .into_iter()
        .map(|adapter| match sync_to_tool(&adapter, &config) {
            Ok(result) => to_sync_view(&adapter, result),
            Err(error) => McpSyncResult {
                key: adapter.key.to_string(),
                name: adapter.name.to_string(),
                skipped: true,
                message: format!("Failed: {error}"),
            },
        })
        .collect())
}

/// Import MCP servers from every installed tool into the central config.
pub fn import_all() -> Result<Vec<ImportResult>, String> {
    let mut central = read_central()?;
    let mut results = Vec::new();
    for adapter in all_mcp_adapters() {
        match import_from_adapter(&adapter, &mut central) {
            Ok(result) => {
                if !result.imported.is_empty() || !result.skipped.is_empty() {
                    results.push(ImportResult {
                        source: result.source,
                        imported: result.imported,
                        skipped: result.skipped,
                        error: None,
                    });
                }
            }
            Err(error) => results.push(ImportResult {
                source: adapter.name.to_string(),
                imported: vec![],
                skipped: vec![],
                error: Some(error),
            }),
        }
    }
    // save_central is atomic and backs up the previous revision; propagate its
    // failure instead of leaving the UI with a silently unsaved import.
    save_central(&central)?;
    Ok(results)
}

/// Compare every installed tool config against the central config and the
/// managed manifest. Only adapters with actual differences are reported.
pub fn drift() -> Result<Vec<McpDrift>, String> {
    let central = read_central()?;
    let managed = read_managed()?;
    let mut drifts = Vec::new();
    for adapter in all_mcp_adapters() {
        let config_path = adapter.resolve_path()?;
        if let Some(drift) = compute_drift(&adapter, &config_path, &central, &managed)? {
            drifts.push(McpDrift {
                tool_key: adapter.key.to_string(),
                tool_name: adapter.name.to_string(),
                missing: drift.missing,
                leftover: drift.leftover,
            });
        }
    }
    Ok(drifts)
}

/// Re-sync every tool so servers aide no longer manages disappear from configs.
pub fn cleanup_drift() -> Result<Vec<McpSyncResult>, String> {
    let config = read_central()?;
    all_mcp_adapters()
        .into_iter()
        .map(|adapter| {
            // Unlike `sync_all_views`, config read errors propagate: cleanup must
            // not report success while a corrupt tool config was left behind.
            let result = sync_to_tool(&adapter, &config)?;
            Ok(to_sync_view(&adapter, result))
        })
        .collect()
}

/// Add or remove aide's own MCP server entry, and on removal clean up the tool
/// configs where aide wrote it. `command` is the sidecar binary path.
pub fn set_agent_server(enable: bool, command: Option<&Path>) -> Result<(), String> {
    let mut central = read_central()?;
    if enable {
        let path = command.ok_or("aide-mcp binary not found next to the running app")?;
        central.servers.insert(
            AGENT_SERVER_NAME.to_string(),
            McpServerConfig {
                command: Some(path.to_string_lossy().to_string()),
                args: None,
                url: None,
                env: None,
                headers: None,
                disabled: Some(false),
                description: Some("aide agent access: install, update and publish skills".into()),
                targets: vec![],
            },
        );
    } else {
        central.servers.remove(AGENT_SERVER_NAME);
    }
    save_central(&central)?;
    if !enable {
        // Disabling must also remove the entry from every tool config where aide
        // wrote it. Best effort: the central entry is already gone, so a failing
        // tool config is surfaced instead of rolled back silently.
        let managed = read_managed().map_err(|error| {
            format!("Agent access disabled, but the MCP manifest is unreadable so tool configs were left untouched: {error}")
        })?;
        let mut first_error: Option<String> = None;
        for adapter in all_mcp_adapters() {
            let installed = adapter
                .resolve_path()
                .map(|path| path.exists())
                .unwrap_or(false);
            if !installed && !managed.tools.contains_key(adapter.key) {
                continue;
            }
            if let Err(error) = sync_to_tool(&adapter, &central) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        if let Some(error) = first_error {
            return Err(format!(
                "Agent access disabled, but syncing one or more tool configs failed: {error}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("aide-mcp-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn strip_jsonc_removes_comments_but_keeps_strings() {
        let input = r#"{
            // line comment
            /* block
               comment */
            "url": "https://example.com/a//b",
            "name": "x /* not a comment */"
        }"#;
        let out = strip_jsonc_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["url"], "https://example.com/a//b");
        assert_eq!(parsed["name"], "x /* not a comment */");
    }

    #[test]
    fn sync_json_upserts_and_preserves_tool_only_servers() {
        let dir = temp_dir("sync_json");
        let path = dir.join("mcp.json");
        fs::write(&path, r#"{"mcpServers": {"toolOnly": {"command": "foo"}}}"#).unwrap();

        let adapter = McpAdapter {
            key: "test",
            name: "Test",
            config_path: "unused",
            mcp_key: "mcpServers",
            format: McpFormat::Json,
        };
        let mcp_value = serde_json::json!({ "central": { "command": "bar" } });
        let result = sync_json(&adapter, &path, &mcp_value, &[], None).unwrap();
        assert!(!result.skipped);

        let parsed: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed["mcpServers"]["central"]["command"], "bar");
        assert_eq!(parsed["mcpServers"]["toolOnly"]["command"], "foo");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_json_root_level_merges_instead_of_replacing() {
        let dir = temp_dir("sync_json_root");
        let path = dir.join("mcp.json");
        fs::write(&path, r#"{"toolOnly": {"command": "foo"}}"#).unwrap();

        let adapter = McpAdapter {
            key: "warp",
            name: "Warp",
            config_path: "unused",
            mcp_key: "",
            format: McpFormat::Json,
        };
        let mcp_value = serde_json::json!({ "central": { "command": "bar" } });
        sync_json(&adapter, &path, &mcp_value, &[], None).unwrap();

        let parsed: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed["toolOnly"]["command"], "foo");
        assert_eq!(parsed["central"]["command"], "bar");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_toml_writes_valid_toml_and_preserves_other_sections() {
        let dir = temp_dir("sync_toml");
        let path = dir.join("config.toml");
        fs::write(
            &path,
            "[other]\nkey = 1\n\n[mcp_servers.toolOnly]\ncommand = \"foo\"\n",
        )
        .unwrap();

        let adapter = McpAdapter {
            key: "codex",
            name: "Codex",
            config_path: "unused",
            mcp_key: "mcp_servers",
            format: McpFormat::Toml,
        };
        let mcp_value = serde_json::json!({
            "central": { "command": "npx", "args": ["-y", "pkg"], "enabled": true }
        });
        let result = sync_toml(&adapter, &path, &mcp_value, &[], None).unwrap();
        assert!(!result.skipped);

        let parsed: toml::Table = fs::read_to_string(&path).unwrap().parse().unwrap();
        assert_eq!(parsed["other"]["key"].as_integer(), Some(1));
        assert_eq!(
            parsed["mcp_servers"]["central"]["command"].as_str(),
            Some("npx")
        );
        assert_eq!(
            parsed["mcp_servers"]["central"]["args"][0].as_str(),
            Some("-y")
        );
        assert_eq!(
            parsed["mcp_servers"]["central"]["enabled"].as_bool(),
            Some(true)
        );
        assert_eq!(
            parsed["mcp_servers"]["toolOnly"]["command"].as_str(),
            Some("foo")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn strip_jsonc_handles_escaped_quotes_and_trailing_commas() {
        let input = r#"{
            "command": "sh",
            "args": ["-c", "echo \"hi\" // still text"],
            "nested": { "a": 1, },
            "list": [1, 2,],
            "url": "https://example.com//x",
        }"#;
        let out = strip_jsonc_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["args"][1], "echo \"hi\" // still text");
        assert_eq!(parsed["url"], "https://example.com//x");
        assert_eq!(parsed["nested"]["a"], 1);
        assert_eq!(parsed["list"][1], 2);
    }

    #[cfg(unix)]
    #[test]
    fn sync_writes_tool_configs_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("perms");
        let json_path = dir.join("mcp.json");
        let adapter = McpAdapter {
            key: "test",
            name: "Test",
            config_path: "unused",
            mcp_key: "mcpServers",
            format: McpFormat::Json,
        };
        let mcp_value =
            serde_json::json!({ "central": { "command": "bar", "env": { "TOKEN": "t" } } });
        sync_json(&adapter, &json_path, &mcp_value, &[], None).unwrap();
        assert_eq!(
            fs::metadata(&json_path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let toml_path = dir.join("config.toml");
        let toml_adapter = McpAdapter {
            key: "codex",
            name: "Codex",
            config_path: "unused",
            mcp_key: "mcp_servers",
            format: McpFormat::Toml,
        };
        sync_toml(&toml_adapter, &toml_path, &mcp_value, &[], None).unwrap();
        assert_eq!(
            fs::metadata(&toml_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_json_refuses_to_overwrite_an_unparsable_config() {
        let dir = temp_dir("corrupt_json");
        let path = dir.join("mcp.json");
        let original = r#"{"mcpServers": {"a": {"command": "x"}"#;
        fs::write(&path, original).unwrap();
        let adapter = McpAdapter {
            key: "test",
            name: "Test",
            config_path: "unused",
            mcp_key: "mcpServers",
            format: McpFormat::Json,
        };
        let err = sync_json(
            &adapter,
            &path,
            &serde_json::json!({ "central": { "command": "bar" } }),
            &[],
            None,
        )
        .unwrap_err();
        assert!(err.contains("Cannot parse"), "unexpected error: {err}");
        // The broken file is preserved byte for byte for manual repair.
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_toml_refuses_to_overwrite_an_unparsable_config() {
        let dir = temp_dir("corrupt_toml");
        let path = dir.join("config.toml");
        let original = "[broken\nkey = ";
        fs::write(&path, original).unwrap();
        let adapter = McpAdapter {
            key: "codex",
            name: "Codex",
            config_path: "unused",
            mcp_key: "mcp_servers",
            format: McpFormat::Toml,
        };
        let err = sync_toml(
            &adapter,
            &path,
            &serde_json::json!({ "central": { "command": "npx" } }),
            &[],
            None,
        )
        .unwrap_err();
        assert!(err.contains("Cannot parse"), "unexpected error: {err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_from_file_reads_toml_adapter() {
        let dir = temp_dir("import_toml");
        let path = dir.join("config.toml");
        fs::write(
            &path,
            "[other]\nkey = 1\n\n[mcp_servers.central]\ncommand = \"npx\"\nargs = [\"-y\", \"pkg\"]\n\n[mcp_servers.central.env]\nTOKEN = \"secret\"\n",
        )
        .unwrap();
        let adapter = McpAdapter {
            key: "codex",
            name: "Codex",
            config_path: "unused",
            mcp_key: "mcp_servers",
            format: McpFormat::Toml,
        };
        let mut central = McpCentralConfig::default();
        let result = import_from_file(&adapter, &path, &mut central).unwrap();
        assert_eq!(result.imported, vec!["central".to_string()]);
        let server = central.servers.get("central").expect("imported server");
        assert_eq!(server.command.as_deref(), Some("npx"));
        assert_eq!(
            server.args.as_deref(),
            Some(&["-y".to_string(), "pkg".to_string()][..])
        );
        assert_eq!(
            server
                .env
                .as_ref()
                .and_then(|e| e.get("TOKEN"))
                .map(|s| s.as_str()),
            Some("secret")
        );
        assert_eq!(server.targets, vec!["codex".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_from_file_reports_parse_errors() {
        let dir = temp_dir("import_broken");
        let path = dir.join("mcp.json");
        fs::write(&path, "{ nope").unwrap();
        let adapter = McpAdapter {
            key: "cursor",
            name: "Cursor",
            config_path: "unused",
            mcp_key: "mcpServers",
            format: McpFormat::Json,
        };
        let mut central = McpCentralConfig::default();
        assert!(import_from_file(&adapter, &path, &mut central).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_to_toml_value_converts_nested_types() {
        let json = serde_json::json!({
            "cmd": "server",
            "count": 3,
            "ratio": 0.5,
            "on": true,
            "list": ["a", "b"],
            "nested": { "k": "v" }
        });
        let toml_value = json_to_toml_value(&json);
        let table = toml_value.as_table().unwrap();
        assert_eq!(table["cmd"].as_str(), Some("server"));
        assert_eq!(table["count"].as_integer(), Some(3));
        assert_eq!(table["ratio"].as_float(), Some(0.5));
        assert_eq!(table["on"].as_bool(), Some(true));
        assert_eq!(table["list"].as_array().unwrap().len(), 2);
        assert_eq!(table["nested"]["k"].as_str(), Some("v"));
    }

    #[test]
    fn trae_and_vscode_paths_follow_the_current_os() {
        let expected_root = if cfg!(target_os = "macos") {
            "Library/Application Support"
        } else if cfg!(target_os = "windows") {
            "AppData/Roaming"
        } else {
            ".config"
        };
        let adapters = all_mcp_adapters();
        for (key, app) in [("trae", "Trae"), ("vscode", "Code")] {
            let adapter = adapters.iter().find(|a| a.key == key).unwrap();
            assert!(
                adapter.config_path.starts_with(expected_root),
                "{} config path {} does not start with {}",
                key,
                adapter.config_path,
                expected_root
            );
            assert!(
                adapter
                    .config_path
                    .ends_with(&format!("{app}/User/mcp.json")),
                "{} config path {} has an unexpected suffix",
                key,
                adapter.config_path
            );
        }
    }

    #[test]
    fn opencode_resolver_prefers_jsonc_then_json_then_jsonc_for_fresh_installs() {
        let base = temp_dir("opencode-resolve");
        // Fresh install: the resolver names the file to create (jsonc).
        assert_eq!(
            opencode_config_path(&base),
            base.join(".config/opencode/opencode.jsonc")
        );

        let dir = base.join(".config/opencode");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("opencode.json"), "{}").unwrap();
        assert_eq!(opencode_config_path(&base), dir.join("opencode.json"));

        fs::write(dir.join("opencode.jsonc"), "{}").unwrap();
        assert_eq!(opencode_config_path(&base), dir.join("opencode.jsonc"));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn opencode_adapter_uses_the_shared_resolver() {
        let base = temp_dir("opencode-adapter");
        let dir = base.join(".config/opencode");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("opencode.json"), "{}").unwrap();
        let adapter = all_mcp_adapters()
            .into_iter()
            .find(|a| a.key == "opencode")
            .unwrap();
        assert_eq!(adapter.resolve_path_in(&base), dir.join("opencode.json"));
        assert_eq!(
            adapter.resolve_path_in(&temp_dir("opencode-adapter-empty")),
            temp_dir("opencode-adapter-empty").join(".config/opencode/opencode.jsonc")
        );
        let _ = fs::remove_dir_all(&base);
    }

    fn server_with(command: &str, disabled: bool) -> McpServerConfig {
        McpServerConfig {
            command: Some(command.to_string()),
            args: None,
            url: None,
            env: None,
            headers: None,
            disabled: if disabled { Some(true) } else { None },
            description: None,
            targets: vec![],
        }
    }

    fn json_adapter() -> McpAdapter {
        McpAdapter {
            key: "test",
            name: "Test",
            config_path: "unused",
            mcp_key: "mcpServers",
            format: McpFormat::Json,
        }
    }

    fn toml_adapter() -> McpAdapter {
        McpAdapter {
            key: "codex",
            name: "Codex",
            config_path: "unused",
            mcp_key: "mcp_servers",
            format: McpFormat::Toml,
        }
    }

    #[test]
    fn managed_manifest_round_trips_and_reports_corruption() {
        let dir = temp_dir("managed");
        let path = dir.join("mcp-managed.json");
        let mut state = McpManagedState::default();
        state.tools.insert(
            "claude_code".to_string(),
            vec!["alpha".to_string(), "beta".to_string()],
        );
        write_managed_at(&path, &state, None).unwrap();
        assert_eq!(read_managed_at(&path).unwrap(), state);
        // Missing file is empty state, not an error.
        assert_eq!(
            read_managed_at(&dir.join("missing.json")).unwrap(),
            McpManagedState::default()
        );
        // A corrupt manifest is an error so cleanup never rewrites it blindly.
        fs::write(&path, "{broken").unwrap();
        assert!(read_managed_at(&path).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_removes_a_managed_json_server_that_is_no_longer_enabled() {
        let dir = temp_dir("unsync-json");
        let path = dir.join("mcp.json");
        fs::write(
            &path,
            r#"{"mcpServers":{"gone":{"command":"old"},"toolOnly":{"command":"keep"}}}"#,
        )
        .unwrap();
        let adapter = json_adapter();
        let mut central = McpCentralConfig::default();
        // "gone" was disabled in the UI; "fresh" is newly enabled.
        central
            .servers
            .insert("gone".to_string(), server_with("old", true));
        central
            .servers
            .insert("fresh".to_string(), server_with("new", false));
        let previously = vec!["gone".to_string()];

        let (result, expected) =
            sync_to_tool_at(&adapter, &path, &central, &previously, None).unwrap();
        assert!(!result.skipped);
        assert!(result.message.contains("removed 1"), "{}", result.message);
        assert_eq!(expected, vec!["fresh".to_string()]);

        let parsed: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(parsed["mcpServers"].get("gone").is_none());
        assert_eq!(parsed["mcpServers"]["fresh"]["command"], "new");
        // A server added outside aide is never deleted.
        assert_eq!(parsed["mcpServers"]["toolOnly"]["command"], "keep");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_does_not_touch_tool_only_servers_when_removing_stale_names() {
        let dir = temp_dir("unsync-tool-only");
        let path = dir.join("mcp.json");
        fs::write(&path, r#"{"mcpServers":{"toolOnly":{"command":"keep"}}}"#).unwrap();
        let adapter = json_adapter();
        let central = McpCentralConfig::default();
        // "ghost" is recorded as managed but never present in the config.
        let previously = vec!["ghost".to_string()];
        let (result, expected) =
            sync_to_tool_at(&adapter, &path, &central, &previously, None).unwrap();
        assert!(!result.skipped);
        assert!(expected.is_empty());
        let parsed: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed["mcpServers"]["toolOnly"]["command"], "keep");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_removes_a_managed_toml_server_and_keeps_tool_only_tables() {
        let dir = temp_dir("unsync-toml");
        let path = dir.join("config.toml");
        fs::write(
            &path,
            "[other]\nkey = 1\n\n[mcp_servers.gone]\ncommand = \"old\"\n\n[mcp_servers.toolOnly]\ncommand = \"keep\"\n",
        )
        .unwrap();
        let adapter = toml_adapter();
        let mut central = McpCentralConfig::default();
        central
            .servers
            .insert("gone".to_string(), server_with("old", true));
        central
            .servers
            .insert("fresh".to_string(), server_with("new", false));
        let previously = vec!["gone".to_string()];

        let (result, expected) =
            sync_to_tool_at(&adapter, &path, &central, &previously, None).unwrap();
        assert!(!result.skipped);
        assert!(result.message.contains("removed 1"), "{}", result.message);
        assert_eq!(expected, vec!["fresh".to_string()]);

        let parsed: toml::Table = fs::read_to_string(&path).unwrap().parse().unwrap();
        assert_eq!(parsed["other"]["key"].as_integer(), Some(1));
        assert!(parsed["mcp_servers"].get("gone").is_none());
        assert_eq!(
            parsed["mcp_servers"]["toolOnly"]["command"].as_str(),
            Some("keep")
        );
        assert_eq!(
            parsed["mcp_servers"]["fresh"]["command"].as_str(),
            Some("new")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_skips_uninstalled_tools_and_does_not_create_a_config() {
        let dir = temp_dir("skip-uninstalled");
        let path = dir.join("mcp.json");
        let adapter = json_adapter();
        let mut central = McpCentralConfig::default();
        central.servers.insert("only_for_other".to_string(), {
            let mut server = server_with("x", false);
            server.targets = vec!["cursor".to_string()];
            server
        });
        let (result, expected) = sync_to_tool_at(&adapter, &path, &central, &[], None).unwrap();
        assert!(result.skipped);
        assert!(expected.is_empty());
        assert!(!path.exists(), "config must not be created for a skip");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn drift_reports_missing_and_leftover_servers() {
        let dir = temp_dir("drift");
        let path = dir.join("mcp.json");
        fs::write(
            &path,
            r#"{"mcpServers":{"leftover":{"command":"old"},"toolOnly":{"command":"x"}}}"#,
        )
        .unwrap();
        let adapter = json_adapter();
        let mut central = McpCentralConfig::default();
        central
            .servers
            .insert("missing".to_string(), server_with("new", false));
        central
            .servers
            .insert("leftover".to_string(), server_with("old", true));
        let mut managed = McpManagedState::default();
        managed
            .tools
            .insert("test".to_string(), vec!["leftover".to_string()]);

        let drift = compute_drift(&adapter, &path, &central, &managed)
            .unwrap()
            .expect("drift expected");
        assert_eq!(drift.missing, vec!["missing".to_string()]);
        assert_eq!(drift.leftover, vec!["leftover".to_string()]);

        // A tool that is not installed and has no entries is skipped entirely.
        assert!(compute_drift(
            &adapter,
            &dir.join("not-installed.json"),
            &McpCentralConfig::default(),
            &McpManagedState::default()
        )
        .unwrap()
        .is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn drift_is_empty_when_the_tool_matches_the_central_config() {
        let dir = temp_dir("drift-clean");
        let path = dir.join("config.toml");
        fs::write(&path, "[mcp_servers.present]\ncommand = \"ok\"\n").unwrap();
        let adapter = toml_adapter();
        let mut central = McpCentralConfig::default();
        central
            .servers
            .insert("present".to_string(), server_with("ok", false));
        let mut managed = McpManagedState::default();
        managed
            .tools
            .insert("codex".to_string(), vec!["present".to_string()]);
        assert!(compute_drift(&adapter, &path, &central, &managed)
            .unwrap()
            .is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
