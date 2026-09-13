// skills.sh registry integration: search public skills and install them from
// GitHub archives. The search endpoint is the same unauthenticated one the
// official `npx skills` CLI uses.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const SEARCH_ENDPOINT: &str = "https://skills.sh/api/search";
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_DOWNLOAD_BYTES: u64 = 25 * 1024 * 1024;
const MAX_EXTRACTED_FILES: usize = 2000;

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

pub fn read_sources(aide_dir: &Path) -> HashMap<String, SkillSource> {
    let path = sources_path(aide_dir);
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

pub fn record_source(aide_dir: &Path, skill: &str, id: &str, source: &str) -> Result<(), String> {
    let mut map = read_sources(aide_dir);
    map.insert(
        skill.to_string(),
        SkillSource {
            id: id.to_string(),
            source: source.to_string(),
            installed_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        },
    );
    fs::create_dir_all(aide_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?;
    fs::write(sources_path(aide_dir), json).map_err(|e| e.to_string())
}

fn download_repo_tarball(owner: &str, repo: &str, dest: &Path) -> Result<(), String> {
    let url = format!("https://codeload.github.com/{owner}/{repo}/tar.gz/HEAD");
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

fn extract_tarball(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    let mut count = 0usize;
    for entry in archive.entries().map_err(|e| format!("Invalid archive: {e}"))? {
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
        entry
            .unpack_in(dest)
            .map_err(|e| format!("Unsafe archive entry: {e}"))?;
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

/// Install one or more skills from a GitHub repository into `skills_dir`,
/// recording provenance under `aide_dir`. Existing skills are skipped.
pub fn install_from_id(
    id: &str,
    skills_dir: &Path,
    aide_dir: &Path,
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
        let archive = tmp.join("repo.tar.gz");
        download_repo_tarball(&owner, &repo, &archive)?;
        let extract_dir = tmp.join("extract");
        fs::create_dir_all(&extract_dir).map_err(|e| e.to_string())?;
        extract_tarball(&archive, &extract_dir)?;
        let root = extract_root(&extract_dir)?;

        let mut candidates = discover_skill_dirs(&root);
        if let Some(name) = &only_skill {
            candidates.retain(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy() == *name)
                    .unwrap_or(false)
            });
            if candidates.is_empty() {
                return Err(format!("Skill \"{name}\" not found in {owner}/{repo}"));
            }
        }

        let mut installed = Vec::new();
        let mut skipped = Vec::new();
        for src in candidates {
            let name = src
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .ok_or("Invalid skill folder name")?;
            let dest = skills_dir.join(&name);
            if dest.exists() {
                skipped.push(name);
                continue;
            }
            crate::copy_dir_recursive(&src, &dest)
                .map_err(|e| format!("Failed to copy \"{name}\": {e}"))?;
            record_source(aide_dir, &name, id, &format!("{owner}/{repo}"))?;
            installed.push(name);
        }
        Ok(RemoteInstallResult { installed, skipped })
    })();

    let _ = fs::remove_dir_all(&tmp);
    result
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
            ("vercel-labs".into(), "skills".into(), Some("find-skills".into()))
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
        record_source(&base, "my-skill", "owner/repo/my-skill", "owner/repo").unwrap();
        let map = read_sources(&base);
        let entry = map.get("my-skill").expect("source recorded");
        assert_eq!(entry.id, "owner/repo/my-skill");
        assert_eq!(entry.source, "owner/repo");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn urlencode_encodes_specials() {
        assert_eq!(urlencode("react native"), "react%20native");
        assert_eq!(urlencode("a-b_c.d~e"), "a-b_c.d~e");
    }
}
