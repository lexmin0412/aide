// Scoped Git backup/share for the central skills store. Canonical skills stay
// in ~/.agents/skills untouched; every scope keeps a mirrored worktree under
// ~/.aide/scopes/<scope> whose origin is the user's own remote (personal vs
// company repos stay separate). Git operations shell out to the system `git`
// so SSH agents and credential helpers work unchanged.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

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

pub fn read_config(aide_dir: &Path) -> GitRemotesConfig {
    fs::read_to_string(config_path(aide_dir))
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

pub fn write_config(aide_dir: &Path, config: &GitRemotesConfig) -> Result<(), String> {
    validate_config(config)?;
    fs::create_dir_all(aide_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(config_path(aide_dir), json).map_err(|e| e.to_string())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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
        if remote.branch.trim().is_empty() {
            return Err(format!("Scope \"{}\" has an empty branch", remote.name));
        }
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

pub fn skills_of_scope(config: &GitRemotesConfig, skills_dir: &Path, scope: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(skills_dir) {
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            if is_skill_dir(&p) {
                if let Some(name) = p.file_name().map(|n| n.to_string_lossy().to_string()) {
                    if effective_scope(config, &name).as_deref() == Some(scope) {
                        out.push(name);
                    }
                }
            }
        }
    }
    out
}

fn scope_worktree(aide_dir: &Path, scope: &str) -> PathBuf {
    aide_dir.join("scopes").join(scope)
}

fn git_exists() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

struct GitOutput {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

fn run_git(dir: &Path, args: &[&str]) -> Result<GitOutput, String> {
    let output = Command::new("git")
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
    let has_identity = Command::new("git")
        .args(["config", "user.email"])
        .current_dir(dir)
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false);
    let mut cmd = Command::new("git");
    if !has_identity {
        // Config overrides must precede the subcommand.
        cmd.arg("-c").arg("user.name=aide").arg("-c").arg("user.email=aide@local");
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

/// Make the worktree exactly mirror the scope's skills: everything except
/// .git is replaced, so deletions propagate on publish.
fn mirror_skills(skills_dir: &Path, names: &[String], worktree: &Path) -> Result<(), String> {
    for entry in fs::read_dir(worktree).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let p = entry.path();
        if p.is_dir() {
            fs::remove_dir_all(&p).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&p).map_err(|e| e.to_string())?;
        }
    }
    for name in names {
        crate::copy_dir_recursive(&skills_dir.join(name), &worktree.join(name))
            .map_err(|e| format!("Failed to copy \"{name}\": {e}"))?;
    }
    Ok(())
}

fn ensure_worktree(
    aide_dir: &Path,
    remote: &GitRemote,
) -> Result<PathBuf, String> {
    if !git_exists() {
        return Err("git was not found in PATH. Install git to use Git backup.".to_string());
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
    let output = Command::new("git")
        .args(["clone", &remote.url])
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
    let head = expect_ok(run_git(dir, &["rev-parse", "--abbrev-ref", "HEAD"]), "git rev-parse")
        .unwrap_or_default();
    if head.trim() == branch {
        return Ok(());
    }
    let exists = run_git(dir, &["show-ref", "--verify", &format!("refs/heads/{branch}")])
        .map(|o| o.status.success())
        .unwrap_or(false);
    if exists {
        expect_ok(run_git(dir, &["checkout", branch]), "git checkout").map(|_| ())
    } else {
        expect_ok(run_git(dir, &["checkout", "-b", branch]), "git checkout -b").map(|_| ())
    }
}

fn push(dir: &Path, branch: &str) -> Result<(), String> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["push", "-u", "origin", branch])
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let push_err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    // Remote may be ahead (teammates pushed): rebase our commit and retry once.
    let pulled = run_git(dir, &["pull", "--rebase", "origin", branch]);
    if !pulled.map(|o| o.status.success()).unwrap_or(false) {
        let _ = run_git(dir, &["rebase", "--abort"]);
        return Err(format!(
            "Push failed and could not be reconciled automatically. Resolve manually in the scope worktree. ({push_err})"
        ));
    }
    let retry = Command::new("git")
        .current_dir(dir)
        .args(["push", "-u", "origin", branch])
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;
    if retry.status.success() {
        Ok(())
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
    let remote = config
        .remotes
        .iter()
        .find(|r| r.name == scope)
        .ok_or_else(|| format!("Unknown scope \"{scope}\""))?
        .clone();
    let skills = skills_of_scope(config, skills_dir, scope);
    let dir = ensure_worktree(aide_dir, &remote)?;
    switch_branch(&dir, &remote.branch)?;

    mirror_skills(skills_dir, &skills, &dir)?;
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
    push(&dir, &remote.branch)?;
    Ok(PublishOutcome {
        committed,
        pushed: true,
        skills,
        message: "Published".to_string(),
    })
}

/// Pull the remote with fast-forward only, then write remote-side skills back
/// into the central store (remote wins for the skills it contains; local-only
/// skills are never deleted).
pub fn pull_scope(
    config: &GitRemotesConfig,
    skills_dir: &Path,
    aide_dir: &Path,
    scope: &str,
) -> Result<PullOutcome, String> {
    let remote = config
        .remotes
        .iter()
        .find(|r| r.name == scope)
        .ok_or_else(|| format!("Unknown scope \"{scope}\""))?
        .clone();
    let dir = ensure_worktree(aide_dir, &remote)?;
    switch_branch(&dir, &remote.branch)?;
    expect_ok(
        run_git(&dir, &["pull", "--ff-only", "origin", &remote.branch]),
        "git pull",
    )?;

    let mut updated = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;
    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if !is_skill_dir(&p) {
            continue;
        }
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let dest = skills_dir.join(&name);
        if dest.exists() {
            fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
        }
        crate::copy_dir_recursive(&p, &dest)
            .map_err(|e| format!("Failed to write \"{name}\": {e}"))?;
        updated.push(name);
    }
    Ok(PullOutcome { updated })
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
                GitRemote { name: "personal".into(), url: "https://github.com/me/skills.git".into(), branch: "main".into(), is_default: true },
                GitRemote { name: "work".into(), url: "git@github.com:company/skills.git".into(), branch: "main".into(), is_default: false },
            ],
            skill_scopes: HashMap::from([("work-skill".to_string(), "work".to_string())]),
        };
        write_config(&base, &cfg).unwrap();
        let read = read_config(&base);
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
                GitRemote { name: "work".into(), url: "x".into(), branch: "main".into(), is_default: true },
                GitRemote { name: "personal".into(), url: "y".into(), branch: "main".into(), is_default: false },
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
                GitRemote { name: "personal".into(), url: "x".into(), branch: "main".into(), is_default: true },
                GitRemote { name: "work".into(), url: "y".into(), branch: "main".into(), is_default: false },
            ],
            skill_scopes: HashMap::from([("theirs".to_string(), "work".to_string())]),
        };
        let work = skills_of_scope(&cfg, &skills, "work");
        assert_eq!(work, vec!["theirs".to_string()]);
        let personal = skills_of_scope(&cfg, &skills, "personal");
        assert_eq!(personal, vec!["mine".to_string(), "unassigned".to_string()]);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn mirror_replaces_worktree_except_git() {
        let base = temp_base("mirror");
        let skills = base.join("skills");
        make_skill(&skills, "alpha", "a");
        make_skill(&skills, "beta", "b");
        let wt = base.join("worktree");
        fs::create_dir_all(wt.join(".git")).unwrap();
        fs::create_dir_all(wt.join("stale-skill")).unwrap();
        fs::write(wt.join("stale-skill").join("SKILL.md"), "old").unwrap();
        fs::write(wt.join("README.md"), "gone too").unwrap();

        mirror_skills(&skills, &["alpha".to_string()], &wt).unwrap();
        assert!(wt.join("alpha").join("SKILL.md").is_file());
        assert!(!wt.join("beta").exists());
        assert!(!wt.join("stale-skill").exists());
        assert!(!wt.join("README.md").exists());
        assert!(wt.join(".git").exists());
        let _ = fs::remove_dir_all(&base);
    }

    // End-to-end publish/pull against a local bare remote. Requires git on PATH
    // (present locally and on CI runners).
    #[test]
    fn publish_and_pull_round_trip_with_bare_remote() {
        let git_ok = Command::new("git").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
        if !git_ok {
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
                url: origin.to_string_lossy().to_string(),
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
            run_git(&base, &["clone", origin.to_str().unwrap(), check.to_str().unwrap()]),
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
        expect_ok(run_git(&check, &["push", "origin", "HEAD:refs/heads/main"]), "push teammate").unwrap();

        // Publish again: non-fast-forward must recover via rebase
        fs::write(skills.join("shared").join("SKILL.md"), "---\nname: shared\n---\nv3 local").unwrap();
        let second = publish_scope(&cfg, &skills, &aide_dir, "personal").unwrap();
        assert!(second.pushed);

        // Overwrite local with remote state via pull. The check clone first
        // syncs to aide's publish (no conflict), then advances the remote.
        expect_ok(run_git(&check, &["pull", "--rebase", "origin", "main"]), "sync check").unwrap();
        fs::write(check.join("shared").join("SKILL.md"), "---\nname: shared\n---\nremote truth").unwrap();
        expect_ok(run_git(&check, &["add", "-A"]), "add2").unwrap();
        commit(&check, "remote truth").unwrap();
        expect_ok(run_git(&check, &["push", "origin", "HEAD:refs/heads/main"]), "push2").unwrap();
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
}
