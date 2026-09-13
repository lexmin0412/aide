//! aide-mcp: headless companion to the aide desktop app.
//!
//! Run modes:
//! - `aide-mcp` / `aide-mcp serve` — MCP stdio JSON-RPC server so AI agents
//!   can install, update and publish skills through aide.
//! - `aide-mcp list|search|install|updates|update|publish|pull` — a plain CLI
//!   over the same core.
//!
//! The server exposes only skills-domain operations; MCP config editing and
//! scope management stay in the GUI on purpose.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use aide_lib::git_sync;
use aide_lib::registry;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const SUPPORTED_PROTOCOLS: [&str; 2] = ["2025-06-18", "2025-03-26"];

fn home_dir() -> Result<std::path::PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())
}

fn skills_dir() -> Result<std::path::PathBuf, String> {
    let dir = aide_lib::resolve_skills_source()?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn aide_dir() -> Result<std::path::PathBuf, String> {
    Ok(home_dir()?.join(".aide"))
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "list_skills",
            "description": "List installed skills from ~/.agents/skills with description, tags, scope and install source.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "search_skills",
            "description": "Case-insensitive full-text search across the files of installed skills.",
            "inputSchema": {
                "type": "object",
                "properties": { "query": { "type": "string", "description": "Search text (min 2 chars)" } },
                "required": ["query"]
            }
        },
        {
            "name": "search_registry",
            "description": "Search the skills.sh community registry for installable skills.",
            "inputSchema": {
                "type": "object",
                "properties": { "query": { "type": "string", "description": "Search text (min 2 chars)" } },
                "required": ["query"]
            }
        },
        {
            "name": "install_skill",
            "description": "Install a skill from a GitHub repo. Id formats: owner/repo, owner/repo/skill, owner/repo@skill, or a github.com URL. Existing skills are skipped; installed content is recorded with provenance.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "string", "description": "e.g. vercel-labs/agent-skills@web-design-guidelines" } },
                "required": ["id"]
            }
        },
        {
            "name": "check_updates",
            "description": "List registry-installed skills whose upstream repo has new commits.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "update_skill",
            "description": "Re-install a registry-installed skill from its upstream HEAD. Local modifications inside that skill are replaced (the replaced copy goes to the trash).",
            "inputSchema": {
                "type": "object",
                "properties": { "skill": { "type": "string", "description": "Skill folder name" } },
                "required": ["skill"]
            }
        },
        {
            "name": "publish_scope",
            "description": "Mirror, commit and push the scope's skills to its git remote. Omit scope to use the default scope.",
            "inputSchema": {
                "type": "object",
                "properties": { "scope": { "type": "string" } }
            }
        },
        {
            "name": "pull_scope",
            "description": "Fast-forward pull a scope's git remote and write remote-side skills back locally. Omit scope to use the default scope.",
            "inputSchema": {
                "type": "object",
                "properties": { "scope": { "type": "string" } }
            }
        }
    ])
}

fn text_result(text: String) -> Result<String, String> {
    Ok(text)
}

