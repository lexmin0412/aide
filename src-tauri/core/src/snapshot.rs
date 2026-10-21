//! Environment snapshots and rollback.
//!
//! A snapshot captures aide's central configuration files (`mcp.json`,
//! `models.json`, `remotes.json`, `skill-sources.json`, `mcp-managed.json`) plus
//! an inventory of the installed skills (name, symlink target, tree hash) under
//! `~/.aide/snapshots/<id>/`. Restoring copies the configuration files back
//! atomically and reports skill drift; skills themselves are never modified.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::storage;

/// Files captured by a snapshot (relative to `~/.aide`).
const CONFIG_FILES: &[&str] = &[
    "mcp.json",
    "models.json",
    "remotes.json",
    "skill-sources.json",
    "mcp-managed.json",
];
const SKILLS_INVENTORY: &str = "skills.json";
const SNAPSHOT_DIR: &str = "snapshots";
const RETENTION: usize = 20;
const MAX_LABEL_CHARS: usize = 80;
const DEFAULT_LABEL: &str = "manual";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub id: String,
    pub created_at: u64,
    pub label: String,
    pub files: Vec<String>,
    pub skill_count: u32,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotSkill {
    pub name: String,
    /// Symlink target for linked skills, `None` for real directories.
    #[serde(default)]
    pub target: Option<String>,
    /// Tree hash at snapshot time; empty when the tree could not be read.
    pub hash: String,
}

#[derive(Debug, Serialize)]
pub struct RestoreResult {
    pub restored_files: Vec<String>,
    pub skill_drift: Vec<String>,
    pub safety_snapshot: String,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Civil date/time from unix seconds, formatted `YYYYMMDD-HHMMSS` (UTC).
fn timestamp_prefix(secs: u64) -> String {
    let days = secs / 86_400;
    let seconds_of_day = secs % 86_400;
    let (hour, minute, second) = (
        seconds_of_day / 3_600,
        (seconds_of_day % 3_600) / 60,
        seconds_of_day % 60,
    );
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// Howard Hinnant's `civil_from_days` algorithm (proleptic Gregorian, UTC).
fn civil_from_days(days: i64) -> (i64, u64, u64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (year + i64::from(month <= 2), month as u64, day as u64)
}

/// Snapshot ids are strictly validated before they are joined into a path.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn snapshots_root(home: &Path) -> PathBuf {
    home.join(".aide").join(SNAPSHOT_DIR)
}

/// Make sure `~/.aide/snapshots` exists and cannot be a symlink escaping home.
fn ensure_snapshots_root(home: &Path) -> Result<PathBuf, String> {
    let aide_dir = home.join(".aide");
    fs::create_dir_all(&aide_dir).map_err(|e| format!("create {}: {e}", aide_dir.display()))?;
    let root = snapshots_root(home);
    match fs::symlink_metadata(&root) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let target = fs::canonicalize(&root)
                .map_err(|e| format!("{} is a dangling symlink: {e}", root.display()))?;
            let canonical_aide = fs::canonicalize(&aide_dir).unwrap_or_else(|_| aide_dir.clone());
            if !target.starts_with(&canonical_aide) {
                return Err(format!(
                    "{} is a symlink that escapes aide's directory; refusing to use it",
                    root.display()
                ));
            }
            Ok(root)
        }
        Ok(meta) if !meta.is_dir() => {
            Err(format!("{} exists but is not a directory", root.display()))
        }
        Ok(_) => Ok(root),
        Err(_) => {
            fs::create_dir_all(&root).map_err(|e| format!("create {}: {e}", root.display()))?;
            Ok(root)
        }
    }
}

/// Resolve a snapshot directory for read/restore/delete. Rejects traversal via
/// the id, symlinked snapshot directories, and anything outside the root.
fn snapshot_dir(home: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err(format!("Invalid snapshot id: {id:?}"));
    }
    let root = snapshots_root(home);
    let dir = root.join(id);
    match fs::symlink_metadata(&dir) {
        Ok(meta) if meta.is_symlink() => {
            Err(format!("Snapshot {id} is a symlink; refusing to use it"))
        }
        Ok(meta) if meta.is_dir() => {
            if let (Ok(canonical_dir), Ok(canonical_root)) =
                (fs::canonicalize(&dir), fs::canonicalize(&root))
            {
                if canonical_dir.starts_with(&canonical_root) {
                    return Ok(dir);
                }
            }
            Err(format!("Snapshot {id} is outside the snapshot directory"))
        }
        Ok(_) => Err(format!("Snapshot {id} is not a directory")),
        Err(_) => Err(format!("Snapshot {id} does not exist")),
    }
}

