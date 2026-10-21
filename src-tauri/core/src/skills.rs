//! Skill domain: the central skill store, SKILL.md parsing, full-text search,
//! tool adaptation/sync, the `list_skills` cache and the agent allowlists used
//! by the sidecar. Everything here is headless and reusable outside Tauri.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;
use std::{fs, io};

use serde::Serialize;

use crate::fsutil::IGNORED_DIRS;

const SKILL_TEXT_EXTENSIONS: &[&str] = &[
    "md", "markdown", "json", "jsonc", "yaml", "yml", "toml", "txt", "js", "ts", "jsx", "tsx",
    "css", "html", "sh", "bash", "env",
];

const SEARCH_MAX_FILE_SIZE: u64 = 512 * 1024;
const SEARCH_MAX_PER_SKILL: usize = 5;
const SEARCH_MAX_MATCHES: usize = 50;

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

pub(crate) fn import_skill_into(source: &Path, skills_dir: &Path) -> Result<String, String> {
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
    crate::fsutil::copy_dir_recursive(source, &dest).map_err(|e| format!("Import failed: {e}"))?;
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
        .filter(|e| {
            e.file_type()
                .map(|t| t.is_dir() || t.is_symlink())
                .unwrap_or(false)
        })
        .map(|e| e.path())
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
        // Guards against symlink cycles; symlinks are skipped below anyway.
        let mut visited: HashSet<PathBuf> = HashSet::new();
        while let Some(current) = stack.pop() {
            let key = fs::canonicalize(&current).unwrap_or_else(|_| current.clone());
            if !visited.insert(key) {
                continue;
            }
            let Ok(entries) = fs::read_dir(&current) else {
                continue;
            };
            let mut paths: Vec<_> = entries.flatten().collect();
            paths.sort_by_key(|e| e.file_name());
            for entry in paths {
                let p = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_symlink() {
                    continue;
                }
                if file_type.is_dir() {
                    if !IGNORED_DIRS.contains(&name.as_str()) {
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
                if entry
                    .metadata()
                    .map(|m| m.len() > SEARCH_MAX_FILE_SIZE)
                    .unwrap_or(true)
                {
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
        serde_norway::Value::Sequence(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        serde_norway::Value::String(s) => s
            .split([' ', ',', '，'])
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

/// Split a markdown document into YAML frontmatter and body. Only a leading
/// `---` line followed by a closing `---` line counts, so a horizontal rule in
/// the body is never mistaken for a delimiter. The body keeps its exact
/// trailing bytes.
pub(crate) fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut offset = 0usize;
    let mut first = true;
    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if first {
            if trimmed.trim() != "---" {
                return None;
            }
            first = false;
            offset += line.len();
            continue;
        }
        if trimmed.trim() == "---" {
            let yaml = &content[content.find('\n').map(|i| i + 1).unwrap_or(0)..offset];
            return Some((yaml, &content[offset + line.len()..]));
        }
        offset += line.len();
    }
    None
}

pub(crate) fn parse_skill_frontmatter(
    content: &str,
) -> (Option<String>, Option<String>, Vec<String>) {
    let Some((yaml_str, _)) = split_frontmatter(content) else {
        return (None, None, Vec::new());
    };
    let Ok(serde_norway::Value::Mapping(mapping)) =
        serde_norway::from_str::<serde_norway::Value>(yaml_str)
    else {
        return (None, None, Vec::new());
    };
    let name = mapping
        .get(serde_norway::Value::String("name".into()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let desc = mapping
        .get(serde_norway::Value::String("description".into()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let tags = mapping
        .get(serde_norway::Value::String("tags".into()))
        .map(extract_tags)
        .filter(|t| !t.is_empty())
        .or_else(|| {
            mapping
                .get(serde_norway::Value::String("metadata".into()))
                .and_then(|v| v.as_mapping())
                .and_then(|m| m.get(serde_norway::Value::String("tags".into())))
                .map(extract_tags)
        })
        .unwrap_or_default();
    (name, desc, tags)
}

/// Frontmatter `name` of the SKILL.md in `dir`, when present and non-empty.
pub(crate) fn skill_frontmatter_name(dir: &Path) -> Option<String> {
    let content = fs::read_to_string(dir.join("SKILL.md")).ok()?;
    parse_skill_frontmatter(&content)
        .0
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
}

// ── list_skills cache ──

/// A file count is cosmetic; a short TTL keeps it warm without rescanning every
/// skill tree on each list call.
const FILE_COUNT_TTL_SECS: u64 = 60;

#[derive(Clone)]
struct CachedMetadata {
    display_name: String,
    description: String,
    tags: Vec<String>,
    source: Option<String>,
    scope: Option<String>,
    /// Validity keys: SKILL.md mtime, skills-directory mtime, and a fingerprint
    /// of the provenance/scope config read on each call.
    skill_md_mtime: Option<u128>,
    skills_dir_mtime: Option<u128>,
    config_fingerprint: u64,
}

#[derive(Clone)]
struct CachedFileCount {
    value: usize,
    skill_dir_mtime: Option<u128>,
    skill_md_mtime: Option<u128>,
    checked_at: u64,
}

#[derive(Clone, Default)]
struct SkillCacheEntry {
    metadata: Option<CachedMetadata>,
    file_count: Option<CachedFileCount>,
}

fn skill_cache() -> &'static Mutex<HashMap<PathBuf, SkillCacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, SkillCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn mtime_nanos(path: &Path) -> Option<u128> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_nanos())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Fingerprint of the provenance and scope inputs cached alongside the parsed
/// SKILL.md, so recording a source or reassigning a scope never shows stale.
fn config_fingerprint(
    sources: &HashMap<String, crate::registry::SkillSource>,
    config: &crate::git_sync::GitRemotesConfig,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut source_entries: Vec<(&String, &crate::registry::SkillSource)> =
        sources.iter().collect();
    source_entries.sort_by_key(|(name, _)| *name);
    for (name, source) in source_entries {
        name.hash(&mut hasher);
        source.source.hash(&mut hasher);
    }
    let mut scopes: Vec<(&String, &String)> = config.skill_scopes.iter().collect();
    scopes.sort_by_key(|(name, _)| *name);
    for (name, scope) in scopes {
        name.hash(&mut hasher);
        scope.hash(&mut hasher);
    }
    let mut remotes: Vec<&str> = config
        .remotes
        .iter()
        .map(|remote| remote.name.as_str())
        .collect();
    remotes.sort_unstable();
    for remote in remotes {
        remote.hash(&mut hasher);
    }
    hasher.finish()
}

fn skill_metadata(skill_md: &Path, name: &str) -> (String, String, Vec<String>) {
    match fs::read_to_string(skill_md) {
        Ok(content) => {
            let (parsed_name, description, tags) = parse_skill_frontmatter(&content);
            (
                parsed_name.unwrap_or_else(|| name.to_string()),
                description.unwrap_or_default(),
                tags,
            )
        }
        Err(_) => (name.to_string(), String::new(), Vec::new()),
    }
}

/// Core of `list_skills`, shared with the aide-mcp agent binary.
pub fn skill_infos() -> Result<Vec<SkillInfo>, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    skill_infos_at(&home, &home.join(".agents").join("skills"), now_secs())
}

/// `skill_infos` with an injectable home, skills directory and clock so the
/// cache behavior can be tested deterministically.
pub(crate) fn skill_infos_at(
    home: &Path,
    skills_dir: &Path,
    now: u64,
) -> Result<Vec<SkillInfo>, String> {
    if !skills_dir.exists() {
        return Ok(Vec::new());
    }

    let aide_dir = home.join(".aide");
    let sources = crate::registry::read_sources(&aide_dir)?;
    let git_config = crate::git_sync::read_config(&aide_dir)?;
    let fingerprint = config_fingerprint(&sources, &git_config);
    let skills_dir_mtime = mtime_nanos(skills_dir);

    let mut cache = skill_cache()
        .lock()
        .map_err(|_| "skill cache is poisoned".to_string())?;
    let mut skills = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for entry in
        fs::read_dir(skills_dir).map_err(|e| format!("read {}: {e}", skills_dir.display()))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        let is_symlink = meta.is_symlink();
        let target_path = if is_symlink {
            fs::read_link(&path)
                .ok()
                .map(|target| target.to_string_lossy().to_string())
        } else {
            None
        };

        let skill_md = path.join("SKILL.md");
        let skill_md_mtime = mtime_nanos(&skill_md);
        let skill_dir_mtime = mtime_nanos(&path);
        seen.insert(path.clone());

        let cached = cache.entry(path.clone()).or_default();
        let metadata_fresh = cached.metadata.as_ref().is_some_and(|metadata| {
            metadata.skill_md_mtime == skill_md_mtime
                && metadata.skills_dir_mtime == skills_dir_mtime
                && metadata.config_fingerprint == fingerprint
        });
        if !metadata_fresh {
            let (display_name, description, tags) = skill_metadata(&skill_md, &name);
            cached.metadata = Some(CachedMetadata {
                display_name,
                description,
                tags,
                source: sources.get(&name).map(|source| source.source.clone()),
                scope: explicit_scope(&git_config, &name),
                skill_md_mtime,
                skills_dir_mtime,
                config_fingerprint: fingerprint,
            });
        }

        let count_fresh = cached.file_count.as_ref().is_some_and(|count| {
            count.checked_at + FILE_COUNT_TTL_SECS > now
                && count.skill_dir_mtime == skill_dir_mtime
                && count.skill_md_mtime == skill_md_mtime
        });
        if !count_fresh {
            let value = crate::fsutil::count_files(&path);
            cached.file_count = Some(CachedFileCount {
                value,
                skill_dir_mtime,
                skill_md_mtime,
                checked_at: now,
            });
        }

        let metadata = cached
            .metadata
            .clone()
            .ok_or_else(|| "skill metadata cache was not populated".to_string())?;
        let file_count = cached
            .file_count
            .as_ref()
            .map(|count| count.value)
            .unwrap_or(0);
        skills.push(SkillInfo {
            name,
            display_name: metadata.display_name,
            description: metadata.description,
            tags: metadata.tags,
            path: path.to_string_lossy().to_string(),
            is_symlink,
            target_path,
            file_count,
            source: metadata.source,
            scope: metadata.scope,
        });
    }

    // Evict skills that disappeared from this store; entries belonging to a
    // different store (tests, other homes) are left untouched.
    cache.retain(|path, _| !path.starts_with(skills_dir) || seen.contains(path));

    skills.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(skills)
}

#[cfg(test)]
fn skill_cache_contains(path: &Path) -> bool {
    skill_cache()
        .lock()
        .map(|cache| cache.contains_key(path))
        .unwrap_or(false)
}

/// Explicit git scope assignment for a skill (None = default scope applies).
pub(crate) fn explicit_scope(
    config: &crate::git_sync::GitRemotesConfig,
    skill: &str,
) -> Option<String> {
    config
        .skill_scopes
        .get(skill)
        .filter(|s| config.remotes.iter().any(|r| &r.name == *s))
        .cloned()
}

/// Rewrite the `tags` array in a SKILL.md frontmatter, keeping the rest of the
/// document untouched. The new frontmatter is re-parsed before it is written so
/// a serialization problem can never corrupt the skill definition.
pub(crate) fn update_skill_tags_in(
    home: &Path,
    skill_dir: &Path,
    tags: &[String],
) -> Result<(), String> {
    let skill_md = skill_dir.join("SKILL.md");
    let content =
        fs::read_to_string(&skill_md).map_err(|e| format!("read {}: {e}", skill_md.display()))?;

    let tags_key = serde_norway::Value::String("tags".into());
    let metadata_key = serde_norway::Value::String("metadata".into());

    let (mut mapping, body) = match split_frontmatter(&content) {
        Some((yaml, body)) => {
            let mapping = if yaml.trim().is_empty() {
                serde_norway::Mapping::new()
            } else {
                match serde_norway::from_str::<serde_norway::Value>(yaml).map_err(|e| {
                    format!("Invalid YAML frontmatter in {}: {e}", skill_md.display())
                })? {
                    serde_norway::Value::Mapping(mapping) => mapping,
                    _ => {
                        return Err(format!(
                            "Frontmatter in {} is not a YAML mapping; refusing to rewrite it",
                            skill_md.display()
                        ))
                    }
                }
            };
            (mapping, body.to_string())
        }
        None => {
            if tags.is_empty() {
                // Nothing to remove and no frontmatter to create.
                return Ok(());
            }
            (serde_norway::Mapping::new(), content.clone())
        }
    };

    if tags.is_empty() {
        mapping.remove(&tags_key);
    } else {
        mapping.insert(
            tags_key.clone(),
            serde_norway::Value::Sequence(
                tags.iter()
                    .map(|t| serde_norway::Value::String(t.clone()))
                    .collect(),
            ),
        );
    }
    // Tags live at the top level only; drop the legacy nested copy so the two
    // cannot drift apart.
    if let Some(serde_norway::Value::Mapping(metadata)) = mapping.get_mut(&metadata_key) {
        metadata.remove(&tags_key);
    }

    let yaml = serde_norway::to_string(&serde_norway::Value::Mapping(mapping))
        .map_err(|e| format!("serialize frontmatter: {e}"))?;
    serde_norway::from_str::<serde_norway::Value>(&yaml)
        .map_err(|e| format!("refusing to write invalid frontmatter: {e}"))?;
    let new_content = format!("---\n{yaml}---\n{body}");

    let backups = home.join(".aide").join("backups");
    crate::storage::write_bytes(&skill_md, new_content.as_bytes(), Some(&backups))
}

#[derive(Serialize)]
pub struct ToolInfo {
    pub key: String,
    pub name: String,
    pub global_skills: String,
    pub detect_dir: String,
    pub project_skills: String,
    pub status: String,
}

fn tool_dir_exists_in(home: &Path, relative: &str) -> bool {
    home.join(relative).exists()
}

pub fn resolve_skills_source() -> Result<std::path::PathBuf, String> {
    dirs::home_dir()
        .map(|h| h.join(".agents").join("skills"))
        .ok_or_else(|| "Cannot find home directory".to_string())
}

pub(crate) fn compute_tool_status_in(home: &Path, t: &crate::adapter::ToolInfo) -> String {
    if !tool_dir_exists_in(home, t.detect_dir) {
        return "not_installed".into();
    }
    if t.global_skills == ".agents/skills" {
        return "compatible".into();
    }
    let target = home.join(t.global_skills);
    if target.is_symlink() {
        if let Ok(link) = std::fs::read_link(&target) {
            let resolved = if link.is_absolute() {
                link
            } else {
                target
                    .parent()
                    .map(|parent| parent.join(&link))
                    .unwrap_or(link)
            };
            if same_path(&resolved, &home.join(".agents").join("skills")) {
                return "synced".into();
            }
        }
    }
    if target.exists() {
        return "has_content".into();
    }
    "ready".into()
}

/// Every supported tool with its current sync status, for the real home.
pub fn tool_infos() -> Vec<ToolInfo> {
    match dirs::home_dir() {
        Some(home) => tool_infos_at(&home),
        None => Vec::new(),
    }
}

/// Tool statuses for an explicit home directory (used by doctor and tests).
pub(crate) fn tool_infos_at(home: &Path) -> Vec<ToolInfo> {
    crate::adapter::all_tools()
        .into_iter()
        .map(|t| {
            let status = compute_tool_status_in(home, &t);
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

/// Do two paths refer to the same location? Falls back to canonicalization so
/// relative and absolute spellings compare equal.
pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// Create the tool-side skills link. Types are per-platform so Linux dev
/// builds keep working too.
pub(crate) fn create_skills_link(source: &Path, target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, target).map_err(|e| e.to_string())
    }
    #[cfg(windows)]
    {
        // `junction` is not available on stable Rust; directory symlinks are the
        // supported equivalent (they may require Developer Mode on some hosts).
        std::os::windows::fs::symlink_dir(source, target).map_err(|e| e.to_string())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (source, target);
        Err("Symlink not supported on this platform".to_string())
    }
}

pub(crate) fn sync_tool_in(
    home: &Path,
    source: &Path,
    tool_key: &str,
    overwrite: bool,
) -> Result<SyncResult, String> {
    let tool = crate::adapter::all_tools()
        .into_iter()
        .find(|t| t.key == tool_key)
        .ok_or_else(|| format!("Unknown tool: {}", tool_key))?;

    let result = |merged: Vec<String>,
                  backed_up: Vec<String>,
                  conflicts: Vec<String>,
                  error: Option<String>|
     -> Result<SyncResult, String> {
        Ok(SyncResult {
            key: tool_key.to_string(),
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
    if !source.is_dir() {
        return result(
            vec![],
            vec![],
            vec![],
            Some("Source ~/.agents/skills does not exist".into()),
        );
    }

    let target = home.join(tool.global_skills);
    let aide_dir = home.join(".aide");
    let backup_root = aide_dir.join("sync-backup").join(tool_key);
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let stamp = format!("{stamp}-{:05}", std::process::id());

    // A link that already points at the central store needs no work. A stale
    // link is removed on its own: unlinking never touches the linked content.
    if let Ok(meta) = fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() {
            // A relative link is resolved against the link's own directory.
            let points_here = fs::read_link(&target)
                .map(|link| {
                    let resolved = if link.is_absolute() {
                        link
                    } else {
                        target
                            .parent()
                            .map(|parent| parent.join(&link))
                            .unwrap_or(link)
                    };
                    same_path(&resolved, source)
                })
                .unwrap_or(false);
            if points_here {
                return result(vec![], vec![], vec![], None);
            }
            fs::remove_file(&target)
                .map_err(|e| format!("Failed to remove stale link {}: {e}", target.display()))?;
        }
    }

    let mut merged = Vec::new();
    let mut backed_up = Vec::new();
    let mut conflicts = Vec::new();

    let target_is_dir = fs::symlink_metadata(&target)
        .map(|m| m.is_dir())
        .unwrap_or(false);
    if target_is_dir {
        for skill_path in crate::fsutil::find_skill_dirs(&target) {
            let name = match skill_path.file_name() {
                Some(n) => n.to_string_lossy().to_string(),
                None => continue,
            };
            let central = source.join(&name);
            if fs::symlink_metadata(&central).is_err() {
                // Tool-only skill: adopt it into the central store instead of
                // deleting it with the replaced directory.
                crate::fsutil::move_path(&skill_path, &central)
                    .map_err(|e| format!("Failed to move tool-only skill \"{name}\": {e}"))?;
                merged.push(name);
            } else {
                // Same skill on both sides: the central copy wins, but the
                // tool-side copy is always preserved in the backup folder.
                let dest = backup_root.join(format!("{name}-{stamp}"));
                crate::fsutil::move_path(&skill_path, &dest)
                    .map_err(|e| format!("Failed to back up \"{name}\": {e}"))?;
                backed_up.push(name.clone());
                if overwrite {
                    conflicts.push(name);
                }
            }
        }
    }

    if fs::symlink_metadata(&target).is_ok() {
        // Rename the whole replaced directory (or file) into the backup folder
        // instead of deleting it, so non-skill content can be recovered by hand.
        let displaced = backup_root.join(format!("{stamp}-replaced"));
        fs::create_dir_all(&backup_root)
            .map_err(|e| format!("create {}: {e}", backup_root.display()))?;
        fs::rename(&target, &displaced).map_err(|e| {
            format!(
                "Failed to move the existing {} aside to {}: {e}",
                target.display(),
                displaced.display()
            )
        })?;

        if let Err(err) = create_skills_link(source, &target) {
            // Put the original content back; a failed sync must not lose data.
            if let Err(restore) = fs::rename(&displaced, &target) {
                return Err(format!(
                    "{err}. The previous content is preserved at {} and could not be moved back automatically: {restore}",
                    displaced.display()
                ));
            }
            return Err(format!("{err} (the previous content was restored)"));
        }
    } else {
        create_skills_link(source, &target)?;
    }

    result(merged, backed_up, conflicts, None)
}

/// Sync the central store into one tool, against the real home directory.
pub fn sync_tool(tool_key: &str, overwrite: bool) -> Result<SyncResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    let source = resolve_skills_source()?;
    sync_tool_in(&home, &source, tool_key, overwrite)
}

/// Core of `sync_all_tools`, shared with the aide-mcp binary.
pub fn sync_all_skills_to_tools() -> Vec<SyncResult> {
    let Ok(home) = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string()) else {
        return Vec::new();
    };
    let source = match resolve_skills_source() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("aide: {e}");
            return Vec::new();
        }
    };
    crate::adapter::all_tools()
        .into_iter()
        .filter(|t| t.global_skills != ".agents/skills")
        .map(|t| {
            let key = t.key.to_string();
            sync_tool_in(&home, &source, &key, false).unwrap_or_else(|e| SyncResult {
                key,
                name: t.name.to_string(),
                success: false,
                merged: vec![],
                backed_up: vec![],
                conflicts: vec![],
                error: Some(e),
            })
        })
        .collect()
}

/// Import a local folder into the central store and record its provenance.
pub fn import_local_skill(source: &str) -> Result<String, String> {
    let skills_dir = resolve_skills_source()?;
    let name = import_skill_into(Path::new(source), &skills_dir)?;
    if let Some(home) = dirs::home_dir() {
        // Record where the skill came from so provenance is visible in the UI.
        let _ =
            crate::registry::record_source(&home.join(".aide"), &name, source, "local", None, None);
    }
    Ok(name)
}

/// Full-text search across the central skill store.
pub fn search_skills(query: &str) -> Vec<SkillSearchMatch> {
    match resolve_skills_source() {
        Ok(dir) if dir.exists() => search_skills_in(&dir, query),
        _ => Vec::new(),
    }
}

/// Rewrite the `tags` array in a SKILL.md of the real skill store.
pub fn update_skill_tags(skill_dir: &Path, tags: &[String]) -> Result<(), String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    update_skill_tags_in(&home, skill_dir, tags)
}

// ── Agent allowlist ──

/// `AIDE_MCP_ALLOW_INSTALL`: comma-separated `owner/repo` prefixes. When the
/// variable is set (non-empty), the sidecar may only install ids that match.
pub fn check_agent_install(allowlist: Option<&str>, id: &str) -> Result<(), String> {
    let Some(entries) = parse_allowlist(allowlist) else {
        return Ok(());
    };
    let normalized = normalize_install_id(id);
    let allowed = entries
        .iter()
        .any(|prefix| normalized == *prefix || normalized.starts_with(&format!("{prefix}/")));
    if allowed {
        return Ok(());
    }
    Err(format!(
        "Blocked by AIDE_MCP_ALLOW_INSTALL: \"{id}\" is not in the allowed owner/repo prefixes ({}).",
        entries.join(", ")
    ))
}

/// `AIDE_MCP_ALLOW_PUBLISH`: comma-separated git scope names. When the variable
/// is set (non-empty), the sidecar may only publish/pull those scopes.
pub fn check_agent_publish(allowlist: Option<&str>, scope: &str) -> Result<(), String> {
    let Some(entries) = parse_allowlist(allowlist) else {
        return Ok(());
    };
    let scope = scope.trim().to_ascii_lowercase();
    if entries.iter().any(|entry| entry == &scope) {
        return Ok(());
    }
    Err(format!(
        "Blocked by AIDE_MCP_ALLOW_PUBLISH: scope \"{scope}\" is not in the allowed list ({}).",
        entries.join(", ")
    ))
}

fn parse_allowlist(raw: Option<&str>) -> Option<Vec<String>> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    let entries: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.trim_end_matches('/').to_ascii_lowercase())
        .collect();
    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

/// Reduce any accepted install id form to its `owner/repo` core.
fn normalize_install_id(id: &str) -> String {
    match crate::registry::parse_skill_id(id) {
        Ok((owner, repo, _)) => format!("{owner}/{repo}").to_ascii_lowercase(),
        Err(_) => id.trim().trim_end_matches('/').to_ascii_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frontmatter_reads_name_description_and_tags() {
        let md =
            "---\nname: my-skill\ndescription: Does things\ntags: [\"a\", \"b\"]\n---\n\n# Body\n";
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
        let dir =
            std::env::temp_dir().join(format!("aide-tags-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), content).unwrap();
        dir
    }

    fn temp_home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aide-lib-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn read_tags(dir: &Path) -> Vec<String> {
        let content = fs::read_to_string(dir.join("SKILL.md")).unwrap();
        parse_skill_frontmatter(&content).2
    }

    #[test]
    fn update_skill_tags_replaces_top_level_tags() {
        let home = temp_home("tags-replace");
        let dir = write_temp_skill("replace", "---\nname: s\ntags: [\"old\"]\n---\nBody\n");
        update_skill_tags_in(&home, &dir, &["new".to_string()]).unwrap();
        assert_eq!(read_tags(&dir), vec!["new".to_string()]);
        let content = fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert!(!content.contains("old"));
        assert!(content.ends_with("Body\n"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_skill_tags_appends_when_missing() {
        let home = temp_home("tags-append");
        let dir = write_temp_skill("append", "---\nname: s\n---\nBody");
        update_skill_tags_in(&home, &dir, &["a".to_string(), "b".to_string()]).unwrap();
        assert_eq!(read_tags(&dir), vec!["a".to_string(), "b".to_string()]);
        let content = fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert!(content.ends_with("Body"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_skill_tags_syncs_metadata_tags_copy() {
        let home = temp_home("tags-metadata");
        let dir = write_temp_skill(
            "metadata",
            "---\nname: s\nmetadata:\n  tags: [\"old\"]\n  other: keep\n---\nBody",
        );
        update_skill_tags_in(&home, &dir, &["fresh".to_string()]).unwrap();
        let content = fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert_eq!(read_tags(&dir), vec!["fresh".to_string()]);
        assert!(!content.contains("old"));
        assert!(content.contains("other: keep"));
        assert!(!content.contains("  tags:"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_skill_tags_clears_existing_tags() {
        let home = temp_home("tags-clear");
        let dir = write_temp_skill("clear", "---\nname: s\ntags: [\"old\"]\n---\nBody");
        update_skill_tags_in(&home, &dir, &[]).unwrap();
        let content = fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert!(!content.contains("tags:"));
        assert!(content.contains("name: s"));
        assert!(content.ends_with("Body"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_skill_tags_handles_block_list_and_quoted_tags() {
        let home = temp_home("tags-block");
        let dir = write_temp_skill(
            "block",
            "---\nname: s\ntags:\n  - old-one\n  - \"old two\"\ndescription: keep me\n---\nBody\n",
        );
        update_skill_tags_in(
            &home,
            &dir,
            &["alpha".to_string(), "beta gamma".to_string()],
        )
        .unwrap();
        let content = fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert_eq!(
            read_tags(&dir),
            vec!["alpha".to_string(), "beta gamma".to_string()]
        );
        assert!(!content.contains("old-one"));
        assert!(!content.contains("old two"));
        assert!(content.contains("description: keep me"));
        let (name, desc, _) = parse_skill_frontmatter(&content);
        assert_eq!(name.as_deref(), Some("s"));
        assert_eq!(desc.as_deref(), Some("keep me"));
        // The backup of the previous revision is kept.
        assert!(home.join(".aide").join("backups").is_dir());
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn update_skill_tags_refuses_broken_frontmatter() {
        let home = temp_home("tags-broken");
        let original = "---\nname: s\ntags: [unclosed\n---\nBody";
        let dir = write_temp_skill("broken", original);
        assert!(update_skill_tags_in(&home, &dir, &["a".to_string()]).is_err());
        // The file is left untouched, not rewritten or truncated.
        assert_eq!(fs::read_to_string(dir.join("SKILL.md")).unwrap(), original);
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn create_file_refuses_existing_path() {
        let base = temp_home("create-file");
        let path = base.join("note.md");
        crate::files::create_file_in(&base, &path).unwrap();
        let err = crate::files::create_file_in(&base, &path).unwrap_err();
        assert!(err.contains("already exists"));
        assert!(fs::metadata(&path).unwrap().is_file());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn create_directory_refuses_path_that_is_a_file() {
        let base = temp_home("create-dir");
        let path = base.join("blocker");
        fs::write(&path, "data").unwrap();
        let err = crate::files::create_directory_in(&base, &path).unwrap_err();
        assert!(err.contains("file already exists"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "data");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn rename_entry_refuses_to_overwrite_destination() {
        let base = temp_home("rename");
        let old = base.join("old.md");
        let new = base.join("new.md");
        fs::write(&old, "old").unwrap();
        fs::write(&new, "keep").unwrap();
        assert!(crate::files::rename_entry_in(&base, &old, &new).is_err());
        assert_eq!(fs::read_to_string(&old).unwrap(), "old");
        assert_eq!(fs::read_to_string(&new).unwrap(), "keep");
        // A free destination still works.
        let moved = base.join("moved.md");
        crate::files::rename_entry_in(&base, &old, &moved).unwrap();
        assert!(moved.is_file());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn file_commands_enforce_the_path_policy() {
        let base = temp_home("policy-commands");
        let outside = std::env::temp_dir().join(format!("aide-lib-outside-{}", std::process::id()));
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&outside).unwrap();

        // Outside home: every command refuses before touching the filesystem.
        assert!(crate::files::write_text_file_in(&base, &outside.join("x.md"), "x").is_err());
        assert!(crate::files::create_file_in(&base, &outside.join("x.md")).is_err());
        assert!(crate::files::create_directory_in(&base, &outside.join("dir")).is_err());
        assert!(
            crate::files::rename_entry_in(&base, &base.join("a.md"), &outside.join("a.md"))
                .is_err()
        );
        assert!(crate::files::delete_entry_in(&base, &outside.join("x.md")).is_err());
        assert!(!outside.join("x.md").exists());

        // Inside home the normal round trip still works.
        let file = base.join("notes/todo.md");
        crate::files::write_text_file_in(&base, &file, "hello").unwrap();
        assert_eq!(
            crate::files::read_text_file_in(&base, &file).unwrap(),
            "hello"
        );
        let renamed = base.join("notes/renamed.md");
        crate::files::rename_entry_in(&base, &file, &renamed).unwrap();
        assert!(crate::files::read_text_file_in(&base, &renamed).unwrap() == "hello");
        crate::files::create_directory_in(&base, &base.join("empty-dir")).unwrap();
        assert!(base.join("empty-dir").is_dir());

        // Sensitive folders are off limits even though they are inside home.
        fs::create_dir_all(base.join(".ssh")).unwrap();
        let key = base.join(".ssh/id_ed25519");
        assert!(crate::files::write_text_file_in(&base, &key, "key").is_err());
        assert!(!key.exists());
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn read_text_file_refuses_files_larger_than_5_mb() {
        let base = temp_home("read-limit");
        let big = base.join("huge.log");
        fs::write(
            &big,
            vec![b'x'; (crate::policy::MAX_READ_BYTES + 1) as usize],
        )
        .unwrap();
        let err = crate::files::read_text_file_in(&base, &big).unwrap_err();
        assert!(err.contains("too large"), "unexpected error: {err}");
        let small = base.join("small.txt");
        fs::write(&small, "ok").unwrap();
        assert_eq!(
            crate::files::read_text_file_in(&base, &small).unwrap(),
            "ok"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn file_commands_allow_symlinked_skill_folders() {
        let base = temp_home("policy-symlink");
        let repo = std::env::temp_dir().join(format!("aide-lib-repo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&repo);
        fs::create_dir_all(repo.join("my-skill")).unwrap();
        fs::write(repo.join("my-skill/SKILL.md"), "---\nname: my-skill\n---\n").unwrap();
        let skills = base.join(".agents/skills");
        fs::create_dir_all(&skills).unwrap();
        std::os::unix::fs::symlink(repo.join("my-skill"), skills.join("my-skill")).unwrap();

        let target = skills.join("my-skill/notes.md");
        crate::files::write_text_file_in(&base, &target, "linked content").unwrap();
        assert_eq!(
            crate::files::read_text_file_in(&base, &target).unwrap(),
            "linked content"
        );
        // It really is the file in the linked checkout.
        assert_eq!(
            fs::read_to_string(repo.join("my-skill/notes.md")).unwrap(),
            "linked content"
        );
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&repo);
    }

    #[cfg(unix)]
    #[test]
    fn sync_tool_backs_up_conflicts_and_adopts_tool_only_skills() {
        let home = temp_home("sync-tool");
        let central = home.join(".agents").join("skills");
        fs::create_dir_all(central.join("shared")).unwrap();
        fs::write(central.join("shared").join("SKILL.md"), "central").unwrap();
        fs::create_dir_all(central.join("central-only")).unwrap();
        fs::write(central.join("central-only").join("SKILL.md"), "central").unwrap();

        let target = home.join(".claude/skills");
        fs::create_dir_all(target.join("tool-only")).unwrap();
        fs::write(target.join("tool-only").join("SKILL.md"), "from tool").unwrap();
        fs::create_dir_all(target.join("shared")).unwrap();
        fs::write(target.join("shared").join("SKILL.md"), "tool copy").unwrap();
        fs::write(target.join("README.md"), "keep me reachable").unwrap();

        let result = sync_tool_in(&home, &central, "claude_code", false).unwrap();
        assert!(result.success);
        assert_eq!(result.merged, vec!["tool-only".to_string()]);
        assert_eq!(result.backed_up, vec!["shared".to_string()]);
        // Link now points at the central store.
        let link = fs::read_link(&target).unwrap();
        assert_eq!(link, central);
        // Tool-only skill was adopted, central copy of the conflict won.
        assert!(central.join("tool-only").join("SKILL.md").is_file());
        assert_eq!(
            fs::read_to_string(central.join("shared").join("SKILL.md")).unwrap(),
            "central"
        );
        // The replaced directory keeps the non-skill files.
        let backup_root = home.join(".aide/sync-backup/claude_code");
        let replaced: Vec<_> = fs::read_dir(&backup_root)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.to_string_lossy().ends_with("-replaced"))
            .collect();
        assert_eq!(replaced.len(), 1);
        assert!(replaced[0].join("README.md").is_file());
        let backed_up: Vec<_> = fs::read_dir(&backup_root)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.to_string_lossy().contains("/shared-"))
            .collect();
        assert_eq!(backed_up.len(), 1);
        assert_eq!(
            fs::read_to_string(backed_up[0].join("SKILL.md")).unwrap(),
            "tool copy"
        );

        // A second sync is a no-op because the link already points at central.
        let again = sync_tool_in(&home, &central, "claude_code", false).unwrap();
        assert!(again.success);
        assert!(again.merged.is_empty() && again.backed_up.is_empty());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn symlink_targets_inside_skills_are_not_followed() {
        #[cfg(unix)]
        {
            let base = temp_home("count-symlink");
            let outside = base.join("outside");
            fs::create_dir_all(&outside).unwrap();
            fs::write(outside.join("big.md"), "x").unwrap();
            let skill = base.join("skill");
            fs::create_dir_all(&skill).unwrap();
            fs::write(skill.join("SKILL.md"), "x").unwrap();
            std::os::unix::fs::symlink(&outside, skill.join("link")).unwrap();
            assert_eq!(crate::fsutil::count_files(&skill), 1);
            let _ = fs::remove_dir_all(&base);
        }
    }

    #[test]
    fn count_files_counts_recursively_and_skips_ignored_dirs() {
        let dir = std::env::temp_dir().join(format!("aide-count-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("refs")).unwrap();
        fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
        fs::write(dir.join("SKILL.md"), "x").unwrap();
        fs::write(dir.join("refs/a.md"), "x").unwrap();
        fs::write(dir.join("node_modules/pkg/index.js"), "x").unwrap();
        assert_eq!(crate::fsutil::count_files(&dir), 2);
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
        assert!(skills_dir
            .join("my-skill")
            .join("refs")
            .join("a.md")
            .is_file());
        assert!(!skills_dir.join("my-skill").join("node_modules").exists());

        let duplicate = import_skill_into(&src, &skills_dir);
        assert!(duplicate.is_err());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn import_skill_rejects_non_directory() {
        let base =
            std::env::temp_dir().join(format!("aide-import-neg-test-{}", std::process::id()));
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
        fs::write(
            beta.join("SKILL.md"),
            "---\nname: beta\n---\nZebra habitat facts",
        )
        .unwrap();

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
    #[test]
    fn split_frontmatter_ignores_horizontal_rules_in_body() {
        let doc = "---\nname: s\n---\nintro\n\n---\n\nafter";
        let (yaml, body) = split_frontmatter(doc).unwrap();
        assert!(yaml.contains("name: s"));
        assert!(body.starts_with("intro"));
        assert!(body.contains("---"));
        assert!(split_frontmatter("# no frontmatter").is_none());
    }

    #[test]
    fn update_skill_tags_creates_frontmatter_when_absent() {
        let home = temp_home("tags-create");
        let dir = write_temp_skill("create", "# Just a body\n");
        update_skill_tags_in(&home, &dir, &["one".to_string()]).unwrap();
        let content = fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert!(content.starts_with("---\n"));
        assert!(content.ends_with("# Just a body\n"));
        assert_eq!(read_tags(&dir), vec!["one".to_string()]);
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[cfg(unix)]
    #[test]
    fn sync_tool_overwrite_also_backs_up_tool_side_copy() {
        let home = temp_home("sync-overwrite");
        let central = home.join(".agents").join("skills");
        fs::create_dir_all(central.join("both")).unwrap();
        fs::write(central.join("both").join("SKILL.md"), "central").unwrap();
        let target = home.join(".claude/skills");
        fs::create_dir_all(target.join("both")).unwrap();
        fs::write(target.join("both").join("SKILL.md"), "tool copy").unwrap();

        let result = sync_tool_in(&home, &central, "claude_code", true).unwrap();
        assert!(result.success);
        // Even with overwrite the tool-side copy survives in the backup folder.
        assert_eq!(result.backed_up, vec!["both".to_string()]);
        assert_eq!(result.conflicts, vec!["both".to_string()]);
        assert_eq!(
            fs::read_to_string(central.join("both").join("SKILL.md")).unwrap(),
            "central"
        );
        let backup_root = home.join(".aide/sync-backup/claude_code");
        let backed_up: Vec<_> = fs::read_dir(&backup_root)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.to_string_lossy().contains("/both-"))
            .collect();
        assert_eq!(backed_up.len(), 1);
        assert_eq!(
            fs::read_to_string(backed_up[0].join("SKILL.md")).unwrap(),
            "tool copy"
        );
        let _ = fs::remove_dir_all(&home);
    }

    fn set_mtime(path: &Path, when: SystemTime) {
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(when))
            .unwrap();
    }

    fn make_cached_skill(skills_dir: &Path, name: &str, description: &str) -> PathBuf {
        let dir = skills_dir.join(name);
        fs::create_dir_all(dir.join("refs")).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\nbody"),
        )
        .unwrap();
        dir
    }

    #[test]
    fn skill_cache_serves_metadata_until_skill_md_changes() {
        let home = temp_home("cache-meta");
        let skills_dir = home.join(".agents/skills");
        let skill = make_cached_skill(&skills_dir, "demo", "first");
        let skill_md = skill.join("SKILL.md");
        let original_mtime = fs::metadata(&skill_md).unwrap().modified().unwrap();

        let first = skill_infos_at(&home, &skills_dir, 1000).unwrap();
        assert_eq!(first[0].description, "first");
        assert!(skill_cache_contains(&skill));

        // Rewrite with a different description but restore the mtime: a cached
        // entry must be served, proving the file was not parsed again.
        fs::write(&skill_md, "---\nname: demo\ndescription: second\n---\nbody").unwrap();
        set_mtime(&skill_md, original_mtime);
        let cached = skill_infos_at(&home, &skills_dir, 1001).unwrap();
        assert_eq!(cached[0].description, "first");

        // A newer SKILL.md mtime invalidates it.
        set_mtime(
            &skill_md,
            original_mtime + std::time::Duration::from_secs(5),
        );
        let refreshed = skill_infos_at(&home, &skills_dir, 1002).unwrap();
        assert_eq!(refreshed[0].description, "second");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn skill_cache_file_count_uses_a_bounded_ttl() {
        let home = temp_home("cache-count");
        let skills_dir = home.join(".agents/skills");
        let skill = make_cached_skill(&skills_dir, "demo", "counted");
        let first = skill_infos_at(&home, &skills_dir, 1000).unwrap();
        assert_eq!(first[0].file_count, 1);

        // A file inside a subdirectory does not change the skill directory's
        // mtime, so the cached count is served until the TTL expires.
        fs::write(skill.join("refs/notes.md"), "x").unwrap();
        let cached = skill_infos_at(&home, &skills_dir, 1030).unwrap();
        assert_eq!(cached[0].file_count, 1);
        let refreshed = skill_infos_at(&home, &skills_dir, 1000 + FILE_COUNT_TTL_SECS + 1).unwrap();
        assert_eq!(refreshed[0].file_count, 2);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn skill_cache_evicts_deleted_skills() {
        let home = temp_home("cache-evict");
        let skills_dir = home.join(".agents/skills");
        let alpha = make_cached_skill(&skills_dir, "alpha", "a");
        make_cached_skill(&skills_dir, "beta", "b");

        let first = skill_infos_at(&home, &skills_dir, 1).unwrap();
        assert_eq!(first.len(), 2);
        assert!(skill_cache_contains(&alpha));

        fs::remove_dir_all(&alpha).unwrap();
        let second = skill_infos_at(&home, &skills_dir, 2).unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].name, "beta");
        assert!(!skill_cache_contains(&alpha));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn skill_cache_refreshes_scope_after_config_change() {
        let home = temp_home("cache-scope");
        let skills_dir = home.join(".agents/skills");
        make_cached_skill(&skills_dir, "demo", "scoped");
        let aides = home.join(".aide");
        fs::create_dir_all(&aides).unwrap();
        let config = crate::git_sync::GitRemotesConfig {
            remotes: vec![
                crate::git_sync::GitRemote {
                    name: "personal".into(),
                    url: "https://example.com/personal.git".into(),
                    branch: "main".into(),
                    is_default: true,
                },
                crate::git_sync::GitRemote {
                    name: "work".into(),
                    url: "https://example.com/work.git".into(),
                    branch: "main".into(),
                    is_default: false,
                },
            ],
            skill_scopes: HashMap::new(),
        };
        crate::git_sync::write_config(&aides, &config).unwrap();
        assert_eq!(
            skill_infos_at(&home, &skills_dir, 10).unwrap()[0].scope,
            None
        );

        // Reassigning the skill changes the config fingerprint, not SKILL.md.
        let mut updated = config.clone();
        updated
            .skill_scopes
            .insert("demo".to_string(), "work".to_string());
        crate::git_sync::write_config(&aides, &updated).unwrap();
        assert_eq!(
            skill_infos_at(&home, &skills_dir, 11).unwrap()[0]
                .scope
                .as_deref(),
            Some("work")
        );
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn agent_install_allowlist_matches_owner_repo_prefixes() {
        // Unset (or empty) means unrestricted.
        assert!(check_agent_install(None, "any/repo").is_ok());
        assert!(check_agent_install(Some(""), "any/repo").is_ok());
        assert!(check_agent_install(Some("   "), "any/repo").is_ok());

        let allow = Some("vercel-labs/agent-skills");
        assert!(check_agent_install(allow, "vercel-labs/agent-skills").is_ok());
        assert!(check_agent_install(allow, "vercel-labs/agent-skills@find-skills").is_ok());
        assert!(check_agent_install(allow, "https://github.com/vercel-labs/agent-skills").is_ok());
        let err = check_agent_install(allow, "evil/repo").unwrap_err();
        assert!(err.contains("AIDE_MCP_ALLOW_INSTALL"), "{err}");

        // An owner-only prefix allows every repo below it.
        assert!(check_agent_install(Some("vercel-labs"), "vercel-labs/anything").is_ok());
        assert!(check_agent_install(Some("vercel-labs"), "other/anything").is_err());
        // Whitespace, case and trailing slashes are tolerated.
        assert!(check_agent_install(
            Some(" Vercel-Labs/Agent-Skills/ , "),
            "vercel-labs/agent-skills/skill"
        )
        .is_ok());
        assert!(check_agent_publish(Some("personal"), "none").is_err());
    }

    #[test]
    fn agent_publish_allowlist_matches_scope_names() {
        assert!(check_agent_publish(None, "personal").is_ok());
        assert!(check_agent_publish(Some("  "), "personal").is_ok());
        assert!(check_agent_publish(Some("personal,work"), "personal").is_ok());
        assert!(check_agent_publish(Some("personal, work "), "WORK").is_ok());
        let err = check_agent_publish(Some("personal"), "company").unwrap_err();
        assert!(err.contains("AIDE_MCP_ALLOW_PUBLISH"), "{err}");
    }

    #[test]
    fn delete_entry_errors_instead_of_deleting_permanently() {
        let base = temp_home("delete-missing");
        let missing = base.join("nope.md");
        let err = crate::files::delete_entry_in(&base, &missing).unwrap_err();
        assert!(err.contains("trash"), "unexpected error: {err}");
        let _ = fs::remove_dir_all(&base);
    }
}
