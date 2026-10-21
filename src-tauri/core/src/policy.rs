//! Filesystem policy for the editor-facing file commands.
//!
//! The GUI renders content from skills installed out of untrusted GitHub
//! repositories, so the raw file commands must not behave like a generic
//! "read and write any path" primitive. Every request is resolved through its
//! nearest existing ancestor and must land below the user's home directory. The
//! one exception is a symlinked skill folder: entries under `~/.agents/skills`
//! often point at a checkout outside home, and editing through that link is
//! allowed because the link itself lives inside the allowed root.
//!
//! A short denylist keeps credential stores off limits even inside home.

use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Files larger than this are refused by `read_text_file`.
pub const MAX_READ_BYTES: u64 = 5 * 1024 * 1024;

/// Paths relative to home whose whole subtree is off limits. Matched
/// case-insensitively because macOS usually spells "Library" capitalized.
const SENSITIVE_DIRS: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".azure",
    ".config/gcloud",
    "library/keychains",
];

/// Paths relative to home that are individually off limits.
const SENSITIVE_FILES: &[&str] = &[".netrc", ".npmrc", ".kube/config", ".docker/config.json"];

/// Symlinked skill folders live here.
const SKILLS_RELATIVE: &str = ".agents/skills";

/// The current user's home directory, canonicalized.
pub fn home_root() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())?;
    canonical_home(&home)
}

fn canonical_home(home: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(home)
        .map_err(|e| format!("Cannot resolve home directory {}: {e}", home.display()))
}

/// Validate a path that will be created or modified.
///
/// Returns the absolute lexical path to operate on: writing through it keeps
/// symlinked skill folders intact instead of replacing them with a real file.
pub fn ensure_writable(path: &Path) -> Result<PathBuf, String> {
    ensure_writable_in(&home_root()?, path)
}

/// Validate a path that will be read.
pub fn ensure_readable(path: &Path) -> Result<(), String> {
    ensure_readable_in(&home_root()?, path)
}

/// Same as `ensure_writable` with an injectable home root (tests, tooling).
pub fn ensure_writable_in(home: &Path, path: &Path) -> Result<PathBuf, String> {
    check_path(home, path)
}

/// Same as `ensure_readable` with an injectable home root (tests, tooling).
pub fn ensure_readable_in(home: &Path, path: &Path) -> Result<(), String> {
    check_path(home, path).map(|_| ())
}

fn check_path(home: &Path, path: &Path) -> Result<PathBuf, String> {
    let lexical_home = absolute_lexical(home)?;
    let canonical_home = canonical_home(home)?;
    let absolute = absolute_lexical(path)?;

    let resolved = resolve_existing_prefix(&absolute)?;
    // A sensitive location reached through a symlink is still sensitive.
    if let Some(denied) = denied_relative(&canonical_home, &resolved) {
        return Err(sensitive_error(&absolute, &denied));
    }
    if resolved.starts_with(&canonical_home) {
        return Ok(absolute);
    }
    // Outside home: only a symlink that lives under ~/.agents/skills may point
    // there (skill folders are commonly linked to a dev checkout).
    if skill_symlink_ancestor(&lexical_home, &absolute).is_some() {
        return Ok(absolute);
    }
    Err(format!(
        "Refusing to access \"{}\": file paths must stay inside the home directory",
        absolute.display()
    ))
}

fn sensitive_error(path: &Path, denied: &str) -> String {
    format!(
        "Refusing to access \"{}\": \"{denied}\" holds credentials and is off limits to aide",
        path.display()
    )
}

/// Reject relative paths and any `..` component, then normalize away `.`
/// components. Running the check before anything touches the filesystem keeps
/// traversal attempts from escaping the allowed roots.
fn absolute_lexical(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!(
            "Refusing to access \"{}\": path must be absolute",
            path.display()
        ));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(format!(
                    "Refusing to access \"{}\": \"..\" is not allowed in file paths",
                    path.display()
                ))
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    Ok(normalized)
}

/// Canonicalize the longest existing prefix of `path` and append the parts that
/// do not exist yet, so a new file can be checked before it is created.
fn resolve_existing_prefix(path: &Path) -> Result<PathBuf, String> {
    let mut ancestor = path.to_path_buf();
    let mut missing: Vec<OsString> = Vec::new();
    loop {
        if fs::symlink_metadata(&ancestor).is_ok() {
            let mut resolved = fs::canonicalize(&ancestor)
                .map_err(|e| format!("Cannot resolve {}: {e}", ancestor.display()))?;
            for part in missing.iter().rev() {
                resolved.push(part);
            }
            return Ok(resolved);
        }
        match ancestor.file_name() {
            Some(name) => {
                missing.push(name.to_os_string());
                if !ancestor.pop() {
                    break;
                }
            }
            None => break,
        }
    }
    Err(format!("Cannot resolve {}", path.display()))
}

