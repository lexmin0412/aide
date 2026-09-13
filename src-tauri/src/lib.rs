use std::path::Path;
use std::time::SystemTime;
use std::{fs, io};

use serde::{Deserialize, Serialize};

pub mod adapter;
pub mod git_sync;
pub mod mcp;
pub mod registry;

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
    pub tags: Vec<String>,
    pub path: String,
    pub is_symlink: bool,
    pub target_path: Option<String>,
    pub file_count: usize,
    /// Install provenance, e.g. "owner/repo" from the skills registry.
    pub source: Option<String>,
    /// Explicitly assigned git scope; None means the default scope applies.
    pub scope: Option<String>,
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

const SKILL_COUNT_IGNORED_DIRS: &[&str] = &[
    "node_modules", ".git", ".pnpm", "dist", ".next", "target",
];

const SKILL_TEXT_EXTENSIONS: &[&str] = &[
    "md", "markdown", "json", "jsonc", "yaml", "yml", "toml", "txt",
    "js", "ts", "jsx", "tsx", "css", "html", "sh", "bash", "env",
];

const SEARCH_MAX_FILE_SIZE: u64 = 512 * 1024;
const SEARCH_MAX_PER_SKILL: usize = 5;
const SEARCH_MAX_MATCHES: usize = 50;

fn count_files(path: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !SKILL_COUNT_IGNORED_DIRS.contains(&name.as_str()) {
                    count += count_files(&p);
                }
            } else {
                count += 1;
            }
        }
    }
    count
}

pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let name = entry.file_name();
        // Never copy dependency/build artifacts into the central store.
        if from.is_dir() && SKILL_COUNT_IGNORED_DIRS.contains(&name.to_string_lossy().as_ref()) {
            continue;
        }
        let to = dst.join(&name);
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn import_skill_into(source: &Path, skills_dir: &Path) -> Result<String, String> {
    if !source.is_dir() {
        return Err(format!("Not a directory: {}", source.display()));
    }
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| "Invalid source folder".to_string())?;
    let dest = skills_dir.join(&name);
    if dest.exists() {
        return Err(format!("A skill named \"{name}\" already exists"));
    }
    copy_dir_recursive(source, &dest).map_err(|e| format!("Import failed: {e}"))?;
    Ok(name)
}

#[derive(Debug, Serialize)]
pub struct SkillSearchMatch {
    pub skill_name: String,
    pub display_name: String,
    pub file_path: String,
    pub line_number: usize,
    pub line_text: String,
}

fn search_file(
    path: &Path,
    query: &str,
    skill_name: &str,
    display_name: &str,
    limit: usize,
) -> io::Result<Vec<SkillSearchMatch>> {
    let content = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if line.to_lowercase().contains(query) {
            let trimmed = line.trim();
            let mut text: String = trimmed.chars().take(160).collect();
            if trimmed.chars().count() > 160 {
                text.push_str("...");
            }
            out.push(SkillSearchMatch {
                skill_name: skill_name.to_string(),
                display_name: display_name.to_string(),
                file_path: path.to_string_lossy().to_string(),
                line_number: idx + 1,
                line_text: text,
            });
            if out.len() >= limit {
                break;
            }
        }
    }
    Ok(out)
}

