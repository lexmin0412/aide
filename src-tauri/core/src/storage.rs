// Crash-safe persistence helpers shared by every module that writes user
// configuration. All writes are atomic (temp file + fsync + rename), every
// overwrite of an existing file is backed up first, and parsing distinguishes
// "missing" from "corrupt" so a damaged file is never silently replaced with
// defaults.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Write `bytes` to `path` atomically: a sibling temp file is fully written and
/// flushed, then renamed over the destination. Readers never observe a
/// half-written file. On unix the replacement keeps the previous file's mode
/// (owner-only 0600 for new files) so secrets never sit in a world-readable file.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    #[cfg(unix)]
    let mode = existing_mode(path).unwrap_or(0o600);
    let tmp = temp_sibling(path);
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&tmp)
            .map_err(|e| format!("create {}: {e}", tmp.display()))?
    };
    #[cfg(not(unix))]
    let mut file = File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
    file.write_all(bytes)
        .map_err(|e| format!("write {}: {e}", tmp.display()))?;
    file.sync_all()
        .map_err(|e| format!("fsync {}: {e}", tmp.display()))?;
    drop(file);
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            // Windows cannot rename over an existing file. Fall back to an
            // in-place copy so the write still lands, then clean up the temp.
            let copied = fs::copy(&tmp, path).and_then(|_| fs::remove_file(&tmp));
            if copied.is_ok() {
                Ok(())
            } else {
                let _ = fs::remove_file(&tmp);
                Err(format!("replace {}: {err}", path.display()))
            }
        }
    }
}

#[cfg(unix)]
fn existing_mode(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.permissions().mode() & 0o777)
}

fn temp_sibling(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "config".to_string());
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_name = format!(".{name}.{stamp}.aide-tmp");
    match path.parent() {
        Some(parent) => parent.join(tmp_name),
        None => PathBuf::from(tmp_name),
    }
}

/// Copy the current content of `path` into `backup_dir` before it is
/// overwritten. Returns the backup path when a file existed.
pub fn backup_file(path: &Path, backup_dir: &Path) -> Result<Option<PathBuf>, String> {
    if !path.exists() {
        return Ok(None);
    }
    fs::create_dir_all(backup_dir).map_err(|e| format!("create {}: {e}", backup_dir.display()))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "config".to_string());
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let target = backup_dir.join(format!("{name}.{stamp}.bak"));
    fs::copy(path, &target).map_err(|e| format!("backup {}: {e}", path.display()))?;
    Ok(Some(target))
}

/// Persist a value as pretty JSON: existing file is backed up first, then the
/// new content is written atomically. On unix the file is created 0600 because
/// these documents can contain API keys.
pub fn write_json<T: Serialize>(
    path: &Path,
    value: &T,
    backup_dir: Option<&Path>,
) -> Result<(), String> {
    let content = serde_json::to_string_pretty(value)
        .map_err(|e| format!("serialize {}: {e}", path.display()))?;
    write_bytes(path, content.as_bytes(), backup_dir)
}

/// Same as `write_json` for plain text.
pub fn write_bytes(path: &Path, bytes: &[u8], backup_dir: Option<&Path>) -> Result<(), String> {
    if let Some(dir) = backup_dir {
        backup_file(path, dir)?;
    }
    atomic_write(path, bytes)
}

/// Read and parse a JSON document, distinguishing a missing file from a
/// corrupt one. Callers must treat `Err` as "do not write": silently falling
/// back to defaults would overwrite a file the user can still repair.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if raw.trim().is_empty() {
        return Err(format!("{} is empty or truncated", path.display()));
    }
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|e| format!("parse {}: {e}", path.display()))
}

/// Make a file readable only by its owner (no-op on non-unix platforms).
pub fn restrict_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("chmod {}: {e}", path.display()))?;
    }
    Ok(())
}

/// Open a file for creation with owner-only permissions on unix.
pub fn create_private(path: &Path) -> Result<File, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aide-storage-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_temp_file() {
        let dir = temp_dir("atomic");
        let path = dir.join("config.json");
        fs::write(&path, "old").unwrap();
        atomic_write(&path, b"new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("aide-tmp"))
            .collect();
        assert!(leftovers.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_existing_mode_and_defaults_new_files_to_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("mode");
        let existing = dir.join("settings.json");
        fs::write(&existing, "{}").unwrap();
        fs::set_permissions(&existing, fs::Permissions::from_mode(0o644)).unwrap();
        atomic_write(&existing, b"{\"a\":1}").unwrap();
        let mode = fs::metadata(&existing).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "existing permissions must be preserved");

        let fresh = dir.join("models.json");
        atomic_write(&fresh, b"{}").unwrap();
        let mode = fs::metadata(&fresh).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "new config files must be owner-only");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_file_copies_existing_content() {
        let dir = temp_dir("backup");
        let path = dir.join("mcp.json");
        fs::write(&path, "{\"a\":1}").unwrap();
        let backup = backup_file(&path, &dir.join("backups")).unwrap().unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), "{\"a\":1}");
        assert!(backup_file(&dir.join("missing.json"), &dir.join("backups"))
            .unwrap()
            .is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_json_distinguishes_missing_from_corrupt() {
        let dir = temp_dir("read");
        let path = dir.join("remotes.json");
        assert!(read_json::<serde_json::Value>(&path).unwrap().is_none());
        fs::write(&path, "{\"broken\":").unwrap();
        assert!(read_json::<serde_json::Value>(&path).is_err());
        fs::write(&path, "   ").unwrap();
        assert!(read_json::<serde_json::Value>(&path).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_json_backs_up_previous_revision() {
        let dir = temp_dir("write");
        let path = dir.join("models.json");
        fs::write(&path, "{\"profiles\":[]}").unwrap();
        let backups = dir.join("backups");
        write_json(&path, &serde_json::json!({"profiles": [1]}), Some(&backups)).unwrap();
        assert!(fs::read_to_string(&path).unwrap().contains("profiles"));
        assert_eq!(fs::read_dir(&backups).unwrap().count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