fn normalize_label(label: Option<&str>) -> String {
    let trimmed = label.unwrap_or("").trim();
    if trimmed.is_empty() {
        return DEFAULT_LABEL.to_string();
    }
    trimmed.chars().take(MAX_LABEL_CHARS).collect()
}

fn skill_inventory(home: &Path) -> Result<Vec<SnapshotSkill>, String> {
    let skills_dir = home.join(".agents").join("skills");
    let mut inventory = Vec::new();
    let Ok(entries) = fs::read_dir(&skills_dir) else {
        return Ok(inventory);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let target = fs::read_link(&path)
            .ok()
            .map(|target| target.to_string_lossy().to_string());
        let hash = crate::hash::tree_hash(&path).unwrap_or_default();
        inventory.push(SnapshotSkill { name, target, hash });
    }
    inventory.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(inventory)
}

/// Create a snapshot of the current environment.
pub fn create_snapshot(label: Option<&str>) -> Result<SnapshotMeta, String> {
    create_snapshot_in(&crate::home_dir()?, label)
}

pub(crate) fn create_snapshot_in(home: &Path, label: Option<&str>) -> Result<SnapshotMeta, String> {
    let root = ensure_snapshots_root(home)?;
    let aide_dir = home.join(".aide");
    let created_at = now_secs();
    let prefix = timestamp_prefix(created_at);

    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let mut attempts = 0u32;
    let (id, dir) = loop {
        let suffix = COUNTER.fetch_add(1, Ordering::Relaxed) & 0xffff;
        let candidate = format!("{prefix}-{suffix:04x}");
        let candidate_dir = root.join(&candidate);
        match fs::create_dir(&candidate_dir) {
            Ok(()) => break (candidate, candidate_dir),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists && attempts < 100 => {
                attempts += 1;
            }
            Err(err) => return Err(format!("create {}: {err}", candidate_dir.display())),
        }
    };

    let result = (|| {
        let mut files = Vec::new();
        let mut size_bytes = 0u64;
        for name in CONFIG_FILES {
            let source = aide_dir.join(name);
            if !source.is_file() {
                // Files absent from the environment are skipped.
                continue;
            }
            let bytes = fs::read(&source).map_err(|e| format!("read {}: {e}", source.display()))?;
            storage::atomic_write(&dir.join(name), &bytes)?;
            size_bytes += bytes.len() as u64;
            files.push((*name).to_string());
        }

        let inventory = skill_inventory(home)?;
        let inventory_path = dir.join(SKILLS_INVENTORY);
        storage::write_json(&inventory_path, &inventory, None)?;
        size_bytes += fs::metadata(&inventory_path).map(|m| m.len()).unwrap_or(0);
        files.push(SKILLS_INVENTORY.to_string());

        let meta = SnapshotMeta {
            id: id.clone(),
            created_at,
            label: normalize_label(label),
            files,
            skill_count: inventory.len() as u32,
            size_bytes,
        };
        storage::write_json(&dir.join("meta.json"), &meta, None)?;
        Ok(meta)
    })();

    match result {
        Ok(meta) => {
            prune_snapshots(home, &root);
            Ok(meta)
        }
        Err(err) => {
            let _ = fs::remove_dir_all(&dir);
            Err(err)
        }
    }
}

/// Keep only the newest `RETENTION` snapshots.
fn prune_snapshots(home: &Path, root: &Path) {
    let mut entries: Vec<(u64, String)> = list_snapshots_in(home)
        .unwrap_or_default()
        .into_iter()
        .map(|meta| (meta.created_at, meta.id))
        .collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    for (_, id) in entries.into_iter().skip(RETENTION) {
        let dir = root.join(id);
        if fs::symlink_metadata(&dir)
            .map(|meta| meta.is_dir() && !meta.is_symlink())
            .unwrap_or(false)
        {
            let _ = fs::remove_dir_all(&dir);
        }
    }
}

/// List snapshots, newest first.
pub fn list_snapshots() -> Result<Vec<SnapshotMeta>, String> {
    list_snapshots_in(&crate::home_dir()?)
}

