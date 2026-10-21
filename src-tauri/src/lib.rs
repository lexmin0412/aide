use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;
use std::{fs, io};

use serde::{Deserialize, Serialize};

pub mod adapter;
pub mod audit;
pub mod git_sync;
pub mod mcp;
pub mod models;
pub mod policy;
pub mod registry;
pub mod storage;

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

const SKILL_COUNT_IGNORED_DIRS: &[&str] =
    &["node_modules", ".git", ".pnpm", "dist", ".next", "target"];

const SKILL_TEXT_EXTENSIONS: &[&str] = &[
    "md", "markdown", "json", "jsonc", "yaml", "yml", "toml", "txt", "js", "ts", "jsx", "tsx",
    "css", "html", "sh", "bash", "env",
];

const SEARCH_MAX_FILE_SIZE: u64 = 512 * 1024;
const SEARCH_MAX_PER_SKILL: usize = 5;
const SEARCH_MAX_MATCHES: usize = 50;

/// Count files below `path`. Symlinks are not followed and never counted, so a
/// symlink cycle cannot make this recurse forever.
fn count_files(path: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !SKILL_COUNT_IGNORED_DIRS.contains(&name.as_str()) {
                    count += count_files(&entry.path());
                }
            } else if file_type.is_file() {
                count += 1;
            }
        }
    }
    count
}

/// Copy a directory tree. Symlinks are skipped instead of followed: following
/// them could escape the skill folder or loop forever.
pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let name = entry.file_name();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        // Never copy dependency/build artifacts into the central store.
        if file_type.is_dir() && SKILL_COUNT_IGNORED_DIRS.contains(&name.to_string_lossy().as_ref())
        {
            continue;
        }
        let to = dst.join(&name);
        if file_type.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn unique_stamp() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos}-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// A hidden, unique sibling path used while staging directory swaps.
pub(crate) fn swap_sibling(dst: &Path, tag: &str) -> PathBuf {
    let name = dst
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "dir".to_string());
    let parent = dst.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(".aide-{tag}-{name}-{}", unique_stamp()))
}

fn remove_path(path: &Path) {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => {
            let _ = fs::remove_dir_all(path);
        }
        Ok(_) => {
            let _ = fs::remove_file(path);
        }
        Err(_) => {}
    }
}

/// Replace `dst` with a copy of `src` without ever leaving `dst` missing or
/// half-written: the copy is staged next to the destination, the previous
/// directory is only moved aside once staging succeeded, and the swap is done
/// with renames. The displaced directory is removed after a successful swap and
/// restored if the final rename fails.
pub(crate) fn copy_dir_into_place(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let staging = swap_sibling(dst, "new");
    copy_dir_recursive(src, &staging).map_err(|e| format!("copy {}: {e}", src.display()))?;

    let existed = fs::symlink_metadata(dst).is_ok();
    let displaced = swap_sibling(dst, "old");
    if existed {
        if let Err(err) = fs::rename(dst, &displaced) {
            let _ = fs::remove_dir_all(&staging);
            return Err(format!("move {} aside: {err}", dst.display()));
        }
    }
    match fs::rename(&staging, dst) {
        Ok(()) => {
            if existed {
                remove_path(&displaced);
            }
            Ok(())
        }
        Err(err) => {
            if existed {
                let _ = fs::rename(&displaced, dst);
            }
            let _ = fs::remove_dir_all(&staging);
            Err(format!("replace {}: {err}", dst.display()))
        }
    }
}

/// Move a file or directory, falling back to copy-then-remove when the two
/// paths live on different devices (or when rename is otherwise unsupported).
pub(crate) fn move_path(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            let meta =
                fs::symlink_metadata(src).map_err(|e| format!("stat {}: {e}", src.display()))?;
            if meta.is_dir() {
                copy_dir_into_place(src, dst)?;
                fs::remove_dir_all(src).map_err(|e| format!("remove {}: {e}", src.display()))
            } else {
                fs::copy(src, dst).map_err(|e| format!("copy {}: {e}", src.display()))?;
                fs::remove_file(src).map_err(|e| format!("remove {}: {e}", src.display()))
            }
        }
    }
}

/// Find skill directories (folders containing SKILL.md) below `root`, without
/// following symlinks. Depth is bounded so malformed trees cannot run away.
fn find_skill_dirs(root: &Path) -> Vec<PathBuf> {
    const MAX_DEPTH: usize = 4;
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<_> = entries.flatten().collect();
        paths.sort_by_key(|e| e.file_name());
        for entry in paths {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if SKILL_COUNT_IGNORED_DIRS.contains(&name.as_str()) {
                continue;
            }
            let path = entry.path();
            if path.join("SKILL.md").is_file() {
                out.push(path);
            } else if depth + 1 < MAX_DEPTH {
                stack.push((path, depth + 1));
            }
        }
    }
    out
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

