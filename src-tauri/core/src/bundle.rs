//! Portable environment bundles (`.aidebundle`).
//!
//! A bundle is a gzipped tar containing a manifest, sanitized central configs
//! and the full contents of the installed skills, so an environment can move to
//! another machine in one file. API keys and secret-looking MCP values are
//! always redacted before export; the recorded paths make the omission visible.
//!
//! Import always takes a safety snapshot first. `merge` only adds missing
//! entries, while `replace` overwrites same-name skills (old copies go to the
//! system trash), MCP servers and model profiles; unrelated entries in the
//! central store are preserved.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

use crate::fsutil::IGNORED_DIRS;
use crate::mcp::{self, McpCentralConfig, McpManagedState};
use crate::models::{self, ModelProfile, ModelProfilesConfig};
use crate::registry;
use crate::snapshot;
use crate::storage;

pub const FORMAT_VERSION: u32 = 1;
/// Export caps for the uncompressed skill payload.
const MAX_SKILL_FILES: usize = 5000;
const MAX_SKILL_BYTES: u64 = 100 * 1024 * 1024;
/// Per-file cap while extracting a bundle.
const MAX_BUNDLE_FILE_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleSkill {
    pub name: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleProfile {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub models: Vec<String>,
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleManifest {
    pub format_version: u32,
    pub created_at: u64,
    pub aide_version: String,
    pub includes_secrets: bool,
    /// Git scopes (and their remote URLs) are never exported.
    #[serde(default)]
    pub git_scopes_exported: bool,
    pub skills: Vec<BundleSkill>,
    pub mcp_servers: Vec<String>,
    pub model_profiles: Vec<BundleProfile>,
    pub redacted_secrets: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BundleExportResult {
    pub path: String,
    pub skills: u32,
    pub mcp_servers: u32,
    pub model_profiles: u32,
    pub bytes: u64,
    pub redacted_secrets: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BundlePreview {
    pub format_version: u32,
    pub created_at: u64,
    pub aide_version: String,
    pub includes_secrets: bool,
    pub skill_count: u32,
    pub new_skills: Vec<String>,
    pub conflicting_skills: Vec<String>,
    pub new_mcp_servers: Vec<String>,
    pub conflicting_mcp_servers: Vec<String>,
    pub new_profiles: Vec<String>,
    pub conflicting_profiles: Vec<String>,
    pub redacted_secrets: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct BundleImportResult {
    pub skills_added: Vec<String>,
    pub skills_replaced: Vec<String>,
    pub mcp_added: Vec<String>,
    pub mcp_replaced: Vec<String>,
    pub profiles_added: Vec<String>,
    pub profiles_replaced: Vec<String>,
    pub safety_snapshot: String,
    pub warnings: Vec<String>,
}

// ── Names, secrets, limits ──

/// Entry names end up as path components, so they are strictly validated.
fn valid_entry_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|c| c == '/' || c == '\\' || c.is_control())
}

/// Conservative secret detector for MCP env/header keys.
fn secret_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    [
        "key",
        "token",
        "secret",
        "password",
        "authorization",
        "credential",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn sanitize_central(central: &McpCentralConfig) -> (McpCentralConfig, Vec<String>) {
    let mut sanitized = central.clone();
    let mut redacted = Vec::new();
    for (server_name, server) in sanitized.servers.iter_mut() {
        for (label, values) in [
            ("env", server.env.as_mut()),
            ("headers", server.headers.as_mut()),
        ] {
            if let Some(values) = values {
                for (key, value) in values.iter_mut() {
                    if secret_key(key) {
                        *value = String::new();
                        redacted.push(format!("mcp:{server_name}/{label}.{key}"));
                    }
                }
            }
        }
    }
    redacted.sort();
    redacted.dedup();
    (sanitized, redacted)
}

/// API keys are never exported, regardless of `include_secrets`.
fn sanitize_profiles(profiles: &ModelProfilesConfig) -> (ModelProfilesConfig, Vec<String>) {
    let mut sanitized = profiles.clone();
    let mut redacted = Vec::new();
    for profile in sanitized.profiles.iter_mut() {
        if !profile.api_key.is_empty() {
            profile.api_key = String::new();
            redacted.push(format!("model:{}/api_key", profile.id));
        }
    }
    (sanitized, redacted)
}

fn profile_summary(profile: &ModelProfile) -> BundleProfile {
    BundleProfile {
        id: profile.id.clone(),
        name: profile.name.clone(),
        provider: profile.provider.clone(),
        base_url: profile.base_url.clone(),
        model: profile.model.clone(),
        models: profile.models.clone(),
        targets: profile.targets.clone(),
    }
}

fn check_export_limits(files: usize, bytes: u64) -> Result<(), String> {
    if files > MAX_SKILL_FILES {
        return Err(format!(
            "Skills contain too many files ({files}); the bundle limit is {MAX_SKILL_FILES}"
        ));
    }
    if bytes > MAX_SKILL_BYTES {
        return Err(format!(
            "Skills exceed the {} MB bundle limit",
            MAX_SKILL_BYTES / (1024 * 1024)
        ));
    }
    Ok(())
}

fn dir_stats(dir: &Path) -> Result<(usize, u64), String> {
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries =
            fs::read_dir(&current).map_err(|e| format!("read {}: {e}", current.display()))?;
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !IGNORED_DIRS.contains(&name.as_str()) {
                    stack.push(path);
                }
            } else if file_type.is_file() {
                files += 1;
                bytes = bytes.saturating_add(entry.metadata().map(|m| m.len()).unwrap_or(0));
            }
        }
    }
    Ok((files, bytes))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn unique_stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn archive_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "bundle".to_string())
}

