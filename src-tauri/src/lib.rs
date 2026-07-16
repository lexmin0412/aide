use std::path::Path;
use std::time::SystemTime;
use std::{fs, io};

use serde::{Deserialize, Serialize};

mod adapter;
mod mcp;

#[derive(Debug, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_at: u64,
    pub extension: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub path: String,
    pub is_symlink: bool,
    pub target_path: Option<String>,
    pub file_count: usize,
}

fn read_dir_entries(path: &Path) -> io::Result<Vec<FileEntry>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        let path = entry.path();
        let modified_at = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        entries.push(FileEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: path.to_string_lossy().to_string(),
            is_dir: path.is_dir(),
            size: meta.len(),
            modified_at,
            extension: path.extension().map(|e| e.to_string_lossy().to_string()),
        });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

fn count_files(path: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !matches!(name.as_str(), "node_modules" | ".git" | ".pnpm" | "dist" | ".next" | "target") {
                    count += 1;
                }
            } else {
                count += 1;
            }
        }
    }
    count
}

fn parse_skill_frontmatter(content: &str) -> (Option<String>, Option<String>) {
    let content = content.trim();
    if !content.starts_with("---") {
        return (None, None);
    }
    let rest = content[3..].trim();
    if let Some(end) = rest.find("---") {
        let yaml_str = &rest[..end];
        if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(yaml_str) {
            let mapping = match &val {
                serde_yaml::Value::Mapping(m) => m,
                _ => return (None, None),
            };
            let name = mapping
                .get(&serde_yaml::Value::String("name".into()))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let desc = mapping
                .get(&serde_yaml::Value::String("description".into()))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            return (name, desc);
        }
    }
    (None, None)
}

#[tauri::command]
fn list_skills() -> Result<Vec<SkillInfo>, String> {
    let skills_dir = dirs::home_dir()
        .ok_or_else(|| "Cannot find home directory".to_string())?
        .join(".agents")
        .join("skills");

    if !skills_dir.exists() {
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();
    for entry in fs::read_dir(&skills_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        let is_symlink = meta.is_symlink();
        let target_path = if is_symlink {
            fs::read_link(&path).ok().map(|p| p.to_string_lossy().to_string())
        } else {
            None
        };

        let (display_name, description) = {
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                if let Ok(content) = fs::read_to_string(&skill_md) {
                    let (n, d) = parse_skill_frontmatter(&content);
                    (n.unwrap_or_else(|| name.clone()), d.unwrap_or_default())
                } else {
                    (name.clone(), String::new())
                }
            } else {
                (name.clone(), String::new())
            }
        };

        let file_count = if is_symlink {
            let real_path = fs::read_link(&path).unwrap_or(path.clone());
            count_files(&real_path)
        } else {
            count_files(&path)
        };

        skills.push(SkillInfo {
            name: name.clone(),
            display_name,
            description,
            path: path.to_string_lossy().to_string(),
            is_symlink,
            target_path,
            file_count,
        });
    }

    skills.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(skills)
}