fn parse_skill_frontmatter(content: &str) -> (Option<String>, Option<String>, Vec<String>) {
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
    sources: &HashMap<String, registry::SkillSource>,
    config: &git_sync::GitRemotesConfig,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut source_entries: Vec<(&String, &registry::SkillSource)> = sources.iter().collect();
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
fn skill_infos_at(home: &Path, skills_dir: &Path, now: u64) -> Result<Vec<SkillInfo>, String> {
    if !skills_dir.exists() {
        return Ok(Vec::new());
    }

    let aide_dir = home.join(".aide");
    let sources = registry::read_sources(&aide_dir)?;
    let git_config = git_sync::read_config(&aide_dir)?;
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
            let value = count_files(&path);
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
fn get_git_config() -> Result<git_sync::GitRemotesConfig, String> {
    let aide_dir = dirs::home_dir()
        .ok_or_else(|| "Cannot find home directory".to_string())?
        .join(".aide");
    git_sync::read_config(&aide_dir)
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
    let mut config = git_sync::read_config(&aide_dir)?;
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

#[tauri::command(async)]
fn publish_scope(scope: String) -> Result<git_sync::PublishOutcome, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    let aide_dir = home.join(".aide");
    let config = git_sync::read_config(&aide_dir)?;
    let skills_dir = resolve_skills_source()?;
    git_sync::publish_scope(&config, &skills_dir, &aide_dir, &scope)
}

#[tauri::command(async)]
fn pull_scope(scope: String) -> Result<git_sync::PullOutcome, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    let aide_dir = home.join(".aide");
    let config = git_sync::read_config(&aide_dir)?;
    let skills_dir = resolve_skills_source()?;
    git_sync::pull_scope(&config, &skills_dir, &aide_dir, &scope)
}

#[tauri::command]
fn import_skill(source: String) -> Result<String, String> {
    let skills_dir = resolve_skills_source()?;
    let name = import_skill_into(Path::new(&source), &skills_dir)?;
    if let Some(home) = dirs::home_dir() {
        // Record where the skill came from so provenance is visible in the UI.
        let _ = registry::record_source(&home.join(".aide"), &name, &source, "local", None);
    }
    Ok(name)
}

#[tauri::command(async)]
fn search_registry(query: String) -> Result<Vec<registry::RegistrySkill>, String> {
    registry::search_registry(&query, 24)
}

#[tauri::command(async)]
fn install_skill_from_registry(id: String) -> Result<registry::RemoteInstallResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    let skills_dir = resolve_skills_source()?;
    fs::create_dir_all(&skills_dir).map_err(|e| e.to_string())?;
    let result = registry::install_from_id(&id, &skills_dir, &home.join(".aide"), false);
    audit::record(
        "install",
        serde_json::json!({
            "source": "gui",
            "id": id,
            "ok": result.is_ok(),
            "installed": result.as_ref().map(|r| r.installed.clone()).unwrap_or_default(),
            "error": result.as_ref().err(),
        }),
    );
    result
}

#[tauri::command(async)]
fn check_skill_updates() -> Result<Vec<registry::SkillUpdate>, String> {
    let aide_dir = dirs::home_dir()
        .ok_or_else(|| "Cannot find home directory".to_string())?
        .join(".aide");
    registry::check_updates(&aide_dir)
}

#[tauri::command(async)]
fn update_skill(skill: String) -> Result<registry::RemoteInstallResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    let aide_dir = home.join(".aide");
    let sources = registry::read_sources(&aide_dir)?;
    let meta = sources
        .get(&skill)
        .ok_or_else(|| format!("No install source recorded for \"{skill}\""))?
        .clone();
    let skills_dir = resolve_skills_source()?;
    fs::create_dir_all(&skills_dir).map_err(|e| e.to_string())?;
    let result = registry::install_from_id(&meta.id, &skills_dir, &aide_dir, true);
    audit::record(
        "update",
        serde_json::json!({
            "source": "gui",
            "skill": skill,
            "id": meta.id,
            "ok": result.is_ok(),
            "error": result.as_ref().err(),
        }),
    );
    result
}

// ── Model/provider management ──

