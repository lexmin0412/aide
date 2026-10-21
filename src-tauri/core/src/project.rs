//! Project-scoped agent configuration ("project profile").
//!
//! A project stores the agent configuration it needs in `<project>/.aide/project.json`
//! and aide applies it to the conventional in-project locations:
//!
//! - skill symlinks in `<project>/.agents/skills` (cross-agent convention) and,
//!   for every selected tool, `<project>/<adapter.project_skills>`;
//! - MCP server entries in `.mcp.json` (Claude Code), `.cursor/mcp.json`
//!   (Cursor) and `.vscode/mcp.json` (VS Code).
//!
//! Safety rules: aide only ever touches symlinks that point into the central
//! `~/.agents/skills` store and only removes the MCP names recorded in its own
//! `managed.json` manifest. Real files are never overwritten, and an unparsable
//! tool config is reported as an error instead of being rewritten.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::adapter;
use crate::fsutil;
use crate::mcp;
use crate::skills;
use crate::storage;

pub const PROJECT_DIR: &str = ".aide";
pub const CONFIG_FILE: &str = "project.json";
pub const MANAGED_FILE: &str = "managed.json";
/// Cross-agent project skills directory, relative to the project root.
pub const SHARED_SKILLS_DIR: &str = ".agents/skills";

/// Tools whose project config aide can maintain an MCP section for.
const MCP_TOOLS: &[&str] = &["claude_code", "cursor", "vscode"];

fn default_version() -> u32 {
    1
}

/// What the user (or the repository) declares for a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub mcp: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        ProjectConfig {
            version: default_version(),
            skills: Vec::new(),
            mcp: Vec::new(),
            tools: Vec::new(),
        }
    }
}

/// What aide actually applied to the project, so a later sync can remove only
/// its own artifacts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectManaged {
    #[serde(default = "default_version")]
    pub version: u32,
    /// Project-relative symlink paths, e.g. `.agents/skills/pdf`.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Tool key -> MCP server names aide wrote into that tool's project file.
    #[serde(default)]
    pub mcp: HashMap<String, Vec<String>>,
}

