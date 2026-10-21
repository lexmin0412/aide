// Scoped Git backup/share for the central skills store. Canonical skills stay
// in ~/.agents/skills untouched; every scope keeps a mirrored worktree under
// ~/.aide/scopes/<scope> whose origin is the user's own remote (personal vs
// company repos stay separate). Git operations shell out to the system `git`
// so SSH agents and credential helpers work unchanged.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::storage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRemote {
    pub name: String,
    pub url: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    #[serde(default)]
    pub is_default: bool,
}

fn default_branch() -> String {
    "main".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitRemotesConfig {
    #[serde(default)]
    pub remotes: Vec<GitRemote>,
    /// Skill folder name -> scope name. Unlisted skills follow the default scope.
    #[serde(default)]
    pub skill_scopes: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct PublishOutcome {
    pub committed: bool,
    pub pushed: bool,
    pub skills: Vec<String>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct PullOutcome {
    pub updated: Vec<String>,
}

pub fn config_path(aide_dir: &Path) -> PathBuf {
    aide_dir.join("remotes.json")
}

/// Read the scope configuration. A missing file yields the default config; a
/// corrupt one is an error so callers never overwrite it with defaults.
pub fn read_config(aide_dir: &Path) -> Result<GitRemotesConfig, String> {
    Ok(storage::read_json(&config_path(aide_dir))?.unwrap_or_default())
}

pub fn write_config(aide_dir: &Path, config: &GitRemotesConfig) -> Result<(), String> {
    validate_config(config)?;
    fs::create_dir_all(aide_dir).map_err(|e| e.to_string())?;
    let path = config_path(aide_dir);
    let backups = aide_dir.join("backups");
    storage::write_json(&path, config, Some(&backups))?;
    // Remote URLs can embed credentials; keep the file owner-only.
    storage::restrict_permissions(&path)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Transport schemes a git remote may use. Anything else (ext::, ftp://, ...)
/// is rejected: a remote URL is user input and must never reach git as a
/// command-line option or an unexpected transport.
const ALLOWED_REMOTE_SCHEMES: &[&str] = &["https", "ssh", "git", "file"];

/// Validate a remote URL before it is stored or handed to git. Accepts the
/// four documented schemes plus scp-like `[user@]host:path`; rejects option
/// injection (`-...`), whitespace, unknown schemes and git protocol helpers.
pub fn validate_remote_url(url: &str) -> Result<(), String> {
    let value = url.trim();
    if value.is_empty() {
        return Err("Remote URL cannot be empty".to_string());
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!(
            "Invalid remote URL \"{url}\": whitespace is not allowed"
        ));
    }
    if value.starts_with('-') {
        return Err(format!(
            "Invalid remote URL \"{url}\": it must not start with '-'"
        ));
    }
    // `ext::sh -c ...`-style protocol helpers are never valid remotes here.
    if value.contains("::") {
        return Err(format!(
            "Invalid remote URL \"{url}\": custom git protocols are not supported"
        ));
    }
    if let Some((scheme, rest)) = value.split_once("://") {
        if !ALLOWED_REMOTE_SCHEMES
            .iter()
            .any(|s| scheme.eq_ignore_ascii_case(s))
        {
            return Err(format!(
                "Unsupported remote URL scheme \"{scheme}\" (allowed: https, ssh, git, file)"
            ));
        }
        if rest.trim_matches('/').is_empty() {
            return Err(format!(
                "Invalid remote URL \"{url}\": missing host or path"
            ));
        }
        return Ok(());
    }
    if is_scp_like_remote(value) {
        Ok(())
    } else {
        Err(format!(
            "Invalid remote URL \"{url}\": expected https://, ssh://, git://, file:// or [user@]host:path"
        ))
    }
}

/// scp-like remote shorthand: `host:path` or `user@host:path`.
fn is_scp_like_remote(value: &str) -> bool {
    if value.contains("://") || value.contains(' ') {
        return false;
    }
    let Some((host_part, path)) = value.split_once(':') else {
        return false;
    };
    if host_part.is_empty() || path.is_empty() || path.starts_with("//") {
        return false;
    }
    // A single letter is a Windows drive (`C:\repo`), not a host.
    if host_part.len() == 1 {
        return false;
    }
    let host = match host_part.split_once('@') {
        Some((user, host)) => {
            if user.is_empty() || user.contains('/') {
                return false;
            }
            host
        }
        None => host_part,
    };
    !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '[' | ']' | ':'))
}

/// Syntactic branch/ref guard mirroring the checks `git check-ref-format`
/// performs, so an invalid branch is rejected before any git command runs.
pub fn validate_branch_ref(branch: &str) -> Result<(), String> {
    if branch.is_empty() {
        return Err("Branch name cannot be empty".to_string());
    }
    if branch.starts_with('-') {
        return Err(format!(
            "Invalid branch \"{branch}\": it must not start with '-'"
        ));
    }
    if branch.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!(
            "Invalid branch \"{branch}\": whitespace is not allowed"
        ));
    }
    if branch == "@"
        || branch.starts_with('/')
        || branch.ends_with('/')
        || branch.ends_with('.')
        || branch.ends_with(".lock")
        || branch.contains("..")
        || branch.contains("@{")
        || branch.contains("//")
    {
        return Err(format!("Invalid branch name \"{branch}\""));
    }
    if branch
        .chars()
        .any(|c| matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
    {
        return Err(format!(
            "Invalid branch name \"{branch}\": contains a reserved character"
        ));
    }
    if branch
        .split('/')
        .any(|part| part.is_empty() || part.starts_with('.'))
    {
        return Err(format!(
            "Invalid branch name \"{branch}\": a component starts with '.'"
        ));
    }
    Ok(())
}

pub fn validate_config(config: &GitRemotesConfig) -> Result<(), String> {
    if config.remotes.len() > 8 {
        return Err("At most 8 remotes are supported".to_string());
    }
    let mut seen = std::collections::HashSet::new();
    let mut default_count = 0;
    for remote in &config.remotes {
        if !valid_name(&remote.name) {
            return Err(format!(
                "Invalid scope name \"{}\" (letters, numbers, dashes only)",
                remote.name
            ));
        }
        if !seen.insert(remote.name.clone()) {
            return Err(format!("Duplicate scope name \"{}\"", remote.name));
        }
        if remote.url.trim().is_empty() {
            return Err(format!("Scope \"{}\" has an empty remote URL", remote.name));
        }
        validate_remote_url(&remote.url).map_err(|e| format!("Scope \"{}\": {e}", remote.name))?;
        if remote.branch.trim().is_empty() {
            return Err(format!("Scope \"{}\" has an empty branch", remote.name));
        }
        validate_branch_ref(&remote.branch)
            .map_err(|e| format!("Scope \"{}\": {e}", remote.name))?;
        if remote.is_default {
            default_count += 1;
        }
    }
    if default_count > 1 {
        return Err("Only one scope can be the default".to_string());
    }
    Ok(())
}

/// The scope unassigned skills belong to: the remote marked default, else the
/// first one. None when no remotes are configured.
pub fn default_scope_name(config: &GitRemotesConfig) -> Option<String> {
    config
        .remotes
        .iter()
        .find(|r| r.is_default)
        .or_else(|| config.remotes.first())
        .map(|r| r.name.clone())
}

pub fn effective_scope(config: &GitRemotesConfig, skill: &str) -> Option<String> {
    let default = default_scope_name(config)?;
    Some(
        config
            .skill_scopes
            .get(skill)
            .cloned()
            .filter(|s| config.remotes.iter().any(|r| r.name == *s))
            .unwrap_or(default),
    )
}

fn is_skill_dir(path: &Path) -> bool {
    path.is_dir() && path.join("SKILL.md").is_file()
}

/// The skills assigned to `scope`. Errors when the central store is missing or
/// unreadable: publishing an empty scope would wipe the remote.
pub fn skills_of_scope(
    config: &GitRemotesConfig,
    skills_dir: &Path,
    scope: &str,
) -> Result<Vec<String>, String> {
    if !skills_dir.is_dir() {
        return Err(format!(
            "Skills directory {} does not exist; refusing to publish scope \"{scope}\"",
            skills_dir.display()
        ));
    }
    let entries = fs::read_dir(skills_dir)
        .map_err(|e| format!("Cannot read {}: {e}", skills_dir.display()))?;
    let mut paths: Vec<_> = entries
        .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    paths.sort();
    let mut out = Vec::new();
    for p in paths {
        if !is_skill_dir(&p) {
            continue;
        }
        if let Some(name) = p.file_name().map(|n| n.to_string_lossy().to_string()) {
            if effective_scope(config, &name).as_deref() == Some(scope) {
                out.push(name);
            }
        }
    }
    Ok(out)
}

fn scope_worktree(aide_dir: &Path, scope: &str) -> PathBuf {
    aide_dir.join("scopes").join(scope)
}

/// Resolve the git executable once. A Finder-launched macOS app inherits the
/// minimal launchd PATH, which usually lacks /opt/homebrew/bin, so resolving
/// `git` through PATH alone would fail even though the user has git installed.
fn git_binary() -> Option<&'static Path> {
    static GIT: OnceLock<Option<PathBuf>> = OnceLock::new();
    GIT.get_or_init(resolve_git).as_deref()
}

