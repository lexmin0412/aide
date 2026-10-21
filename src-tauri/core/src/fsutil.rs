//! Small filesystem helpers shared by the skill store, git scopes and the
//! registry installer: safe recursive copies, directory swaps and skill
//! discovery. Nothing here knows about Tauri or about individual tools.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

/// Directories never copied into the central store or scanned for skills.
pub(crate) const IGNORED_DIRS: &[&str] =
    &["node_modules", ".git", ".pnpm", "dist", ".next", "target"];

/// Count files below `path`. Symlinks are not followed and never counted, so a
/// symlink cycle cannot make this recurse forever.
pub(crate) fn count_files(path: &Path) -> usize {
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
                if !IGNORED_DIRS.contains(&name.as_str()) {
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
        if file_type.is_dir() && IGNORED_DIRS.contains(&name.to_string_lossy().as_ref()) {
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

pub(crate) fn unique_stamp() -> String {
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

pub(crate) fn remove_path(path: &Path) {
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
pub(crate) fn find_skill_dirs(root: &Path) -> Vec<PathBuf> {
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
            if IGNORED_DIRS.contains(&name.as_str()) {
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