fn call_tool(name: &str, args: &Value) -> Result<String, String> {
    match name {
        "list_skills" => {
            let skills = aide_lib::skill_infos()?;
            text_result(serde_json::to_string_pretty(&skills).map_err(|e| e.to_string())?)
        }
        "search_skills" => {
            let query = args["query"]
                .as_str()
                .ok_or("Missing required argument: query")?;
            let matches =
                aide_lib::search_skills_in(&skills_dir()?, query);
            text_result(
                serde_json::to_string_pretty(&matches).map_err(|e| e.to_string())?,
            )
        }
        "search_registry" => {
            let query = args["query"]
                .as_str()
                .ok_or("Missing required argument: query")?;
            let skills = registry::search_registry(query, 20)?;
            text_result(
                serde_json::to_string_pretty(&skills).map_err(|e| e.to_string())?,
            )
        }
        "install_skill" => {
            let id = args["id"].as_str().ok_or("Missing required argument: id")?;
            let result = registry::install_from_id(id, &skills_dir()?, &aide_dir()?, false)?;
            if !result.installed.is_empty() {
                text_result(format!(
                    "Installed: {}",
                    result.installed.join(", ")
                ))
            } else if !result.skipped.is_empty() {
                text_result(format!("Already installed: {}", result.skipped.join(", ")))
            } else {
                text_result("Nothing to install".into())
            }
        }
        "check_updates" => {
            let updates = registry::check_updates(&aide_dir()?);
            if updates.is_empty() {
                return text_result("All registry-installed skills are up to date".into());
            }
            text_result(
                serde_json::to_string_pretty(&updates).map_err(|e| e.to_string())?,
            )
        }
        "update_skill" => {
            let skill = args["skill"]
                .as_str()
                .ok_or("Missing required argument: skill")?;
            let sources = registry::read_sources(&aide_dir()?);
            let meta = sources
                .get(skill)
                .ok_or_else(|| format!("No install source recorded for \"{skill}\""))?
                .clone();
            let result =
                registry::install_from_id(&meta.id, &skills_dir()?, &aide_dir()?, true)?;
            text_result(format!("Updated: {}", result.installed.join(", ")))
        }
        "publish_scope" | "pull_scope" => {
            let aide = aide_dir()?;
            let config = git_sync::read_config(&aide);
            let scope = match args["scope"].as_str() {
                Some(s) => s.to_string(),
                None => git_sync::default_scope_name(&config)
                    .ok_or("No git scopes configured in ~/.aide/remotes.json")?,
            };
            let outcome = if name == "publish_scope" {
                let o = git_sync::publish_scope(&config, &skills_dir()?, &aide, &scope)?;
                format!(
                    "Published scope \"{scope}\": {} skill(s), committed: {}, pushed: {}",
                    o.skills.len(),
                    o.committed,
                    o.pushed
                )
            } else {
                let o = git_sync::pull_scope(&config, &skills_dir()?, &aide, &scope)?;
                format!(
                    "Pulled scope \"{scope}\": {} skill(s) updated from remote",
                    o.updated.len()
                )
            };
            text_result(outcome)
        }
        _ => Err(format!("Unknown tool: {name}")),
    }
}

fn run_mcp_server() -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    eprintln!("aide-mcp {VERSION} listening on stdio");
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        if method.starts_with("notifications/") {
            continue;
        }
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        let response = match method {
            "initialize" => {
                let requested = params["protocolVersion"].as_str().unwrap_or("2025-06-18");
                let negotiated = if SUPPORTED_PROTOCOLS.contains(&requested) {
                    requested
                } else {
                    "2025-06-18"
                };
                Ok(json!({
                    "protocolVersion": negotiated,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "aide", "version": VERSION }
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_definitions() })),
            "tools/call" => {
                let tool_name = params["name"].as_str().unwrap_or("").to_string();
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
                match call_tool(&tool_name, &arguments) {
                    Ok(text) => Ok(json!({ "content": [ { "type": "text", "text": text } ] })),
                    Err(e) => Ok(json!({
                        "content": [ { "type": "text", "text": e } ],
                        "isError": true
                    })),
                }
            }
            other => Err((
                -32601,
                format!("Method not found: {other}"),
            )),
        };

        let payload = match response {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": code, "message": message }
            }),
        };
        writeln!(stdout, "{payload}").map_err(|e| e.to_string())?;
        stdout.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn usage() -> String {
    format!(
        "aide-mcp {VERSION}\n\n\
         Usage:\n  \
         aide-mcp [serve]              Run the MCP stdio server for AI agents\n  \
         aide-mcp list                 List installed skills\n  \
         aide-mcp search <query>       Full-text search installed skills\n  \
         aide-mcp search-registry <q>  Search the skills.sh registry\n  \
         aide-mcp install <id>         Install a skill from GitHub\n  \
         aide-mcp updates              List skills with upstream changes\n  \
         aide-mcp update <skill>       Update a skill from its upstream\n  \
         aide-mcp publish [scope]      Publish a scope to its git remote\n  \
         aide-mcp pull [scope]         Pull a scope from its git remote"
    )
}