// ── Export ──

fn validate_export_dest(dest: &Path) -> Result<PathBuf, String> {
    if !dest.is_absolute() {
        return Err(format!(
            "Bundle destination must be absolute: {}",
            dest.display()
        ));
    }
    let parent = dest
        .parent()
        .ok_or_else(|| "Bundle destination has no parent directory".to_string())?;
    if !parent.is_dir() {
        return Err(format!(
            "Bundle destination directory does not exist: {}",
            parent.display()
        ));
    }
    if dest.is_dir() {
        return Err(format!(
            "Bundle destination is a directory: {}",
            dest.display()
        ));
    }
    Ok(dest.to_path_buf())
}

pub fn export_bundle(dest: &Path, include_secrets: bool) -> Result<BundleExportResult, String> {
    export_bundle_in(&crate::home_dir()?, dest, include_secrets)
}

pub(crate) fn export_bundle_in(
    home: &Path,
    dest: &Path,
    include_secrets: bool,
) -> Result<BundleExportResult, String> {
    let dest = validate_export_dest(dest)?;
    let aide_dir = home.join(".aide");
    let skills_dir = home.join(".agents").join("skills");

    // Read (and thereby validate) the central configs first: exporting a bundle
    // from a corrupt environment would produce something that cannot be imported.
    let central = mcp::read_central_at(&aide_dir.join("mcp.json"))?;
    let (sanitized_mcp, mut redacted) = sanitize_central(&central);
    let profiles = models::read_profiles(&aide_dir)?;
    let (sanitized_profiles, profile_redacted) = sanitize_profiles(&profiles);
    redacted.extend(profile_redacted);
    redacted.sort();
    redacted.dedup();

    let work_dir = aide_dir
        .join("tmp")
        .join(format!("bundle-export-{}", unique_stamp()));
    let staging = work_dir.join("skills");
    fs::create_dir_all(&staging).map_err(|e| format!("create {}: {e}", staging.display()))?;

    let result = (|| {
        let mut skills: Vec<BundleSkill> = Vec::new();
        let mut skill_dirs: Vec<(String, PathBuf)> = Vec::new();
        let mut total_files = 0usize;
        let mut total_bytes = 0u64;

        if skills_dir.is_dir() {
            let mut entries: Vec<_> = fs::read_dir(&skills_dir)
                .map_err(|e| format!("read {}: {e}", skills_dir.display()))?
                .flatten()
                .collect();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let name = entry.file_name().to_string_lossy().to_string();
                if !valid_entry_name(&name) {
                    continue;
                }
                let source = entry.path();
                if !source.is_dir() {
                    continue;
                }
                let staged = staging.join(&name);
                fsutil_copy(&source, &staged)?;
                let (files, bytes) = dir_stats(&staged)?;
                total_files += files;
                total_bytes = total_bytes.saturating_add(bytes);
                check_export_limits(total_files, total_bytes)?;
                let hash = crate::hash::tree_hash(&staged)?;
                skills.push(BundleSkill {
                    name: name.clone(),
                    hash,
                });
                skill_dirs.push((name, staged));
            }
        }

        let manifest = BundleManifest {
            format_version: FORMAT_VERSION,
            created_at: now_secs(),
            aide_version: env!("CARGO_PKG_VERSION").to_string(),
            includes_secrets: include_secrets,
            git_scopes_exported: false,
            mcp_servers: sanitized_mcp.servers.keys().cloned().collect(),
            model_profiles: sanitized_profiles
                .profiles
                .iter()
                .map(profile_summary)
                .collect(),
            redacted_secrets: redacted.clone(),
            skills,
        };

        // Only config files that exist in this environment are exported; an
        // import then leaves the target's own files alone.
        let mut config_entries: Vec<(String, Vec<u8>)> = Vec::new();
        if aide_dir.join("mcp.json").is_file() {
            config_entries.push((
                "config/mcp.json".to_string(),
                serde_json::to_vec_pretty(&sanitized_mcp).map_err(|e| e.to_string())?,
            ));
        }
        if aide_dir.join("models.json").is_file() {
            config_entries.push((
                "config/models.json".to_string(),
                serde_json::to_vec_pretty(&sanitized_profiles).map_err(|e| e.to_string())?,
            ));
        }
        let managed_path = aide_dir.join("mcp-managed.json");
        if managed_path.is_file() {
            let managed = mcp::read_managed_at(&managed_path)?;
            config_entries.push((
                "config/mcp-managed.json".to_string(),
                serde_json::to_vec_pretty(&managed).map_err(|e| e.to_string())?,
            ));
        }

        let temp = dest.with_file_name(format!(".{}.{}.tmp", archive_name(&dest), unique_stamp()));
        let write_result = (|| {
            let file =
                fs::File::create(&temp).map_err(|e| format!("create {}: {e}", temp.display()))?;
            let encoder = GzEncoder::new(file, Compression::default());
            let mut builder = tar::Builder::new(encoder);
            let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
            append_bytes(&mut builder, "manifest.json", &manifest_bytes)?;
            for (name, bytes) in &config_entries {
                append_bytes(&mut builder, name, bytes)?;
            }
            for (name, staged) in &skill_dirs {
                builder
                    .append_dir_all(format!("skills/{name}"), staged)
                    .map_err(|e| format!("archive skills/{name}: {e}"))?;
            }
            let encoder = builder
                .into_inner()
                .map_err(|e| format!("finish archive: {e}"))?;
            encoder
                .finish()
                .map_err(|e| format!("finish archive: {e}"))?;
            replace_file(&temp, &dest)
        })();
        if let Err(err) = write_result {
            let _ = fs::remove_file(&temp);
            return Err(err);
        }
        let bytes = fs::metadata(&dest).map(|meta| meta.len()).unwrap_or(0);
        Ok(BundleExportResult {
            path: dest.to_string_lossy().to_string(),
            skills: manifest.skills.len() as u32,
            mcp_servers: manifest.mcp_servers.len() as u32,
            model_profiles: manifest.model_profiles.len() as u32,
            bytes,
            redacted_secrets: redacted,
        })
    })();

    let _ = fs::remove_dir_all(&work_dir);
    result
}