pub fn search_skills_in(dir: &Path, query: &str) -> Vec<SkillSearchMatch> {
    let q = query.trim().to_lowercase();
    let mut matches = Vec::new();
    if q.is_empty() {
        return matches;
    }
    let Ok(skills) = fs::read_dir(dir) else {
        return matches;
    };
    let mut skill_paths: Vec<_> = skills
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    skill_paths.sort();
    'outer: for skill_path in skill_paths {
        let skill_name = skill_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let display_name = fs::read_to_string(skill_path.join("SKILL.md"))
            .ok()
            .and_then(|c| parse_skill_frontmatter(&c).0)
            .unwrap_or_else(|| skill_name.clone());

        let mut per_skill = 0;
        let mut stack = vec![skill_path.clone()];
        while let Some(current) = stack.pop() {
            let Ok(entries) = fs::read_dir(&current) else { continue };
            let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            paths.sort();
            for p in paths {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if p.is_dir() {
                    if !SKILL_COUNT_IGNORED_DIRS.contains(&name.as_str()) {
                        stack.push(p);
                    }
                    continue;
                }
                let ext_ok = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| SKILL_TEXT_EXTENSIONS.contains(&e.to_lowercase().as_str()))
                    .unwrap_or(false);
                if !ext_ok {
                    continue;
                }
                if fs::metadata(&p).map(|m| m.len() > SEARCH_MAX_FILE_SIZE).unwrap_or(true) {
                    continue;
                }
                let remaining = SEARCH_MAX_PER_SKILL.saturating_sub(per_skill);
                if remaining == 0 {
                    break;
                }
                if let Ok(found) = search_file(&p, &q, &skill_name, &display_name, remaining) {
                    per_skill += found.len();
                    matches.extend(found);
                }
                if per_skill >= SEARCH_MAX_PER_SKILL || matches.len() >= SEARCH_MAX_MATCHES {
                    break;
                }
            }
            if per_skill >= SEARCH_MAX_PER_SKILL || matches.len() >= SEARCH_MAX_MATCHES {
                break;
            }
        }
        if matches.len() >= SEARCH_MAX_MATCHES {
            break 'outer;
        }
    }
    matches
}

fn extract_tags(value: &serde_norway::Value) -> Vec<String> {
    match value {
        serde_norway::Value::Sequence(arr) => {
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
        }
        serde_norway::Value::String(s) => {
            s.split(|c: char| c == ' ' || c == ',' || c == '，')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        }
        _ => Vec::new(),
    }
}

fn parse_skill_frontmatter(content: &str) -> (Option<String>, Option<String>, Vec<String>) {
    let content = content.trim();
    if !content.starts_with("---") {
        return (None, None, Vec::new());
    }
    let rest = content[3..].trim();
    if let Some(end) = rest.find("---") {
        let yaml_str = &rest[..end];
        if let Ok(val) = serde_norway::from_str::<serde_norway::Value>(yaml_str) {
            let mapping = match &val {
                serde_norway::Value::Mapping(m) => m,
                _ => return (None, None, Vec::new()),
            };
            let name = mapping
                .get(&serde_norway::Value::String("name".into()))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let desc = mapping
                .get(&serde_norway::Value::String("description".into()))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let tags = mapping
                .get(&serde_norway::Value::String("tags".into()))
                .map(extract_tags)
                .filter(|t| !t.is_empty())
                .or_else(|| {
                    mapping
                        .get(&serde_norway::Value::String("metadata".into()))
                        .and_then(|v| v.as_mapping())
                        .and_then(|m| m.get(&serde_norway::Value::String("tags".into())))
                        .map(extract_tags)
                })
                .unwrap_or_default();
            return (name, desc, tags);
        }
    }
    (None, None, Vec::new())
}

/// Core of `list_skills`, shared with the aide-mcp agent binary.
pub fn skill_infos() -> Result<Vec<SkillInfo>, String> {
    let skills_dir = dirs::home_dir()
        .ok_or_else(|| "Cannot find home directory".to_string())?
        .join(".agents")
        .join("skills");

    if !skills_dir.exists() {
        return Ok(Vec::new());
    }

    let sources = registry::read_sources(
        &dirs::home_dir()
            .map(|h| h.join(".aide"))
            .unwrap_or_default(),
    );
    let git_config = git_sync::read_config(
        &dirs::home_dir()
            .map(|h| h.join(".aide"))
            .unwrap_or_default(),
    );

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

        let (display_name, description, tags) = {
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                if let Ok(content) = fs::read_to_string(&skill_md) {
                    let (n, d, t) = parse_skill_frontmatter(&content);
                    (n.unwrap_or_else(|| name.clone()), d.unwrap_or_default(), t)
                } else {
                    (name.clone(), String::new(), Vec::new())
                }
            } else {
                (name.clone(), String::new(), Vec::new())
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
            tags,
            path: path.to_string_lossy().to_string(),
            is_symlink,
            target_path,
            file_count,
            source: sources.get(&name).map(|s| s.source.clone()),
            scope: explicit_scope(&git_config, &name),
        });
    }

    skills.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(skills)
}