pub(crate) fn list_snapshots_in(home: &Path) -> Result<Vec<SnapshotMeta>, String> {
    let root = snapshots_root(home);
    let mut snapshots = Vec::new();
    let Ok(entries) = fs::read_dir(&root) else {
        return Ok(snapshots);
    };
    for entry in entries.flatten() {
        let id = entry.file_name().to_string_lossy().to_string();
        if !valid_id(&id) {
            continue;
        }
        let path = entry.path();
        if !fs::symlink_metadata(&path)
            .map(|meta| meta.is_dir() && !meta.is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        let meta_path = path.join("meta.json");
        if let Ok(Some(meta)) = storage::read_json::<SnapshotMeta>(&meta_path) {
            snapshots.push(meta);
        }
    }
    snapshots.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    Ok(snapshots)
}

/// Restore the central configuration files from a snapshot.
pub fn restore_snapshot(id: &str) -> Result<RestoreResult, String> {
    restore_snapshot_in(&crate::home_dir()?, id)
}

pub(crate) fn restore_snapshot_in(home: &Path, id: &str) -> Result<RestoreResult, String> {
    let dir = snapshot_dir(home, id)?;
    // Reading the metadata validates the snapshot; a corrupt meta.json is an
    // error even when every config file is still readable.
    let _meta = storage::read_json::<SnapshotMeta>(&dir.join("meta.json"))?.unwrap_or_else(|| {
        SnapshotMeta {
            id: id.to_string(),
            created_at: 0,
            label: String::new(),
            files: Vec::new(),
            skill_count: 0,
            size_bytes: 0,
        }
    });

    // Read everything into memory before creating the safety snapshot: pruning
    // may evict the snapshot being restored, but its content is already here.
    let mut payload: Vec<(String, Vec<u8>)> = Vec::new();
    for name in CONFIG_FILES {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let bytes = fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        payload.push(((*name).to_string(), bytes));
    }
    let inventory: Vec<SnapshotSkill> =
        storage::read_json::<Vec<SnapshotSkill>>(&dir.join(SKILLS_INVENTORY))?.unwrap_or_default();

    let safety_label = format!("safety before restoring {id}");
    let safety = create_snapshot_in(home, Some(&safety_label))?;

    let aide_dir = home.join(".aide");
    let mut restored_files = Vec::new();
    for (name, bytes) in payload {
        storage::atomic_write(&aide_dir.join(&name), &bytes)?;
        restored_files.push(name);
    }

    let skills_dir = home.join(".agents").join("skills");
    let mut skill_drift = Vec::new();
    for entry in inventory {
        let current = crate::hash::tree_hash(&skills_dir.join(&entry.name)).ok();
        if current.as_deref() != Some(entry.hash.as_str()) {
            skill_drift.push(entry.name);
        }
    }
    skill_drift.sort();

    Ok(RestoreResult {
        restored_files,
        skill_drift,
        safety_snapshot: safety.id,
    })
}

/// Delete one snapshot directory.
pub fn delete_snapshot(id: &str) -> Result<(), String> {
    delete_snapshot_in(&crate::home_dir()?, id)
}

pub(crate) fn delete_snapshot_in(home: &Path, id: &str) -> Result<(), String> {
    let dir = snapshot_dir(home, id)?;
    fs::remove_dir_all(&dir).map_err(|e| format!("delete {}: {e}", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aide-snap-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_config(home: &Path, name: &str, json: &str) {
        fs::create_dir_all(home.join(".aide")).unwrap();
        fs::write(home.join(".aide").join(name), json).unwrap();
    }

    fn make_skill(home: &Path, name: &str) {
        let dir = home.join(".agents/skills").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), format!("---\nname: {name}\n---\n")).unwrap();
    }

    #[test]
    fn create_list_and_delete_on_an_empty_home() {
        let home = temp_home("empty");
        let meta = create_snapshot_in(&home, None).unwrap();
        assert!(valid_id(&meta.id), "invalid id {}", meta.id);
        assert_eq!(meta.label, "manual");
        assert_eq!(meta.skill_count, 0);
        assert_eq!(meta.files, vec!["skills.json".to_string()]);
        assert!(meta.size_bytes > 0);
        assert!(valid_id(&meta.id));
        assert_eq!(meta.id.len(), timestamp_prefix(meta.created_at).len() + 5);

        let listed = list_snapshots_in(&home).unwrap();
        assert_eq!(listed, vec![meta.clone()]);

        delete_snapshot_in(&home, &meta.id).unwrap();
        assert!(list_snapshots_in(&home).unwrap().is_empty());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn labels_are_trimmed_defaulted_and_truncated() {
        let home = temp_home("labels");
        let default = create_snapshot_in(&home, Some("   ")).unwrap();
        assert_eq!(default.label, "manual");
        let trimmed = create_snapshot_in(&home, Some("  before upgrade  ")).unwrap();
        assert_eq!(trimmed.label, "before upgrade");
        let long_label = "x".repeat(200);
        let long = create_snapshot_in(&home, Some(&long_label)).unwrap();
        assert_eq!(long.label.chars().count(), MAX_LABEL_CHARS);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn snapshot_captures_configs_and_skill_inventory() {
        let home = temp_home("capture");
        write_config(&home, "mcp.json", "{\"servers\":{}}");
        write_config(&home, "models.json", "{\"profiles\":[]}");
        make_skill(&home, "alpha");

        let meta = create_snapshot_in(&home, Some("capture")).unwrap();
        assert_eq!(meta.skill_count, 1);
        assert!(meta.files.contains(&"mcp.json".to_string()));
        assert!(meta.files.contains(&"models.json".to_string()));
        assert!(!meta.files.contains(&"remotes.json".to_string()));
        assert!(meta.files.contains(&"skills.json".to_string()));

        let dir = home.join(".aide/snapshots").join(&meta.id);
        let inventory: Vec<SnapshotSkill> = storage::read_json(&dir.join("skills.json"))
            .unwrap()
            .unwrap();
        assert_eq!(inventory.len(), 1);
        assert_eq!(inventory[0].name, "alpha");
        assert_eq!(
            inventory[0].hash,
            crate::hash::tree_hash(&home.join(".agents/skills/alpha")).unwrap()
        );
        let copied: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("mcp.json")).unwrap()).unwrap();
        assert_eq!(copied["servers"], serde_json::json!({}));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn retention_keeps_the_twenty_newest_snapshots() {
        let home = temp_home("retention");
        let mut ids = Vec::new();
        for index in 0..(RETENTION + 3) {
            let meta = create_snapshot_in(&home, Some(&format!("snap-{index}"))).unwrap();
            ids.push(meta.id);
        }
        let listed = list_snapshots_in(&home).unwrap();
        assert_eq!(listed.len(), RETENTION);
        assert!(!listed.iter().any(|meta| meta.id == ids[0]));
        assert!(!listed.iter().any(|meta| meta.id == ids[1]));
        assert!(listed.iter().any(|meta| meta.id == ids[RETENTION + 2]));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn invalid_snapshot_ids_are_rejected() {
        let home = temp_home("ids");
        let meta = create_snapshot_in(&home, None).unwrap();
        for id in ["..", "../evil", "a/b", "a\\b", "", "with space", "/etc"] {
            assert!(delete_snapshot_in(&home, id).is_err(), "id {id:?} accepted");
            assert!(
                restore_snapshot_in(&home, id).is_err(),
                "id {id:?} accepted"
            );
        }
        assert!(delete_snapshot_in(&home, "missing-id").is_err());
        // Directories with invalid names are ignored by the listing.
        fs::create_dir_all(home.join(".aide/snapshots/not an id")).unwrap();
        let listed = list_snapshots_in(&home).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, meta.id);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn restore_restores_config_and_reports_skill_drift() {
        let home = temp_home("restore");
        write_config(&home, "mcp.json", "{\"servers\":{\"old\":{}}}");
        make_skill(&home, "alpha");
        let snapshot = create_snapshot_in(&home, Some("before change")).unwrap();

        // Diverge the environment: config changed, skill tree edited.
        write_config(&home, "mcp.json", "{\"servers\":{\"new\":{}}}");
        fs::write(home.join(".agents/skills/alpha/extra.txt"), "local edit").unwrap();

        let restored = restore_snapshot_in(&home, &snapshot.id).unwrap();
        assert_eq!(restored.restored_files, vec!["mcp.json".to_string()]);
        assert_eq!(restored.skill_drift, vec!["alpha".to_string()]);
        let mcp: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(home.join(".aide/mcp.json")).unwrap())
                .unwrap();
        assert!(mcp["servers"].get("old").is_some());
        assert!(mcp["servers"].get("new").is_none());
        // Skills are never touched by a restore.
        assert!(home.join(".agents/skills/alpha/extra.txt").is_file());

        // A safety snapshot was taken before the restore.
        assert!(valid_id(&restored.safety_snapshot));
        let listed = list_snapshots_in(&home).unwrap();
        assert_eq!(listed.len(), 2);
        let safety = listed
            .iter()
            .find(|meta| meta.id == restored.safety_snapshot)
            .expect("safety snapshot");
        assert!(safety.label.starts_with("safety before restoring"));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn restore_skips_files_absent_from_the_snapshot() {
        let home = temp_home("absent");
        write_config(&home, "mcp.json", "{\"servers\":{}}");
        let snapshot = create_snapshot_in(&home, None).unwrap();
        // A models.json created after the snapshot is not deleted by a restore.
        write_config(&home, "models.json", "{\"profiles\":[]}");
        let restored = restore_snapshot_in(&home, &snapshot.id).unwrap();
        assert_eq!(restored.restored_files, vec!["mcp.json".to_string()]);
        assert!(home.join(".aide/models.json").is_file());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn restore_rejects_a_corrupt_snapshot() {
        let home = temp_home("corrupt");
        let meta = create_snapshot_in(&home, None).unwrap();
        fs::write(
            home.join(".aide/snapshots")
                .join(&meta.id)
                .join("meta.json"),
            "{broken",
        )
        .unwrap();
        assert!(restore_snapshot_in(&home, &meta.id).is_err());
        let _ = fs::remove_dir_all(&home);
    }
}