fn fsutil_copy(source: &Path, dest: &Path) -> Result<(), String> {
    crate::fsutil::copy_dir_recursive(source, dest)
        .map_err(|e| format!("copy {}: {e}", source.display()))
}

fn append_bytes<W: Write>(
    builder: &mut tar::Builder<W>,
    name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, name, bytes)
        .map_err(|e| format!("archive {name}: {e}"))
}

/// Rename over the destination, falling back to copy+remove on Windows.
fn replace_file(temp: &Path, dest: &Path) -> Result<(), String> {
    match fs::rename(temp, dest) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(temp, dest).map_err(|e| format!("write {}: {e}", dest.display()))?;
            fs::remove_file(temp).map_err(|e| format!("remove {}: {e}", temp.display()))
        }
    }
}

// ── Extraction and manifest reading (preview + import) ──

/// Extract a bundle into a fresh temp directory. Returns the temp root; the
/// payload lives in `<root>/extract` and the caller removes `<root>`.
fn extract_bundle_to_temp(home: &Path, path: &Path, tag: &str) -> Result<PathBuf, String> {
    if !path.is_file() {
        return Err(format!("Bundle not found: {}", path.display()));
    }
    let root = home
        .join(".aide")
        .join("tmp")
        .join(format!("bundle-{tag}-{}", unique_stamp()));
    let extract = root.join("extract");
    fs::create_dir_all(&extract).map_err(|e| format!("create {}: {e}", extract.display()))?;
    let extracted = registry::extract_tarball_limited(
        path,
        &extract,
        MAX_SKILL_FILES,
        MAX_SKILL_BYTES,
        MAX_BUNDLE_FILE_BYTES,
    );
    match extracted {
        Ok(()) => Ok(root),
        Err(err) => {
            let _ = fs::remove_dir_all(&root);
            Err(err)
        }
    }
}