#[tauri::command]
fn list_skills() -> Result<Vec<SkillInfo>, String> {
    skill_infos()
}

#[tauri::command]
fn list_directory(path: String) -> Result<Vec<FileEntry>, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {}", path));
    }
    read_dir_entries(dir).map_err(|e| e.to_string())
}

/// Explicit git scope assignment for a skill (None = default scope applies).
fn explicit_scope(config: &git_sync::GitRemotesConfig, skill: &str) -> Option<String> {
    config
        .skill_scopes
        .get(skill)
        .filter(|s| config.remotes.iter().any(|r| &r.name == *s))
        .cloned()
}

#[tauri::command]
fn get_git_config() -> git_sync::GitRemotesConfig {
    git_sync::read_config(
        &dirs::home_dir()
            .map(|h| h.join(".aide"))
            .unwrap_or_default(),
    )
}

#[tauri::command]
fn save_git_config(config: git_sync::GitRemotesConfig) -> Result<(), String> {
    let aide_dir = dirs::home_dir()
        .map(|h| h.join(".aide"))
        .ok_or_else(|| "Cannot find home directory".to_string())?;
    git_sync::write_config(&aide_dir, &config)
}

#[tauri::command]
fn set_skill_scope(skill: String, scope: Option<String>) -> Result<(), String> {
    let aide_dir = dirs::home_dir()
        .map(|h| h.join(".aide"))
        .ok_or_else(|| "Cannot find home directory".to_string())?;
    let mut config = git_sync::read_config(&aide_dir);
    match scope {
        Some(s) => {
            config.skill_scopes.insert(skill, s);
        }
        None => {
            config.skill_scopes.remove(&skill);
        }
    }
    git_sync::write_config(&aide_dir, &config)
}

#[tauri::command]
fn publish_scope(scope: String) -> Result<git_sync::PublishOutcome, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    let aide_dir = home.join(".aide");
    let config = git_sync::read_config(&aide_dir);
    let skills_dir = resolve_skills_source()?;
    git_sync::publish_scope(&config, &skills_dir, &aide_dir, &scope)
}

#[tauri::command]
fn pull_scope(scope: String) -> Result<git_sync::PullOutcome, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    let aide_dir = home.join(".aide");
    let config = git_sync::read_config(&aide_dir);
    let skills_dir = resolve_skills_source()?;
    git_sync::pull_scope(&config, &skills_dir, &aide_dir, &scope)
}

#[tauri::command]
fn import_skill(source: String) -> Result<String, String> {    let skills_dir = resolve_skills_source()?;
    let name = import_skill_into(Path::new(&source), &skills_dir)?;
    if let Some(home) = dirs::home_dir() {
        // Record where the skill came from so provenance is visible in the UI.
        let _ = registry::record_source(&home.join(".aide"), &name, &source, "local", None);
    }
    Ok(name)
}

#[tauri::command]
fn search_registry(query: String) -> Result<Vec<registry::RegistrySkill>, String> {
    registry::search_registry(&query, 24)
}

#[tauri::command]
fn install_skill_from_registry(id: String) -> Result<registry::RemoteInstallResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    let skills_dir = resolve_skills_source()?;
    fs::create_dir_all(&skills_dir).map_err(|e| e.to_string())?;
    registry::install_from_id(&id, &skills_dir, &home.join(".aide"), false)
}

