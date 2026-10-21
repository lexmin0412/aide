//! Tauri command layer for aide.
//!
//! All domain logic lives in the `aide-core` crate so the GUI, the CLI and the
//! `aide-mcp` sidecar share one implementation. This file only declares the
//! `#[tauri::command]` entry points, resolves the sidecar binary for agent
//! access, and registers the invoke handler. Keep it thin: behavior belongs in
//! `aide-core`.

use std::path::Path;

use serde::Serialize;

use aide_core::mcp::{ImportResult, McpDrift, McpServerView, McpSyncResult, McpToolView};
use aide_core::project::{ProjectConfig, ProjectStatus, ProjectSyncResult};
use aide_core::{audit, files, git_sync, mcp, models, project, registry, skills};
use aide_core::{FileEntry, SkillInfo, SkillSearchMatch, SyncResult, ToolInfo};

// ── Skills ──

#[tauri::command]
fn list_skills() -> Result<Vec<SkillInfo>, String> {
    skills::skill_infos()
}

#[tauri::command]
fn import_skill(source: String) -> Result<String, String> {
    skills::import_local_skill(&source)
}

#[tauri::command(async)]
fn search_registry(query: String) -> Result<Vec<registry::RegistrySkill>, String> {
    registry::search_registry(&query, 24)
}

#[tauri::command(async)]
fn install_skill_from_registry(id: String) -> Result<registry::RemoteInstallResult, String> {
    let result = registry::install_default(&id, false);
    audit::record(
        "install",
        serde_json::json!({
            "source": "gui",
            "id": id,
            "ok": result.is_ok(),
            "installed": result.as_ref().map(|outcome| outcome.installed.clone()).unwrap_or_default(),
            "error": result.as_ref().err(),
        }),
    );
    result
}

#[tauri::command(async)]
fn check_skill_updates() -> Result<Vec<registry::SkillUpdate>, String> {
    registry::check_updates_default()
}

#[tauri::command(async)]
fn update_skill(skill: String) -> Result<registry::RemoteInstallResult, String> {
    let result = registry::update_default(&skill);
    audit::record(
        "update",
        serde_json::json!({
            "source": "gui",
            "skill": skill,
            "ok": result.is_ok(),
            "error": result.as_ref().err(),
        }),
    );
    result
}

#[tauri::command(async)]
fn search_skills(query: String) -> Result<Vec<SkillSearchMatch>, String> {
    Ok(skills::search_skills(&query))
}

#[tauri::command]
fn update_skill_tags(path: String, tags: Vec<String>) -> Result<(), String> {
    skills::update_skill_tags(Path::new(&path), &tags)
}

/// Aggregated health report for the doctor page. Async because the skill update
/// check can perform GitHub HEAD lookups on a cold 10-minute cache.
#[tauri::command(async)]
fn get_doctor_report() -> Result<aide_core::doctor::DoctorReport, String> {
    aide_core::doctor::doctor_report()
}

// ── Project-scoped profiles ──

#[tauri::command(async)]
fn read_project_config(project_path: String) -> Result<Option<ProjectConfig>, String> {
    project::read_project_config(Path::new(&project_path))
}

#[tauri::command(async)]
fn save_project_config(project_path: String, config: ProjectConfig) -> Result<(), String> {
    project::save_project_config(Path::new(&project_path), &config)
}

#[tauri::command(async)]
fn get_project_status(project_path: String) -> Result<ProjectStatus, String> {
    project::project_status(Path::new(&project_path))
}

#[tauri::command(async)]
fn sync_project(project_path: String) -> Result<ProjectSyncResult, String> {
    project::sync_project(Path::new(&project_path))
}

// ── File-editing commands (path policy lives in aide-core) ──

#[tauri::command]
fn list_directory(path: String) -> Result<Vec<FileEntry>, String> {
    files::list_directory(Path::new(&path))
}

#[tauri::command]
fn read_text_file(path: String) -> Result<String, String> {
    files::read_text_file(Path::new(&path))
}

#[tauri::command]
fn write_text_file(path: String, content: String) -> Result<(), String> {
    files::write_text_file(Path::new(&path), &content)
}

#[tauri::command]
fn create_file(path: String) -> Result<(), String> {
    files::create_file(Path::new(&path))
}

#[tauri::command]
fn create_directory(path: String) -> Result<(), String> {
    files::create_directory(Path::new(&path))
}

#[tauri::command]
fn delete_entry(path: String) -> Result<(), String> {
    files::delete_entry(Path::new(&path))
}

#[tauri::command]
fn rename_entry(old_path: String, new_path: String) -> Result<(), String> {
    files::rename_entry(Path::new(&old_path), Path::new(&new_path))
}

#[tauri::command]
fn file_exists(path: String) -> bool {
    files::file_exists(Path::new(&path))
}

#[tauri::command]
fn get_file_mtime(path: String) -> Result<u64, String> {
    files::file_mtime(Path::new(&path))
}

#[tauri::command]
fn get_home_dir() -> String {
    aide_core::home_dir_string()
}

// ── Tool sync ──

#[tauri::command]
fn list_tools() -> Vec<ToolInfo> {
    skills::tool_infos()
}

#[tauri::command]
fn check_sync_statuses() -> Vec<ToolInfo> {
    skills::tool_infos()
}

#[tauri::command(async)]
fn sync_tool(tool_key: String, overwrite: Option<bool>) -> Result<SyncResult, String> {
    skills::sync_tool(&tool_key, overwrite.unwrap_or(false))
}

#[tauri::command(async)]
fn sync_all_tools() -> Vec<SyncResult> {
    skills::sync_all_skills_to_tools()
}

// ── MCP servers ──

#[tauri::command]
fn list_mcp_servers() -> Result<Vec<McpServerView>, String> {
    mcp::server_views()
}