fn read_manifest(extract: &Path) -> Result<BundleManifest, String> {
    let manifest_path = extract.join("manifest.json");
    let manifest = storage::read_json::<BundleManifest>(&manifest_path)?
        .ok_or_else(|| "Not an aide bundle: manifest.json is missing".to_string())?;
    if manifest.format_version != FORMAT_VERSION {
        return Err(format!(
            "Unsupported bundle format version {} (expected {FORMAT_VERSION})",
            manifest.format_version
        ));
    }
    for skill in &manifest.skills {
        if !valid_entry_name(&skill.name) {
            return Err(format!(
                "Bundle contains an invalid skill name: {:?}",
                skill.name
            ));
        }
    }
    for server in &manifest.mcp_servers {
        if !valid_entry_name(server) {
            return Err(format!(
                "Bundle contains an invalid MCP server name: {:?}",
                server
            ));
        }
    }
    for profile in &manifest.model_profiles {
        if !valid_entry_name(&profile.id) {
            return Err(format!(
                "Bundle contains an invalid profile id: {:?}",
                profile.id
            ));
        }
    }
    Ok(manifest)
}

fn read_bundle_mcp(extract: &Path) -> Result<Option<McpCentralConfig>, String> {
    let path = extract.join("config/mcp.json");
    if !path.is_file() {
        return Ok(None);
    }
    let config = storage::read_json::<McpCentralConfig>(&path)?
        .ok_or_else(|| "Bundle config/mcp.json is empty".to_string())?;
    for name in config.servers.keys() {
        if !valid_entry_name(name) {
            return Err(format!(
                "Bundle contains an invalid MCP server name: {:?}",
                name
            ));
        }
    }
    Ok(Some(config))
}

fn read_bundle_models(extract: &Path) -> Result<Option<ModelProfilesConfig>, String> {
    let path = extract.join("config/models.json");
    if !path.is_file() {
        return Ok(None);
    }
    let config = storage::read_json::<ModelProfilesConfig>(&path)?
        .ok_or_else(|| "Bundle config/models.json is empty".to_string())?;
    for profile in &config.profiles {
        if !valid_entry_name(&profile.id) {
            return Err(format!(
                "Bundle contains an invalid profile id: {:?}",
                profile.id
            ));
        }
    }
    Ok(Some(config))
}

fn read_bundle_managed(extract: &Path) -> Result<Option<McpManagedState>, String> {
    let path = extract.join("config/mcp-managed.json");
    if !path.is_file() {
        return Ok(None);
    }
    storage::read_json::<McpManagedState>(&path)
}

fn current_skill_names(home: &Path) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let Ok(entries) = fs::read_dir(home.join(".agents").join("skills")) else {
        return names;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if entry.path().is_dir() && valid_entry_name(&name) {
            names.insert(name);
        }
    }
    names
}

// ── Preview ──

pub fn preview_bundle(path: &Path) -> Result<BundlePreview, String> {
    preview_bundle_in(&crate::home_dir()?, path)
}

