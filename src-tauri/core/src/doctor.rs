//! Aggregated environment health report ("doctor").
//!
//! The doctor answers one question: is the agent environment consistent? It
//! aggregates the existing checks (skills, tool sync state, MCP drift, model
//! profiles, central config parsing, agent access, audit log) instead of
//! reimplementing them.
//!
//! Every section is fault tolerant: a single failing check degrades to an empty
//! value plus a recorded error and never fails the report. Missing files are
//! normal; only files that exist and cannot be parsed are reported.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::mcp::{self, McpDrift};
use crate::models;
use crate::registry;
use crate::skills;

/// The managed MCP manifest lives next to the central config.
const MANAGED_FILE: &str = "mcp-managed.json";

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub generated_at: u64,
    pub skills: SkillsSection,
    pub tools: Vec<ToolHealth>,
    pub mcp: McpSection,
    pub models: ModelsSection,
    pub configs: Vec<ConfigProblem>,
    pub agent: AgentSection,
    pub audit: AuditSection,
}

#[derive(Debug, Default, Serialize)]
pub struct SkillsSection {
    pub total: u32,
    /// Skills installed as symlinks into another checkout.
    pub linked: u32,
    /// Directories without a SKILL.md, or dangling symlinks.
    pub broken: Vec<String>,
    pub updates_available: u32,
    pub update_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ToolHealth {
    pub key: String,
    pub name: String,
    pub status: String,
    pub global_skills: String,
}

#[derive(Debug, Default, Serialize)]
pub struct McpSection {
    pub servers: u32,
    pub disabled: u32,
    pub drift: Vec<McpDrift>,
    pub errors: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ModelsSection {
    pub profiles: u32,
    /// `"<profile name> -> <tool key>"` for targets missing from their config.
    pub missing_targets: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ConfigProblem {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Default, Serialize)]
pub struct AgentSection {
    pub enabled: bool,
    pub binary_available: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct AuditSection {
    pub path: String,
    pub size_bytes: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Health report for the real home directory.
pub fn doctor_report() -> Result<DoctorReport, String> {
    let home = crate::home_dir()?;
    doctor_report_in(&home)
}

/// Health report for an explicit home directory (tests, tooling).
pub fn doctor_report_in(home: &Path) -> Result<DoctorReport, String> {
    Ok(DoctorReport {
        generated_at: now_secs(),
        skills: skills_section(home),
        tools: tools_section(home),
        mcp: mcp_section(home),
        models: models_section(home),
        configs: config_problems(home),
        agent: agent_section(home),
        audit: audit_section(home),
    })
}

fn skills_section(home: &Path) -> SkillsSection {
    let skills_dir = home.join(".agents").join("skills");
    let mut section = SkillsSection::default();
    if skills_dir.is_dir() {
        // Reuse the cached listing; if a central config is unparsable the
        // listing errors, so degrade to a raw directory count (the corrupt file
        // itself is reported by `config_problems`).
        match skills::skill_infos_at(home, &skills_dir, now_secs()) {
            Ok(infos) => {
                section.total = infos.len() as u32;
                section.linked = infos.iter().filter(|info| info.is_symlink).count() as u32;
            }
            Err(_) => {
                let (total, linked) = count_skill_dirs(&skills_dir);
                section.total = total;
                section.linked = linked;
            }
        }
        section.broken = broken_skills(&skills_dir);
    }
    // The update check reuses the 10-minute GitHub HEAD cache; a network/API
    // failure reports `update_error` and leaves the count at 0.
    match registry::check_updates(&home.join(".aide")) {
        Ok(updates) => section.updates_available = updates.len() as u32,
        Err(err) => section.update_error = Some(err),
    }
    section
}

/// Fallback counts when the cached listing cannot be produced.
fn count_skill_dirs(skills_dir: &Path) -> (u32, u32) {
    let mut total = 0u32;
    let mut linked = 0u32;
    let Ok(entries) = fs::read_dir(skills_dir) else {
        return (0, 0);
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            if entry.path().is_dir() {
                total += 1;
                linked += 1;
            }
        } else if file_type.is_dir() {
            total += 1;
        }
    }
    (total, linked)
}

fn broken_skills(skills_dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(skills_dir) else {
        return Vec::new();
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    let mut broken = Vec::new();
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            // Dangling link, or a linked folder with no SKILL.md.
            if !path.exists() || (path.is_dir() && !path.join("SKILL.md").is_file()) {
                broken.push(name);
            }
            continue;
        }
        if file_type.is_dir() && !path.join("SKILL.md").is_file() {
            broken.push(name);
        }
    }
    broken
}

fn tools_section(home: &Path) -> Vec<ToolHealth> {
    skills::tool_infos_at(home)
        .into_iter()
        .map(|tool| ToolHealth {
            key: tool.key,
            name: tool.name,
            status: tool.status,
            global_skills: tool.global_skills,
        })
        .collect()
}

fn mcp_section(home: &Path) -> McpSection {
    let aide_dir = home.join(".aide");
    // Parse failures are reported by `config_problems`; here a corrupt central
    // config simply means "no known servers".
    let central = mcp::read_central_at(&aide_dir.join("mcp.json")).unwrap_or_default();
    let managed = mcp::read_managed_at(&aide_dir.join(MANAGED_FILE)).unwrap_or_default();

    let mut section = McpSection {
        servers: central.servers.len() as u32,
        disabled: central
            .servers
            .values()
            .filter(|server| server.disabled.unwrap_or(false))
            .count() as u32,
        ..Default::default()
    };
    for adapter in mcp::all_mcp_adapters() {
        let config_path = adapter.resolve_path_in(home);
        match mcp::compute_drift(&adapter, &config_path, &central, &managed) {
            Ok(Some(drift)) => section.drift.push(McpDrift {
                tool_key: adapter.key.to_string(),
                tool_name: adapter.name.to_string(),
                missing: drift.missing,
                leftover: drift.leftover,
            }),
            Ok(None) => {}
            Err(err) => section.errors.push(format!("{}: {err}", adapter.name)),
        }
    }
    section
}

fn models_section(home: &Path) -> ModelsSection {
    let mut section = ModelsSection::default();
    let config = match models::read_profiles(&home.join(".aide")) {
        Ok(config) => config,
        Err(err) => {
            section.errors.push(err);
            return section;
        }
    };
    section.profiles = config.profiles.len() as u32;
    for profile in &config.profiles {
        for target in &profile.targets {
            match target_present(home, profile, target) {
                Ok(Some(true)) | Ok(None) => {}
                Ok(Some(false)) => section
                    .missing_targets
                    .push(format!("{} -> {target}", profile.name)),
                Err(err) => section
                    .errors
                    .push(format!("{} -> {target}: {err}", profile.name)),
            }
        }
    }
    section
}

/// Is the profile's provider block present in the target tool config?
///
/// `Ok(None)` means the target has no file-based config and is skipped, a
/// missing config file counts as `Ok(Some(false))` (missing target).
fn target_present(
    home: &Path,
    profile: &models::ModelProfile,
    target: &str,
) -> Result<Option<bool>, String> {
    let marker = models::profile_provider_name(profile);
    match target {
        "claude_code" => {
            let path = home.join(".claude").join("settings.json");
            if !path.exists() {
                return Ok(Some(false));
            }
            let root = models::read_jsonc_object(&path)?;
            let present = root
                .get("env")
                .and_then(|env| env.as_object())
                .is_some_and(|env| {
                    env.contains_key("ANTHROPIC_API_KEY")
                        || env.contains_key("ANTHROPIC_AUTH_TOKEN")
                });
            Ok(Some(present))
        }
        "opencode" => {
            let path = mcp::opencode_config_path(home);
            if !path.exists() {
                return Ok(Some(false));
            }
            let root = models::read_jsonc_object(&path)?;
            let present = root
                .get("provider")
                .and_then(|providers| providers.as_object())
                .is_some_and(|providers| providers.contains_key(&marker));
            Ok(Some(present))
        }
        "codex" => {
            let path = home.join(".codex").join("config.toml");
            if !path.exists() {
                return Ok(Some(false));
            }
            let table = models::read_toml_table(&path)?;
            let present = table
                .get("model_providers")
                .and_then(|providers| providers.as_table())
                .is_some_and(|providers| providers.contains_key(&marker));
            Ok(Some(present))
        }
        // Tools without file-based model configuration cannot drift.
        _ => Ok(None),
    }
}

fn config_problems(home: &Path) -> Vec<ConfigProblem> {
    let aide_dir = home.join(".aide");
    let mut problems = Vec::new();

    let mut record = |path: PathBuf, error: String| {
        problems.push(ConfigProblem {
            path: path.to_string_lossy().to_string(),
            error,
        });
    };

    if let Err(err) = mcp::read_central_at(&aide_dir.join("mcp.json")) {
        record(aide_dir.join("mcp.json"), err);
    }
    if let Err(err) = models::read_profiles(&aide_dir) {
        record(aide_dir.join("models.json"), err);
    }
    if let Err(err) = crate::git_sync::read_config(&aide_dir) {
        record(aide_dir.join("remotes.json"), err);
    }
    if let Err(err) = registry::read_sources(&aide_dir) {
        record(aide_dir.join("skill-sources.json"), err);
    }
    if let Err(err) = mcp::read_managed_at(&aide_dir.join(MANAGED_FILE)) {
        record(aide_dir.join(MANAGED_FILE), err);
    }
    problems
}

fn agent_section(home: &Path) -> AgentSection {
    let central = mcp::read_central_at(&home.join(".aide").join("mcp.json")).unwrap_or_default();
    let sidecar = mcp::sidecar_path();
    // Same rule as `get_agent_server`: the registered command must still point
    // at the sidecar shipped with the running build.
    let enabled = sidecar.as_ref().is_some_and(|current| {
        central
            .servers
            .get(mcp::AGENT_SERVER_NAME)
            .is_some_and(|server| {
                !server.disabled.unwrap_or(false)
                    && server
                        .command
                        .as_deref()
                        .is_some_and(|command| Path::new(command) == current.as_path())
            })
    });
    AgentSection {
        enabled,
        binary_available: sidecar.is_some(),
    }
}

fn audit_section(home: &Path) -> AuditSection {
    let path = home.join(".aide").join("audit.jsonl");
    let size_bytes = fs::metadata(&path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    AuditSection {
        path: path.to_string_lossy().to_string(),
        size_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage;

    fn temp_home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aide-doctor-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_central(home: &Path, servers: Vec<(&str, Option<bool>, Vec<&str>)>) {
        let mut map = std::collections::HashMap::new();
        for (name, disabled, targets) in servers {
            map.insert(
                name.to_string(),
                mcp::McpServerConfig {
                    command: Some("npx".into()),
                    args: None,
                    url: None,
                    env: None,
                    headers: None,
                    disabled,
                    description: None,
                    targets: targets.into_iter().map(str::to_string).collect(),
                },
            );
        }
        storage::write_json(
            &home.join(".aide").join("mcp.json"),
            &mcp::McpCentralConfig { servers: map },
            None,
        )
        .unwrap();
    }

    #[test]
    fn report_serializes_the_agreed_json_contract() {
        let home = temp_home("contract");
        write_central(&home, vec![("depot", Some(true), vec!["cursor"])]);
        let report = doctor_report_in(&home).unwrap();
        let value = serde_json::to_value(&report).unwrap();
        let object = value.as_object().expect("report object");
        for key in [
            "generated_at",
            "skills",
            "tools",
            "mcp",
            "models",
            "configs",
            "agent",
            "audit",
        ] {
            assert!(object.contains_key(key), "missing top-level key {key}");
        }
        let skills = &value["skills"];
        for key in [
            "total",
            "linked",
            "broken",
            "updates_available",
            "update_error",
        ] {
            assert!(skills.get(key).is_some(), "missing skills.{key}");
        }
        let mcp_section = &value["mcp"];
        for key in ["servers", "disabled", "drift", "errors"] {
            assert!(mcp_section.get(key).is_some(), "missing mcp.{key}");
        }
        for key in ["profiles", "missing_targets", "errors"] {
            assert!(value["models"].get(key).is_some(), "missing models.{key}");
        }
        for key in ["enabled", "binary_available"] {
            assert!(value["agent"].get(key).is_some(), "missing agent.{key}");
        }
        for key in ["path", "size_bytes"] {
            assert!(value["audit"].get(key).is_some(), "missing audit.{key}");
        }
        // Tool entries expose exactly the contract fields.
        let tool = value["tools"].as_array().unwrap()[0].as_object().unwrap();
        let mut keys: Vec<&str> = tool.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, vec!["global_skills", "key", "name", "status"]);
        // A disabled central server shows up in the counters.
        assert_eq!(value["mcp"]["servers"], 1);
        assert_eq!(value["mcp"]["disabled"], 1);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn empty_environment_reports_zeros_and_is_not_an_error() {
        let home = temp_home("empty");
        let report = doctor_report_in(&home).unwrap();

        assert_eq!(report.skills.total, 0);
        assert_eq!(report.skills.linked, 0);
        assert!(report.skills.broken.is_empty());
        assert_eq!(report.skills.updates_available, 0);
        assert!(report.skills.update_error.is_none());
        assert_eq!(report.tools.len(), crate::adapter::all_tools().len());
        assert!(report
            .tools
            .iter()
            .all(|tool| tool.status == "not_installed"));
        assert_eq!(report.mcp.servers, 0);
        assert_eq!(report.mcp.disabled, 0);
        assert!(report.mcp.drift.is_empty());
        assert!(report.mcp.errors.is_empty());
        assert_eq!(report.models.profiles, 0);
        assert!(report.models.missing_targets.is_empty());
        assert!(report.models.errors.is_empty());
        assert!(report.configs.is_empty());
        assert!(!report.agent.enabled);
        assert!(report.audit.path.ends_with("audit.jsonl"));
        assert_eq!(report.audit.size_bytes, 0);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn skill_without_skill_md_is_reported_broken() {
        let home = temp_home("broken-skill");
        let skills = home.join(".agents").join("skills");
        fs::create_dir_all(skills.join("good")).unwrap();
        fs::write(
            skills.join("good").join("SKILL.md"),
            "---\nname: good\n---\n",
        )
        .unwrap();
        fs::create_dir_all(skills.join("broken")).unwrap();

        let report = doctor_report_in(&home).unwrap();
        // `total` mirrors the cached listing, which lists every directory in
        // the store; `broken` calls out the ones that cannot load as a skill.
        assert_eq!(report.skills.total, 2);
        assert_eq!(report.skills.broken, vec!["broken".to_string()]);
        let _ = fs::remove_dir_all(&home);
    }

    #[cfg(unix)]
    #[test]
    fn linked_skills_are_counted_and_dangling_links_are_broken() {
        let home = temp_home("linked-skill");
        let skills = home.join(".agents").join("skills");
        let outside = home.join("checkout-skill");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("SKILL.md"), "---\nname: linked\n---\n").unwrap();
        fs::create_dir_all(&skills).unwrap();
        std::os::unix::fs::symlink(&outside, skills.join("linked")).unwrap();
        std::os::unix::fs::symlink(home.join("missing-target"), skills.join("dangling")).unwrap();

        let report = doctor_report_in(&home).unwrap();
        assert_eq!(report.skills.total, 1);
        assert_eq!(report.skills.linked, 1);
        assert_eq!(report.skills.broken, vec!["dangling".to_string()]);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn corrupt_central_config_is_reported_without_failing_the_report() {
        let home = temp_home("corrupt-config");
        fs::create_dir_all(home.join(".aide")).unwrap();
        fs::write(home.join(".aide").join("mcp.json"), "{\"servers\": ").unwrap();

        let report = doctor_report_in(&home).unwrap();
        assert_eq!(report.mcp.servers, 0);
        let problem = report
            .configs
            .iter()
            .find(|problem| problem.path.ends_with("mcp.json"))
            .expect("mcp config problem");
        assert!(problem.error.contains("parse"), "{}", problem.error);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn disabled_servers_are_counted() {
        let home = temp_home("disabled");
        write_central(
            &home,
            vec![
                ("enabled-one", None, vec![]),
                ("disabled-one", Some(true), vec![]),
            ],
        );
        let report = doctor_report_in(&home).unwrap();
        assert_eq!(report.mcp.servers, 2);
        assert_eq!(report.mcp.disabled, 1);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn unparsable_tool_config_is_reported_as_mcp_drift_error() {
        let home = temp_home("drift-error");
        write_central(&home, vec![("depot", None, vec!["cursor"])]);
        fs::create_dir_all(home.join(".cursor")).unwrap();
        fs::write(home.join(".cursor").join("mcp.json"), "{\"mcpServers\": ").unwrap();

        let report = doctor_report_in(&home).unwrap();
        assert_eq!(report.mcp.errors.len(), 1);
        assert!(
            report.mcp.errors[0].contains("Cursor"),
            "{}",
            report.mcp.errors[0]
        );
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn missing_model_target_is_reported_and_presence_clears_it() {
        let home = temp_home("models");
        let aide_dir = home.join(".aide");
        fs::create_dir_all(&aide_dir).unwrap();
        let config = models::ModelProfilesConfig {
            profiles: vec![models::ModelProfile {
                id: "p1".into(),
                name: "Personal".into(),
                provider: "anthropic".into(),
                base_url: String::new(),
                api_key: "sk-test".into(),
                model: String::new(),
                models: vec![],
                targets: vec!["claude_code".into(), "cursor".into()],
            }],
        };
        models::write_profiles(&aide_dir, &config).unwrap();

        let report = doctor_report_in(&home).unwrap();
        assert_eq!(report.models.profiles, 1);
        assert_eq!(
            report.models.missing_targets,
            vec!["Personal -> claude_code".to_string()]
        );
        assert!(report.models.errors.is_empty());

        // Writing the target config clears the missing-target entry; the
        // unsupported `cursor` target is skipped instead of reported missing.
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::write(
            home.join(".claude").join("settings.json"),
            "{\"env\": {\"ANTHROPIC_API_KEY\": \"sk-test\"}}",
        )
        .unwrap();
        let refreshed = doctor_report_in(&home).unwrap();
        assert!(refreshed.models.missing_targets.is_empty());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn corrupt_sources_file_is_reported_and_skills_still_counted() {
        let home = temp_home("corrupt-sources");
        let skills = home.join(".agents").join("skills");
        fs::create_dir_all(skills.join("alpha")).unwrap();
        fs::write(
            skills.join("alpha").join("SKILL.md"),
            "---\nname: alpha\n---\n",
        )
        .unwrap();
        fs::create_dir_all(home.join(".aide")).unwrap();
        fs::write(home.join(".aide").join("skill-sources.json"), "{broken").unwrap();

        let report = doctor_report_in(&home).unwrap();
        // The cached listing errored on the corrupt provenance file; the raw
        // scan still reports the installed skill.
        assert_eq!(report.skills.total, 1);
        assert!(report.skills.update_error.is_some());
        assert!(report
            .configs
            .iter()
            .any(|problem| problem.path.ends_with("skill-sources.json")));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn audit_size_and_agent_are_reflected() {
        let home = temp_home("audit-agent");
        fs::create_dir_all(home.join(".aide")).unwrap();
        fs::write(
            home.join(".aide").join("audit.jsonl"),
            "{\"action\":\"install\"}\n",
        )
        .unwrap();

        let report = doctor_report_in(&home).unwrap();
        assert!(report.audit.size_bytes > 0);
        assert!(report.audit.path.ends_with("audit.jsonl"));
        assert!(!report.agent.enabled);
        // Binary availability mirrors the shared sidecar locator (false in test
        // builds, where no sidecar sits next to the test binary).
        assert_eq!(report.agent.binary_available, mcp::sidecar_path().is_some());
        let _ = fs::remove_dir_all(&home);
    }
}
