// skills.sh registry integration: search public skills and install them from
// GitHub archives. The search endpoint is the same unauthenticated one the
// official `npx skills` CLI uses.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::storage;

const SEARCH_ENDPOINT: &str = "https://skills.sh/api/search";
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_DOWNLOAD_BYTES: u64 = 25 * 1024 * 1024;
const MAX_EXTRACTED_FILES: usize = 2000;
/// Upper bounds on what an (untrusted) archive may expand to on disk.
const MAX_EXTRACTED_TOTAL_BYTES: u64 = 150 * 1024 * 1024;
const MAX_EXTRACTED_FILE_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RegistrySkill {
    /// Full id, e.g. "owner/repo/skill"
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub installs: u64,
    /// "owner/repo"
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    skills: Vec<RegistrySkill>,
}

/// Provenance record persisted to ~/.aide/skill-sources.json so installs can
/// later be updated, grouped or synced per origin (e.g. scoped git remotes).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillSource {
    pub id: String,
    pub source: String,
    pub installed_at: u64,
    /// HEAD commit of the skill's repo subtree at install/update time.
    #[serde(default)]
    pub commit: Option<String>,
    /// Tree hash of the installed skill directory at install/update time.
    /// Legacy records do not have it; it is computed lazily for previews.
    #[serde(default)]
    pub hash: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SkillUpdate {
    pub skill: String,
    pub id: String,
    pub source: String,
    pub installed_commit: String,
    pub latest_commit: String,
}

/// How long a GitHub HEAD lookup stays valid on disk. `check_updates` runs on
/// page open and fans out one API call per repository/subtree; caching the
/// HEADs prevents a rate-limit hit when the user re-opens the page.
const UPDATE_CACHE_TTL: u64 = 10 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HeadCacheEntry {
    commit: String,
    fetched_at: u64,
}

fn head_cache_path(aide_dir: &Path) -> PathBuf {
    aide_dir.join("cache").join("github-heads.json")
}