#[tauri::command]
fn list_directory(path: String) -> Result<Vec<FileEntry>, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {}", path));
    }
    read_dir_entries(dir).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_text_file(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn write_text_file(path: String, content: String) -> Result<(), String> {
    if let Some(parent) = Path::new(&path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, &content).map_err(|e| e.to_string())
}

#[tauri::command]
fn create_file(path: String) -> Result<(), String> {
    if let Some(parent) = Path::new(&path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::File::create(&path).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn create_directory(path: String) -> Result<(), String> {
    fs::create_dir_all(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_entry(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if p.is_dir() {
        fs::remove_dir_all(p).map_err(|e| e.to_string())
    } else {
        fs::remove_file(p).map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn rename_entry(old_path: String, new_path: String) -> Result<(), String> {
    fs::rename(&old_path, &new_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn file_exists(path: String) -> bool {
    Path::new(&path).exists()
}

#[derive(Serialize)]
pub struct ToolInfo {
    key: String,
    name: String,
    global_skills: String,
    detect_dir: String,
    project_skills: String,
    status: String,
}

fn tool_dir_exists(relative: &str) -> bool {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };
    home.join(relative).exists()
}

fn resolve_skills_source() -> Result<std::path::PathBuf, String> {
    dirs::home_dir()
        .map(|h| h.join(".agents").join("skills"))
        .ok_or_else(|| "Cannot find home directory".to_string())
}

fn compute_tool_status(t: &adapter::ToolInfo) -> String {
    if !tool_dir_exists(t.detect_dir) {
        return "not_installed".into();
    }
    if t.global_skills == ".agents/skills" {
        return "compatible".into();
    }
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return "unknown".into(),
    };
    let target = home.join(t.global_skills);
    if target.is_symlink() {
        if let Ok(link) = std::fs::read_link(&target) {
            if link == resolve_skills_source().unwrap_or_default() {
                return "synced".into();
            }
        }
    }
    if target.exists() {
        return "has_content".into();
    }
    "ready".into()
}

#[tauri::command]
fn check_sync_statuses() -> Vec<ToolInfo> {
    adapter::all_tools()
        .into_iter()
        .map(|t| {
            let status = compute_tool_status(&t);
            ToolInfo {
                key: t.key.to_string(),
                name: t.name.to_string(),
                global_skills: t.global_skills.to_string(),
                detect_dir: t.detect_dir.to_string(),
                project_skills: t.project_skills.to_string(),
                status,
            }
        })
        .collect()
}

#[derive(Serialize)]
pub struct SyncResult {
    pub key: String,
    pub name: String,
    pub success: bool,
    pub merged: Vec<String>,
    pub conflicts: Vec<String>,
    pub error: Option<String>,
}

fn gather_skills(dir: &Path) -> io::Result<Vec<String>> {
    let mut skills = Vec::new();
    if !dir.exists() {
        return Ok(skills);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.join("SKILL.md").exists() {
            skills.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    Ok(skills)
}

#[tauri::command]
fn sync_tool(tool_key: String) -> Result<SyncResult, String> {
    let tool = adapter::all_tools()
        .into_iter()
        .find(|t| t.key == tool_key)
        .ok_or_else(|| format!("Unknown tool: {}", tool_key))?;

    if tool.global_skills == ".agents/skills" {
        return Ok(SyncResult {
            key: tool_key,
            name: tool.name.to_string(),
            success: true,
            merged: vec![],
            conflicts: vec![],
            error: None,
        });
    }

    let source = resolve_skills_source()?;
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    let target = home.join(tool.global_skills);

    if !source.exists() {
        return Ok(SyncResult {
            key: tool_key,
            name: tool.name.to_string(),
            success: true,
            merged: vec![],
            conflicts: vec![],
            error: Some("Source ~/.agents/skills does not exist".into()),
        });
    }

    // Already a symlink pointing to source
    if target.is_symlink() {
        if let Ok(link) = std::fs::read_link(&target) {
            if link == source {
                return Ok(SyncResult {
                    key: tool_key,
                    name: tool.name.to_string(),
                    success: true,
                    merged: vec![],
                    conflicts: vec![],
                    error: None,
                });
            }
        }
        std::fs::remove_file(&target).map_err(|e| e.to_string())?;
    }

    let mut merged = Vec::new();
    let mut conflicts = Vec::new();

    if target.exists() && !target.is_symlink() {
        let target_skills = gather_skills(&target).map_err(|e| e.to_string())?;
        for skill in &target_skills {
            let src_skill_dir = source.join(skill);
            let tgt_skill_dir = target.join(skill);

            if !src_skill_dir.exists() {
                if let Ok(_) = fs::rename(&tgt_skill_dir, &src_skill_dir) {
                    merged.push(skill.clone());
                }
            } else {
                conflicts.push(skill.clone());
            }
        }

        fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    std::os::unix::fs::symlink(&source, &target).map_err(|e| e.to_string())?;
    #[cfg(not(target_os = "macos"))]
    return Err("Symlink not supported on this platform".to_string());

    Ok(SyncResult {
        key: tool_key,
        name: tool.name.to_string(),
        success: conflicts.is_empty(),
        merged,
        conflicts,
        error: None,
    })
}

#[tauri::command]
fn sync_all_tools() -> Vec<SyncResult> {
    let tools = adapter::all_tools();
    tools
        .into_iter()
        .filter_map(|t| {
            if t.global_skills == ".agents/skills" {
                return None;
            }
            sync_tool(t.key.to_string()).ok()
        })
        .collect()
}

#[tauri::command]
fn list_tools() -> Vec<ToolInfo> {
    adapter::all_tools()
        .into_iter()
        .map(|t| {
            let status = compute_tool_status(&t);
            ToolInfo {
                key: t.key.to_string(),
                name: t.name.to_string(),
                global_skills: t.global_skills.to_string(),
                detect_dir: t.detect_dir.to_string(),
                project_skills: t.project_skills.to_string(),
                status,
            }
        })
        .collect()
}

#[derive(Serialize, Deserialize)]
pub struct McpServerView {
    pub name: String,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub disabled: Option<bool>,
    pub description: Option<String>,
    pub targets: Vec<String>,
}

#[tauri::command]
fn list_mcp_servers() -> Result<Vec<McpServerView>, String> {
    let config = mcp::read_central()?;
    Ok(config
        .servers
        .into_iter()
        .map(|(name, s)| McpServerView {
            name,
            command: s.command,
            args: s.args,
            url: s.url,
            env: s.env,
            disabled: s.disabled,
            description: s.description,
            targets: s.targets,
        })
        .collect())
}

#[tauri::command]
fn save_mcp_servers(servers: Vec<McpServerView>) -> Result<(), String> {
    let config = mcp::McpCentralConfig {
        servers: servers
            .into_iter()
            .map(|s| {
                let name = s.name;
                (name, mcp::McpServerConfig {
                    command: s.command,
                    args: s.args,
                    url: s.url,
                    env: s.env,
                    disabled: s.disabled,
                    description: s.description,
                    targets: s.targets,
                })
            })
            .collect(),
    };
    mcp::save_central(&config)
}

#[derive(Serialize)]
pub struct McpSyncResult {
    pub key: String,
    pub name: String,
    pub skipped: bool,
    pub message: String,
}

#[tauri::command]
fn sync_mcp_tool(tool_key: String) -> Result<McpSyncResult, String> {
    let adapter = mcp::all_mcp_adapters()
        .into_iter()
        .find(|a| a.key == tool_key)
        .ok_or_else(|| format!("Unknown tool: {}", tool_key))?;
    let config = mcp::read_central()?;
    let result = mcp::sync_to_tool(&adapter, &config)?;
    Ok(McpSyncResult {
        key: tool_key,
        name: adapter.name.to_string(),
        skipped: result.skipped,
        message: result.message,
    })
}

#[tauri::command]
fn sync_mcp_all() -> Vec<McpSyncResult> {
    let config = mcp::read_central().ok();
    let config = match config {
        Some(c) => c,
        None => return vec![],
    };
    mcp::all_mcp_adapters()
        .into_iter()
        .filter_map(|adapter| {
            let result = mcp::sync_to_tool(&adapter, &config).ok()?;
            Some(McpSyncResult {
                key: adapter.key.to_string(),
                name: adapter.name.to_string(),
                skipped: result.skipped,
                message: result.message,
            })
        })
        .collect()
}

#[derive(Serialize)]
pub struct ImportResult {
    pub source: String,
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
}

#[tauri::command]
fn import_mcp_all() -> Vec<ImportResult> {
    let mut central = match mcp::read_central() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut results = Vec::new();
    for adapter in mcp::all_mcp_adapters() {
        match mcp::import_from_adapter(&adapter, &mut central) {
            Ok(result) => {
                if !result.imported.is_empty() || !result.skipped.is_empty() {
                    results.push(ImportResult {
                        source: result.source,
                        imported: result.imported,
                        skipped: result.skipped,
                    });
                }
            }
            Err(_e) => {
                results.push(ImportResult {
                    source: adapter.name.to_string(),
                    imported: vec![],
                    skipped: vec![],
                });
            }
        }
    }
    let _ = mcp::save_central(&central);
    results
}

#[tauri::command]
fn get_home_dir() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_tools,
            list_mcp_servers,
            save_mcp_servers,
            sync_mcp_tool,
            sync_mcp_all,
            import_mcp_all,
            check_sync_statuses,
            sync_tool,
            sync_all_tools,
            list_skills,
            list_directory,
            read_text_file,
            write_text_file,
            create_file,
            create_directory,
            delete_entry,
            rename_entry,
            file_exists,
            get_home_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
