// Model/provider management backed by models.dev. Users store named profiles
// (provider + API key + optional base URL and model selection) in a local
// 0600 file and sync them into the tools that support file-based model
// configuration. Keys live only in ~/.aide/models.json — never in git.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::mcp::strip_jsonc_comments;
use crate::storage;

const MODELS_DEV_API: &str = "https://models.dev/api.json";
const CACHE_TTL: u64 = 24 * 60 * 60;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

// ── Central profile store ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub id: String,
    pub name: String,
    /// models.dev provider id, e.g. "anthropic"
    pub provider: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    /// Optional default model id written to tools that take one.
    #[serde(default)]
    pub model: String,
    /// Model ids exposed to tools that enumerate a provider's models.
    #[serde(default)]
    pub models: Vec<String>,
    /// Tool keys this profile syncs to.
    #[serde(default)]
    pub targets: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct ModelProfilesConfig {
    #[serde(default)]
    pub profiles: Vec<ModelProfile>,
}

pub fn profiles_path(aide_dir: &Path) -> PathBuf {
    aide_dir.join("models.json")
}

/// Read the profile store. A missing file yields the empty config; a corrupt
/// one is an error so callers never replace it with defaults.
pub fn read_profiles(aide_dir: &Path) -> Result<ModelProfilesConfig, String> {
    Ok(storage::read_json(&profiles_path(aide_dir))?.unwrap_or_default())
}

pub fn write_profiles(aide_dir: &Path, config: &ModelProfilesConfig) -> Result<(), String> {
    validate_profiles(config)?;
    let path = profiles_path(aide_dir);
    let backups = aide_dir.join("backups");
    storage::write_json(&path, config, Some(&backups))?;
    // The file holds API keys.
    storage::restrict_permissions(&path)
}

// ── Tool config file helpers ──

/// Parse a JSON/JSONC tool config. A missing or empty file is an empty object;
/// a corrupt file is an error so a sync can never replace it with defaults.
pub(crate) fn read_jsonc_object(path: &Path) -> Result<Value, String> {
    let empty = || Value::Object(Default::default());
    if !path.exists() {
        return Ok(empty());
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(empty());
    }
    serde_json::from_str(&strip_jsonc_comments(&raw)).map_err(|e| {
        format!(
            "Cannot parse {}: {e}. Fix or move the file before syncing; it was left untouched.",
            path.display()
        )
    })
}

pub(crate) fn read_toml_table(path: &Path) -> Result<toml::Table, String> {
    if !path.exists() {
        return Ok(toml::Table::new());
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(toml::Table::new());
    }
    raw.parse().map_err(|e| {
        format!(
            "Cannot parse {}: {e}. Fix or move the file before syncing; it was left untouched.",
            path.display()
        )
    })
}

/// Write a tool config atomically, keeping a backup of the previous revision
/// under ~/.aide/backups.
fn write_tool_config(path: &Path, home: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let backups = home.join(".aide").join("backups");
    storage::write_bytes(path, content.as_bytes(), Some(&backups))?;
    // These configs hold API keys or auth env vars; keep them owner-only.
    storage::restrict_permissions(path)
}

fn validate_profiles(config: &ModelProfilesConfig) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for profile in &config.profiles {
        if profile.name.trim().is_empty() {
            return Err("Profile name cannot be empty".to_string());
        }
        if !seen.insert(profile.id.clone()) {
            return Err(format!("Duplicate profile id \"{}\"", profile.id));
        }
        if profile.provider.trim().is_empty() {
            return Err(format!("Profile \"{}\" has no provider", profile.name));
        }
        if profile.api_key.trim().is_empty() {
            return Err(format!("Profile \"{}\" has an empty API key", profile.name));
        }
    }
    Ok(())
}