fn cli_main(args: &[String]) -> Result<String, String> {
    match args.first().map(|s| s.as_str()) {
        None | Some("serve") => {
            run_mcp_server()?;
            Ok(String::new())
        }
        Some("--help") | Some("-h") => Ok(usage()),
        Some("--version") | Some("-V") => Ok(format!("aide-mcp {VERSION}")),
        Some("list") => {
            let skills = aide_lib::skill_infos()?;
            if skills.is_empty() {
                return Ok("No skills installed".into());
            }
            Ok(skills
                .iter()
                .map(|s| {
                    format!(
                        "{}{} - {}",
                        s.display_name,
                        if s.is_symlink { " (symlink)" } else { "" },
                        if s.description.is_empty() {
                            "(no description)"
                        } else {
                            &s.description
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        Some("search") => {
            let query = args
                .get(1)
                .ok_or("Usage: aide-mcp search <query>")?;
            let matches = aide_lib::search_skills_in(&skills_dir()?, query);
            if matches.is_empty() {
                return Ok("No matches".into());
            }
            Ok(matches
                .iter()
                .map(|m| format!("{}:{}  {}", m.display_name, m.line_number, m.line_text))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        Some("search-registry") => {
            let query = args
                .get(1)
                .ok_or("Usage: aide-mcp search-registry <query>")?;
            let skills = registry::search_registry(query, 20)?;
            Ok(skills
                .iter()
                .map(|s| format!("{}  ({} installs)  https://skills.sh/{}", s.id, s.installs, s.id))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        Some("install") => {
            let id = args.get(1).ok_or("Usage: aide-mcp install <id>")?;
            let result = registry::install_from_id(id, &skills_dir()?, &aide_dir()?, false)?;
            if !result.installed.is_empty() {
                Ok(format!("Installed: {}", result.installed.join(", ")))
            } else if !result.skipped.is_empty() {
                Ok(format!("Already installed: {}", result.skipped.join(", ")))
            } else {
                Ok("Nothing to install".into())
            }
        }
        Some("updates") => {
            let updates = registry::check_updates(&aide_dir()?);
            if updates.is_empty() {
                Ok("All registry-installed skills are up to date".into())
            } else {
                Ok(updates
                    .iter()
                    .map(|u| format!("{}  ({} -> {})", u.skill, &u.installed_commit[..7.min(u.installed_commit.len())], &u.latest_commit[..7.min(u.latest_commit.len())]))
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
        }
        Some("update") => {
            let skill = args.get(1).ok_or("Usage: aide-mcp update <skill>")?;
            let sources = registry::read_sources(&aide_dir()?);
            let meta = sources
                .get(skill)
                .ok_or_else(|| format!("No install source recorded for \"{skill}\""))?
                .clone();
            let result = registry::install_from_id(&meta.id, &skills_dir()?, &aide_dir()?, true)?;
            Ok(format!("Updated: {}", result.installed.join(", ")))
        }
        Some("publish") | Some("pull") => {
            let mode = args[0].clone();
            let aide = aide_dir()?;
            let config = git_sync::read_config(&aide);
            let scope = match args.get(1) {
                Some(s) => s.clone(),
                None => git_sync::default_scope_name(&config)
                    .ok_or("No git scopes configured in ~/.aide/remotes.json")?,
            };
            if mode == "publish" {
                let o = git_sync::publish_scope(&config, &skills_dir()?, &aide, &scope)?;
                Ok(format!(
                    "Published \"{scope}\": {} skill(s), committed: {}, pushed: {}",
                    o.skills.len(),
                    o.committed,
                    o.pushed
                ))
            } else {
                let o = git_sync::pull_scope(&config, &skills_dir()?, &aide, &scope)?;
                Ok(format!(
                    "Pulled \"{scope}\": {} skill(s) updated",
                    o.updated.len()
                ))
            }
        }
        Some(other) => Err(format!(
            "Unknown command \"{other}\".\n\n{}",
            usage()
        )),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli_main(&args) {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