#[tauri::command]
fn save_mcp_servers(servers: Vec<McpServerView>) -> Result<(), String> {
    mcp::save_server_views(servers)
}

#[tauri::command]
fn list_mcp_tools() -> Vec<McpToolView> {
    mcp::tool_views()
}

#[tauri::command(async)]
fn sync_mcp_tool(tool_key: String) -> Result<McpSyncResult, String> {
    mcp::sync_tool_view(&tool_key)
}

#[tauri::command(async)]
fn sync_mcp_all() -> Result<Vec<McpSyncResult>, String> {
    mcp::sync_all_views()
}

#[tauri::command(async)]
fn import_mcp_all() -> Result<Vec<ImportResult>, String> {
    mcp::import_all()
}

#[tauri::command(async)]
fn get_mcp_drift() -> Result<Vec<McpDrift>, String> {
    mcp::drift()
}

#[tauri::command(async)]
fn cleanup_mcp_drift() -> Result<Vec<McpSyncResult>, String> {
    mcp::cleanup_drift()
}

// ── Model profiles ──

#[tauri::command(async)]
fn list_model_providers(force: Option<bool>) -> Result<Vec<models::ProviderSummary>, String> {
    models::fetch_models_dev(&aide_core::aide_dir()?, force.unwrap_or(false))
}

#[tauri::command]
fn list_model_profiles() -> Result<models::ModelProfilesConfig, String> {
    models::read_profiles(&aide_core::aide_dir()?)
}

#[tauri::command]
fn save_model_profiles(config: models::ModelProfilesConfig) -> Result<(), String> {
    models::write_profiles(&aide_core::aide_dir()?, &config)
}

#[tauri::command]
fn sync_model_profile(profile_id: String) -> Result<Vec<models::ModelSyncResult>, String> {
    models::sync_profile_by_id(&profile_id)
}

#[tauri::command]
fn sync_all_model_profiles() -> Result<Vec<models::ModelProfileSyncResult>, String> {
    models::sync_all_default()
}

// ── Git scopes ──

#[tauri::command]
fn get_git_config() -> Result<git_sync::GitRemotesConfig, String> {
    git_sync::read_default()
}

#[tauri::command]
fn save_git_config(config: git_sync::GitRemotesConfig) -> Result<(), String> {
    git_sync::write_default(&config)
}

#[tauri::command]
fn set_skill_scope(skill: String, scope: Option<String>) -> Result<(), String> {
    git_sync::set_skill_scope(&skill, scope.as_deref())
}

#[tauri::command(async)]
fn publish_scope(scope: String) -> Result<git_sync::PublishOutcome, String> {
    git_sync::publish_default(&scope)
}

#[tauri::command(async)]
fn pull_scope(scope: String) -> Result<git_sync::PullOutcome, String> {
    git_sync::pull_default(&scope)
}

// ── Agent access (sidecar registration) ──

#[derive(Serialize)]
pub struct AgentServerStatus {
    pub enabled: bool,
    pub binary_available: bool,
    pub command: Option<String>,
}

fn agent_server_status() -> AgentServerStatus {
    let binary = mcp::sidecar_path();
    // Enabled only when the registered command still points at the sidecar that
    // ships with the running build: after an upgrade the stored path is stale
    // and the agent would silently fail to start.
    let enabled = binary.as_ref().is_some_and(|current| {
        mcp::read_central()
            .map(|central| {
                central
                    .servers
                    .get(mcp::AGENT_SERVER_NAME)
                    .map(|server| {
                        !server.disabled.unwrap_or(false)
                            && server
                                .command
                                .as_deref()
                                .map(|command| Path::new(command) == current.as_path())
                                .unwrap_or(false)
                    })
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    });
    AgentServerStatus {
        enabled,
        binary_available: binary.is_some(),
        command: binary.map(|path| path.to_string_lossy().to_string()),
    }
}

#[tauri::command]
fn get_agent_server() -> AgentServerStatus {
    agent_server_status()
}

#[tauri::command]
fn enable_agent_access(enable: bool) -> Result<AgentServerStatus, String> {
    mcp::set_agent_server(enable, mcp::sidecar_path().as_deref())?;
    audit::record(
        "enable_agent_access",
        serde_json::json!({ "source": "gui", "enabled": enable }),
    );
    Ok(agent_server_status())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            list_tools,
            list_mcp_tools,
            list_mcp_servers,
            save_mcp_servers,
            sync_mcp_tool,
            sync_mcp_all,
            import_mcp_all,
            get_mcp_drift,
            cleanup_mcp_drift,
            check_sync_statuses,
            check_skill_updates,
            update_skill,
            get_agent_server,
            enable_agent_access,
            list_model_providers,
            list_model_profiles,
            save_model_profiles,
            sync_model_profile,
            sync_all_model_profiles,
            sync_tool,
            sync_all_tools,
            list_skills,
            import_skill,
            search_registry,
            install_skill_from_registry,
            search_skills,
            list_directory,
            read_text_file,
            write_text_file,
            create_file,
            create_directory,
            delete_entry,
            rename_entry,
            file_exists,
            get_file_mtime,
            get_home_dir,
            get_git_config,
            save_git_config,
            set_skill_scope,
            publish_scope,
            pull_scope,
            update_skill_tags,
            get_doctor_report,
            read_project_config,
            save_project_config,
            get_project_status,
            sync_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_server_status_keeps_its_json_contract() {
        let status = AgentServerStatus {
            enabled: true,
            binary_available: true,
            command: Some("/tmp/aide-mcp".to_string()),
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["enabled"], true);
        assert_eq!(json["binary_available"], true);
        assert_eq!(json["command"], "/tmp/aide-mcp");
    }
}