// ── models.dev registry (fetched + cached for 24h) ──

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelSummary {
    pub id: String,
    pub name: String,
    pub context: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderSummary {
    pub id: String,
    pub name: String,
    pub npm: Option<String>,
    /// Default API base URL. models.dev carries no such metadata, so this
    /// comes from its fields when present, else from aide's builtin table.
    #[serde(default)]
    pub base_url: Option<String>,
    pub models: Vec<ModelSummary>,
}

/// Well-known API endpoints for mainstream providers. Stable facts, kept
/// minimal; anything not listed falls back to a manual base URL.
fn builtin_base_url(id: &str) -> Option<&'static str> {
    match id {
        "openai" => Some("https://api.openai.com/v1"),
        "anthropic" => Some("https://api.anthropic.com"),
        "google" => Some("https://generativelanguage.googleapis.com/v1beta"),
        "deepseek" => Some("https://api.deepseek.com"),
        "moonshotai" => Some("https://api.moonshot.cn/v1"),
        "zhipuai" => Some("https://open.bigmodel.cn/api/paas/v4"),
        "mistral" => Some("https://api.mistral.ai/v1"),
        "groq" => Some("https://api.groq.com/openai/v1"),
        "openrouter" => Some("https://openrouter.ai/api/v1"),
        "xai" => Some("https://api.x.ai/v1"),
        "siliconflow" => Some("https://api.siliconflow.cn/v1"),
        "volcengine" => Some("https://ark.cn-beijing.volces.com/api/v3"),
        _ => None,
    }
}

fn cache_path(aide_dir: &Path) -> PathBuf {
    // v2: cache entries carry the base_url field.
    aide_dir.join("cache").join("models-dev-v2.json")
}