pub(crate) fn preview_bundle_in(home: &Path, path: &Path) -> Result<BundlePreview, String> {
    let root = extract_bundle_to_temp(home, path, "preview")?;
    let extract = root.join("extract");
    let result = (|| {
        let manifest = read_manifest(&extract)?;
        let bundle_mcp = read_bundle_mcp(&extract)?;
        let bundle_models = read_bundle_models(&extract)?;
        let current_mcp = mcp::read_central_at(&home.join(".aide").join("mcp.json"))?;
        let current_models = models::read_profiles(&home.join(".aide"))?;
        let current_skills = current_skill_names(home);

        let mut warnings = Vec::new();
        for skill in &manifest.skills {
            let dir = extract.join("skills").join(&skill.name);
            if !dir.is_dir() {
                warnings.push(format!(
                    "Skill payload is missing from the bundle: {}",
                    skill.name
                ));
                continue;
            }
            let actual = crate::hash::tree_hash(&dir)?;
            if actual != skill.hash {
                warnings.push(format!(
                    "Skill {} hash differs from the manifest",
                    skill.name
                ));
            }
        }

        let mut new_skills = Vec::new();
        let mut conflicting_skills = Vec::new();
        for skill in &manifest.skills {
            if current_skills.contains(&skill.name) {
                conflicting_skills.push(skill.name.clone());
            } else {
                new_skills.push(skill.name.clone());
            }
        }

        let (new_mcp_servers, conflicting_mcp_servers) = match &bundle_mcp {
            Some(bundle) => {
                let mut new = Vec::new();
                let mut conflicting = Vec::new();
                for name in bundle.servers.keys() {
                    if current_mcp.servers.contains_key(name) {
                        conflicting.push(name.clone());
                    } else {
                        new.push(name.clone());
                    }
                }
                (new, conflicting)
            }
            None => (Vec::new(), Vec::new()),
        };

        let (new_profiles, conflicting_profiles) = match &bundle_models {
            Some(bundle) => {
                let mut new = Vec::new();
                let mut conflicting = Vec::new();
                for profile in &bundle.profiles {
                    if current_models
                        .profiles
                        .iter()
                        .any(|existing| existing.id == profile.id)
                    {
                        conflicting.push(profile.id.clone());
                    } else {
                        new.push(profile.id.clone());
                    }
                }
                (new, conflicting)
            }
            None => (Vec::new(), Vec::new()),
        };

        Ok(BundlePreview {
            format_version: manifest.format_version,
            created_at: manifest.created_at,
            aide_version: manifest.aide_version,
            includes_secrets: manifest.includes_secrets,
            skill_count: manifest.skills.len() as u32,
            new_skills,
            conflicting_skills,
            new_mcp_servers,
            conflicting_mcp_servers,
            new_profiles,
            conflicting_profiles,
            redacted_secrets: manifest.redacted_secrets,
            warnings,
        })
    })();
    let _ = fs::remove_dir_all(&root);
    result
}

// ── Import ──