impl Default for ProjectManaged {
    fn default() -> Self {
        ProjectManaged {
            version: default_version(),
            skills: Vec::new(),
            mcp: HashMap::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProjectTool {
    pub key: String,
    pub name: String,
    pub project_skills: String,
    pub detected_globally: bool,
    pub supports_mcp: bool,
}

#[derive(Debug, Serialize)]
pub struct ProjectStatus {
    pub project_root: String,
    pub has_config: bool,
    pub config: ProjectConfig,
    pub tools: Vec<ProjectTool>,
    pub missing_skills: Vec<String>,
    pub missing_mcp: Vec<String>,
    pub linked_skills: u32,
    pub warnings: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ProjectSkillSync {
    pub linked: u32,
    pub removed: u32,
    pub missing: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ProjectMcpSync {
    pub tools_written: u32,
    pub removed: u32,
    pub unsupported: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ProjectSyncResult {
    pub skills: ProjectSkillSync,
    pub mcp: ProjectMcpSync,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

// ── Paths and validation ──

fn project_aide_dir(root: &Path) -> PathBuf {
    root.join(PROJECT_DIR)
}

fn config_path(root: &Path) -> PathBuf {
    project_aide_dir(root).join(CONFIG_FILE)
}

fn managed_path(root: &Path) -> PathBuf {
    project_aide_dir(root).join(MANAGED_FILE)
}

fn central_skills(home: &Path) -> PathBuf {
    home.join(".agents").join("skills")
}

fn central_mcp_path(home: &Path) -> PathBuf {
    home.join(".aide").join("mcp.json")
}

fn supports_mcp(tool_key: &str) -> bool {
    MCP_TOOLS.contains(&tool_key)
}

/// Project-relative path with forward slashes, for stable `managed.json`.
fn relative_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Reject projects whose `.aide` directory is a symlink escaping the project.
fn check_aide_dir(root: &Path) -> Result<(), String> {
    let aide = project_aide_dir(root);
    match fs::symlink_metadata(&aide) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let target = fs::canonicalize(&aide)
                .map_err(|e| format!("{} is a dangling symlink: {e}", aide.display()))?;
            if !target.starts_with(root) {
                return Err(format!(
                    "{} is a symlink that escapes the project; refusing to use it",
                    aide.display()
                ));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Validate a project root: absolute, existing directory, and neither a
/// filesystem root nor one of aide's own directories.
pub fn validate_project_root(project_path: &Path) -> Result<PathBuf, String> {
    validate_project_root_in(&crate::home_dir()?, project_path)
}

pub(crate) fn validate_project_root_in(
    home: &Path,
    project_path: &Path,
) -> Result<PathBuf, String> {
    if !project_path.is_absolute() {
        return Err(format!(
            "Project path must be absolute: {}",
            project_path.display()
        ));
    }
    if !project_path.is_dir() {
        return Err(format!(
            "Project path is not an existing directory: {}",
            project_path.display()
        ));
    }
    let root = fs::canonicalize(project_path)
        .map_err(|e| format!("Cannot resolve {}: {e}", project_path.display()))?;
    let canonical_home = fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());

    if root.parent().is_none() {
        return Err("The filesystem root cannot be used as a project".to_string());
    }
    if root == canonical_home {
        return Err("The home directory cannot be used as a project".to_string());
    }
    for forbidden in [canonical_home.join(".aide"), canonical_home.join(".agents")] {
        if root.starts_with(&forbidden) {
            return Err(format!(
                "{} is inside aide's own configuration directories",
                root.display()
            ));
        }
    }
    check_aide_dir(&root)?;
    Ok(root)
}

// ── Config and manifest IO ──

pub fn read_project_config(project_path: &Path) -> Result<Option<ProjectConfig>, String> {
    read_project_config_in(&crate::home_dir()?, project_path)
}

pub(crate) fn read_project_config_in(
    home: &Path,
    project_path: &Path,
) -> Result<Option<ProjectConfig>, String> {
    let root = validate_project_root_in(home, project_path)?;
    storage::read_json(&config_path(&root))
}

pub fn save_project_config(project_path: &Path, config: &ProjectConfig) -> Result<(), String> {
    save_project_config_in(&crate::home_dir()?, project_path, config)
}

pub(crate) fn save_project_config_in(
    home: &Path,
    project_path: &Path,
    config: &ProjectConfig,
) -> Result<(), String> {
    let root = validate_project_root_in(home, project_path)?;
    let aide_dir = project_aide_dir(&root);
    fs::create_dir_all(&aide_dir).map_err(|e| format!("create {}: {e}", aide_dir.display()))?;
    // Re-check after creating: `.aide` may now exist as a fresh directory.
    check_aide_dir(&root)?;
    let path = config_path(&root);
    let backups = aide_dir.join("backups");
    storage::write_json(&path, config, Some(&backups))
}

fn read_managed(root: &Path, warnings: &mut Vec<String>) -> ProjectManaged {
    match storage::read_json::<ProjectManaged>(&managed_path(root)) {
        Ok(Some(state)) => state,
        Ok(None) => ProjectManaged::default(),
        Err(err) => {
            warnings.push(format!("Ignoring unreadable {MANAGED_FILE}: {err}"));
            ProjectManaged::default()
        }
    }
}

fn write_managed(root: &Path, state: &ProjectManaged) -> Result<(), String> {
    let aide_dir = project_aide_dir(root);
    fs::create_dir_all(&aide_dir).map_err(|e| format!("create {}: {e}", aide_dir.display()))?;
    let backups = aide_dir.join("backups");
    storage::write_json(&managed_path(root), state, Some(&backups))
}

// ── Status ──

pub fn project_status(project_path: &Path) -> Result<ProjectStatus, String> {
    project_status_in(&crate::home_dir()?, project_path)
}

pub(crate) fn project_status_in(home: &Path, project_path: &Path) -> Result<ProjectStatus, String> {
    let root = validate_project_root_in(home, project_path)?;
    let config = read_project_config_in(home, &root)?.unwrap_or_default();
    let has_config = config_path(&root).exists();
    let skills_store = central_skills(home);
    let mut warnings = Vec::new();

    let missing_skills = dedupe(&config.skills)
        .into_iter()
        .filter(|name| !skills_store.join(name).is_dir())
        .collect();

    let central_mcp = match mcp::read_central_at(&central_mcp_path(home)) {
        Ok(central) => central,
        Err(err) => {
            warnings.push(format!("Cannot read the central MCP config: {err}"));
            mcp::McpCentralConfig::default()
        }
    };
    let missing_mcp = dedupe(&config.mcp)
        .into_iter()
        .filter(|name| !central_mcp.servers.contains_key(name))
        .collect();

    let tools = adapter::all_tools()
        .into_iter()
        .map(|tool| ProjectTool {
            key: tool.key.to_string(),
            name: tool.name.to_string(),
            project_skills: tool.project_skills.to_string(),
            detected_globally: home.join(tool.detect_dir).exists(),
            supports_mcp: supports_mcp(tool.key),
        })
        .collect();

    let managed = read_managed(&root, &mut warnings);
    let linked_skills = count_valid_links(&root, &skills_store, &managed) as u32;

    Ok(ProjectStatus {
        project_root: root.to_string_lossy().to_string(),
        has_config,
        config,
        tools,
        missing_skills,
        missing_mcp,
        linked_skills,
        warnings,
    })
}

fn count_valid_links(root: &Path, skills_store: &Path, managed: &ProjectManaged) -> usize {
    managed
        .skills
        .iter()
        .filter_map(|relative| safe_join(root, relative))
        .filter(|path| points_into_store(path, skills_store))
        .count()
}

// ── Path helpers ──

/// Join a project-relative path, refusing absolute paths and `..` escapes.
fn safe_join(root: &Path, relative: &str) -> Option<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute() {
        return None;
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return None;
    }
    Some(root.join(path))
}

/// Resolve a symlink target against the link's directory.
fn resolved_link_target(link: &Path) -> Option<PathBuf> {
    let target = fs::read_link(link).ok()?;
    if target.is_absolute() {
        Some(target)
    } else {
        Some(link.parent()?.join(target))
    }
}

/// Does this path point into the central skill store? Works for dangling links
/// too by checking the lexical target.
fn points_into_store(path: &Path, skills_store: &Path) -> bool {
    let Some(target) = resolved_link_target(path) else {
        return false;
    };
    if target.starts_with(skills_store) {
        return true;
    }
    match (fs::canonicalize(&target), fs::canonicalize(skills_store)) {
        (Ok(target), Ok(store)) => target.starts_with(store),
        _ => false,
    }
}

fn link_points_to(link: &Path, source: &Path) -> bool {
    let Some(target) = resolved_link_target(link) else {
        return false;
    };
    if target == source {
        return true;
    }
    match (fs::canonicalize(&target), fs::canonicalize(source)) {
        (Ok(target), Ok(source)) => target == source,
        _ => false,
    }
}

/// Create the directory if (and only if) it stays inside the project, even when
/// one of its ancestors is a symlink pointing elsewhere.
fn ensure_dir_inside_project(root: &Path, dir: &Path) -> Result<bool, String> {
    let mut ancestor = dir.to_path_buf();
    loop {
        match fs::symlink_metadata(&ancestor) {
            Ok(meta) => {
                if !meta.is_dir() {
                    return Ok(false);
                }
                let canonical = fs::canonicalize(&ancestor)
                    .map_err(|e| format!("Cannot resolve {}: {e}", ancestor.display()))?;
                if !canonical.starts_with(root) {
                    return Ok(false);
                }
                break;
            }
            Err(_) => {
                if !ancestor.pop() {
                    return Ok(false);
                }
            }
        }
    }
    fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    Ok(true)
}

fn dedupe(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.to_string()))
        .map(|value| value.to_string())
        .collect()
}

/// Create a symlink, falling back to a recursive copy when links are not
/// supported on this platform/filesystem.
fn link_or_copy(source: &Path, dest: &Path, warnings: &mut Vec<String>) -> Result<(), String> {
    match skills::create_skills_link(source, dest) {
        Ok(()) => Ok(()),
        Err(link_err) => match fsutil::copy_dir_recursive(source, dest) {
            Ok(()) => {
                warnings.push(format!(
                    "Links are unavailable, copied {} instead ({link_err})",
                    dest.display()
                ));
                Ok(())
            }
            Err(copy_err) => Err(format!(
                "Could not link or copy {}: {copy_err}",
                dest.display()
            )),
        },
    }
}

// ── Sync ──

pub fn sync_project(project_path: &Path) -> Result<ProjectSyncResult, String> {
    sync_project_in(&crate::home_dir()?, project_path)
}

pub(crate) fn sync_project_in(
    home: &Path,
    project_path: &Path,
) -> Result<ProjectSyncResult, String> {
    let root = validate_project_root_in(home, project_path)?;
    let config = read_project_config_in(home, &root)?.ok_or_else(|| {
        format!(
            "No project config found at {}; save the project profile first",
            config_path(&root).display()
        )
    })?;
    let central = mcp::read_central_at(&central_mcp_path(home))
        .map_err(|e| format!("Cannot read the central MCP config: {e}"))?;

    let mut result = ProjectSyncResult::default();
    let mut managed = read_managed(&root, &mut result.warnings);
    let skills_store = central_skills(home);

    result.skills = sync_skills(&root, &skills_store, &config, &mut managed, &mut result);
    result.mcp = sync_mcp(&root, &central, &config, &mut managed, &mut result);
    managed.version = default_version();
    if let Err(err) = write_managed(&root, &managed) {
        result.errors.push(err);
    }
    Ok(result)
}

fn managed_skill_dirs(
    root: &Path,
    config: &ProjectConfig,
    warnings: &mut Vec<String>,
) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = vec![root.join(SHARED_SKILLS_DIR)];
    let mut seen: HashSet<PathBuf> = dirs.iter().cloned().collect();
    let adapters = adapter::all_tools();
    for key in dedupe(&config.tools) {
        match adapters.iter().find(|tool| tool.key == key) {
            Some(tool) => {
                let dir = root.join(tool.project_skills);
                // Many adapters share `.agents/skills`; manage each dir once.
                if seen.insert(dir.clone()) {
                    dirs.push(dir);
                }
            }
            None => warnings.push(format!("Unknown tool key in project config: {key}")),
        }
    }
    dirs
}

fn sync_skills(
    root: &Path,
    skills_store: &Path,
    config: &ProjectConfig,
    managed: &mut ProjectManaged,
    result: &mut ProjectSyncResult,
) -> ProjectSkillSync {
    let mut outcome = ProjectSkillSync::default();
    let mut warnings: Vec<String> = std::mem::take(&mut result.warnings);
    let dirs = managed_skill_dirs(root, config, &mut warnings);
    let wanted = dedupe(&config.skills);
    let wanted_set: HashSet<&str> = wanted.iter().map(String::as_str).collect();

    let mut desired_links: HashSet<PathBuf> = HashSet::new();
    let mut removed_paths: HashSet<PathBuf> = HashSet::new();

    for dir in &dirs {
        match ensure_dir_inside_project(root, dir) {
            Ok(true) => {}
            Ok(false) => {
                warnings.push(format!(
                    "Skipping {}: it resolves outside the project",
                    relative_string(root, dir)
                ));
                continue;
            }
            Err(err) => {
                result.errors.push(err);
                continue;
            }
        }

        for name in &wanted {
            let source = skills_store.join(name);
            if !source.is_dir() {
                if !outcome.missing.contains(name) {
                    outcome.missing.push(name.clone());
                }
                continue;
            }
            let dest = dir.join(name);
            let relative = relative_string(root, &dest);
            match fs::symlink_metadata(&dest) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    if link_points_to(&dest, &source) {
                        outcome.linked += 1;
                        desired_links.insert(dest);
                        continue;
                    }
                    if !points_into_store(&dest, skills_store) {
                        warnings.push(format!(
                            "Skipping {relative}: it is a symlink not managed by aide"
                        ));
                        continue;
                    }
                    // Aide-managed link to the wrong skill: replace it.
                    if let Err(err) = fs::remove_file(&dest) {
                        result
                            .errors
                            .push(format!("Could not replace {relative}: {err}"));
                        continue;
                    }
                }
                Ok(_) => {
                    warnings.push(format!(
                        "Skipping {relative}: a real file or directory already exists there"
                    ));
                    continue;
                }
                Err(_) => {}
            }
            match link_or_copy(&source, &dest, &mut warnings) {
                Ok(()) => {
                    outcome.linked += 1;
                    desired_links.insert(dest);
                }
                Err(err) => result.errors.push(err),
            }
        }

        // Remove aide links for skills that are no longer selected.
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if wanted_set.contains(name.as_str()) {
                    continue;
                }
                if fs::symlink_metadata(&path)
                    .map(|meta| meta.file_type().is_symlink())
                    .unwrap_or(false)
                    && points_into_store(&path, skills_store)
                {
                    match fs::remove_file(&path) {
                        Ok(()) => {
                            removed_paths.insert(path);
                        }
                        Err(err) => result.errors.push(format!(
                            "Could not remove stale link {}: {err}",
                            relative_string(root, &path)
                        )),
                    }
                }
            }
        }
    }