fn resolve_git() -> Option<PathBuf> {
    let env_override = std::env::var_os("AIDE_GIT_BIN").map(PathBuf::from);
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    let common: Vec<PathBuf> = [
        "/opt/homebrew/bin/git",
        "/usr/local/bin/git",
        "/usr/bin/git",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    find_git(env_override.as_deref(), &path_dirs, &common)
}

fn git_executable_name() -> &'static str {
    if cfg!(windows) {
        "git.exe"
    } else {
        "git"
    }
}

/// Resolution order: `AIDE_GIT_BIN`, then PATH, then common install locations.
/// Split from `resolve_git` so the search is unit-testable with temp dirs.
fn find_git(
    env_override: Option<&Path>,
    path_dirs: &[PathBuf],
    common: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(path) = env_override {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
    }
    for dir in path_dirs {
        let candidate = dir.join(git_executable_name());
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    common.iter().find(|p| p.is_file()).cloned()
}

fn git_command() -> Command {
    match git_binary() {
        Some(path) => Command::new(path),
        None => Command::new("git"),
    }
}

fn git_exists() -> bool {
    git_binary().is_some()
}

/// Exact check performed by git itself. It may expand `@{-1}`, so it runs
/// inside the scope worktree where possible.
fn ensure_valid_branch(dir: &Path, branch: &str) -> Result<(), String> {
    validate_branch_ref(branch)?;
    let out = run_git(dir, &["check-ref-format", "--branch", branch])?;
    if out.status.success() {
        Ok(())
    } else {
        let detail = out.stderr.trim();
        Err(if detail.is_empty() {
            format!("Invalid branch name \"{branch}\"")
        } else {
            format!("Invalid branch name \"{branch}\": {detail}")
        })
    }
}

struct GitOutput {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

fn run_git(dir: &Path, args: &[&str]) -> Result<GitOutput, String> {
    let output = git_command()
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;
    Ok(GitOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn expect_ok(result: Result<GitOutput, String>, context: &str) -> Result<String, String> {
    let out = result?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        let detail = if out.stderr.trim().is_empty() {
            out.stdout.trim().to_string()
        } else {
            out.stderr.trim().to_string()
        };
        Err(format!("{context}: {detail}"))
    }
}

/// Commits need an identity; fall back to a local one when the machine has no
/// global git identity configured.
fn commit(dir: &Path, message: &str) -> Result<(), String> {
    let has_identity = git_command()
        .args(["config", "user.email"])
        .current_dir(dir)
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false);
    let mut cmd = git_command();
    if !has_identity {
        // Config overrides must precede the subcommand.
        cmd.arg("-c")
            .arg("user.name=aide")
            .arg("-c")
            .arg("user.email=aide@local");
    }
    let out = cmd
        .current_dir(dir)
        .args(["commit", "-m", message])
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git commit: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Mirror the scope's skills into the worktree.
///
/// Only directories that look like skills (they contain SKILL.md) are managed;
/// repository-level files such as README.md or .github are left untouched. A
/// skill directory that the current scope no longer renders was pushed by a
/// teammate (or is still referenced by the central store): it is copied back
/// into the central store when missing there, so a publish cannot delete data
/// that only exists remotely. Returns the rescued skill names.
fn mirror_skills(
    skills_dir: &Path,
    names: &[String],
    worktree: &Path,
) -> Result<Vec<String>, String> {
    let wanted: HashSet<&str> = names.iter().map(|s| s.as_str()).collect();
    let mut rescued = Vec::new();

    let entries = fs::read_dir(worktree).map_err(|e| e.to_string())?;
    let mut paths: Vec<_> = entries
        .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    paths.sort();
    for p in paths {
        let name = match p.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };
        if name == ".git" || wanted.contains(name.as_str()) {
            continue;
        }
        let is_dir = fs::symlink_metadata(&p)
            .map(|m| m.is_dir())
            .unwrap_or(false);
        if !is_dir || !is_skill_dir(&p) {
            // README, .github and any other non-skill content stays in place.
            continue;
        }
        let central = skills_dir.join(&name);
        if fs::symlink_metadata(&central).is_err() {
            crate::fsutil::copy_dir_into_place(&p, &central)
                .map_err(|e| format!("Failed to restore remote skill \"{name}\": {e}"))?;
            rescued.push(name.clone());
        }
        fs::remove_dir_all(&p)
            .map_err(|e| format!("Failed to remove worktree copy of \"{name}\": {e}"))?;
    }

    for name in names {
        let src = skills_dir.join(name);
        if !is_skill_dir(&src) {
            return Err(format!(
                "Skill \"{name}\" is assigned to this scope but is missing from {}",
                skills_dir.display()
            ));
        }
        // Stage the copy and swap it in so a failed copy never leaves a
        // half-written skill in the worktree.
        crate::fsutil::copy_dir_into_place(&src, &worktree.join(name))
            .map_err(|e| format!("Failed to copy \"{name}\": {e}"))?;
    }
    Ok(rescued)
}

/// Write skills from the worktree back into the central store. Only skills that
/// belong to `scope` may overwrite the central copy; a skill unknown to the
/// local store is written as well (it was pushed to this scope by someone
/// else). Returns the names that were newly created centrally.
fn write_back_from_worktree(
    config: &GitRemotesConfig,
    worktree: &Path,
    skills_dir: &Path,
    scope: &str,
) -> Result<Vec<String>, String> {
    let mut created = Vec::new();
    let entries = fs::read_dir(worktree).map_err(|e| e.to_string())?;
    let mut paths: Vec<_> = entries
        .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    paths.sort();
    for p in paths {
        if !is_skill_dir(&p) {
            continue;
        }
        let name = match p.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };
        // A skill explicitly assigned to a different scope is never written
        // back from this scope's worktree.
        if let Some(owner) = config.skill_scopes.get(&name) {
            if owner != scope {
                continue;
            }
        }
        let central = skills_dir.join(&name);
        let belongs_here = effective_scope(config, &name).as_deref() == Some(scope);
        let exists = fs::symlink_metadata(&central).is_ok();
        if !belongs_here {
            if exists {
                // Canonical copy belongs to another scope; do not cross-pollute.
                continue;
            }
            created.push(name.clone());
        }
        crate::fsutil::copy_dir_into_place(&p, &central)
            .map_err(|e| format!("Failed to write \"{name}\" back: {e}"))?;
    }
    Ok(created)
}

fn ensure_worktree(aide_dir: &Path, remote: &GitRemote) -> Result<PathBuf, String> {
    if !git_exists() {
        return Err(
            "git was not found. Install git (or set AIDE_GIT_BIN) to use Git backup.".to_string(),
        );
    }
    let dir = scope_worktree(aide_dir, &remote.name);
    if dir.join(".git").exists() {
        return Ok(dir);
    }
    if dir.exists() {
        // Empty leftover directory: clone needs it gone.
        fs::remove_dir(&dir).map_err(|e| e.to_string())?;
    } else {
        fs::create_dir_all(dir.parent().unwrap()).map_err(|e| e.to_string())?;
    }
    // `--` keeps a remote URL that begins with '-' from being parsed as an
    // option; validate_config rejects those URLs as well.
    let output = git_command()
        .arg("clone")
        .arg("--")
        .arg(&remote.url)
        .arg(&dir)
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Clone failed for scope \"{}\": {}",
            remote.name,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(dir)
}

fn switch_branch(dir: &Path, branch: &str) -> Result<(), String> {
    // The ref is validated by git before it is used in checkout/push/pull.
    ensure_valid_branch(dir, branch)?;
    let head = expect_ok(
        run_git(dir, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "git rev-parse",
    )
    .unwrap_or_default();
    if head.trim() == branch {
        return Ok(());
    }
    let exists = run_git(
        dir,
        &["show-ref", "--verify", &format!("refs/heads/{branch}")],
    )
    .map(|o| o.status.success())
    .unwrap_or(false);
    if exists {
        expect_ok(run_git(dir, &["checkout", branch]), "git checkout").map(|_| ())
    } else {
        expect_ok(run_git(dir, &["checkout", "-b", branch]), "git checkout -b").map(|_| ())
    }
}

fn push(
    config: &GitRemotesConfig,
    dir: &Path,
    branch: &str,
    skills_dir: &Path,
    scope: &str,
) -> Result<Vec<String>, String> {
    let output = git_command()
        .current_dir(dir)
        .args(["push", "-u", "origin", "--", branch])
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;
    if output.status.success() {
        return Ok(Vec::new());
    }
    let push_err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    // Remote may be ahead (teammates pushed): rebase our commit and retry once.
    let pulled = run_git(dir, &["pull", "--rebase", "origin", "--", branch]);
    if !pulled.map(|o| o.status.success()).unwrap_or(false) {
        let _ = run_git(dir, &["rebase", "--abort"]);
        return Err(format!(
            "Push failed and could not be reconciled automatically. Resolve manually in the scope worktree. ({push_err})"
        ));
    }
    // The rebase brought the teammates' commits into the worktree; surface their
    // skills locally before pushing so nothing is only reachable through git.
    let created = write_back_from_worktree(config, dir, skills_dir, scope)?;
    let retry = git_command()
        .current_dir(dir)
        .args(["push", "-u", "origin", "--", branch])
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;
    if retry.status.success() {
        Ok(created)
    } else {
        Err(format!(
            "Push failed: {}",
            String::from_utf8_lossy(&retry.stderr).trim()
        ))
    }
}

pub fn publish_scope(
    config: &GitRemotesConfig,
    skills_dir: &Path,
    aide_dir: &Path,
    scope: &str,
) -> Result<PublishOutcome, String> {
    // Re-validate a possibly hand-edited config before any git command runs:
    // remote URLs and branch refs are user input.
    validate_config(config)?;
    let remote = config
        .remotes
        .iter()
        .find(|r| r.name == scope)
        .ok_or_else(|| format!("Unknown scope \"{scope}\""))?
        .clone();
    // Resolve and validate the skill set before touching git: publishing an
    // empty scope would delete everything from the remote.
    let skills = skills_of_scope(config, skills_dir, scope)?;
    if skills.is_empty() {
        return Err(format!(
            "Scope \"{scope}\" contains no skills; refusing to publish because that would clear the remote"
        ));
    }

    let dir = ensure_worktree(aide_dir, &remote)?;
    switch_branch(&dir, &remote.branch)?;

    let rescued = mirror_skills(skills_dir, &skills, &dir)?;
    expect_ok(run_git(&dir, &["add", "-A"]), "git add")?;
    let status = expect_ok(run_git(&dir, &["status", "--porcelain"]), "git status")?;
    let mut committed = false;
    if !status.trim().is_empty() {
        let msg = format!(
            "aide: sync {} ({} skill{}) at {}",
            scope,
            skills.len(),
            if skills.len() == 1 { "" } else { "s" },
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        );
        commit(&dir, &msg)?;
        committed = true;
    }
    let mut recovered = push(config, &dir, &remote.branch, skills_dir, scope)?;
    recovered.extend(rescued);
    recovered.sort();
    recovered.dedup();
    let message = if recovered.is_empty() {
        "Published".to_string()
    } else {
        format!(
            "Published; {} skill(s) restored from the remote locally: {}",
            recovered.len(),
            recovered.join(", ")
        )
    };
    Ok(PublishOutcome {
        committed,
        pushed: true,
        skills,
        message,
    })
}

/// Pull the remote with fast-forward only, then write remote-side skills back
/// into the central store. Remote content wins for the skills of this scope;
/// skills assigned to other scopes and local-only skills are never touched or
/// deleted.
pub fn pull_scope(
    config: &GitRemotesConfig,
    skills_dir: &Path,
    aide_dir: &Path,
    scope: &str,
) -> Result<PullOutcome, String> {
    validate_config(config)?;
    let remote = config
        .remotes
        .iter()
        .find(|r| r.name == scope)
        .ok_or_else(|| format!("Unknown scope \"{scope}\""))?
        .clone();
    let dir = ensure_worktree(aide_dir, &remote)?;
    switch_branch(&dir, &remote.branch)?;
    expect_ok(
        run_git(&dir, &["pull", "--ff-only", "origin", "--", &remote.branch]),
        "git pull",
    )?;

    fs::create_dir_all(skills_dir).map_err(|e| format!("create {}: {e}", skills_dir.display()))?;

    let mut updated = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;
    let mut paths: Vec<_> = entries
        .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    paths.sort();
    for p in paths {
        if !is_skill_dir(&p) {
            continue;
        }
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if effective_scope(config, &name).as_deref() != Some(scope) {
            // Never let one scope overwrite a skill owned by another scope.
            continue;
        }
        let dest = skills_dir.join(&name);
        // Stage next to the destination and swap with renames: the current copy
        // stays readable until the replacement is complete.
        crate::fsutil::copy_dir_into_place(&p, &dest)
            .map_err(|e| format!("Failed to write \"{name}\": {e}"))?;
        updated.push(name);
    }
    Ok(PullOutcome { updated })
}

// ── Default-home helpers ──

/// Read `~/.aide/remotes.json`.
pub fn read_default() -> Result<GitRemotesConfig, String> {
    read_config(&crate::aide_dir()?)
}

/// Write `~/.aide/remotes.json`.
pub fn write_default(config: &GitRemotesConfig) -> Result<(), String> {
    write_config(&crate::aide_dir()?, config)
}

/// Assign a skill to a scope (None clears the explicit assignment).
pub fn set_skill_scope(skill: &str, scope: Option<&str>) -> Result<(), String> {
    let aide_dir = crate::aide_dir()?;
    let mut config = read_config(&aide_dir)?;
    match scope {
        Some(scope) => {
            config
                .skill_scopes
                .insert(skill.to_string(), scope.to_string());
        }
        None => {
            config.skill_scopes.remove(skill);
        }
    }
    write_config(&aide_dir, &config)
}

/// Publish a scope against the real home directory and central skill store.
pub fn publish_default(scope: &str) -> Result<PublishOutcome, String> {
    let aide_dir = crate::aide_dir()?;
    let config = read_config(&aide_dir)?;
    let skills_dir = crate::skills::resolve_skills_source()?;
    publish_scope(&config, &skills_dir, &aide_dir, scope)
}

/// Pull a scope against the real home directory and central skill store.
pub fn pull_default(scope: &str) -> Result<PullOutcome, String> {
    let aide_dir = crate::aide_dir()?;
    let config = read_config(&aide_dir)?;
    let skills_dir = crate::skills::resolve_skills_source()?;
    pull_scope(&config, &skills_dir, &aide_dir, scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_base(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aide-git-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Portable file:// URL for a local repository path.
    fn file_url(path: &Path) -> String {
        let p = path.to_string_lossy().replace('\\', "/");
        if p.starts_with('/') {
            format!("file://{p}")
        } else {
            format!("file:///{p}")
        }
    }

    fn make_skill(root: &Path, name: &str, body: &str) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name}\n---\n{body}"),
        )
        .unwrap();
    }

    #[test]
    fn config_round_trip_and_validation() {
        let base = temp_base("config");
        let cfg = GitRemotesConfig {
            remotes: vec![
                GitRemote {
                    name: "personal".into(),
                    url: "https://github.com/me/skills.git".into(),
                    branch: "main".into(),
                    is_default: true,
                },
                GitRemote {
                    name: "work".into(),
                    url: "git@github.com:company/skills.git".into(),
                    branch: "main".into(),
                    is_default: false,
                },
            ],
            skill_scopes: HashMap::from([("work-skill".to_string(), "work".to_string())]),
        };
        write_config(&base, &cfg).unwrap();
        let read = read_config(&base).unwrap();
        assert_eq!(read.remotes.len(), 2);
        assert_eq!(read.skill_scopes.get("work-skill").unwrap(), "work");

        // Duplicate names rejected
        let mut bad = cfg.clone();
        bad.remotes.push(bad.remotes[0].clone());
        assert!(write_config(&base, &bad).is_err());
        // Invalid characters rejected
        let mut bad2 = cfg.clone();
        bad2.remotes[1].name = "bad name!".into();
        assert!(write_config(&base, &bad2).is_err());
        // Two defaults rejected
        let mut bad3 = cfg.clone();
        bad3.remotes[1].is_default = true;
        assert!(write_config(&base, &bad3).is_err());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn scope_resolution_uses_explicit_then_default() {
        let cfg = GitRemotesConfig {
            remotes: vec![
                GitRemote {
                    name: "work".into(),
                    url: "x".into(),
                    branch: "main".into(),
                    is_default: true,
                },
                GitRemote {
                    name: "personal".into(),
                    url: "y".into(),
                    branch: "main".into(),
                    is_default: false,
                },
            ],
            skill_scopes: HashMap::from([
                ("a".to_string(), "personal".to_string()),
                ("ghost".to_string(), "deleted-scope".to_string()),
            ]),
        };
        assert_eq!(effective_scope(&cfg, "a").as_deref(), Some("personal"));
        assert_eq!(effective_scope(&cfg, "b").as_deref(), Some("work"));
        // Assignment to a removed scope falls back to default
        assert_eq!(effective_scope(&cfg, "ghost").as_deref(), Some("work"));
        let empty = GitRemotesConfig::default();
        assert_eq!(effective_scope(&empty, "a"), None);
    }

    #[test]
    fn skills_of_scope_filters_correctly() {
        let base = temp_base("filter");
        let skills = base.join("skills");
        make_skill(&skills, "mine", "personal stuff");
        make_skill(&skills, "theirs", "company stuff");
        make_skill(&skills, "unassigned", "default scope");
        fs::create_dir_all(skills.join("not-a-skill")).unwrap();
        let cfg = GitRemotesConfig {
            remotes: vec![
                GitRemote {
                    name: "personal".into(),
                    url: "x".into(),
                    branch: "main".into(),
                    is_default: true,
                },
                GitRemote {
                    name: "work".into(),
                    url: "y".into(),
                    branch: "main".into(),
                    is_default: false,
                },
            ],
            skill_scopes: HashMap::from([("theirs".to_string(), "work".to_string())]),
        };
        let work = skills_of_scope(&cfg, &skills, "work").unwrap();
        assert_eq!(work, vec!["theirs".to_string()]);
        let personal = skills_of_scope(&cfg, &skills, "personal").unwrap();
        assert_eq!(personal, vec!["mine".to_string(), "unassigned".to_string()]);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn skills_of_scope_errors_when_skills_dir_is_missing() {
        let cfg = GitRemotesConfig {
            remotes: vec![GitRemote {
                name: "personal".into(),
                url: "x".into(),
                branch: "main".into(),
                is_default: true,
            }],
            skill_scopes: HashMap::new(),
        };
        let missing = std::env::temp_dir().join("aide-git-missing-skills-dir");
        assert!(skills_of_scope(&cfg, &missing, "personal").is_err());
    }

    #[test]
    fn publish_refuses_empty_scope_before_touching_the_remote() {
        let base = temp_base("refuse-empty");
        let skills = base.join("skills");
        fs::create_dir_all(&skills).unwrap();
        // A skill owned by the other scope, so "work" resolves to zero skills.
        make_skill(&skills, "personal-skill", "mine");
        let cfg = GitRemotesConfig {
            remotes: vec![
                GitRemote {
                    name: "personal".into(),
                    url: "https://example.com/personal.git".into(),
                    branch: "main".into(),
                    is_default: true,
                },
                GitRemote {
                    name: "work".into(),
                    url: "https://example.com/work.git".into(),
                    branch: "main".into(),
                    is_default: false,
                },
            ],
            skill_scopes: HashMap::new(),
        };
        let aide_dir = base.join("aide");
        let err = publish_scope(&cfg, &skills, &aide_dir, "work").unwrap_err();
        assert!(err.contains("no skills"), "unexpected error: {err}");
        // No worktree was created, so nothing could have been pushed.
        assert!(!aide_dir.join("scopes").join("work").exists());
        // A missing central store is refused as well.
        let err = publish_scope(&cfg, &base.join("nope"), &aide_dir, "personal").unwrap_err();
        assert!(err.contains("does not exist"), "unexpected error: {err}");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn mirror_preserves_repo_files_and_rescues_remote_skills() {
        let base = temp_base("mirror-safe");
        let skills = base.join("skills");
        make_skill(&skills, "alpha", "a");
        let wt = base.join("worktree");
        fs::create_dir_all(wt.join(".git")).unwrap();
        fs::create_dir_all(wt.join(".github")).unwrap();
        fs::write(wt.join(".github/workflows.yml"), "ci").unwrap();
        fs::write(wt.join("README.md"), "repo docs").unwrap();
        // Stale copy of a skill that is no longer part of the scope.
        make_skill(&wt, "stale", "old");
        // Teammate-pushed skill that never existed locally.
        make_skill(&wt, "from-teammate", "remote only");

        let rescued = mirror_skills(&skills, &["alpha".to_string()], &wt).unwrap();
        // Both worktree-only skills were rescued: the teammate's new skill and
        // the copy that no longer exists in the central store.
        assert_eq!(
            rescued,
            vec!["from-teammate".to_string(), "stale".to_string()]
        );
        assert!(wt.join("alpha").join("SKILL.md").is_file());
        assert!(!wt.join("stale").exists());
        assert!(wt.join("README.md").is_file());
        assert!(wt.join(".github/workflows.yml").is_file());
        assert!(wt.join(".git").exists());
        // The teammate skill was not lost: it now exists in the central store.
        assert!(skills.join("from-teammate").join("SKILL.md").is_file());
        // A worktree skill that still exists centrally is not overwritten: the
        // canonical copy wins and no rescue is reported.
        make_skill(&skills, "stale", "canonical");
        make_skill(&wt, "stale", "remote version");
        let rescued = mirror_skills(&skills, &["alpha".to_string()], &wt).unwrap();
        assert!(rescued.is_empty());
        let canonical = fs::read_to_string(skills.join("stale").join("SKILL.md")).unwrap();
        assert!(canonical.contains("canonical"));
        assert!(!wt.join("stale").exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn mirror_refuses_to_drop_a_skill_missing_from_the_store() {
        let base = temp_base("mirror-missing");
        let skills = base.join("skills");
        fs::create_dir_all(&skills).unwrap();
        let wt = base.join("worktree");
        fs::create_dir_all(wt.join(".git")).unwrap();
        let err = mirror_skills(&skills, &["ghost".to_string()], &wt).unwrap_err();
        assert!(err.contains("missing"));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn write_back_skips_skills_owned_by_another_scope() {
        let base = temp_base("pull-scope-filter");
        let skills = base.join("skills");
        make_skill(&skills, "mine", "local");
        let wt = base.join("worktree");
        make_skill(&wt, "mine", "remote");
        make_skill(&wt, "not-in-this-scope", "other");
        let cfg = GitRemotesConfig {
            remotes: vec![
                GitRemote {
                    name: "personal".into(),
                    url: "x".into(),
                    branch: "main".into(),
                    is_default: true,
                },
                GitRemote {
                    name: "other".into(),
                    url: "y".into(),
                    branch: "main".into(),
                    is_default: false,
                },
            ],
            skill_scopes: HashMap::from([("not-in-this-scope".to_string(), "other".to_string())]),
        };
        assert_eq!(
            effective_scope(&cfg, "not-in-this-scope").as_deref(),
            Some("other")
        );
        let created = write_back_from_worktree(&cfg, &wt, &skills, "personal").unwrap();
        assert!(created.is_empty());
        // "mine" belongs to the default (personal) scope and is updated.
        let content = fs::read_to_string(skills.join("mine").join("SKILL.md")).unwrap();
        assert!(content.contains("remote"));
        // The other scope's skill is not written back at all.
        assert!(!skills.join("not-in-this-scope").exists());
        let _ = fs::remove_dir_all(&base);
    }

    // End-to-end publish/pull against a local bare remote. Requires git on PATH
    // (present locally and on CI runners).
    #[test]
    fn publish_and_pull_round_trip_with_bare_remote() {
        if !git_exists() {
            return;
        }
        let base = temp_base("e2e");
        let origin = base.join("origin.git");
        fs::create_dir_all(&origin).unwrap();
        expect_ok(
            run_git(&base, &["init", "--bare", origin.to_str().unwrap()]),
            "git init --bare",
        )
        .unwrap();

        let skills = base.join("skills");
        make_skill(&skills, "shared", "v1");
        make_skill(&skills, "local-only", "keep me");
        let aide_dir = base.join("aide");
        let cfg = GitRemotesConfig {
            remotes: vec![GitRemote {
                name: "personal".into(),
                url: file_url(&origin),
                branch: "main".into(),
                is_default: true,
            }],
            skill_scopes: HashMap::new(),
        };

        // Publish
        let outcome = publish_scope(&cfg, &skills, &aide_dir, "personal").unwrap();
        assert!(outcome.committed && outcome.pushed);
        assert_eq!(outcome.skills.len(), 2);

        // Verify content landed in the bare remote
        let check = base.join("check");
        fs::create_dir_all(&check).unwrap();
        expect_ok(
            run_git(
                &base,
                &["clone", origin.to_str().unwrap(), check.to_str().unwrap()],
            ),
            "git clone check",
        )
        .unwrap();
        assert!(check.join("shared").join("SKILL.md").is_file());
        assert!(check.join("local-only").join("SKILL.md").is_file());

        // Simulate a teammate push: adds a new skill (different file, so the
        // later rebase stays conflict-free) plus a remote change to "shared".
        make_skill(&check, "from-teammate", "new upstream skill");
        expect_ok(run_git(&check, &["add", "-A"]), "add").unwrap();
        commit(&check, "teammate update").unwrap();
        expect_ok(
            run_git(&check, &["push", "origin", "HEAD:refs/heads/main"]),
            "push teammate",
        )
        .unwrap();

        // Publish again: non-fast-forward must recover via rebase
        fs::write(
            skills.join("shared").join("SKILL.md"),
            "---\nname: shared\n---\nv3 local",
        )
        .unwrap();
        let second = publish_scope(&cfg, &skills, &aide_dir, "personal").unwrap();
        assert!(second.pushed);

        // Overwrite local with remote state via pull. The check clone first
        // syncs to aide's publish (no conflict), then advances the remote.
        expect_ok(
            run_git(&check, &["pull", "--rebase", "origin", "main"]),
            "sync check",
        )
        .unwrap();
        fs::write(
            check.join("shared").join("SKILL.md"),
            "---\nname: shared\n---\nremote truth",
        )
        .unwrap();
        expect_ok(run_git(&check, &["add", "-A"]), "add2").unwrap();
        commit(&check, "remote truth").unwrap();
        expect_ok(
            run_git(&check, &["push", "origin", "HEAD:refs/heads/main"]),
            "push2",
        )
        .unwrap();
        let pulled = pull_scope(&cfg, &skills, &aide_dir, "personal").unwrap();
        assert!(pulled.updated.contains(&"shared".to_string()));
        assert!(pulled.updated.contains(&"from-teammate".to_string()));
        let body = fs::read_to_string(skills.join("shared").join("SKILL.md")).unwrap();
        assert!(body.contains("remote truth"));
        // Local-only skills survive pull
        assert!(skills.join("local-only").join("SKILL.md").is_file());
        // Teammate's skill arrived locally
        assert!(skills.join("from-teammate").join("SKILL.md").is_file());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn find_git_prefers_env_then_path_then_common_locations() {
        let base = temp_base("find-git");
        let exe = git_executable_name();
        let env_git = base.join("env").join(exe);
        let path_git = base.join("path").join(exe);
        let common_git = base.join("common").join(exe);
        for path in [&env_git, &path_git, &common_git] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }
        let path_dirs = vec![base.join("empty"), base.join("path")];
        let common = vec![common_git.clone()];

        // AIDE_GIT_BIN wins when it points at a file.
        assert_eq!(
            find_git(Some(&env_git), &path_dirs, &common),
            Some(env_git.clone())
        );
        // A missing override is ignored in favour of PATH.
        assert_eq!(
            find_git(Some(&base.join("nope").join(exe)), &path_dirs, &common),
            Some(path_git.clone())
        );
        // PATH is searched before the common install locations.
        assert_eq!(find_git(None, &path_dirs, &common), Some(path_git.clone()));
        // Without PATH hits the macOS/Homebrew locations are used.
        assert_eq!(
            find_git(None, &[base.join("empty")], &common),
            Some(common_git)
        );
        // Nothing found means "git missing", which fails cleanly.
        assert_eq!(
            find_git(None, &[base.join("empty")], &[base.join("absent")]),
            None
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn remote_url_validation_accepts_documented_forms() {
        for url in [
            "https://github.com/me/skills.git",
            "ssh://git@github.com/me/skills.git",
            "git://example.com/repo.git",
            "file:///tmp/skills.git",
            "git@github.com:company/skills.git",
            "github.com:company/skills.git",
            "https://example.com:8443/repo.git",
        ] {
            assert!(
                validate_remote_url(url).is_ok(),
                "expected valid: {url} ({:?})",
                validate_remote_url(url)
            );
        }
    }

    #[test]
    fn remote_url_validation_rejects_option_injection_and_unknown_schemes() {
        for url in [
            "-oProxyCommand=/bin/sh",
            "-u",
            "https://",
            "ftp://example.com/repo.git",
            "http://example.com/repo.git",
            "javascript://alert(1)",
            "ext::sh -c touch% /tmp/pwn",
            " https://example.com/repo.git",
            "C:\\repo",
            "git@host:",
            "has space/repo.git",
            "@host:repo",
        ] {
            assert!(validate_remote_url(url).is_err(), "expected invalid: {url}");
        }
    }

    #[test]
    fn branch_ref_validation_rejects_dangerous_refs() {
        for branch in ["main", "feature/mcp-drift", "release-1.2", "a_b"] {
            assert!(
                validate_branch_ref(branch).is_ok(),
                "expected valid branch: {branch}"
            );
        }
        for branch in [
            "",
            "-x",
            "a b",
            "a..b",
            "a~b",
            "a^b",
            "a:b",
            "a?b",
            "a*b",
            "a[b",
            "a\\b",
            "@{upstream}",
            "/absolute",
            "trailing/",
            "trailing.",
            "x.lock",
            ".hidden",
            "a//b",
            "@",
        ] {
            assert!(
                validate_branch_ref(branch).is_err(),
                "expected invalid branch: {branch}"
            );
        }
    }

    #[test]
    fn config_validation_rejects_bad_urls_and_refs() {
        let mut config = GitRemotesConfig {
            remotes: vec![GitRemote {
                name: "personal".into(),
                url: "https://example.com/skills.git".into(),
                branch: "main".into(),
                is_default: true,
            }],
            skill_scopes: HashMap::new(),
        };
        assert!(validate_config(&config).is_ok());
        config.remotes[0].url = "-oProxyCommand=/bin/sh".into();
        assert!(validate_config(&config).is_err());
        config.remotes[0].url = "https://example.com/skills.git".into();
        config.remotes[0].branch = "bad..branch".into();
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn publish_rejects_hand_edited_invalid_remote_before_touching_git() {
        let base = temp_base("bad-remote");
        let skills = base.join("skills");
        fs::create_dir_all(&skills).unwrap();
        make_skill(&skills, "alpha", "a");
        let cfg = GitRemotesConfig {
            remotes: vec![GitRemote {
                name: "personal".into(),
                url: "-oProxyCommand=/bin/sh".into(),
                branch: "main".into(),
                is_default: true,
            }],
            skill_scopes: HashMap::new(),
        };
        let aide_dir = base.join("aide");
        let err = publish_scope(&cfg, &skills, &aide_dir, "personal").unwrap_err();
        assert!(err.contains("must not start with '-'"), "unexpected: {err}");
        assert!(!aide_dir.join("scopes").exists());
        let _ = fs::remove_dir_all(&base);
    }
}
