//! File-editing commands backing the built-in editor and file tree. Every
//! operation goes through the path policy in `crate::policy`, so the GUI can
//! never be tricked into writing outside the user's home or into credential
//! stores by a skill that renders untrusted content.

use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_at: u64,
    pub extension: Option<String>,
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

/// Read a UTF-8 text file through the path policy, refusing huge files so a
/// runaway log cannot be pulled into memory over IPC.
pub(crate) fn read_text_file_in(home: &Path, path: &Path) -> Result<String, String> {
    crate::policy::ensure_readable_in(home, path)?;
    let meta = fs::metadata(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if meta.len() > crate::policy::MAX_READ_BYTES {
        return Err(format!(
            "{} is too large to open ({:.1} MB; the limit is {} MB)",
            path.display(),
            meta.len() as f64 / (1024.0 * 1024.0),
            crate::policy::MAX_READ_BYTES / (1024 * 1024)
        ));
    }
    fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))
}

pub(crate) fn write_text_file_in(home: &Path, path: &Path, content: &str) -> Result<(), String> {
    let safe = crate::policy::ensure_writable_in(home, path)?;
    // Config files and skill sources are edited in-app; a crash mid-write must
    // never truncate them, so reuse the same atomic writer as the config stores.
    crate::storage::atomic_write(&safe, content.as_bytes())
}

pub(crate) fn create_file_in(home: &Path, path: &Path) -> Result<(), String> {
    let safe = crate::policy::ensure_writable_in(home, path)?;
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

pub(crate) fn create_directory_in(home: &Path, path: &Path) -> Result<(), String> {
    let safe = crate::policy::ensure_writable_in(home, path)?;
    match fs::symlink_metadata(&safe) {
        Ok(meta) if !meta.is_dir() => {
            return Err(format!("A file already exists at {}", safe.display()));
        }
        _ => {}
    }
    fs::create_dir_all(&safe).map_err(|e| format!("create {}: {e}", safe.display()))
}

pub(crate) fn delete_entry_in(home: &Path, path: &Path) -> Result<(), String> {
    let safe = crate::policy::ensure_writable_in(home, path)?;
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

pub(crate) fn rename_entry_in(home: &Path, old_path: &Path, new_path: &Path) -> Result<(), String> {
    let old_safe = crate::policy::ensure_writable_in(home, old_path)?;
    let new_safe = crate::policy::ensure_writable_in(home, new_path)?;
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

// ── Public file API (real home, path policy applied) ──

fn home() -> Result<std::path::PathBuf, String> {
    crate::policy::home_root()
}

/// List a directory for the file tree.
pub fn list_directory(path: &Path) -> Result<Vec<FileEntry>, String> {
    if !path.is_dir() {
        return Err(format!("Not a directory: {}", path.display()));
    }
    read_dir_entries(path).map_err(|e| e.to_string())
}

pub fn read_text_file(path: &Path) -> Result<String, String> {
    read_text_file_in(&home()?, path)
}

pub fn write_text_file(path: &Path, content: &str) -> Result<(), String> {
    write_text_file_in(&home()?, path, content)
}

pub fn create_file(path: &Path) -> Result<(), String> {
    create_file_in(&home()?, path)
}

pub fn create_directory(path: &Path) -> Result<(), String> {
    create_directory_in(&home()?, path)
}

pub fn delete_entry(path: &Path) -> Result<(), String> {
    delete_entry_in(&home()?, path)
}

pub fn rename_entry(old_path: &Path, new_path: &Path) -> Result<(), String> {
    rename_entry_in(&home()?, old_path, new_path)
}

pub fn file_exists(path: &Path) -> bool {
    path.exists()
}

/// File modification time in unix seconds, 0 when unknown.
pub fn file_mtime(path: &Path) -> Result<u64, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    Ok(meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0))
}