#[tauri::command]
fn check_skill_updates() -> Vec<registry::SkillUpdate> {
    registry::check_updates(
        &dirs::home_dir()
            .map(|h| h.join(".aide"))
            .unwrap_or_default(),
    )
}

#[tauri::command]
fn update_skill(skill: String) -> Result<registry::RemoteInstallResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    let aide_dir = home.join(".aide");
    let sources = registry::read_sources(&aide_dir);
    let meta = sources
        .get(&skill)
        .ok_or_else(|| format!("No install source recorded for \"{skill}\""))?
        .clone();
    let skills_dir = resolve_skills_source()?;
    fs::create_dir_all(&skills_dir).map_err(|e| e.to_string())?;
    registry::install_from_id(&meta.id, &skills_dir, &aide_dir, true)
}

const AGENT_SERVER_NAME: &str = "aide";

/// The aide-mcp sidecar binary sits next to the main executable in packaged
/// builds and in target/debug during development.
fn sidecar_path() -> Option<std::path::PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    ["aide-mcp", "aide-mcp.exe"]
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.exists())
}

#[derive(Serialize)]
pub struct AgentServerStatus {
    pub enabled: bool,
    pub binary_available: bool,
    pub command: Option<String>,
}

#[tauri::command]
fn get_agent_server() -> AgentServerStatus {
    let binary = sidecar_path();
    let enabled = binary.is_some()
        && mcp::read_central()
            .map(|c| {
                c.servers
                    .get(AGENT_SERVER_NAME)
                    .map(|s| !s.disabled.unwrap_or(false))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
    AgentServerStatus {
        enabled,
        binary_available: binary.is_some(),
        command: binary.map(|p| p.to_string_lossy().to_string()),
    }
}

#[tauri::command]
fn enable_agent_access(enable: bool) -> Result<AgentServerStatus, String> {
    let mut central = mcp::read_central()?;
    if enable {
        let path = sidecar_path()
            .ok_or("aide-mcp binary not found next to the running app")?;
        central.servers.insert(
            AGENT_SERVER_NAME.to_string(),
            mcp::McpServerConfig {
                command: Some(path.to_string_lossy().to_string()),
                args: None,
                url: None,
                env: None,
                headers: None,
                disabled: Some(false),
                description: Some(
                    "aide agent access: install, update and publish skills".into(),
                ),
                targets: vec![],
            },
        );
    } else {
        central.servers.remove(AGENT_SERVER_NAME);
    }
    mcp::save_central(&central)?;
    Ok(get_agent_server())
}

#[tauri::command]
fn search_skills(query: String) -> Result<Vec<SkillSearchMatch>, String> {
    let dir = resolve_skills_source()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    Ok(search_skills_in(&dir, &query))
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
    // Move to the system trash instead of permanent deletion so mistakes are
    // recoverable. Fall back to permanent removal if the trash operation fails.
    if trash::delete(&path).is_ok() {
        return Ok(());
    }
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
fn update_skill_tags(path: String, tags: Vec<String>) -> Result<(), String> {
    let skill_md = Path::new(&path).join("SKILL.md");
    let content = fs::read_to_string(&skill_md).map_err(|e| e.to_string())?;
    let content = content.trim();

    let tags_line = if tags.is_empty() {
        String::new()
    } else {
        let joined = tags
            .iter()
            .map(|t| format!("\"{}\"", t))
            .collect::<Vec<_>>()
            .join(", ");
        format!("tags: [{}]", joined)
    };

    let new_content = if content.starts_with("---") {
        let rest = &content[3..];
        if let Some(end) = rest.find("---") {
            let front = &rest[..end];
            let body_start = end + 3;
            let body = &rest[body_start..];

            let lines: Vec<&str> = front.lines().collect();
            let mut out_lines: Vec<String> = Vec::new();
            let mut tags_written = false;
            let mut inside_metadata = false;

            for line in &lines {
                let indent = line.len() - line.trim_start().len();
                let trimmed = line.trim();
                if indent == 0 {
                    inside_metadata = trimmed.starts_with("metadata:");
                    if trimmed.starts_with("tags:") && !tags_written {
                        if !tags_line.is_empty() {
                            out_lines.push(tags_line.clone());
                        }
                        tags_written = true;
                        continue;
                    }
                } else if inside_metadata && trimmed.starts_with("tags:") {
                    // Tags are canonical at the top level; drop the nested
                    // metadata copy so the two cannot go out of sync.
                    continue;
                }
                out_lines.push(line.to_string());
            }

            if !tags_written && !tags_line.is_empty() {
                out_lines.push(tags_line);
            }

            format!("---\n{}\n---{}", out_lines.join("\n"), body)
        } else {
            return Err("Invalid frontmatter: no closing ---".to_string());
        }
    } else if !tags_line.is_empty() {
        format!("---\n{}\n---\n{}", tags_line, content)
    } else {
        return Ok(());
    };

    fs::write(&skill_md, &new_content).map_err(|e| e.to_string())
}

#[tauri::command]
fn file_exists(path: String) -> bool {
    Path::new(&path).exists()
}

#[tauri::command]
fn get_file_mtime(path: String) -> Result<u64, String> {
    let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
    Ok(meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0))
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

pub fn resolve_skills_source() -> Result<std::path::PathBuf, String> {
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
    pub backed_up: Vec<String>,
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
fn sync_tool(tool_key: String, overwrite: Option<bool>) -> Result<SyncResult, String> {
    let tool = adapter::all_tools()
        .into_iter()
        .find(|t| t.key == tool_key)
        .ok_or_else(|| format!("Unknown tool: {}", tool_key))?;
    let overwrite = overwrite.unwrap_or(false);

    let result = |merged: Vec<String>, backed_up: Vec<String>, conflicts: Vec<String>, error: Option<String>| -> Result<SyncResult, String> {
        Ok(SyncResult {
            key: tool_key.clone(),
            name: tool.name.to_string(),
            success: error.is_none(),
            merged,
            backed_up,
            conflicts,
            error,
        })
    };

    if tool.global_skills == ".agents/skills" {
        return result(vec![], vec![], vec![], None);
    }

    let source = resolve_skills_source()?;
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    let target = home.join(tool.global_skills);

    if !source.exists() {
        return result(
            vec![],
            vec![],
            vec![],
            Some("Source ~/.agents/skills does not exist".into()),
        );
    }

    // Already a symlink pointing to source
    if target.is_symlink() {
        if let Ok(link) = std::fs::read_link(&target) {
            if link == source {
                return result(vec![], vec![], vec![], None);
            }
        }
        std::fs::remove_file(&target).map_err(|e| e.to_string())?;
    }

    let mut merged = Vec::new();
    let mut backed_up = Vec::new();
    let mut conflicts = Vec::new();

    if target.exists() && !target.is_symlink() {
        let target_skills = gather_skills(&target).map_err(|e| e.to_string())?;
        for skill in &target_skills {
            let src_skill_dir = source.join(skill);
            let tgt_skill_dir = target.join(skill);

            if !src_skill_dir.exists() {
                if fs::rename(&tgt_skill_dir, &src_skill_dir).is_ok() {
                    merged.push(skill.clone());
                }
                continue;
            }

            // Same skill exists on both sides. Either the central copy wins
            // (explicit overwrite) or the tool-side copy is preserved in a
            // timestamped backup before the target directory is replaced.
            if overwrite {
                conflicts.push(skill.clone());
                continue;
            }
            let ts = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup_dir = home
                .join(".aide")
                .join("sync-backup")
                .join(&tool_key)
                .join(format!("{}-{}", skill, ts));
            if let Some(parent) = backup_dir.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if fs::rename(&tgt_skill_dir, &backup_dir).is_ok() {
                backed_up.push(skill.clone());
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
    #[cfg(target_os = "windows")]
    std::os::windows::fs::junction(&source, &target).map_err(|e| e.to_string())?;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return Err("Symlink not supported on this platform".to_string());

    result(merged, backed_up, conflicts, None)
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
            sync_tool(t.key.to_string(), None).ok()
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
    pub headers: Option<std::collections::HashMap<String, String>>,
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
            headers: s.headers,
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
                    headers: s.headers,
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
pub struct McpToolView {
    pub key: String,
    pub name: String,
}

#[tauri::command]
fn list_mcp_tools() -> Vec<McpToolView> {
    mcp::all_mcp_adapters()
        .into_iter()
        .map(|a| McpToolView {
            key: a.key.to_string(),
            name: a.name.to_string(),
        })
        .collect()
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            list_tools,
            list_mcp_tools,
            list_mcp_servers,
            save_mcp_servers,
            sync_mcp_tool,
            sync_mcp_all,
            import_mcp_all,
            check_sync_statuses,
            check_skill_updates,
            update_skill,
            get_agent_server,
            enable_agent_access,
            sync_tool,
            sync_all_tools,
            list_skills,
            import_skill,
            search_registry,
            install_skill_from_registry,
            search_skills,
            list_directory,
            read_text_file,
            write_text_file,
            create_file,
            create_directory,
            delete_entry,
            rename_entry,
            file_exists,
            get_file_mtime,
            get_home_dir,
            get_git_config,
            save_git_config,
            set_skill_scope,
            publish_scope,
            pull_scope,
            update_skill_tags,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frontmatter_reads_name_description_and_tags() {
        let md = "---\nname: my-skill\ndescription: Does things\ntags: [\"a\", \"b\"]\n---\n\n# Body\n";
        let (name, desc, tags) = parse_skill_frontmatter(md);
        assert_eq!(name.as_deref(), Some("my-skill"));
        assert_eq!(desc.as_deref(), Some("Does things"));
        assert_eq!(tags, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_frontmatter_reads_tags_from_metadata_block() {
        let md = "---\nname: my-skill\nmetadata:\n  tags: [\"x\", \"y\"]\n---\nBody";
        let (_, _, tags) = parse_skill_frontmatter(md);
        assert_eq!(tags, vec!["x".to_string(), "y".to_string()]);
    }

    #[test]
    fn parse_frontmatter_supports_comma_separated_string_tags() {
        let md = "---\nname: s\ntags: rust, tauri\n---\nBody";
        let (_, _, tags) = parse_skill_frontmatter(md);
        assert_eq!(tags, vec!["rust".to_string(), "tauri".to_string()]);
    }

    #[test]
    fn parse_frontmatter_returns_empty_without_frontmatter() {
        let (name, desc, tags) = parse_skill_frontmatter("# Just markdown");
        assert_eq!(name, None);
        assert_eq!(desc, None);
        assert!(tags.is_empty());
    }

    fn write_temp_skill(name: &str, content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aide-tags-test-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), content).unwrap();
        dir
    }

    #[test]
    fn update_skill_tags_replaces_top_level_tags() {
        let dir = write_temp_skill(
            "replace",
            "---\nname: s\ntags: [\"old\"]\n---\nBody",
        );
        update_skill_tags(dir.to_string_lossy().to_string(), vec!["new".into()]).unwrap();
        let content = fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert!(content.contains("tags: [\"new\"]"));
        assert!(!content.contains("old"));
        assert!(content.ends_with("Body"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_skill_tags_appends_when_missing() {
        let dir = write_temp_skill("append", "---\nname: s\n---\nBody");
        update_skill_tags(dir.to_string_lossy().to_string(), vec!["a".into(), "b".into()]).unwrap();
        let content = fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert!(content.contains("tags: [\"a\", \"b\"]"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_skill_tags_syncs_metadata_tags_copy() {
        let dir = write_temp_skill(
            "metadata",
            "---\nname: s\nmetadata:\n  tags: [\"old\"]\n  other: keep\n---\nBody",
        );
        update_skill_tags(dir.to_string_lossy().to_string(), vec!["fresh".into()]).unwrap();
        let content = fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert!(content.contains("tags: [\"fresh\"]"));
        assert!(!content.contains("old"));
        assert!(content.contains("other: keep"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_skill_tags_clears_existing_tags() {
        let dir = write_temp_skill("clear", "---\nname: s\ntags: [\"old\"]\n---\nBody");
        update_skill_tags(dir.to_string_lossy().to_string(), vec![]).unwrap();
        let content = fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert!(!content.contains("tags:"));
        assert!(content.contains("name: s"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn count_files_counts_recursively_and_skips_ignored_dirs() {
        let dir = std::env::temp_dir().join(format!(
            "aide-count-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("refs")).unwrap();
        fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
        fs::write(dir.join("SKILL.md"), "x").unwrap();
        fs::write(dir.join("refs/a.md"), "x").unwrap();
        fs::write(dir.join("node_modules/pkg/index.js"), "x").unwrap();
        assert_eq!(count_files(&dir), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_skill_copies_folder_and_rejects_duplicates() {
        let base = std::env::temp_dir().join(format!("aide-import-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let src = base.join("source").join("my-skill");
        fs::create_dir_all(src.join("refs")).unwrap();
        fs::create_dir_all(src.join("node_modules").join("pkg")).unwrap();
        fs::write(src.join("SKILL.md"), "---\nname: my-skill\n---\nhi").unwrap();
        fs::write(src.join("refs").join("a.md"), "doc").unwrap();
        fs::write(src.join("node_modules").join("pkg").join("i.js"), "x").unwrap();

        let skills_dir = base.join("skills");
        let imported = import_skill_into(&src, &skills_dir).unwrap();
        assert_eq!(imported, "my-skill");
        assert!(skills_dir.join("my-skill").join("SKILL.md").is_file());
        assert!(skills_dir.join("my-skill").join("refs").join("a.md").is_file());
        assert!(!skills_dir.join("my-skill").join("node_modules").exists());

        let duplicate = import_skill_into(&src, &skills_dir);
        assert!(duplicate.is_err());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn import_skill_rejects_non_directory() {
        let base = std::env::temp_dir().join(format!("aide-import-neg-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let file = base.join("plain.txt");
        fs::write(&file, "x").unwrap();
        assert!(import_skill_into(&file, &base.join("skills")).is_err());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn search_finds_matches_across_skills_case_insensitively() {
        let base = std::env::temp_dir().join(format!("aide-search-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let alpha = base.join("alpha");
        let beta = base.join("beta");
        fs::create_dir_all(&alpha).unwrap();
        fs::create_dir_all(&beta).unwrap();
        fs::write(
            alpha.join("SKILL.md"),
            "---\nname: alpha\ndescription: Alpha skill\n---\nuse the ZEBRA mode",
        )
        .unwrap();
        fs::write(alpha.join("notes.md"), "nothing here").unwrap();
        fs::write(beta.join("SKILL.md"), "---\nname: beta\n---\nZebra habitat facts").unwrap();

        let matches = search_skills_in(&base, "zebra");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].skill_name, "alpha");
        assert_eq!(matches[0].display_name, "alpha");
        assert_eq!(matches[0].line_number, 5);
        assert!(matches[0].line_text.contains("ZEBRA"));
        assert_eq!(matches[1].skill_name, "beta");

        let empty = search_skills_in(&base, "");
        assert!(empty.is_empty());
        let none = search_skills_in(&base, "unicorn");
        assert!(none.is_empty());
        let _ = fs::remove_dir_all(&base);
    }
}