pub fn fetch_models_dev(aide_dir: &Path, force: bool) -> Result<Vec<ProviderSummary>, String> {
    let cache = cache_path(aide_dir);
    if !force {
        if let Ok(meta) = fs::metadata(&cache) {
            if let Ok(age) = meta
                .modified()
                .map(|m| m.elapsed().map(|d| d.as_secs()).unwrap_or(0))
            {
                if age < CACHE_TTL {
                    if let Ok(raw) = fs::read_to_string(&cache) {
                        if let Ok(providers) = serde_json::from_str(&raw) {
                            return Ok(providers);
                        }
                    }
                }
            }
        }
    }
    let body = ureq::get(MODELS_DEV_API)
        .timeout(REQUEST_TIMEOUT)
        .call()
        .map_err(|e| format!("Failed to fetch models.dev: {e}"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    let providers = parse_models_dev(&body)?;
    if let Some(parent) = cache.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(&providers) {
        let _ = fs::write(&cache, json);
    }
    Ok(providers)
}

fn parse_models_dev(body: &str) -> Result<Vec<ProviderSummary>, String> {
    let raw: BTreeMap<String, Value> =
        serde_json::from_str(body).map_err(|e| format!("Invalid models.dev response: {e}"))?;
    let mut out = Vec::new();
    for value in raw.values() {
        let id = value["id"].as_str().unwrap_or("").to_string();
        let name = value["name"].as_str().unwrap_or("").to_string();
        if id.is_empty() || name.is_empty() {
            continue;
        }
        let mut models = Vec::new();
        if let Some(map) = value["models"].as_object() {
            for m in map.values() {
                let mid = m["id"].as_str().unwrap_or("").to_string();
                if mid.is_empty() {
                    continue;
                }
                models.push(ModelSummary {
                    name: m["name"].as_str().unwrap_or(&mid).to_string(),
                    context: m["limit"]["context"].as_u64(),
                    id: mid,
                });
            }
        }
        models.sort_by_key(|m| m.name.to_lowercase());
        let base_url = ["base_url", "baseURL", "api"]
            .iter()
            .find_map(|k| value[*k].as_str())
            .map(|s| s.to_string())
            .or_else(|| builtin_base_url(&id).map(|s| s.to_string()));
        out.push(ProviderSummary {
            id,
            name,
            npm: value["npm"].as_str().map(|s| s.to_string()),
            base_url,
            models,
        });
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    Ok(out)
}

// ── Sync writers ──

#[derive(Debug, Serialize)]
pub struct ModelSyncResult {
    pub tool: String,
    pub ok: bool,
    pub message: String,
}

pub(crate) fn profile_provider_name(profile: &ModelProfile) -> String {
    format!("aide-{}", profile.id)
}

/// Claude Code reads provider settings from env entries in
/// ~/.claude/settings.json. Official Anthropic keys and Anthropic-protocol
/// proxies (custom base URL) are supported.
fn sync_claude_code(profile: &ModelProfile, home: &Path) -> Result<ModelSyncResult, String> {
    let tool = "claude_code".to_string();
    let is_anthropic = profile.provider == "anthropic";
    if !is_anthropic && profile.base_url.trim().is_empty() {
        return Ok(ModelSyncResult {
            tool,
            ok: false,
            message:
                "Skipped: Claude Code needs the anthropic provider or a base URL (protocol proxy)"
                    .into(),
        });
    }
    let path = home.join(".claude").join("settings.json");
    let mut root = read_jsonc_object(&path)?;
    if !root.is_object() {
        root = Value::Object(Default::default());
    }
    let obj = root.as_object_mut().unwrap();
    let env = obj
        .entry("env")
        .or_insert_with(|| Value::Object(Default::default()));
    if !env.is_object() {
        *env = Value::Object(Default::default());
    }
    let env = env.as_object_mut().unwrap();
    let mut replaced: Vec<String> = Vec::new();
    let mut set_env = |env: &mut serde_json::Map<String, Value>, key: &str, value: Value| {
        if env.insert(key.into(), value).is_some() {
            replaced.push(key.to_string());
        }
    };
    if is_anthropic && profile.base_url.trim().is_empty() {
        set_env(env, "ANTHROPIC_API_KEY", json!(profile.api_key));
    } else {
        set_env(env, "ANTHROPIC_AUTH_TOKEN", json!(profile.api_key));
        set_env(env, "ANTHROPIC_BASE_URL", json!(profile.base_url.trim()));
    }
    if !profile.model.trim().is_empty() {
        set_env(env, "ANTHROPIC_MODEL", json!(profile.model.trim()));
    }
    let content = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    write_tool_config(&path, home, &content)?;
    let mut message = "Updated ~/.claude/settings.json env".to_string();
    if !replaced.is_empty() {
        message.push_str(&format!(" (replaced existing {})", replaced.join(", ")));
    }
    Ok(ModelSyncResult {
        tool,
        ok: true,
        message,
    })
}

/// OpenCode supports arbitrary providers in opencode.json via the ai-sdk.
fn sync_opencode(
    profile: &ModelProfile,
    provider_meta: Option<&ProviderSummary>,
    home: &Path,
) -> Result<ModelSyncResult, String> {
    let tool = "opencode".to_string();
    // Shared resolver: MCP sync writes the same file (jsonc when present, or
    // the json fallback / jsonc for a fresh install).
    let path = crate::mcp::opencode_config_path(home);
    let mut root = read_jsonc_object(&path)?;
    if !root.is_object() {
        root = Value::Object(Default::default());
    }
    let obj = root.as_object_mut().unwrap();
    let providers = obj
        .entry("provider")
        .or_insert_with(|| Value::Object(Default::default()));
    if !providers.is_object() {
        *providers = Value::Object(Default::default());
    }
    let key = profile_provider_name(profile);
    let mut entry = json!({
        "options": {
            "apiKey": profile.api_key,
        }
    });
    if let Some(meta) = provider_meta {
        if let Some(npm) = &meta.npm {
            entry["npm"] = json!(npm);
        }
        entry["name"] = json!(meta.name);
    }
    if !profile.base_url.trim().is_empty() {
        entry["options"]["baseURL"] = json!(profile.base_url.trim());
    }
    let mut models = serde_json::Map::new();
    for model in &profile.models {
        let mut obj = serde_json::Map::new();
        if let Some(meta) = provider_meta.and_then(|p| p.models.iter().find(|m| &m.id == model)) {
            obj.insert("name".into(), json!(meta.name));
            if let Some(context) = meta.context {
                obj.insert("limit".into(), json!({ "context": context }));
            }
        }
        models.insert(model.clone(), Value::Object(obj));
    }
    entry["models"] = Value::Object(models);
    providers.as_object_mut().unwrap().insert(key, entry);

    let content = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    write_tool_config(&path, home, &content)?;
    Ok(ModelSyncResult {
        tool,
        ok: true,
        message: format!(
            "Provider \"aide-{}\" written to {} ({} model(s))",
            profile.id,
            path.display(),
            profile.models.len()
        ),
    })
}

/// Codex reads model providers from config.toml. The API key goes into an
/// env var Codex resolves via env_key; the writer reports the variable name.
fn sync_codex(profile: &ModelProfile, home: &Path) -> Result<ModelSyncResult, String> {
    let tool = "codex".to_string();
    let path = home.join(".codex").join("config.toml");
    let mut table = read_toml_table(&path)?;
    let provider_key = profile_provider_name(profile);
    let env_key = format!(
        "AIDE_{}_API_KEY",
        profile.provider.to_uppercase().replace(['-', '.'], "_")
    );
    let mut provider = toml::Table::new();
    provider.insert(
        "name".into(),
        toml::Value::String(format!("aide/{}", profile.provider)),
    );
    if !profile.base_url.trim().is_empty() {
        provider.insert(
            "base_url".into(),
            toml::Value::String(profile.base_url.trim().to_string()),
        );
    }
    provider.insert("env_key".into(), toml::Value::String(env_key.clone()));
    let providers = table
        .entry("model_providers")
        .or_insert_with(|| toml::Value::Table(Default::default()));
    if !providers.is_table() {
        *providers = toml::Value::Table(Default::default());
    }
    providers
        .as_table_mut()
        .unwrap()
        .insert(provider_key, toml::Value::Table(provider));
    if !profile.model.trim().is_empty() {
        table.insert(
            "model".into(),
            toml::Value::String(profile.model.trim().to_string()),
        );
    }
    let content = toml::to_string_pretty(&toml::Value::Table(table)).map_err(|e| e.to_string())?;
    write_tool_config(&path, home, &content)?;
    Ok(ModelSyncResult {
        tool,
        ok: true,
        message: format!(
            "Provider written to config.toml; export {}=\"<key>\" for Codex to read it",
            env_key
        ),
    })
}

pub fn sync_profile(
    profile: &ModelProfile,
    providers: &[ProviderSummary],
    home: &Path,
) -> Vec<ModelSyncResult> {
    let provider_meta = providers.iter().find(|p| p.id == profile.provider);
    let mut results = Vec::new();
    for target in &profile.targets {
        let result = match target.as_str() {
            "claude_code" => sync_claude_code(profile, home),
            "opencode" => sync_opencode(profile, provider_meta, home),
            "codex" => sync_codex(profile, home),
            other => Ok(ModelSyncResult {
                tool: other.to_string(),
                ok: false,
                message: "Skipped: this tool has no file-based model configuration".into(),
            }),
        };
        match result {
            Ok(r) => results.push(r),
            Err(e) => results.push(ModelSyncResult {
                tool: target.clone(),
                ok: false,
                message: e,
            }),
        }
    }
    results
}

#[derive(Debug, Serialize)]
pub struct ModelProfileSyncResult {
    pub profile_id: String,
    pub profile_name: String,
    pub results: Vec<ModelSyncResult>,
}

pub fn sync_all_profiles(
    config: &ModelProfilesConfig,
    providers: &[ProviderSummary],
    home: &Path,
) -> Vec<ModelProfileSyncResult> {
    config
        .profiles
        .iter()
        .map(|p| ModelProfileSyncResult {
            profile_id: p.id.clone(),
            profile_name: p.name.clone(),
            results: sync_profile(p, providers, home),
        })
        .collect()
}

// ── Default-home helpers ──

/// Sync one stored profile into its target tools.
pub fn sync_profile_by_id(profile_id: &str) -> Result<Vec<ModelSyncResult>, String> {
    let aide_dir = crate::aide_dir()?;
    let home = crate::home_dir()?;
    let config = read_profiles(&aide_dir)?;
    let profile = config
        .profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("Unknown profile \"{profile_id}\""))?
        .clone();
    let providers = fetch_models_dev(&aide_dir, false)?;
    Ok(sync_profile(&profile, &providers, &home))
}

/// Sync every stored profile into its target tools.
pub fn sync_all_default() -> Result<Vec<ModelProfileSyncResult>, String> {
    let aide_dir = crate::aide_dir()?;
    let home = crate::home_dir()?;
    let config = read_profiles(&aide_dir)?;
    let providers = fetch_models_dev(&aide_dir, false)?;
    Ok(sync_all_profiles(&config, &providers, &home))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_base(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aide-models-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_profile() -> ModelProfile {
        ModelProfile {
            id: "test-id".into(),
            name: "Personal".into(),
            provider: "anthropic".into(),
            base_url: String::new(),
            api_key: "sk-ant-test123".into(),
            model: "claude-sonnet-4-6".into(),
            models: vec!["claude-sonnet-4-6".into()],
            targets: vec!["claude_code".into(), "opencode".into()],
        }
    }

    #[test]
    fn profiles_round_trip_and_validation() {
        let base = temp_base("config");
        let config = ModelProfilesConfig {
            profiles: vec![sample_profile()],
        };
        write_profiles(&base, &config).unwrap();
        let read = read_profiles(&base).unwrap();
        assert_eq!(read.profiles.len(), 1);
        assert_eq!(read.profiles[0].provider, "anthropic");

        let mut bad = config.clone();
        bad.profiles[0].api_key = String::new();
        assert!(write_profiles(&base, &bad).is_err());
        let mut bad2 = config.clone();
        bad2.profiles.push(bad2.profiles[0].clone());
        assert!(write_profiles(&base, &bad2).is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(profiles_path(&base))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn read_profiles_distinguishes_missing_from_corrupt() {
        let base = temp_base("corrupt");
        assert!(read_profiles(&base).unwrap().profiles.is_empty());
        fs::write(profiles_path(&base), "{\"profiles\": [").unwrap();
        assert!(read_profiles(&base).is_err());
        // A corrupt file is never silently replaced by a write from the UI.
        let err = read_profiles(&base).unwrap_err();
        assert!(err.contains("parse"), "unexpected error: {err}");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn write_profiles_backs_up_previous_revision() {
        let base = temp_base("backup");
        let config = ModelProfilesConfig {
            profiles: vec![sample_profile()],
        };
        write_profiles(&base, &config).unwrap();
        let mut updated = config.clone();
        updated.profiles[0].name = "Renamed".into();
        write_profiles(&base, &updated).unwrap();
        let backups = base.join("backups");
        assert_eq!(fs::read_dir(&backups).unwrap().count(), 1);
        assert_eq!(read_profiles(&base).unwrap().profiles[0].name, "Renamed");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sync_claude_code_aborts_on_corrupt_settings_file() {
        let base = temp_base("claude-corrupt");
        let home = base.join("home");
        fs::create_dir_all(home.join(".claude")).unwrap();
        let path = home.join(".claude/settings.json");
        let original = r#"{"env": {"EDITOR": "vim""#;
        fs::write(&path, original).unwrap();
        let profile = sample_profile();
        let err = sync_claude_code(&profile, &home).unwrap_err();
        assert!(err.contains("Cannot parse"), "unexpected error: {err}");
        // Nothing is overwritten; the user can still repair the file.
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sync_codex_aborts_on_corrupt_toml_file() {
        let base = temp_base("codex-corrupt");
        let home = base.join("home");
        fs::create_dir_all(home.join(".codex")).unwrap();
        let path = home.join(".codex/config.toml");
        let original = "[broken\nkey = ";
        fs::write(&path, original).unwrap();
        let profile = sample_profile();
        let err = sync_codex(&profile, &home).unwrap_err();
        assert!(err.contains("Cannot parse"), "unexpected error: {err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn parse_models_dev_extracts_providers_and_models() {
        let body = r#"{
            "anthropic": { "id": "anthropic", "name": "Anthropic", "npm": "@ai-sdk/anthropic",
                "models": { "claude-sonnet-4-6": { "id": "claude-sonnet-4-6", "name": "Claude Sonnet 4.6", "limit": { "context": 1000000 } } } },
            "skipme": { "id": "", "name": "", "models": {} }
        }"#;
        let providers = parse_models_dev(body).unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "anthropic");
        assert_eq!(
            providers[0].base_url.as_deref(),
            Some("https://api.anthropic.com")
        );
        assert_eq!(providers[0].models.len(), 1);
        assert_eq!(providers[0].models[0].context, Some(1000000));
    }

    #[test]
    fn sync_claude_code_merges_env_and_preserves_other_keys() {
        let base = temp_base("claude");
        let home = base.join("home");
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::write(
            home.join(".claude/settings.json"),
            r#"{"env": {"EDITOR": "vim"}, "model": "opus"}"#,
        )
        .unwrap();

        let mut profile = sample_profile();
        profile.base_url = "https://proxy.example.com".into();
        let result = sync_claude_code(&profile, &home).unwrap();
        assert!(result.ok);
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(home.join(".claude/settings.json")).unwrap())
                .unwrap();
        assert_eq!(settings["env"]["EDITOR"], "vim");
        assert_eq!(settings["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-ant-test123");
        assert_eq!(
            settings["env"]["ANTHROPIC_BASE_URL"],
            "https://proxy.example.com"
        );
        assert_eq!(settings["env"]["ANTHROPIC_MODEL"], "claude-sonnet-4-6");
        assert_eq!(settings["model"], "opus");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sync_claude_code_skips_non_anthropic_without_base_url() {
        let base = temp_base("claude-skip");
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();
        let mut profile = sample_profile();
        profile.provider = "openai".into();
        let result = sync_claude_code(&profile, &home).unwrap();
        assert!(!result.ok);
        assert!(!home.join(".claude/settings.json").exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sync_opencode_upserts_provider_and_preserves_existing() {
        let base = temp_base("opencode");
        let home = base.join("home");
        fs::create_dir_all(home.join(".config/opencode")).unwrap();
        fs::write(
            home.join(".config/opencode/opencode.json"),
            r#"{ "provider": { "existing": { "options": { "apiKey": "keep" } } }, "theme": "dark" }"#,
        )
        .unwrap();

        let profile = sample_profile();
        let result = sync_opencode(&profile, None, &home).unwrap();
        assert!(result.ok);
        let parsed: Value = serde_json::from_str(
            &fs::read_to_string(home.join(".config/opencode/opencode.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(parsed["theme"], "dark");
        assert_eq!(parsed["provider"]["existing"]["options"]["apiKey"], "keep");
        let key = format!("aide-{}", profile.id);
        assert_eq!(
            parsed["provider"][&key]["options"]["apiKey"],
            "sk-ant-test123"
        );
        assert!(parsed["provider"][&key]["models"]["claude-sonnet-4-6"].is_object());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sync_opencode_creates_jsonc_for_a_fresh_install() {
        let base = temp_base("opencode-fresh");
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();

        let profile = sample_profile();
        let result = sync_opencode(&profile, None, &home).unwrap();
        assert!(result.ok);

        let jsonc = home.join(".config/opencode/opencode.jsonc");
        assert!(jsonc.is_file(), "fresh install must create opencode.jsonc");
        assert!(!home.join(".config/opencode/opencode.json").exists());
        let parsed: Value = serde_json::from_str(&fs::read_to_string(&jsonc).unwrap()).unwrap();
        let key = format!("aide-{}", profile.id);
        assert_eq!(
            parsed["provider"][&key]["options"]["apiKey"],
            "sk-ant-test123"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sync_opencode_keeps_using_an_existing_json_file() {
        let base = temp_base("opencode-existing-json");
        let home = base.join("home");
        let dir = home.join(".config/opencode");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("opencode.json"), "{}").unwrap();

        let profile = sample_profile();
        sync_opencode(&profile, None, &home).unwrap();
        assert!(dir.join("opencode.json").is_file());
        assert!(!dir.join("opencode.jsonc").exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn synced_tool_configs_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let base = temp_base("perms-tool");
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();
        let profile = sample_profile();

        sync_claude_code(&profile, &home).unwrap();
        let settings = home.join(".claude/settings.json");
        assert_eq!(
            fs::metadata(&settings).unwrap().permissions().mode() & 0o777,
            0o600
        );

        sync_opencode(&profile, None, &home).unwrap();
        // A fresh install writes OpenCode's JSONC config.
        let opencode = home.join(".config/opencode/opencode.jsonc");
        assert_eq!(
            fs::metadata(&opencode).unwrap().permissions().mode() & 0o777,
            0o600
        );

        sync_codex(&profile, &home).unwrap();
        let codex = home.join(".codex/config.toml");
        assert_eq!(
            fs::metadata(&codex).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sync_codex_writes_valid_toml_provider() {
        let base = temp_base("codex");
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();

        let mut profile = sample_profile();
        profile.base_url = "https://api.example.com/v1".into();
        let result = sync_codex(&profile, &home).unwrap();
        assert!(result.ok);

        let parsed: toml::Table = fs::read_to_string(home.join(".codex/config.toml"))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(parsed["model"].as_str(), Some("claude-sonnet-4-6"));
        let key = format!("aide-{}", profile.id);
        assert_eq!(
            parsed["model_providers"][&key]["base_url"].as_str(),
            Some("https://api.example.com/v1")
        );
        assert!(parsed["model_providers"][&key]["env_key"]
            .as_str()
            .unwrap()
            .starts_with("AIDE_"));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sync_all_reports_unsupported_targets() {
        let base = temp_base("all");
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();
        let mut profile = sample_profile();
        profile.targets = vec!["claude_code".into(), "cursor".into()];
        let config = ModelProfilesConfig {
            profiles: vec![profile],
        };
        let results = sync_all_profiles(&config, &[], &home);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].results.len(), 2);
        assert!(results[0].results[0].ok);
        assert!(!results[0].results[1].ok);
        assert!(results[0].results[1]
            .message
            .contains("no file-based model configuration"));
        let _ = fs::remove_dir_all(&base);
    }
}