/// Best-effort cache read: this file is derived data, so a corrupt cache is
/// discarded instead of blocking the update check.
fn read_head_cache(aide_dir: &Path) -> HashMap<String, HeadCacheEntry> {
    fs::read_to_string(head_cache_path(aide_dir))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_head_cache(
    aide_dir: &Path,
    cache: &HashMap<String, HeadCacheEntry>,
) -> Result<(), String> {
    storage::write_json(&head_cache_path(aide_dir), cache, None)
}

/// Identity of a GitHub HEAD lookup: a repository, optionally limited to a
/// subtree. Several installed skills can share one entry.
fn head_cache_key(owner: &str, repo: &str, subtree: Option<&str>) -> String {
    match subtree {
        Some(subtree) => format!("{owner}/{repo}/{subtree}"),
        None => format!("{owner}/{repo}"),
    }
}

/// Return a cached HEAD commit when it is younger than `UPDATE_CACHE_TTL`,
/// otherwise call `fetch`. The bool reports whether the cache was modified.
fn cached_or_fetch_head(
    key: &str,
    cache: &mut HashMap<String, HeadCacheEntry>,
    now: u64,
    fetch: impl FnOnce() -> Result<String, String>,
) -> Result<(String, bool), String> {
    if let Some(entry) = cache.get(key) {
        if now.saturating_sub(entry.fetched_at) < UPDATE_CACHE_TTL {
            return Ok((entry.commit.clone(), false));
        }
    }
    let commit = fetch()?;
    cache.insert(
        key.to_string(),
        HeadCacheEntry {
            commit: commit.clone(),
            fetched_at: now,
        },
    );
    Ok((commit, true))
}

/// Cached HEAD lookup for a single repository/subtree, used by the update
/// preview. Reuses the same 10-minute on-disk cache as `check_updates`.
pub(crate) fn head_commit_cached(
    aide_dir: &Path,
    owner: &str,
    repo: &str,
    subtree: Option<&str>,
) -> Result<String, String> {
    let now = now_secs();
    let mut cache = read_head_cache(aide_dir);
    let key = head_cache_key(owner, repo, subtree);
    let (commit, changed) = cached_or_fetch_head(&key, &mut cache, now, || {
        latest_commit(owner, repo, subtree)
    })?;
    if changed {
        let _ = write_head_cache(aide_dir, &cache);
    }
    Ok(commit)
}

/// Deduplicated HEAD lookups required by the recorded install sources:
/// (cache key, owner, repo, subtree). Several skills of one repository share a
/// single lookup.
fn update_targets(
    sources: &HashMap<String, SkillSource>,
) -> Vec<(String, String, String, Option<String>)> {
    let mut targets: Vec<(String, String, String, Option<String>)> = sources
        .values()
        .filter(|meta| meta.commit.is_some())
        .filter_map(|meta| {
            let parts: Vec<&str> = meta.id.split('/').collect();
            if parts.len() < 2 {
                return None;
            }
            let subtree = (parts.len() > 2).then(|| parts[2].to_string());
            let key = head_cache_key(parts[0], parts[1], subtree.as_deref());
            Some((key, parts[0].to_string(), parts[1].to_string(), subtree))
        })
        .collect();
    targets.sort();
    targets.dedup();
    targets
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Latest commit touching `path` (or the repo root when None) via the
/// unauthenticated GitHub API.
fn latest_commit(owner: &str, repo: &str, path: Option<&str>) -> Result<String, String> {
    let mut url = format!("https://api.github.com/repos/{owner}/{repo}/commits?per_page=1");
    if let Some(p) = path {
        url.push_str(&format!("&path={}", urlencode(p)));
    }
    let body = ureq::get(&url)
        .timeout(REQUEST_TIMEOUT)
        .call()
        .map_err(|e| format!("GitHub API failed: {e}"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    let parsed: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    parsed[0]["sha"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Unexpected GitHub API response for {owner}/{repo}"))
}

pub fn search_registry(query: &str, limit: usize) -> Result<Vec<RegistrySkill>, String> {
    let q = query.trim();
    if q.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let url = format!("{}?q={}&limit={}", SEARCH_ENDPOINT, urlencode(q), limit);
    let body = ureq::get(&url)
        .timeout(REQUEST_TIMEOUT)
        .call()
        .map_err(|e| format!("Registry search failed: {e}"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    let parsed: SearchResponse =
        serde_json::from_str(&body).map_err(|e| format!("Invalid registry response: {e}"))?;
    Ok(parsed.skills)
}

fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Parse "owner/repo", "owner/repo@skill", "owner/repo/skill",
/// "https://github.com/owner/repo(...)" into (owner, repo, optional skill).
pub fn parse_skill_id(id: &str) -> Result<(String, String, Option<String>), String> {
    let cleaned = id.trim();
    let cleaned = cleaned
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("github.com/")
        .trim_end_matches(".git")
        .trim_matches('/');
    let cleaned = match cleaned.find("/tree/") {
        Some(i) => &cleaned[..i],
        None => cleaned,
    };
    let cleaned = cleaned.replace('@', "/");
    let parts: Vec<&str> = cleaned.split('/').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        [owner, repo] => Ok((owner.to_string(), repo.to_string(), None)),
        [owner, repo, skill] => Ok((owner.to_string(), repo.to_string(), Some(skill.to_string()))),
        _ => Err(format!(
            "Invalid skill id \"{id}\". Expected owner/repo or owner/repo/skill"
        )),
    }
}

/// Directories that qualify as skills: contain a SKILL.md. If the repository
/// root itself holds one, the whole repo is a single skill.
pub fn discover_skill_dirs(root: &Path) -> Vec<PathBuf> {
    if root.join("SKILL.md").is_file() {
        return vec![root.to_path_buf()];
    }
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            if p.is_dir() && p.join("SKILL.md").is_file() {
                out.push(p);
            }
        }
    }
    out
}

pub fn sources_path(aide_dir: &Path) -> PathBuf {
    aide_dir.join("skill-sources.json")
}

/// Read install provenance. A missing file is empty; a corrupt one is an error
/// so it is never replaced with an empty map.
pub fn read_sources(aide_dir: &Path) -> Result<HashMap<String, SkillSource>, String> {
    Ok(storage::read_json(&sources_path(aide_dir))?.unwrap_or_default())
}

/// Compare each registry-installed skill's recorded commit with the remote
/// HEAD for its subtree. Entries without a recorded commit (local imports,
/// failed lookups) are skipped.
pub fn check_updates(aide_dir: &Path) -> Result<Vec<SkillUpdate>, String> {
    let sources = read_sources(aide_dir)?;
    let now = now_secs();
    let mut cache = read_head_cache(aide_dir);
    let mut cache_changed = false;
    let mut latest_by_key: HashMap<String, String> = HashMap::new();

    for (key, owner, repo, subtree) in update_targets(&sources) {
        let result = cached_or_fetch_head(&key, &mut cache, now, || {
            latest_commit(&owner, &repo, subtree.as_deref())
        });
        if let Ok((commit, changed)) = result {
            cache_changed |= changed;
            latest_by_key.insert(key, commit);
        }
        // Network/API failures are skipped per repository, as before: a flaky
        // lookup for one repo must not hide updates for the others.
    }
    if cache_changed {
        // Cache writes are best effort; a failure only costs the next lookup.
        let _ = write_head_cache(aide_dir, &cache);
    }

    let mut entries: Vec<(&String, &SkillSource)> = sources.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let mut updates = Vec::new();
    for (skill, meta) in entries {
        let Some(installed_commit) = meta.commit.clone() else {
            continue;
        };
        let parts: Vec<&str> = meta.id.split('/').collect();
        if parts.len() < 2 {
            continue;
        }
        let subtree = (parts.len() > 2).then(|| parts[2]);
        let key = head_cache_key(parts[0], parts[1], subtree);
        let Some(latest) = latest_by_key.get(&key) else {
            continue;
        };
        if *latest != installed_commit {
            updates.push(SkillUpdate {
                skill: skill.clone(),
                id: meta.id.clone(),
                source: meta.source.clone(),
                installed_commit,
                latest_commit: latest.clone(),
            });
        }
    }
    Ok(updates)
}

pub fn record_source(
    aide_dir: &Path,
    skill: &str,
    id: &str,
    source: &str,
    commit: Option<String>,
    hash: Option<String>,
) -> Result<(), String> {
    let mut map = read_sources(aide_dir)?;
    map.insert(
        skill.to_string(),
        SkillSource {
            id: id.to_string(),
            source: source.to_string(),
            installed_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            commit,
            hash,
        },
    );
    let backups = aide_dir.join("backups");
    storage::write_json(&sources_path(aide_dir), &map, Some(&backups))
}

fn download_repo_tarball(
    owner: &str,
    repo: &str,
    reference: &str,
    dest: &Path,
) -> Result<(), String> {
    // Download the exact commit that was resolved before the download: asking
    // for HEAD twice could persist a commit id that no longer matches the code.
    let url = format!("https://codeload.github.com/{owner}/{repo}/tar.gz/{reference}");
    let mut reader = ureq::get(&url)
        .timeout(DOWNLOAD_TIMEOUT)
        .call()
        .map_err(|e| format!("Download failed: {e}"))?
        .into_reader();
    let mut file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 16384];
    let mut total: u64 = 0;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > MAX_DOWNLOAD_BYTES {
            return Err("Repository archive exceeds the 25 MB limit".to_string());
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Guard against decompression bombs: a small archive must not be able to fill
/// the disk. Returns the running total of extracted file bytes.
fn check_extraction_limits(size: u64, running_total: u64) -> Result<u64, String> {
    if size > MAX_EXTRACTED_FILE_BYTES {
        return Err("Archive contains a file larger than the 20 MB limit".to_string());
    }
    let total = running_total.saturating_add(size);
    if total > MAX_EXTRACTED_TOTAL_BYTES {
        return Err("Archive expands beyond the 150 MB limit".to_string());
    }
    Ok(total)
}

fn extract_tarball(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    let mut count = 0usize;
    let mut total_bytes: u64 = 0;
    for entry in archive
        .entries()
        .map_err(|e| format!("Invalid archive: {e}"))?
    {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let entry_type = entry.header().entry_type();
        if !(entry_type.is_file() || entry_type.is_dir()) {
            // Skip symlinks/hardlinks from untrusted archives.
            continue;
        }
        count += 1;
        if count > MAX_EXTRACTED_FILES {
            return Err("Archive contains too many files".to_string());
        }
        if entry_type.is_file() {
            let size = entry.header().size().map_err(|e| e.to_string())?;
            total_bytes = check_extraction_limits(size, total_bytes)?;
        }
        // unpack_in returns false when the entry would escape `dest`.
        let unpacked = entry
            .unpack_in(dest)
            .map_err(|e| format!("Unsafe archive entry: {e}"))?;
        if !unpacked {
            return Err("Archive contains an entry outside the extraction directory".to_string());
        }
    }
    Ok(())
}

fn extract_root(dest: &Path) -> Result<PathBuf, String> {
    let mut entries = fs::read_dir(dest).map_err(|e| e.to_string())?;
    let first = entries
        .next()
        .ok_or("Archive is empty")?
        .map_err(|e| e.to_string())?;
    Ok(first.path())
}

#[derive(Debug, Serialize)]
pub struct RemoteInstallResult {
    pub installed: Vec<String>,
    pub skipped: Vec<String>,
}

/// A folder name is only accepted when it stays inside the skills directory.
fn valid_folder_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return None;
    }
    if trimmed
        .chars()
        .any(|c| c == '/' || c == '\\' || c.is_control())
    {
        return None;
    }
    Some(trimmed.to_string())
}

/// Name of the folder the skill is installed into: the frontmatter `name` when
/// present, else the skill segment of the id, else the repository name. The
/// GitHub archive root name (e.g. "repo-main") is never used.
pub fn skill_dest_name(src: &Path, only_skill: Option<&str>, repo: &str) -> Result<String, String> {
    if let Some(name) =
        crate::skills::skill_frontmatter_name(src).and_then(|n| valid_folder_name(&n))
    {
        return Ok(name);
    }
    if let Some(skill) = only_skill.and_then(valid_folder_name) {
        return Ok(skill);
    }
    valid_folder_name(repo)
        .ok_or_else(|| format!("Cannot derive a skill folder name for \"{repo}\""))
}

/// Does this archive directory correspond to the skill requested by id?
fn dir_matches_skill(dir: &Path, wanted: &str) -> bool {
    let dir_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if dir_name == wanted {
        return true;
    }
    crate::skills::skill_frontmatter_name(dir).as_deref() == Some(wanted)
}

/// Install one or more skills from a GitHub repository into `skills_dir`,
/// recording provenance under `aide_dir`. Existing skills are skipped unless
/// `force`, in which case the replaced copy is moved to the trash first.
pub fn install_from_id(
    id: &str,
    skills_dir: &Path,
    aide_dir: &Path,
    force: bool,
) -> Result<RemoteInstallResult, String> {
    let (owner, repo, only_skill) = parse_skill_id(id)?;
    if owner.is_empty() || repo.is_empty() {
        return Err(format!("Invalid skill id \"{id}\""));
    }

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = aide_dir.join("tmp").join(format!("skill-install-{ts}"));
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

    let result = (|| {
        // Resolve the commit before downloading so the recorded provenance is
        // exactly the tree that gets installed even if HEAD moves meanwhile.
        let head_commit = latest_commit(&owner, &repo, only_skill.as_deref()).ok();
        let archive = tmp.join("repo.tar.gz");
        download_repo_tarball(
            &owner,
            &repo,
            head_commit.as_deref().unwrap_or("HEAD"),
            &archive,
        )?;
        let extract_dir = tmp.join("extract");
        fs::create_dir_all(&extract_dir).map_err(|e| e.to_string())?;
        extract_tarball(&archive, &extract_dir)?;
        let root = extract_root(&extract_dir)?;

        let mut candidates = discover_skill_dirs(&root);
        if let Some(name) = &only_skill {
            candidates.retain(|p| dir_matches_skill(p, name));
            if candidates.is_empty() {
                return Err(format!("Skill \"{name}\" not found in {owner}/{repo}"));
            }
        }

        let mut installed = Vec::new();
        let mut skipped = Vec::new();
        for src in candidates {
            let name = skill_dest_name(&src, only_skill.as_deref(), &repo)?;
            let dest = skills_dir.join(&name);
            if dest.exists() && !force {
                skipped.push(name);
                continue;
            }
            if dest.exists() {
                // Replaced content goes to the trash so an update is reversible.
                // A trash failure aborts instead of deleting permanently.
                trash::delete(&dest).map_err(|e| {
                    format!(
                        "Could not move the previous \"{name}\" to the trash; update aborted ({e})"
                    )
                })?;
            }
            // Stage next to the destination and swap in, so a failed copy never
            // leaves a partial skill behind.
            crate::fsutil::copy_dir_into_place(&src, &dest)
                .map_err(|e| format!("Failed to install \"{name}\": {e}"))?;
            let hash = crate::hash::tree_hash(&dest).ok();
            record_source(
                aide_dir,
                &name,
                id,
                &format!("{owner}/{repo}"),
                head_commit.clone(),
                hash,
            )?;
            installed.push(name);
        }
        Ok(RemoteInstallResult { installed, skipped })
    })();

    let _ = fs::remove_dir_all(&tmp);
    result
}

// ── Update preview ──

/// What would change if the registry skill were updated to its latest commit.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkillUpdatePreview {
    pub skill: String,
    pub id: String,
    pub installed_commit: Option<String>,
    pub latest_commit: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
    pub local_modified: bool,
}

/// Compare two file maps and classify the differences from the local view:
/// `added` (upstream only), `removed` (local only), `changed` (same path,
/// different content hash). All lists are sorted.
pub fn diff_file_maps(
    local: &BTreeMap<String, String>,
    upstream: &BTreeMap<String, String>,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let added = upstream
        .keys()
        .filter(|path| !local.contains_key(*path))
        .cloned()
        .collect();
    let removed = local
        .keys()
        .filter(|path| !upstream.contains_key(*path))
        .cloned()
        .collect();
    let changed = upstream
        .iter()
        .filter(|(path, hash)| local.get(*path).is_some_and(|local| local != *hash))
        .map(|(path, _)| path.clone())
        .collect();
    (added, removed, changed)
}

/// Was the installed copy edited since it was installed? Legacy entries without
/// a recorded hash are treated as unmodified.
fn is_locally_modified(recorded: Option<&String>, current: &str) -> bool {
    recorded.is_some_and(|hash| hash != current)
}

/// Preview a registry skill update without touching the installed copy.
pub fn preview_skill_update(skill: &str) -> Result<SkillUpdatePreview, String> {
    preview_skill_update_in(&crate::home_dir()?, skill)
}

pub(crate) fn preview_skill_update_in(
    home: &Path,
    skill: &str,
) -> Result<SkillUpdatePreview, String> {
    let aide_dir = home.join(".aide");
    let skills_dir = home.join(".agents").join("skills");
    let sources = read_sources(&aide_dir)?;
    let meta = sources
        .get(skill)
        .ok_or_else(|| format!("No install source recorded for \"{skill}\""))?
        .clone();
    let (owner, repo, only_skill) = parse_skill_id(&meta.id)?;
    let latest_commit = head_commit_cached(&aide_dir, &owner, &repo, only_skill.as_deref())?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = aide_dir.join("tmp").join(format!("skill-preview-{ts}"));
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

    let result = (|| {
        let archive = tmp.join("repo.tar.gz");
        download_repo_tarball(&owner, &repo, &latest_commit, &archive)?;
        let extract_dir = tmp.join("extract");
        fs::create_dir_all(&extract_dir).map_err(|e| e.to_string())?;
        extract_tarball(&archive, &extract_dir)?;
        let root = extract_root(&extract_dir)?;
        let upstream_dir = find_upstream_skill(&root, only_skill.as_deref(), &repo, skill)?;
        let upstream = crate::hash::file_map(&upstream_dir)?;

        let installed = skills_dir.join(skill);
        let local = if installed.is_dir() {
            crate::hash::file_map(&installed)?
        } else {
            BTreeMap::new()
        };
        let (added, removed, changed) = diff_file_maps(&local, &upstream);
        let current_hash = crate::hash::tree_hash_map(&local);
        Ok(SkillUpdatePreview {
            skill: skill.to_string(),
            id: meta.id.clone(),
            installed_commit: meta.commit.clone(),
            latest_commit,
            added,
            removed,
            changed,
            local_modified: is_locally_modified(meta.hash.as_ref(), &current_hash),
        })
    })();

    let _ = fs::remove_dir_all(&tmp);
    result
}

/// Locate the folder inside the downloaded repository that corresponds to the
/// installed skill: the requested id segment, the installed folder name, the
/// frontmatter name, or the derived destination name.
fn find_upstream_skill(
    root: &Path,
    only_skill: Option<&str>,
    repo: &str,
    installed: &str,
) -> Result<PathBuf, String> {
    let candidates = discover_skill_dirs(root);
    if candidates.is_empty() {
        return Err(format!("No skill folder found in {repo}"));
    }
    for candidate in &candidates {
        if let Some(segment) = only_skill {
            if dir_matches_skill(candidate, segment) {
                return Ok(candidate.clone());
            }
        }
        let dir_name = candidate
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if dir_name == installed {
            return Ok(candidate.clone());
        }
        if crate::skills::skill_frontmatter_name(candidate).as_deref() == Some(installed) {
            return Ok(candidate.clone());
        }
        if skill_dest_name(candidate, only_skill, repo).ok().as_deref() == Some(installed) {
            return Ok(candidate.clone());
        }
    }
    if candidates.len() == 1 {
        return Ok(candidates[0].clone());
    }
    Err(format!("Cannot locate skill \"{installed}\" in {repo}"))
}

// ── Default-home helpers ──

/// Install a skill into the real central store.
pub fn install_default(id: &str, force: bool) -> Result<RemoteInstallResult, String> {
    let home = crate::home_dir()?;
    let skills_dir = crate::skills::resolve_skills_source()?;
    std::fs::create_dir_all(&skills_dir).map_err(|e| e.to_string())?;
    install_from_id(id, &skills_dir, &home.join(".aide"), force)
}

/// Reinstall the skill recorded in `~/.aide/skill-sources.json`.
pub fn update_default(skill: &str) -> Result<RemoteInstallResult, String> {
    let aide_dir = crate::aide_dir()?;
    let sources = read_sources(&aide_dir)?;
    let meta = sources
        .get(skill)
        .ok_or_else(|| format!("No install source recorded for \"{skill}\""))?
        .clone();
    install_default(&meta.id, true)
}

/// Check the real provenance store for upstream updates.
pub fn check_updates_default() -> Result<Vec<SkillUpdate>, String> {
    check_updates(&crate::aide_dir()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skill_id_handles_all_forms() {
        assert_eq!(
            parse_skill_id("vercel-labs/agent-skills").unwrap(),
            ("vercel-labs".into(), "agent-skills".into(), None)
        );
        assert_eq!(
            parse_skill_id("vercel-labs/agent-skills@web-design-guidelines").unwrap(),
            (
                "vercel-labs".into(),
                "agent-skills".into(),
                Some("web-design-guidelines".into())
            )
        );
        assert_eq!(
            parse_skill_id("https://github.com/vercel-labs/skills/find-skills").unwrap(),
            (
                "vercel-labs".into(),
                "skills".into(),
                Some("find-skills".into())
            )
        );
        assert_eq!(
            parse_skill_id("https://github.com/owner/repo/tree/main").unwrap(),
            ("owner".into(), "repo".into(), None)
        );
        assert!(parse_skill_id("just-a-name").is_err());
    }

    #[test]
    fn discover_finds_root_skill_and_nested_skills() {
        let base = std::env::temp_dir().join(format!("aide-discover-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);

        // Root-level skill
        let root_skill = base.join("repo-root");
        fs::create_dir_all(&root_skill).unwrap();
        fs::write(root_skill.join("SKILL.md"), "x").unwrap();
        let found = discover_skill_dirs(&root_skill);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], root_skill);

        // Multi-skill repo
        let multi = base.join("repo-multi");
        fs::create_dir_all(multi.join("alpha")).unwrap();
        fs::create_dir_all(multi.join("beta")).unwrap();
        fs::create_dir_all(multi.join("not-a-skill")).unwrap();
        fs::write(multi.join("alpha").join("SKILL.md"), "x").unwrap();
        fs::write(multi.join("beta").join("SKILL.md"), "x").unwrap();
        let found = discover_skill_dirs(&multi);
        assert_eq!(found.len(), 2);
        assert!(found[0].ends_with("alpha"));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sources_round_trip() {
        let base = std::env::temp_dir().join(format!("aide-sources-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        record_source(
            &base,
            "my-skill",
            "owner/repo/my-skill",
            "owner/repo",
            Some("abc".into()),
            None,
        )
        .unwrap();
        let map = read_sources(&base).unwrap();
        let entry = map.get("my-skill").expect("source recorded");
        assert_eq!(entry.id, "owner/repo/my-skill");
        assert_eq!(entry.source, "owner/repo");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn skill_dest_name_prefers_frontmatter_then_id_then_repo() {
        let base = std::env::temp_dir().join(format!("aide-name-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);

        let from_frontmatter = base.join("repo-main");
        fs::create_dir_all(&from_frontmatter).unwrap();
        fs::write(
            from_frontmatter.join("SKILL.md"),
            "---\nname: find-skills\n---\nbody",
        )
        .unwrap();
        assert_eq!(
            skill_dest_name(&from_frontmatter, None, "repo").unwrap(),
            "find-skills"
        );

        // No frontmatter name: fall back to the requested skill segment.
        let no_frontmatter = base.join("some-dir");
        fs::create_dir_all(&no_frontmatter).unwrap();
        fs::write(no_frontmatter.join("SKILL.md"), "plain body").unwrap();
        assert_eq!(
            skill_dest_name(&no_frontmatter, Some("wanted-skill"), "repo").unwrap(),
            "wanted-skill"
        );
        // ... and finally to the repository name, never the archive root.
        assert_eq!(
            skill_dest_name(&no_frontmatter, None, "repo").unwrap(),
            "repo"
        );

        // A frontmatter name that tries to escape the skills directory is
        // ignored in favour of the safe fallbacks.
        let evil = base.join("evil");
        fs::create_dir_all(&evil).unwrap();
        fs::write(evil.join("SKILL.md"), "---\nname: ../escape\n---\n").unwrap();
        assert_eq!(
            skill_dest_name(&evil, Some("safe"), "repo").unwrap(),
            "safe"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn dir_matches_skill_checks_dir_and_frontmatter_name() {
        let base = std::env::temp_dir().join(format!("aide-match-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let dir = base.join("folder-name");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "---\nname: pretty-name\n---\n").unwrap();
        assert!(dir_matches_skill(&dir, "folder-name"));
        assert!(dir_matches_skill(&dir, "pretty-name"));
        assert!(!dir_matches_skill(&dir, "other"));
        let _ = fs::remove_dir_all(&base);
    }

    fn write_escaping_archive(dir: &Path) -> PathBuf {
        let path = dir.join("escape.tar.gz");
        let file = fs::File::create(&path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        // The tar builder refuses to write traversal paths, so craft the header
        // by hand to simulate a malicious archive.
        let mut header = tar::Header::new_gnu();
        header.set_size(4);
        header.set_mode(0o644);
        {
            let old = header.as_old_mut();
            let name = b"../escape.txt";
            old.name[..name.len()].copy_from_slice(name);
        }
        header.set_cksum();
        builder.append(&header, &b"nope"[..]).unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        path
    }

    fn write_test_archive(dir: &Path, entries: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join("test.tar.gz");
        let file = fs::File::create(&path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        for (name, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
        path
    }

    #[test]
    fn extract_tarball_stages_files_and_rejects_escaping_entries() {
        let base = std::env::temp_dir().join(format!("aide-tar-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let archive = write_test_archive(&base, &[("repo-main/SKILL.md", b"hi")]);
        let dest = base.join("out");
        fs::create_dir_all(&dest).unwrap();
        extract_tarball(&archive, &dest).unwrap();
        assert!(dest.join("repo-main/SKILL.md").is_file());

        let evil = write_escaping_archive(&base);
        let dest2 = base.join("out2");
        fs::create_dir_all(&dest2).unwrap();
        assert!(extract_tarball(&evil, &dest2).is_err());
        assert!(!base.join("escape.txt").exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn extraction_limits_reject_oversized_members() {
        assert!(check_extraction_limits(1024, 0).is_ok());
        assert!(check_extraction_limits(MAX_EXTRACTED_FILE_BYTES + 1, 0).is_err());
        assert!(check_extraction_limits(1024, MAX_EXTRACTED_TOTAL_BYTES).is_err());
    }

    #[test]
    fn read_sources_distinguishes_missing_from_corrupt() {
        let base = std::env::temp_dir().join(format!("aide-sources-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        assert!(read_sources(&base).unwrap().is_empty());
        fs::write(sources_path(&base), "{\"broken\":").unwrap();
        assert!(read_sources(&base).is_err());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn diff_file_maps_classifies_added_removed_and_changed() {
        let local = BTreeMap::from([
            ("SKILL.md".to_string(), "local-skill".to_string()),
            ("removed.txt".to_string(), "bye".to_string()),
            ("same.txt".to_string(), "same".to_string()),
        ]);
        let upstream = BTreeMap::from([
            ("SKILL.md".to_string(), "upstream-skill".to_string()),
            ("added.txt".to_string(), "hi".to_string()),
            ("same.txt".to_string(), "same".to_string()),
        ]);
        let (added, removed, changed) = diff_file_maps(&local, &upstream);
        assert_eq!(added, vec!["added.txt".to_string()]);
        assert_eq!(removed, vec!["removed.txt".to_string()]);
        assert_eq!(changed, vec!["SKILL.md".to_string()]);
        assert!(diff_file_maps(&upstream, &upstream).0.is_empty());
    }

    #[test]
    fn local_modified_uses_the_recorded_tree_hash() {
        let current = crate::hash::sha256_hex(b"installed");
        assert!(!is_locally_modified(None, &current)); // legacy entry
        assert!(!is_locally_modified(Some(&current), &current));
        let recorded = crate::hash::sha256_hex(b"other");
        assert!(is_locally_modified(Some(&recorded), &current));
    }

    #[test]
    fn preview_serializes_the_agreed_shape() {
        let preview = SkillUpdatePreview {
            skill: "demo".into(),
            id: "owner/repo/demo".into(),
            installed_commit: Some("abc".into()),
            latest_commit: "def".into(),
            added: vec!["a.txt".into()],
            removed: vec!["b.txt".into()],
            changed: vec!["SKILL.md".into()],
            local_modified: true,
        };
        let value = serde_json::to_value(&preview).unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "added",
                "changed",
                "id",
                "installed_commit",
                "latest_commit",
                "local_modified",
                "removed",
                "skill"
            ]
        );
        assert_eq!(value["installed_commit"], "abc");
        assert_eq!(value["local_modified"], true);
    }

    #[test]
    fn preview_requires_a_recorded_install_source() {
        let home = std::env::temp_dir().join(format!("aide-preview-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join(".aide")).unwrap();
        fs::create_dir_all(home.join(".agents/skills")).unwrap();
        let err = preview_skill_update_in(&home, "ghost").unwrap_err();
        assert!(err.contains("No install source recorded"), "{err}");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn urlencode_encodes_specials() {
        assert_eq!(urlencode("react native"), "react%20native");
        assert_eq!(urlencode("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn update_targets_dedupes_by_owner_repo_and_subtree() {
        let mut sources = HashMap::new();
        for (skill, id) in [
            ("a", "owner/repo/skill-a"),
            ("b", "owner/repo/skill-b"),
            ("c", "owner/repo/other-subtree"),
            ("d", "owner/other-repo"),
            ("e", "not-an-id"),
        ] {
            sources.insert(
                skill.to_string(),
                SkillSource {
                    id: id.to_string(),
                    source: "owner/repo".to_string(),
                    installed_at: 0,
                    commit: Some("sha".to_string()),
                    hash: None,
                },
            );
        }
        // A source without a recorded commit needs no lookup.
        sources.insert(
            "local".to_string(),
            SkillSource {
                id: "owner/repo/local".to_string(),
                source: "local".to_string(),
                installed_at: 0,
                commit: None,
                hash: None,
            },
        );
        let targets = update_targets(&sources);
        let keys: Vec<&str> = targets.iter().map(|(key, ..)| key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "owner/other-repo",
                "owner/repo/other-subtree",
                "owner/repo/skill-a",
                "owner/repo/skill-b",
            ]
        );
    }

    #[test]
    fn head_cache_serves_fresh_entries_and_refetches_expired_ones() {
        let mut cache = HashMap::new();
        let mut fetches = 0;
        let now = 10_000;

        let (first, changed) = cached_or_fetch_head("owner/repo", &mut cache, now, || {
            fetches += 1;
            Ok("sha-1".to_string())
        })
        .unwrap();
        assert_eq!(first, "sha-1");
        assert!(changed);

        // Inside the TTL the cached commit is returned without a network call.
        let (cached, changed) = cached_or_fetch_head("owner/repo", &mut cache, now + 60, || {
            fetches += 1;
            Ok("sha-2".to_string())
        })
        .unwrap();
        assert_eq!(cached, "sha-1");
        assert!(!changed);
        assert_eq!(fetches, 1);

        // At the TTL boundary the entry is refreshed.
        let (refreshed, changed) =
            cached_or_fetch_head("owner/repo", &mut cache, now + UPDATE_CACHE_TTL, || {
                fetches += 1;
                Ok("sha-2".to_string())
            })
            .unwrap();
        assert_eq!(refreshed, "sha-2");
        assert!(changed);
        assert_eq!(fetches, 2);

        // A failed lookup is not cached, so the next check retries it.
        assert!(cached_or_fetch_head("owner/broken", &mut cache, now, || {
            Err("github down".to_string())
        })
        .is_err());
        assert!(!cache.contains_key("owner/broken"));
    }

    #[test]
    fn head_cache_round_trips_and_ignores_a_corrupt_cache_file() {
        let base = std::env::temp_dir().join(format!("aide-head-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let mut cache = HashMap::new();
        cache.insert(
            "owner/repo".to_string(),
            HeadCacheEntry {
                commit: "abc".to_string(),
                fetched_at: 42,
            },
        );
        write_head_cache(&base, &cache).unwrap();
        let read = read_head_cache(&base);
        assert_eq!(
            read.get("owner/repo").map(|e| e.commit.as_str()),
            Some("abc")
        );
        assert_eq!(read.get("owner/repo").map(|e| e.fetched_at), Some(42));

        // Derived data: a broken cache is ignored, not fatal.
        fs::write(head_cache_path(&base), "{broken").unwrap();
        assert!(read_head_cache(&base).is_empty());
        let _ = fs::remove_dir_all(&base);
    }
}