#[tauri::command(async)]
fn list_model_providers(force: Option<bool>) -> Result<Vec<models::ProviderSummary>, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    models::fetch_models_dev(&home.join(".aide"), force.unwrap_or(false))
}

#[tauri::command]
fn list_model_profiles() -> Result<models::ModelProfilesConfig, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    models::read_profiles(&home.join(".aide"))
}

#[tauri::command]
fn save_model_profiles(config: models::ModelProfilesConfig) -> Result<(), String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    models::write_profiles(&home.join(".aide"), &config)
}

#[tauri::command]
fn sync_model_profile(profile_id: String) -> Result<Vec<models::ModelSyncResult>, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    let aide_dir = home.join(".aide");
    let config = models::read_profiles(&aide_dir)?;
    let profile = config
        .profiles
        .iter()
        .find(|p| p.id == profile_id)
        .ok_or_else(|| format!("Unknown profile \"{profile_id}\""))?
        .clone();
    let providers = models::fetch_models_dev(&aide_dir, false)?;
    Ok(models::sync_profile(&profile, &providers, &home))
}

#[tauri::command]
fn sync_all_model_profiles() -> Result<Vec<models::ModelProfileSyncResult>, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    let aide_dir = home.join(".aide");
    let config = models::read_profiles(&aide_dir)?;
    let providers = models::fetch_models_dev(&aide_dir, false)?;
    Ok(models::sync_all_profiles(&config, &providers, &home))
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
    match registry::parse_skill_id(id) {
        Ok((owner, repo, _)) => format!("{owner}/{repo}").to_ascii_lowercase(),
        Err(_) => id.trim().trim_end_matches('/').to_ascii_lowercase(),
    }
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
    // Enabled only when the registered command still points at the sidecar that
    // ships with the running build: after an upgrade the stored path is stale
    // and the agent would silently fail to start.
    let enabled = binary.as_ref().is_some_and(|current| {
        mcp::read_central()
            .map(|central| {
                central
                    .servers
                    .get(AGENT_SERVER_NAME)
                    .map(|s| {
                        !s.disabled.unwrap_or(false)
                            && s.command
                                .as_deref()
                                .map(|cmd| Path::new(cmd) == current.as_path())
                                .unwrap_or(false)
                    })
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    });
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
        let path = sidecar_path().ok_or("aide-mcp binary not found next to the running app")?;
        central.servers.insert(
            AGENT_SERVER_NAME.to_string(),
            mcp::McpServerConfig {
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
    mcp::save_central(&central)?;
    audit::record(
        "enable_agent_access",
        serde_json::json!({ "source": "gui", "enabled": enable }),
    );
    if !enable {
        // Disabling must also remove the entry from every tool config where aide
        // wrote it. Best effort: the central entry is already gone, so a failing
        // tool config is surfaced instead of rolled back silently.
        let managed = mcp::read_managed().map_err(|e| {
            format!("Agent access disabled, but the MCP manifest is unreadable so tool configs were left untouched: {e}")
        })?;
        let mut first_error: Option<String> = None;
        for adapter in mcp::all_mcp_adapters() {
            let installed = adapter
                .resolve_path()
                .map(|path| path.exists())
                .unwrap_or(false);
            if !installed && !managed.tools.contains_key(adapter.key) {
                continue;
            }
            if let Err(err) = mcp::sync_to_tool(&adapter, &central) {
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
        if let Some(err) = first_error {
            return Err(format!(
                "Agent access disabled, but syncing one or more tool configs failed: {err}"
            ));
        }
    }
    Ok(get_agent_server())
}

#[tauri::command(async)]
fn search_skills(query: String) -> Result<Vec<SkillSearchMatch>, String> {
    let dir = resolve_skills_source()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    Ok(search_skills_in(&dir, &query))
}

/// Read a UTF-8 text file through the path policy, refusing huge files so a
/// runaway log cannot be pulled into memory over IPC.
fn read_text_file_in(home: &Path, path: &Path) -> Result<String, String> {
    policy::ensure_readable_in(home, path)?;
    let meta = fs::metadata(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if meta.len() > policy::MAX_READ_BYTES {
        return Err(format!(
            "{} is too large to open ({:.1} MB; the limit is {} MB)",
            path.display(),
            meta.len() as f64 / (1024.0 * 1024.0),
            policy::MAX_READ_BYTES / (1024 * 1024)
        ));
    }
    fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))
}

fn write_text_file_in(home: &Path, path: &Path, content: &str) -> Result<(), String> {
    let safe = policy::ensure_writable_in(home, path)?;
    // Config files and skill sources are edited in-app; a crash mid-write must
    // never truncate them, so reuse the same atomic writer as the config stores.
    storage::atomic_write(&safe, content.as_bytes())
}

fn create_file_in(home: &Path, path: &Path) -> Result<(), String> {
    let safe = policy::ensure_writable_in(home, path)?;
    if fs::symlink_metadata(&safe).is_ok() {
        return Err(format!(
            "A file or folder already exists at {}",
            safe.display()
        ));
    }
    if let Some(parent) = safe.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    // create_new guarantees we never truncate a file created concurrently.
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&safe)
        .map_err(|e| format!("create {}: {e}", safe.display()))?;
    Ok(())
}

fn create_directory_in(home: &Path, path: &Path) -> Result<(), String> {
    let safe = policy::ensure_writable_in(home, path)?;
    match fs::symlink_metadata(&safe) {
        Ok(meta) if !meta.is_dir() => {
            return Err(format!("A file already exists at {}", safe.display()));
        }
        _ => {}
    }
    fs::create_dir_all(&safe).map_err(|e| format!("create {}: {e}", safe.display()))
}

fn delete_entry_in(home: &Path, path: &Path) -> Result<(), String> {
    let safe = policy::ensure_writable_in(home, path)?;
    // Move to the system trash instead of permanent deletion so mistakes are
    // recoverable. Never silently fall back to a permanent delete: a failing
    // trash would otherwise destroy user data with no way back.
    trash::delete(&safe).map_err(|e| {
        format!(
            "Could not move {} to the trash; nothing was deleted ({e})",
            safe.display()
        )
    })
}

fn rename_entry_in(home: &Path, old_path: &Path, new_path: &Path) -> Result<(), String> {
    let old_safe = policy::ensure_writable_in(home, old_path)?;
    let new_safe = policy::ensure_writable_in(home, new_path)?;
    if fs::symlink_metadata(&new_safe).is_ok() {
        return Err(format!(
            "A file or folder already exists at {}",
            new_safe.display()
        ));
    }
    if fs::symlink_metadata(&old_safe).is_err() {
        return Err(format!("No such file or folder: {}", old_safe.display()));
    }
    fs::rename(&old_safe, &new_safe).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_text_file(path: String) -> Result<String, String> {
    read_text_file_in(&policy::home_root()?, Path::new(&path))
}

#[tauri::command]
fn write_text_file(path: String, content: String) -> Result<(), String> {
    write_text_file_in(&policy::home_root()?, Path::new(&path), &content)
}

#[tauri::command]
fn create_file(path: String) -> Result<(), String> {
    create_file_in(&policy::home_root()?, Path::new(&path))
}

#[tauri::command]
fn create_directory(path: String) -> Result<(), String> {
    create_directory_in(&policy::home_root()?, Path::new(&path))
}

#[tauri::command]
fn delete_entry(path: String) -> Result<(), String> {
    delete_entry_in(&policy::home_root()?, Path::new(&path))
}

#[tauri::command]
fn rename_entry(old_path: String, new_path: String) -> Result<(), String> {
    rename_entry_in(
        &policy::home_root()?,
        Path::new(&old_path),
        Path::new(&new_path),
    )
}

/// Rewrite the `tags` array in a SKILL.md frontmatter, keeping the rest of the
/// document untouched. The new frontmatter is re-parsed before it is written so
/// a serialization problem can never corrupt the skill definition.
fn update_skill_tags_in(home: &Path, skill_dir: &Path, tags: &[String]) -> Result<(), String> {
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
    storage::write_bytes(&skill_md, new_content.as_bytes(), Some(&backups))
}

#[tauri::command]
fn update_skill_tags(path: String, tags: Vec<String>) -> Result<(), String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    update_skill_tags_in(&home, Path::new(&path), &tags)
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
            let resolved = if link.is_absolute() {
                link
            } else {
                target
                    .parent()
                    .map(|parent| parent.join(&link))
                    .unwrap_or(link)
            };
            if same_path(&resolved, &resolve_skills_source().unwrap_or_default()) {
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

/// Do two paths refer to the same location? Falls back to canonicalization so
/// relative and absolute spellings compare equal.
fn same_path(a: &Path, b: &Path) -> bool {
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
fn create_skills_link(source: &Path, target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, target).map_err(|e| e.to_string())
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::junction(source, target).map_err(|e| e.to_string())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (source, target);
        Err("Symlink not supported on this platform".to_string())
    }
}

fn sync_tool_in(
    home: &Path,
    source: &Path,
    tool_key: &str,
    overwrite: bool,
) -> Result<SyncResult, String> {
    let tool = adapter::all_tools()
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
        for skill_path in find_skill_dirs(&target) {
            let name = match skill_path.file_name() {
                Some(n) => n.to_string_lossy().to_string(),
                None => continue,
            };
            let central = source.join(&name);
            if fs::symlink_metadata(&central).is_err() {
                // Tool-only skill: adopt it into the central store instead of
                // deleting it with the replaced directory.
                move_path(&skill_path, &central)
                    .map_err(|e| format!("Failed to move tool-only skill \"{name}\": {e}"))?;
                merged.push(name);
            } else {
                // Same skill on both sides: the central copy wins, but the
                // tool-side copy is always preserved in the backup folder.
                let dest = backup_root.join(format!("{name}-{stamp}"));
                move_path(&skill_path, &dest)
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

#[tauri::command(async)]
fn sync_tool(tool_key: String, overwrite: Option<bool>) -> Result<SyncResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    let source = resolve_skills_source()?;
    sync_tool_in(&home, &source, &tool_key, overwrite.unwrap_or(false))
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
    adapter::all_tools()
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

#[tauri::command(async)]
fn sync_all_tools() -> Vec<SyncResult> {
    sync_all_skills_to_tools()
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
                (
                    name,
                    mcp::McpServerConfig {
                        command: s.command,
                        args: s.args,
                        url: s.url,
                        env: s.env,
                        headers: s.headers,
                        disabled: s.disabled,
                        description: s.description,
                        targets: s.targets,
                    },
                )
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

#[tauri::command(async)]
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

#[tauri::command(async)]
fn sync_mcp_all() -> Result<Vec<McpSyncResult>, String> {
    let config = mcp::read_central()?;
    Ok(mcp::all_mcp_adapters()
        .into_iter()
        .map(|adapter| {
            let result = mcp::sync_to_tool(&adapter, &config);
            match result {
                Ok(result) => McpSyncResult {
                    key: adapter.key.to_string(),
                    name: adapter.name.to_string(),
                    skipped: result.skipped,
                    message: result.message,
                },
                Err(err) => McpSyncResult {
                    key: adapter.key.to_string(),
                    name: adapter.name.to_string(),
                    skipped: true,
                    message: format!("Failed: {err}"),
                },
            }
        })
        .collect())
}

#[derive(Serialize)]
pub struct ImportResult {
    pub source: String,
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[tauri::command(async)]
fn import_mcp_all() -> Result<Vec<ImportResult>, String> {
    let mut central = mcp::read_central()?;
    let mut results = Vec::new();
    for adapter in mcp::all_mcp_adapters() {
        match mcp::import_from_adapter(&adapter, &mut central) {
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
            Err(err) => results.push(ImportResult {
                source: adapter.name.to_string(),
                imported: vec![],
                skipped: vec![],
                error: Some(err),
            }),
        }
    }
    // save_central is atomic and backs up the previous revision; propagate its
    // failure instead of leaving the UI with a silently unsaved import.
    mcp::save_central(&central)?;
    Ok(results)
}

#[derive(Serialize)]
pub struct McpDrift {
    pub tool_key: String,
    pub tool_name: String,
    pub missing: Vec<String>,
    pub leftover: Vec<String>,
}

/// Compare every installed tool config against the central config and the
/// managed manifest. Only adapters with actual differences are reported.
pub fn mcp_drift() -> Result<Vec<McpDrift>, String> {
    let central = mcp::read_central()?;
    let managed = mcp::read_managed()?;
    let mut drifts = Vec::new();
    for adapter in mcp::all_mcp_adapters() {
        let config_path = adapter.resolve_path()?;
        if let Some(drift) = mcp::compute_drift(&adapter, &config_path, &central, &managed)? {
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

#[tauri::command(async)]
fn get_mcp_drift() -> Result<Vec<McpDrift>, String> {
    mcp_drift()
}

#[tauri::command(async)]
fn cleanup_mcp_drift() -> Result<Vec<McpSyncResult>, String> {
    let config = mcp::read_central()?;
    mcp::all_mcp_adapters()
        .into_iter()
        .map(|adapter| {
            // Unlike `sync_mcp_all`, config read errors propagate: cleanup must
            // not report success while a corrupt tool config was left behind.
            let result = mcp::sync_to_tool(&adapter, &config)?;
            Ok(McpSyncResult {
                key: adapter.key.to_string(),
                name: adapter.name.to_string(),
                skipped: result.skipped,
                message: result.message,
            })
        })
        .collect()
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
            get_mcp_drift,
            cleanup_mcp_drift,
            check_sync_statuses,
            check_skill_updates,
            update_skill,
            get_agent_server,
            enable_agent_access,
            list_model_providers,
            list_model_profiles,
            save_model_profiles,
            sync_model_profile,
            sync_all_model_profiles,
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
        create_file_in(&base, &path).unwrap();
        let err = create_file_in(&base, &path).unwrap_err();
        assert!(err.contains("already exists"));
        assert!(fs::metadata(&path).unwrap().is_file());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn create_directory_refuses_path_that_is_a_file() {
        let base = temp_home("create-dir");
        let path = base.join("blocker");
        fs::write(&path, "data").unwrap();
        let err = create_directory_in(&base, &path).unwrap_err();
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
        assert!(rename_entry_in(&base, &old, &new).is_err());
        assert_eq!(fs::read_to_string(&old).unwrap(), "old");
        assert_eq!(fs::read_to_string(&new).unwrap(), "keep");
        // A free destination still works.
        let moved = base.join("moved.md");
        rename_entry_in(&base, &old, &moved).unwrap();
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
        assert!(write_text_file_in(&base, &outside.join("x.md"), "x").is_err());
        assert!(create_file_in(&base, &outside.join("x.md")).is_err());
        assert!(create_directory_in(&base, &outside.join("dir")).is_err());
        assert!(rename_entry_in(&base, &base.join("a.md"), &outside.join("a.md")).is_err());
        assert!(delete_entry_in(&base, &outside.join("x.md")).is_err());
        assert!(!outside.join("x.md").exists());

        // Inside home the normal round trip still works.
        let file = base.join("notes/todo.md");
        write_text_file_in(&base, &file, "hello").unwrap();
        assert_eq!(read_text_file_in(&base, &file).unwrap(), "hello");
        let renamed = base.join("notes/renamed.md");
        rename_entry_in(&base, &file, &renamed).unwrap();
        assert!(read_text_file_in(&base, &renamed).unwrap() == "hello");
        create_directory_in(&base, &base.join("empty-dir")).unwrap();
        assert!(base.join("empty-dir").is_dir());

        // Sensitive folders are off limits even though they are inside home.
        fs::create_dir_all(base.join(".ssh")).unwrap();
        let key = base.join(".ssh/id_ed25519");
        assert!(write_text_file_in(&base, &key, "key").is_err());
        assert!(!key.exists());
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn read_text_file_refuses_files_larger_than_5_mb() {
        let base = temp_home("read-limit");
        let big = base.join("huge.log");
        fs::write(&big, vec![b'x'; (policy::MAX_READ_BYTES + 1) as usize]).unwrap();
        let err = read_text_file_in(&base, &big).unwrap_err();
        assert!(err.contains("too large"), "unexpected error: {err}");
        let small = base.join("small.txt");
        fs::write(&small, "ok").unwrap();
        assert_eq!(read_text_file_in(&base, &small).unwrap(), "ok");
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
        write_text_file_in(&base, &target, "linked content").unwrap();
        assert_eq!(read_text_file_in(&base, &target).unwrap(), "linked content");
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
    fn symlink_targets_inside_skills_are_not_followed_by_count_files() {
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
            assert_eq!(count_files(&skill), 1);
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
        let config = git_sync::GitRemotesConfig {
            remotes: vec![
                git_sync::GitRemote {
                    name: "personal".into(),
                    url: "https://example.com/personal.git".into(),
                    branch: "main".into(),
                    is_default: true,
                },
                git_sync::GitRemote {
                    name: "work".into(),
                    url: "https://example.com/work.git".into(),
                    branch: "main".into(),
                    is_default: false,
                },
            ],
            skill_scopes: HashMap::new(),
        };
        git_sync::write_config(&aides, &config).unwrap();
        assert_eq!(
            skill_infos_at(&home, &skills_dir, 10).unwrap()[0].scope,
            None
        );

        // Reassigning the skill changes the config fingerprint, not SKILL.md.
        let mut updated = config.clone();
        updated
            .skill_scopes
            .insert("demo".to_string(), "work".to_string());
        git_sync::write_config(&aides, &updated).unwrap();
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
        let err = delete_entry_in(&base, &missing).unwrap_err();
        assert!(err.contains("trash"), "unexpected error: {err}");
        let _ = fs::remove_dir_all(&base);
    }
}