    // Paths recorded in the previous manifest but no longer desired (including
    // dirs that are no longer managed because the tool was deselected).
    for relative in &managed.skills {
        let Some(path) = safe_join(root, relative) else {
            warnings.push(format!("Ignoring unsafe managed path: {relative}"));
            continue;
        };
        if desired_links.contains(&path) {
            continue;
        }
        if fs::symlink_metadata(&path)
            .map(|meta| meta.file_type().is_symlink())
            .unwrap_or(false)
            && points_into_store(&path, skills_store)
        {
            match fs::remove_file(&path) {
                Ok(()) => {
                    removed_paths.insert(path);
                }
                Err(err) => result
                    .errors
                    .push(format!("Could not remove stale link {relative}: {err}")),
            }
        }
    }

    outcome.removed = removed_paths.len() as u32;
    let mut managed_skills: Vec<String> = desired_links
        .iter()
        .map(|path| relative_string(root, path))
        .collect();
    managed_skills.sort();
    managed.skills = managed_skills;
    outcome.missing.sort();
    result.warnings = warnings;
    outcome
}

/// Project MCP target for a tool: (path relative to project, JSON key).
fn project_mcp_target(tool_key: &str) -> Option<(&'static str, &'static str)> {
    match tool_key {
        "claude_code" => Some((".mcp.json", "mcpServers")),
        "cursor" => Some((".cursor/mcp.json", "mcpServers")),
        "vscode" => Some((".vscode/mcp.json", "servers")),
        _ => None,
    }
}