#[cfg(test)]
thread_local! {
    static TRASH_HOOK: std::cell::RefCell<Option<fn(&Path) -> Result<(), String>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn set_trash_hook(hook: Option<fn(&Path) -> Result<(), String>>) {
    TRASH_HOOK.with(|slot| *slot.borrow_mut() = hook);
}

fn trash_path(path: &Path) -> Result<(), String> {
    #[cfg(test)]
    {
        if let Some(hook) = TRASH_HOOK.with(|slot| *slot.borrow()) {
            return hook(path);
        }
    }
    trash::delete(path).map_err(|e| {
        format!(
            "Could not move {} to the trash; import aborted ({e})",
            path.display()
        )
    })
}

pub fn import_bundle(path: &Path, mode: &str) -> Result<BundleImportResult, String> {
    import_bundle_in(&crate::home_dir()?, path, mode)
}

pub(crate) fn import_bundle_in(
    home: &Path,
    path: &Path,
    mode: &str,
) -> Result<BundleImportResult, String> {
    let replace = match mode {
        "merge" => false,
        "replace" => true,
        other => {
            return Err(format!(
                "Invalid import mode {other:?}; expected \"merge\" or \"replace\""
            ))
        }
    };
    let archive = archive_name(path);
    let root = extract_bundle_to_temp(home, path, "import")?;
    let extract = root.join("extract");
    let result = (|| {
        let manifest = read_manifest(&extract)?;
        let bundle_mcp = read_bundle_mcp(&extract)?;
        let bundle_models = read_bundle_models(&extract)?;
        let bundle_managed = read_bundle_managed(&extract)?;

        let aide_dir = home.join(".aide");
        let backups = aide_dir.join("backups");
        let mcp_path = aide_dir.join("mcp.json");
        let models_path = aide_dir.join("models.json");
        let managed_path = aide_dir.join("mcp-managed.json");
        // Read current state before mutating anything: a corrupt central config
        // must abort the import instead of being replaced by bundle defaults.
        let mut current_mcp = mcp::read_central_at(&mcp_path)?;
        let mut current_models = models::read_profiles(&aide_dir)?;
        let mut current_managed = mcp::read_managed_at(&managed_path)?;
        let skills_dir = home.join(".agents").join("skills");

        let safety = snapshot::create_snapshot_in(home, Some("safety before bundle import"))?;
        let mut outcome = BundleImportResult {
            safety_snapshot: safety.id,
            ..Default::default()
        };

        for skill in &manifest.skills {
            let source = extract.join("skills").join(&skill.name);
            if !source.is_dir() {
                outcome
                    .warnings
                    .push(format!("Skill payload is missing: {}", skill.name));
                continue;
            }
            let actual_hash = crate::hash::tree_hash(&source)?;
            if actual_hash != skill.hash {
                outcome.warnings.push(format!(
                    "Skill {} hash differs from the manifest",
                    skill.name
                ));
            }
            let dest = skills_dir.join(&skill.name);
            let exists = fs::symlink_metadata(&dest).is_ok();
            if exists && !replace {
                outcome.warnings.push(format!(
                    "Skill already exists, kept the current version: {}",
                    skill.name
                ));
                continue;
            }
            if exists {
                // Old copy goes to the trash before the new one is staged in.
                trash_path(&dest)?;
                fsutil_copy(&source, &dest)?;
                outcome.skills_replaced.push(skill.name.clone());
            } else {
                fs::create_dir_all(&skills_dir)
                    .map_err(|e| format!("create {}: {e}", skills_dir.display()))?;
                fsutil_copy(&source, &dest)?;
                outcome.skills_added.push(skill.name.clone());
            }
            let provenance = format!("bundle:{archive}");
            let _ = registry::record_source(
                &aide_dir,
                &skill.name,
                &provenance,
                &provenance,
                None,
                Some(skill.hash.clone()),
            );
        }

        if let Some(bundle) = bundle_mcp {
            let mut names: Vec<&String> = bundle.servers.keys().collect();
            names.sort();
            for name in names {
                let exists = current_mcp.servers.contains_key(name);
                if exists && !replace {
                    outcome.warnings.push(format!(
                        "MCP server already exists, kept the current one: {name}"
                    ));
                    continue;
                }
                let server = bundle.servers[name].clone();
                current_mcp.servers.insert(name.clone(), server);
                if exists {
                    outcome.mcp_replaced.push(name.clone());
                } else {
                    outcome.mcp_added.push(name.clone());
                }
            }
            storage::write_json(&mcp_path, &current_mcp, Some(&backups))?;
            storage::restrict_permissions(&mcp_path)?;
        }

        if let Some(bundle) = bundle_models {
            for profile in bundle.profiles {
                let existing = current_models
                    .profiles
                    .iter()
                    .position(|current| current.id == profile.id);
                match existing {
                    Some(index) if replace => {
                        current_models.profiles[index] = profile.clone();
                        outcome.profiles_replaced.push(profile.id);
                    }
                    Some(_) => outcome.warnings.push(format!(
                        "Model profile already exists, kept the current one: {}",
                        profile.id
                    )),
                    None => {
                        outcome.profiles_added.push(profile.id.clone());
                        current_models.profiles.push(profile);
                    }
                }
            }
            storage::write_json(&models_path, &current_models, Some(&backups))?;
            storage::restrict_permissions(&models_path)?;
        }

        if let Some(bundle) = bundle_managed {
            // The manifest tracks what aide wrote; never shrink it on import.
            for (tool, names) in bundle.tools {
                let entry = current_managed.tools.entry(tool).or_default();
                for name in names {
                    if !entry.contains(&name) {
                        entry.push(name);
                    }
                }
                entry.sort();
            }
            storage::write_json(&managed_path, &current_managed, Some(&backups))?;
        }

        outcome.skills_added.sort();
        outcome.skills_replaced.sort();
        outcome.mcp_added.sort();
        outcome.mcp_replaced.sort();
        outcome.profiles_added.sort();
        outcome.profiles_replaced.sort();
        Ok(outcome)
    })();
    let _ = fs::remove_dir_all(&root);
    result
}

#[cfg(test)]
mod tests;