/// Return the denied denylist entry when `path` (canonical) is inside one.
fn denied_relative(home: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(home).ok()?;
    let lowered = relative
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    for dir in SENSITIVE_DIRS {
        if lowered == *dir || lowered.starts_with(&format!("{dir}/")) {
            return Some((*dir).to_string());
        }
    }
    for file in SENSITIVE_FILES {
        if lowered == *file {
            return Some((*file).to_string());
        }
    }
    None
}

/// Find a symlink at or above `path` that is still located under
/// `<home>/.agents/skills`. Only such links may resolve outside home.
fn skill_symlink_ancestor(home: &Path, path: &Path) -> Option<PathBuf> {
    let skills_root = home.join(SKILLS_RELATIVE);
    let mut current = path.to_path_buf();
    while current.starts_with(&skills_root) {
        if let Ok(meta) = fs::symlink_metadata(&current) {
            if meta.file_type().is_symlink() {
                return Some(current);
            }
        }
        if current == skills_root {
            return None;
        }
        if !current.pop() {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aide-policy-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn allows_paths_below_home() {
        let home = temp_home("allow");
        let file = home.join("skills/demo/SKILL.md");
        assert_eq!(ensure_writable_in(&home, &file).unwrap(), file);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "x").unwrap();
        assert!(ensure_readable_in(&home, &file).is_ok());
        // Home itself is inside the allowed root.
        assert!(ensure_writable_in(&home, &home).is_ok());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn rejects_paths_outside_home() {
        let home = temp_home("outside");
        let outside = std::env::temp_dir().join(format!("aide-policy-out-{}", std::process::id()));
        fs::create_dir_all(&outside).unwrap();
        let err = ensure_writable_in(&home, &outside.join("evil.txt")).unwrap_err();
        assert!(err.contains("inside the home directory"), "{err}");
        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn rejects_parent_dir_traversal() {
        let home = temp_home("traversal");
        let sneaky = home.join("..").join("escaped.txt");
        let err = ensure_writable_in(&home, &sneaky).unwrap_err();
        assert!(err.contains(".."), "{err}");
        // A plain relative path is refused too.
        assert!(ensure_writable_in(&home, Path::new("relative.txt")).is_err());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn rejects_sensitive_paths_inside_home() {
        let home = temp_home("deny");
        fs::create_dir_all(home.join(".ssh")).unwrap();
        fs::write(home.join(".ssh/id_ed25519"), "key").unwrap();
        let err = ensure_writable_in(&home, &home.join(".ssh/id_ed25519")).unwrap_err();
        assert!(err.contains("off limits"), "{err}");
        assert!(ensure_readable_in(&home, &home.join(".ssh/id_ed25519")).is_err());
        // A sensitive file that does not exist yet is refused as well.
        assert!(ensure_writable_in(&home, &home.join(".aws/credentials")).is_err());
        assert!(ensure_writable_in(&home, &home.join(".netrc")).is_err());
        assert!(
            ensure_writable_in(&home, &home.join("Library/Keychains/login.keychain-db")).is_err()
        );
        // Normal dotfiles stay writable.
        assert!(ensure_writable_in(&home, &home.join(".zshrc")).is_ok());
        let _ = fs::remove_dir_all(&home);
    }

    #[cfg(unix)]
    #[test]
    fn allows_symlinked_skill_folders_outside_home() {
        let home = temp_home("skill-link");
        let repo = std::env::temp_dir().join(format!("aide-policy-repo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&repo);
        fs::create_dir_all(repo.join("my-skill")).unwrap();
        fs::write(repo.join("my-skill/SKILL.md"), "---\nname: my-skill\n---\n").unwrap();
        let skills = home.join(SKILLS_RELATIVE);
        fs::create_dir_all(&skills).unwrap();
        std::os::unix::fs::symlink(repo.join("my-skill"), skills.join("my-skill")).unwrap();

        // Writing through the skill symlink is allowed, even for new files.
        let target = skills.join("my-skill/notes.md");
        assert_eq!(ensure_writable_in(&home, &target).unwrap(), target);
        fs::write(&target, "note").unwrap();
        assert!(ensure_readable_in(&home, &target).is_ok());
        // The same folder reached directly stays outside the allowed roots.
        assert!(ensure_writable_in(&home, &repo.join("my-skill/notes.md")).is_err());
        // A symlink outside the skills root does not grant access.
        fs::create_dir_all(home.join(".config")).unwrap();
        std::os::unix::fs::symlink(&repo, home.join(".config/linked")).unwrap();
        assert!(ensure_writable_in(&home, &home.join(".config/linked/file.txt")).is_err());
        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&repo);
    }

    #[cfg(unix)]
    #[test]
    fn skill_symlink_into_a_sensitive_folder_is_still_denied() {
        let home = temp_home("skill-deny");
        let skills = home.join(SKILLS_RELATIVE);
        fs::create_dir_all(&skills).unwrap();
        fs::create_dir_all(home.join(".ssh")).unwrap();
        std::os::unix::fs::symlink(home.join(".ssh"), skills.join("evil")).unwrap();
        assert!(ensure_writable_in(&home, &skills.join("evil/authorized_keys")).is_err());
        let _ = fs::remove_dir_all(&home);
    }
}