fn sync_mcp(
    root: &Path,
    central: &mcp::McpCentralConfig,
    config: &ProjectConfig,
    managed: &mut ProjectManaged,
    result: &mut ProjectSyncResult,
) -> ProjectMcpSync {
    let mut outcome = ProjectMcpSync::default();
    let selected: Vec<String> = dedupe(&config.mcp)
        .into_iter()
        .filter(|name| {
            central
                .servers
                .get(name)
                .map(|server| !server.disabled.unwrap_or(false))
                .unwrap_or(false)
        })
        .collect();
    let unavailable: Vec<String> = dedupe(&config.mcp)
        .into_iter()
        .filter(|name| {
            central
                .servers
                .get(name)
                .map(|server| server.disabled.unwrap_or(false))
                .unwrap_or(true)
        })
        .collect();
    for name in unavailable {
        result.warnings.push(format!(
            "MCP server not available (missing or disabled): {name}"
        ));
    }
    let backups = project_aide_dir(root).join("backups");

    for tool_key in dedupe(&config.tools) {
        let Some((relative, json_key)) = project_mcp_target(&tool_key) else {
            outcome.unsupported.push(tool_key);
            continue;
        };
        let path = root.join(relative);
        let previous = managed.mcp.get(&tool_key).cloned().unwrap_or_default();
        let existing = if path.exists() {
            match fs::read_to_string(&path) {
                Ok(raw) if raw.trim().is_empty() => serde_json::Value::Object(Default::default()),
                Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                    Ok(value) => value,
                    Err(err) => {
                        // Never rewrite a config we cannot parse.
                        result.errors.push(format!(
                            "Cannot parse {}; {tool_key} left untouched ({err})",
                            path.display()
                        ));
                        continue;
                    }
                },
                Err(err) => {
                    result
                        .errors
                        .push(format!("Cannot read {}: {err}", path.display()));
                    continue;
                }
            }
        } else {
            serde_json::Value::Object(Default::default())
        };

        if selected.is_empty() && previous.is_empty() && !path.exists() {
            managed.mcp.remove(&tool_key);
            continue;
        }

        let mut root_value = existing;
        if !root_value.is_object() {
            result.errors.push(format!(
                "{} has a non-object JSON root; {tool_key} left untouched",
                path.display()
            ));
            continue;
        }
        let serviced = {
            let object = root_value.as_object_mut().expect("checked object");
            let section = object
                .entry(json_key.to_string())
                .or_insert_with(|| serde_json::Value::Object(Default::default()));
            if !section.is_object() {
                result.errors.push(format!(
                    "{} has a non-object `{json_key}` section; {tool_key} left untouched",
                    path.display()
                ));
                continue;
            }
            let section = section.as_object_mut().expect("checked object");
            for name in &selected {
                if let Some(server) = central.servers.get(name) {
                    section.insert(name.clone(), mcp::standard_server_json(server));
                }
            }
            let mut removed = 0u32;
            for name in &previous {
                if !selected.iter().any(|selected| selected == name)
                    && section.remove(name.as_str()).is_some()
                {
                    removed += 1;
                }
            }
            (removed, !section.is_empty())
        };
        let (removed, section_has_entries) = serviced;
        outcome.removed += removed;

        // Drop an empty aide-created section to keep the file tidy.
        if !section_has_entries {
            if let Some(object) = root_value.as_object_mut() {
                object.remove(json_key);
            }
        }

        let content = match serde_json::to_string_pretty(&root_value) {
            Ok(content) => content,
            Err(err) => {
                result
                    .errors
                    .push(format!("Cannot serialize {}: {err}", path.display()));
                continue;
            }
        };
        if let Some(parent) = path.parent() {
            match ensure_dir_inside_project(root, parent) {
                Ok(true) => {}
                Ok(false) => {
                    result.errors.push(format!(
                        "Skipping {}: it resolves outside the project",
                        path.display()
                    ));
                    continue;
                }
                Err(err) => {
                    result.errors.push(err);
                    continue;
                }
            }
        }
        if let Err(err) = storage::write_bytes(&path, content.as_bytes(), Some(&backups)) {
            result
                .errors
                .push(format!("Cannot write {}: {err}", path.display()));
            continue;
        }
        outcome.tools_written += 1;
        if selected.is_empty() {
            managed.mcp.remove(&tool_key);
        } else {
            let mut names = selected.clone();
            names.sort();
            managed.mcp.insert(tool_key, names);
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{McpCentralConfig, McpServerConfig};

    fn temp_base(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aide-project-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    struct Env {
        base: PathBuf,
        home: PathBuf,
        project: PathBuf,
    }

    impl Env {
        fn new(tag: &str) -> Self {
            let base = temp_base(tag);
            let home = base.join("home");
            let project = base.join("project");
            fs::create_dir_all(&home).unwrap();
            fs::create_dir_all(&project).unwrap();
            Env {
                base,
                home,
                project,
            }
        }

        fn make_skill(&self, name: &str) {
            let dir = self.home.join(".agents/skills").join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("SKILL.md"), format!("---\nname: {name}\n---\n")).unwrap();
        }

        fn central_server(&self, servers: Vec<(&str, Option<bool>)>) {
            let mut map = HashMap::new();
            for (name, disabled) in servers {
                map.insert(
                    name.to_string(),
                    McpServerConfig {
                        command: Some("npx".into()),
                        args: None,
                        url: None,
                        env: None,
                        headers: None,
                        disabled,
                        description: None,
                        targets: vec![],
                    },
                );
            }
            fs::create_dir_all(self.home.join(".aide")).unwrap();
            storage::write_json(
                &self.home.join(".aide/mcp.json"),
                &McpCentralConfig { servers: map },
                None,
            )
            .unwrap();
        }

        fn save(&self, config: &ProjectConfig) {
            save_project_config_in(&self.home, &self.project, config).unwrap();
        }

        fn managed(&self) -> ProjectManaged {
            storage::read_json::<ProjectManaged>(&managed_path(&self.project))
                .unwrap()
                .unwrap_or_default()
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    fn is_link(path: &Path) -> bool {
        fs::symlink_metadata(path)
            .map(|meta| meta.file_type().is_symlink())
            .unwrap_or(false)
    }

    fn shared_adapter_keys() -> Vec<String> {
        adapter::all_tools()
            .into_iter()
            .filter(|tool| tool.project_skills == SHARED_SKILLS_DIR)
            .map(|tool| tool.key.to_string())
            .collect()
    }

    fn keys_of(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    }

    #[test]
    fn serialized_contracts_have_the_agreed_shape() {
        assert_eq!(
            keys_of(&serde_json::to_value(ProjectConfig::default()).unwrap()),
            vec!["mcp", "skills", "tools", "version"]
        );
        assert_eq!(
            keys_of(&serde_json::to_value(ProjectManaged::default()).unwrap()),
            vec!["mcp", "skills", "version"]
        );

        let env = Env::new("contract");
        env.save(&ProjectConfig::default());
        let status =
            serde_json::to_value(project_status_in(&env.home, &env.project).unwrap()).unwrap();
        assert_eq!(
            keys_of(&status),
            vec![
                "config",
                "has_config",
                "linked_skills",
                "missing_mcp",
                "missing_skills",
                "project_root",
                "tools",
                "warnings"
            ]
        );
        assert_eq!(
            keys_of(&status["tools"][0]),
            vec![
                "detected_globally",
                "key",
                "name",
                "project_skills",
                "supports_mcp"
            ]
        );

        let sync = serde_json::to_value(sync_project_in(&env.home, &env.project).unwrap()).unwrap();
        assert_eq!(keys_of(&sync), vec!["errors", "mcp", "skills", "warnings"]);
        assert_eq!(
            keys_of(&sync["skills"]),
            vec!["linked", "missing", "removed"]
        );
        assert_eq!(
            keys_of(&sync["mcp"]),
            vec!["removed", "tools_written", "unsupported"]
        );
    }

    #[test]
    fn config_roundtrip_missing_and_corrupt() {
        let env = Env::new("config");
        assert!(read_project_config_in(&env.home, &env.project)
            .unwrap()
            .is_none());

        let config = ProjectConfig {
            version: 1,
            skills: vec!["alpha".into()],
            mcp: vec!["srv".into()],
            tools: vec!["claude_code".into()],
        };
        env.save(&config);
        assert!(env.project.join(".aide/project.json").is_file());
        assert_eq!(
            read_project_config_in(&env.home, &env.project).unwrap(),
            Some(config)
        );
        // Missing fields fall back to their defaults.
        fs::write(
            env.project.join(".aide/project.json"),
            "{\"skills\":[\"x\"]}",
        )
        .unwrap();
        let parsed = read_project_config_in(&env.home, &env.project)
            .unwrap()
            .unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.skills, vec!["x".to_string()]);
        assert!(parsed.mcp.is_empty() && parsed.tools.is_empty());

        // A corrupt config is an error, not an empty default.
        fs::write(env.project.join(".aide/project.json"), "{\"skills\": ").unwrap();
        assert!(read_project_config_in(&env.home, &env.project).is_err());
    }

    #[test]
    fn path_validation_rejects_root_home_and_aide_dirs() {
        let env = Env::new("validate");
        // Relative and non-existent paths are refused.
        assert!(validate_project_root_in(&env.home, Path::new("relative/project")).is_err());
        assert!(validate_project_root_in(&env.home, &env.home.join("missing")).is_err());
        // The project must be a directory.
        let file = env.base.join("file.txt");
        fs::write(&file, "x").unwrap();
        assert!(validate_project_root_in(&env.home, &file).is_err());
        // Home, aide's own directories and the filesystem root are refused.
        assert!(validate_project_root_in(&env.home, &env.home).is_err());
        fs::create_dir_all(env.home.join(".aide/workspace")).unwrap();
        assert!(validate_project_root_in(&env.home, &env.home.join(".aide/workspace")).is_err());
        fs::create_dir_all(env.home.join(".agents/skills")).unwrap();
        assert!(validate_project_root_in(&env.home, &env.home.join(".agents/skills")).is_err());
        #[cfg(unix)]
        assert!(validate_project_root_in(&env.home, Path::new("/")).is_err());

        let root = validate_project_root_in(&env.home, &env.project).unwrap();
        assert_eq!(root, fs::canonicalize(&env.project).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn path_validation_rejects_escaping_aide_symlink() {
        let env = Env::new("validate-symlink");
        let outside = env.base.join("outside");
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, env.project.join(".aide")).unwrap();
        assert!(validate_project_root_in(&env.home, &env.project).is_err());
        assert!(
            save_project_config_in(&env.home, &env.project, &ProjectConfig::default()).is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn sync_links_shared_dir_once_across_many_adapters() {
        let env = Env::new("dedupe");
        env.make_skill("alpha");
        let shared = shared_adapter_keys();
        assert!(
            shared.len() >= 15,
            "expected shared .agents/skills adapters"
        );
        env.save(&ProjectConfig {
            skills: vec!["alpha".into()],
            tools: shared,
            ..Default::default()
        });

        let result = sync_project_in(&env.home, &env.project).unwrap();
        let link = env.project.join(".agents/skills/alpha");
        assert!(is_link(&link));
        assert_eq!(
            fs::read_link(&link).unwrap(),
            env.home.join(".agents/skills/alpha")
        );
        // 15 adapters share one directory: one link, one manifest entry.
        assert_eq!(result.skills.linked, 1);
        assert_eq!(result.skills.removed, 0);
        assert!(result.errors.is_empty());
        assert_eq!(
            env.managed().skills,
            vec![".agents/skills/alpha".to_string()]
        );
        assert_eq!(env.managed().version, 1);
    }

    #[cfg(unix)]
    #[test]
    fn sync_writes_links_into_each_selected_tool_dir() {
        let env = Env::new("tool-dirs");
        env.make_skill("alpha");
        env.save(&ProjectConfig {
            skills: vec!["alpha".into()],
            tools: vec!["claude_code".into()],
            ..Default::default()
        });

        let result = sync_project_in(&env.home, &env.project).unwrap();
        assert!(is_link(&env.project.join(".agents/skills/alpha")));
        assert!(is_link(&env.project.join(".claude/skills/alpha")));
        assert_eq!(result.skills.linked, 2);
        assert_eq!(
            env.managed().skills,
            vec![
                ".agents/skills/alpha".to_string(),
                ".claude/skills/alpha".to_string()
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn deselecting_a_skill_removes_only_its_link() {
        let env = Env::new("removal");
        env.make_skill("alpha");
        env.make_skill("beta");
        env.save(&ProjectConfig {
            skills: vec!["alpha".into(), "beta".into()],
            ..Default::default()
        });
        sync_project_in(&env.home, &env.project).unwrap();
        assert!(is_link(&env.project.join(".agents/skills/beta")));

        env.save(&ProjectConfig {
            skills: vec!["alpha".into()],
            ..Default::default()
        });
        let result = sync_project_in(&env.home, &env.project).unwrap();
        assert!(!env.project.join(".agents/skills/beta").exists());
        assert!(is_link(&env.project.join(".agents/skills/alpha")));
        assert_eq!(result.skills.removed, 1);
        assert_eq!(
            env.managed().skills,
            vec![".agents/skills/alpha".to_string()]
        );
    }

    #[cfg(unix)]
    #[test]
    fn foreign_files_and_symlinks_are_never_removed() {
        let env = Env::new("foreign");
        env.make_skill("alpha");
        let skills_dir = env.project.join(".agents/skills");
        fs::create_dir_all(&skills_dir).unwrap();
        // A real directory and a symlink pointing outside the central store.
        fs::create_dir_all(skills_dir.join("real-dir")).unwrap();
        let outside = env.base.join("elsewhere");
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, skills_dir.join("foreign-link")).unwrap();
        write_managed(
            &env.project,
            &ProjectManaged {
                version: 1,
                skills: vec![".agents/skills/foreign-link".into(), "../escape".into()],
                mcp: HashMap::new(),
            },
        )
        .unwrap();

        env.save(&ProjectConfig {
            skills: vec!["alpha".into()],
            ..Default::default()
        });
        let result = sync_project_in(&env.home, &env.project).unwrap();

        assert!(skills_dir.join("real-dir").is_dir());
        assert!(is_link(&skills_dir.join("foreign-link")));
        assert!(is_link(&skills_dir.join("alpha")));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("unsafe managed path")));
        assert_eq!(
            env.managed().skills,
            vec![".agents/skills/alpha".to_string()]
        );
    }

    #[cfg(unix)]
    #[test]
    fn real_directory_in_place_of_a_skill_is_skipped_with_a_warning() {
        let env = Env::new("real-dir");
        env.make_skill("alpha");
        let skills_dir = env.project.join(".agents/skills");
        fs::create_dir_all(skills_dir.join("alpha")).unwrap();
        fs::write(skills_dir.join("alpha/local.txt"), "keep").unwrap();
        env.save(&ProjectConfig {
            skills: vec!["alpha".into()],
            ..Default::default()
        });

        let result = sync_project_in(&env.home, &env.project).unwrap();
        assert_eq!(result.skills.linked, 0);
        assert!(skills_dir.join("alpha/local.txt").is_file());
        assert!(result.warnings.iter().any(|w| w.contains("real file")));
    }

    #[cfg(unix)]
    #[test]
    fn missing_skills_are_reported_without_aborting() {
        let env = Env::new("missing");
        env.make_skill("alpha");
        env.save(&ProjectConfig {
            skills: vec!["alpha".into(), "ghost".into()],
            ..Default::default()
        });
        let result = sync_project_in(&env.home, &env.project).unwrap();
        assert_eq!(result.skills.missing, vec!["ghost".to_string()]);
        assert!(is_link(&env.project.join(".agents/skills/alpha")));
        assert!(result.errors.is_empty());
    }

    #[test]
    fn sync_requires_a_saved_config() {
        let env = Env::new("no-config");
        let err = sync_project_in(&env.home, &env.project).unwrap_err();
        assert!(err.contains("No project config"), "{err}");
    }

    #[test]
    fn mcp_merge_preserves_foreign_entries_and_removes_only_managed() {
        let env = Env::new("mcp");
        env.central_server(vec![("aide-srv", None), ("off-srv", Some(true))]);
        let claude_path = env.project.join(".mcp.json");
        fs::write(
            &claude_path,
            r#"{"mcpServers":{"foreign":{"command":"custom"}},"otherKey":1}"#,
        )
        .unwrap();
        env.save(&ProjectConfig {
            mcp: vec!["aide-srv".into(), "off-srv".into()],
            tools: vec!["claude_code".into(), "cursor".into()],
            ..Default::default()
        });

        let first = sync_project_in(&env.home, &env.project).unwrap();
        assert_eq!(first.mcp.tools_written, 2);
        assert!(first
            .warnings
            .iter()
            .any(|warning| warning.contains("off-srv")));
        let claude: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&claude_path).unwrap()).unwrap();
        assert_eq!(claude["mcpServers"]["foreign"]["command"], "custom");
        assert_eq!(claude["mcpServers"]["aide-srv"]["command"], "npx");
        assert!(claude["mcpServers"].get("off-srv").is_none());
        assert_eq!(claude["otherKey"], 1);
        let cursor_path = env.project.join(".cursor/mcp.json");
        let cursor: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&cursor_path).unwrap()).unwrap();
        assert_eq!(cursor["mcpServers"]["aide-srv"]["command"], "npx");
        assert_eq!(
            env.managed().mcp.get("claude_code"),
            Some(&vec!["aide-srv".to_string()])
        );
        assert_eq!(
            env.managed().mcp.get("cursor"),
            Some(&vec!["aide-srv".to_string()])
        );

        // Deselecting the server removes only aide's own entries.
        env.save(&ProjectConfig {
            mcp: vec![],
            tools: vec!["claude_code".into(), "cursor".into()],
            ..Default::default()
        });
        let second = sync_project_in(&env.home, &env.project).unwrap();
        assert_eq!(second.mcp.removed, 2);
        let claude: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&claude_path).unwrap()).unwrap();
        assert_eq!(claude["mcpServers"]["foreign"]["command"], "custom");
        assert!(claude["mcpServers"].get("aide-srv").is_none());
        let cursor: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&cursor_path).unwrap()).unwrap();
        assert!(cursor.get("mcpServers").is_none());
        assert!(env.managed().mcp.is_empty());
    }

    #[test]
    fn unparsable_project_mcp_config_is_left_untouched() {
        let env = Env::new("mcp-corrupt");
        env.central_server(vec![("aide-srv", None)]);
        let path = env.project.join(".mcp.json");
        fs::write(&path, "{not json").unwrap();
        env.save(&ProjectConfig {
            mcp: vec!["aide-srv".into()],
            tools: vec!["claude_code".into()],
            ..Default::default()
        });

        let result = sync_project_in(&env.home, &env.project).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{not json");
        assert!(result
            .errors
            .iter()
            .any(|error| error.contains("claude_code")));
        assert_eq!(result.mcp.tools_written, 0);
    }

    #[test]
    fn tools_without_project_mcp_support_are_reported() {
        let env = Env::new("unsupported");
        env.central_server(vec![("aide-srv", None)]);
        env.save(&ProjectConfig {
            mcp: vec!["aide-srv".into()],
            tools: vec!["warp".into()],
            ..Default::default()
        });
        let result = sync_project_in(&env.home, &env.project).unwrap();
        assert_eq!(result.mcp.unsupported, vec!["warp".to_string()]);
        assert_eq!(result.mcp.tools_written, 0);
    }

    #[cfg(unix)]
    #[test]
    fn status_reports_tools_missing_entries_and_link_count() {
        let env = Env::new("status");
        env.make_skill("alpha");
        fs::create_dir_all(env.home.join(".claude")).unwrap();
        env.central_server(vec![("srv", None)]);
        env.save(&ProjectConfig {
            skills: vec!["alpha".into(), "ghost".into()],
            mcp: vec!["srv".into(), "missing-srv".into()],
            tools: vec!["claude_code".into()],
            ..Default::default()
        });

        let status = project_status_in(&env.home, &env.project).unwrap();
        assert!(status.has_config);
        assert_eq!(
            status.project_root,
            fs::canonicalize(&env.project).unwrap().to_string_lossy()
        );
        assert_eq!(status.tools.len(), adapter::all_tools().len());
        let claude = status
            .tools
            .iter()
            .find(|tool| tool.key == "claude_code")
            .expect("claude tool");
        assert_eq!(claude.project_skills, ".claude/skills");
        assert!(claude.detected_globally);
        assert!(claude.supports_mcp);
        let warp = status
            .tools
            .iter()
            .find(|tool| tool.key == "warp")
            .expect("warp tool");
        assert!(!warp.supports_mcp);
        assert_eq!(status.missing_skills, vec!["ghost".to_string()]);
        assert_eq!(status.missing_mcp, vec!["missing-srv".to_string()]);
        assert_eq!(status.linked_skills, 0);

        sync_project_in(&env.home, &env.project).unwrap();
        let after = project_status_in(&env.home, &env.project).unwrap();
        assert_eq!(after.linked_skills, 2);
    }
}
