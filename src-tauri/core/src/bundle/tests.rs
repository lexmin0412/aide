use super::*;
use crate::models::ModelProfile;
use std::collections::HashMap;
use std::io::Read;

fn temp_base(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aide-bundle-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn home(base: &Path, name: &str) -> PathBuf {
    let dir = base.join(name);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_skill(home_dir: &Path, name: &str, body: &str) {
    let dir = home_dir.join(".agents/skills").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\n---\n{body}"),
    )
    .unwrap();
    fs::write(dir.join("notes.md"), "reference notes").unwrap();
}

fn write_models_profiles(home_dir: &Path, profiles: &[(&str, &str, &str)]) {
    let mut config = ModelProfilesConfig::default();
    for (id, name, api_key) in profiles {
        config.profiles.push(ModelProfile {
            id: (*id).to_string(),
            name: (*name).to_string(),
            provider: "anthropic".to_string(),
            base_url: String::new(),
            api_key: (*api_key).to_string(),
            model: "claude".to_string(),
            models: vec!["claude".to_string()],
            targets: vec!["claude_code".to_string()],
        });
    }
    fs::create_dir_all(home_dir.join(".aide")).unwrap();
    storage::write_json(&home_dir.join(".aide/models.json"), &config, None).unwrap();
}

fn write_mcp_servers(home_dir: &Path, servers: &[(&str, &str)]) {
    let mut map = HashMap::new();
    for (name, command) in servers {
        map.insert(
            (*name).to_string(),
            mcp::McpServerConfig {
                command: Some((*command).to_string()),
                args: None,
                url: None,
                env: None,
                headers: None,
                disabled: None,
                description: None,
                targets: vec![],
            },
        );
    }
    fs::create_dir_all(home_dir.join(".aide")).unwrap();
    storage::write_json(
        &home_dir.join(".aide/mcp.json"),
        &McpCentralConfig { servers: map },
        None,
    )
    .unwrap();
}

fn write_mcp_with_secrets(home_dir: &Path) {
    let mut env = HashMap::new();
    env.insert("API_KEY".to_string(), "sk-env-secret".to_string());
    env.insert("DEBUG".to_string(), "1".to_string());
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), "Bearer abc".to_string());
    headers.insert("X-Custom".to_string(), "plain".to_string());
    let mut servers = HashMap::new();
    servers.insert(
        "depot".to_string(),
        mcp::McpServerConfig {
            command: Some("npx".to_string()),
            args: None,
            url: None,
            env: Some(env),
            headers: Some(headers),
            disabled: None,
            description: None,
            targets: vec![],
        },
    );
    fs::create_dir_all(home_dir.join(".aide")).unwrap();
    storage::write_json(
        &home_dir.join(".aide/mcp.json"),
        &McpCentralConfig { servers },
        None,
    )
    .unwrap();
}

fn read_archive_entry(archive: &Path, wanted: &str) -> Vec<u8> {
    let file = fs::File::open(archive).unwrap();
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar_archive = tar::Archive::new(decoder);
    for entry in tar_archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let path = entry.path().unwrap().to_string_lossy().to_string();
        if path == wanted {
            let mut buffer = Vec::new();
            entry.read_to_end(&mut buffer).unwrap();
            return buffer;
        }
    }
    panic!("archive entry {wanted} not found");
}

fn write_custom_archive(path: &Path, entries: &[(&str, &[u8])]) {
    let file = fs::File::create(path).unwrap();
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (name, bytes) in entries {
        append_bytes(&mut builder, name, bytes).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap();
}

fn exported_bundle(base: &Path, source_home: &Path) -> PathBuf {
    let bundle = base.join("env.aidebundle");
    export_bundle_in(source_home, &bundle, false).unwrap();
    bundle
}

#[test]
fn export_preview_and_import_merge_roundtrip() {
    let base = temp_base("roundtrip");
    let source = home(&base, "source");
    make_skill(&source, "alpha", "v1");
    write_mcp_servers(&source, &[("depot", "npx")]);
    write_models_profiles(&source, &[("p1", "Personal", "sk-secret")]);

    let bundle = base.join("env.aidebundle");
    let exported = export_bundle_in(&source, &bundle, false).unwrap();
    assert!(bundle.is_file());
    assert_eq!(exported.skills, 1);
    assert_eq!(exported.mcp_servers, 1);
    assert_eq!(exported.model_profiles, 1);
    assert!(exported.bytes > 0);
    assert!(exported.path.ends_with("env.aidebundle"));

    // Manifest carries the skills and hashes and never claims git scopes.
    let manifest: serde_json::Value =
        serde_json::from_slice(&read_archive_entry(&bundle, "manifest.json")).unwrap();
    assert_eq!(manifest["format_version"], 1);
    assert_eq!(manifest["git_scopes_exported"], false);
    assert_eq!(manifest["skills"][0]["name"], "alpha");
    assert_eq!(
        manifest["skills"][0]["hash"],
        crate::hash::tree_hash(&source.join(".agents/skills/alpha")).unwrap()
    );

    let target = home(&base, "target");
    let preview = preview_bundle_in(&target, &bundle).unwrap();
    assert_eq!(preview.new_skills, vec!["alpha".to_string()]);
    assert!(preview.conflicting_skills.is_empty());
    assert_eq!(preview.new_mcp_servers, vec!["depot".to_string()]);
    assert_eq!(preview.new_profiles, vec!["p1".to_string()]);
    assert!(preview.warnings.is_empty());

    let imported = import_bundle_in(&target, &bundle, "merge").unwrap();
    assert_eq!(imported.skills_added, vec!["alpha".to_string()]);
    assert_eq!(imported.mcp_added, vec!["depot".to_string()]);
    assert_eq!(imported.profiles_added, vec!["p1".to_string()]);
    assert!(imported.skills_replaced.is_empty());
    assert!(valid_snapshot_id(&imported.safety_snapshot));

    assert_eq!(
        fs::read_to_string(target.join(".agents/skills/alpha/notes.md")).unwrap(),
        "reference notes"
    );
    let mcp = mcp::read_central_at(&target.join(".aide/mcp.json")).unwrap();
    assert!(mcp.servers.contains_key("depot"));
    let models_config = models::read_profiles(&target.join(".aide")).unwrap();
    assert_eq!(models_config.profiles.len(), 1);

    // Provenance points at the bundle and carries the manifest hash.
    let sources = registry::read_sources(&target.join(".aide")).unwrap();
    let entry = sources.get("alpha").expect("provenance");
    assert_eq!(entry.source, "bundle:env.aidebundle");
    assert!(entry.hash.is_some());
    let _ = fs::remove_dir_all(&base);
}

fn valid_snapshot_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[test]
fn export_redacts_api_keys_and_secret_looking_mcp_values() {
    let base = temp_base("redact");
    let source = home(&base, "source");
    make_skill(&source, "alpha", "v1");
    write_mcp_with_secrets(&source);
    write_models_profiles(&source, &[("p1", "Personal", "sk-secret")]);

    let bundle = base.join("redacted.aidebundle");
    let exported = export_bundle_in(&source, &bundle, false).unwrap();
    assert!(exported
        .redacted_secrets
        .contains(&"mcp:depot/env.API_KEY".to_string()));
    assert!(exported
        .redacted_secrets
        .contains(&"mcp:depot/headers.Authorization".to_string()));
    assert!(exported
        .redacted_secrets
        .contains(&"model:p1/api_key".to_string()));

    let mcp: serde_json::Value =
        serde_json::from_slice(&read_archive_entry(&bundle, "config/mcp.json")).unwrap();
    let server = &mcp["servers"]["depot"];
    assert_eq!(server["env"]["API_KEY"], "");
    assert_eq!(server["env"]["DEBUG"], "1");
    assert_eq!(server["headers"]["Authorization"], "");
    assert_eq!(server["headers"]["X-Custom"], "plain");

    let models_config: serde_json::Value =
        serde_json::from_slice(&read_archive_entry(&bundle, "config/models.json")).unwrap();
    assert_eq!(models_config["profiles"][0]["api_key"], "");

    // Imported profiles keep the structure but no key.
    let target = home(&base, "target");
    import_bundle_in(&target, &bundle, "merge").unwrap();
    let imported = models::read_profiles(&target.join(".aide")).unwrap();
    assert_eq!(imported.profiles[0].api_key, "");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn preview_detects_conflicts() {
    let base = temp_base("conflicts");
    let source = home(&base, "source");
    make_skill(&source, "alpha", "v1");
    write_mcp_servers(&source, &[("depot", "npx")]);
    write_models_profiles(&source, &[("p1", "Personal", "sk")]);
    let bundle = exported_bundle(&base, &source);

    let target = home(&base, "target");
    make_skill(&target, "alpha", "existing");
    write_mcp_servers(&target, &[("depot", "old")]);
    write_models_profiles(&target, &[("p1", "Old", "sk-old")]);

    let preview = preview_bundle_in(&target, &bundle).unwrap();
    assert!(preview.new_skills.is_empty());
    assert_eq!(preview.conflicting_skills, vec!["alpha".to_string()]);
    assert_eq!(preview.conflicting_mcp_servers, vec!["depot".to_string()]);
    assert_eq!(preview.conflicting_profiles, vec!["p1".to_string()]);
    let _ = fs::remove_dir_all(&base);
}

fn fake_trash(path: &Path) -> Result<(), String> {
    fs::remove_dir_all(path).map_err(|e| format!("fake trash {}: {e}", path.display()))
}

#[test]
fn replace_mode_overwrites_conflicts_and_keeps_unrelated_entries() {
    let base = temp_base("replace");
    let source = home(&base, "source");
    make_skill(&source, "alpha", "new body");
    write_mcp_servers(&source, &[("depot", "new-command")]);
    write_models_profiles(&source, &[("p1", "Personal", "sk")]);
    let bundle = exported_bundle(&base, &source);

    let target = home(&base, "target");
    make_skill(&target, "alpha", "old body");
    make_skill(&target, "beta", "unrelated");
    write_mcp_servers(&target, &[("depot", "old-command"), ("mine", "keep-me")]);
    write_models_profiles(&target, &[("p1", "Old", "old"), ("p2", "Mine", "mine")]);

    set_trash_hook(Some(fake_trash));
    let imported = import_bundle_in(&target, &bundle, "replace").unwrap();
    set_trash_hook(None);

    assert_eq!(imported.skills_replaced, vec!["alpha".to_string()]);
    assert!(imported.skills_added.is_empty());
    assert_eq!(imported.mcp_replaced, vec!["depot".to_string()]);
    assert_eq!(imported.profiles_replaced, vec!["p1".to_string()]);

    let body = fs::read_to_string(target.join(".agents/skills/alpha/SKILL.md")).unwrap();
    assert!(body.contains("new body"));
    let mcp = mcp::read_central_at(&target.join(".aide/mcp.json")).unwrap();
    assert_eq!(
        mcp.servers.get("depot").and_then(|s| s.command.clone()),
        Some("new-command".to_string())
    );
    let models_config = models::read_profiles(&target.join(".aide")).unwrap();
    let p1 = models_config
        .profiles
        .iter()
        .find(|profile| profile.id == "p1")
        .unwrap();
    assert_eq!(p1.name, "Personal");
    // Unrelated entries survive a replace import.
    assert!(mcp.servers.contains_key("mine"));
    assert!(models_config.profiles.iter().any(|p| p.id == "p2"));
    assert!(target.join(".agents/skills/beta/SKILL.md").is_file());
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn merge_import_preserves_existing_and_reports_conflicts() {
    let base = temp_base("merge");
    let source = home(&base, "source");
    make_skill(&source, "alpha", "new");
    write_mcp_servers(&source, &[("depot", "npx")]);
    write_models_profiles(&source, &[("p1", "Personal", "sk")]);
    let bundle = exported_bundle(&base, &source);

    let target = home(&base, "target");
    make_skill(&target, "alpha", "keep me");
    write_mcp_servers(&target, &[("depot", "old"), ("mine", "keep")]);
    write_models_profiles(&target, &[("p1", "Old", "old"), ("p2", "Mine", "mine")]);

    let imported = import_bundle_in(&target, &bundle, "merge").unwrap();
    assert!(imported.skills_added.is_empty());
    assert!(imported.mcp_added.is_empty());
    assert!(imported.profiles_added.is_empty());
    assert!(imported
        .warnings
        .iter()
        .any(|warning| warning.contains("Skill already exists")));
    assert!(imported
        .warnings
        .iter()
        .any(|warning| warning.contains("MCP server already exists")));

    let body = fs::read_to_string(target.join(".agents/skills/alpha/SKILL.md")).unwrap();
    assert!(body.contains("keep me"));
    let mcp = mcp::read_central_at(&target.join(".aide/mcp.json")).unwrap();
    assert_eq!(
        mcp.servers.get("depot").and_then(|s| s.command.clone()),
        Some("old".to_string())
    );
    assert!(mcp.servers.contains_key("mine"));
    let models_config = models::read_profiles(&target.join(".aide")).unwrap();
    assert_eq!(
        models_config
            .profiles
            .iter()
            .find(|p| p.id == "p1")
            .map(|p| p.name.clone()),
        Some("Old".to_string())
    );
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn export_without_any_configs_works() {
    let base = temp_base("empty");
    let source = home(&base, "source");
    let bundle = base.join("empty.aidebundle");
    let exported = export_bundle_in(&source, &bundle, false).unwrap();
    assert_eq!(exported.skills, 0);
    assert_eq!(exported.mcp_servers, 0);
    assert_eq!(exported.model_profiles, 0);
    assert!(exported.redacted_secrets.is_empty());

    let target = home(&base, "target");
    let preview = preview_bundle_in(&target, &bundle).unwrap();
    assert_eq!(preview.skill_count, 0);
    assert!(preview.new_skills.is_empty());
    let imported = import_bundle_in(&target, &bundle, "merge").unwrap();
    assert!(imported.skills_added.is_empty());
    assert!(!target.join(".aide/mcp.json").exists());
    assert!(!target.join(".aide/models.json").exists());
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn malicious_archive_entries_are_rejected() {
    let base = temp_base("malicious");
    let target = home(&base, "target");
    let archive = base.join("evil.aidebundle");
    // Hand-crafted tar entry with a traversal path (the tar builder refuses).
    let file = fs::File::create(&archive).unwrap();
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_size(4);
    header.set_mode(0o644);
    {
        let old = header.as_old_mut();
        let name = b"../escape.txt";
        old.name[..name.len()].copy_from_slice(name);
    }
    header.set_cksum();
    builder.append(&header, &b"nope"[..]).unwrap();
    builder.into_inner().unwrap().finish().unwrap();

    assert!(preview_bundle_in(&target, &archive).is_err());
    assert!(import_bundle_in(&target, &archive, "merge").is_err());
    assert!(!base.join("escape.txt").exists());
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn oversized_archives_are_rejected() {
    let base = temp_base("oversized");
    let target = home(&base, "target");
    let archive = base.join("big.aidebundle");
    let file = fs::File::create(&archive).unwrap();
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let manifest = serde_json::to_vec(&serde_json::json!({
        "format_version": 1,
        "created_at": 0,
        "aide_version": "0.0.0",
        "includes_secrets": false,
        "git_scopes_exported": false,
        "skills": [],
        "mcp_servers": [],
        "model_profiles": [],
        "redacted_secrets": [],
    }))
    .unwrap();
    append_bytes(&mut builder, "manifest.json", &manifest).unwrap();
    for index in 0..=MAX_SKILL_FILES {
        append_bytes(&mut builder, &format!("skills/bulk/file-{index}.txt"), b"x").unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap();

    let err = preview_bundle_in(&target, &archive).unwrap_err();
    assert!(err.contains("too many files"), "{err}");
    assert!(import_bundle_in(&target, &archive, "merge").is_err());
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn export_limits_and_invalid_mode_and_names_are_rejected() {
    assert!(check_export_limits(MAX_SKILL_FILES, MAX_SKILL_BYTES).is_ok());
    assert!(check_export_limits(MAX_SKILL_FILES + 1, 0).is_err());
    assert!(check_export_limits(0, MAX_SKILL_BYTES + 1).is_err());
    assert!(!valid_entry_name("../evil"));
    assert!(!valid_entry_name("a/b"));
    assert!(!valid_entry_name(""));
    assert!(!valid_entry_name(".."));
    assert!(valid_entry_name("pdf-skills"));

    let base = temp_base("invalid");
    let source = home(&base, "source");
    let bundle = exported_bundle(&base, &source);
    let target = home(&base, "target");
    assert!(import_bundle_in(&target, &bundle, "bogus").is_err());

    let evil = base.join("bad-name.aidebundle");
    let manifest = serde_json::to_vec(&serde_json::json!({
        "format_version": 1,
        "created_at": 0,
        "aide_version": "0.0.0",
        "includes_secrets": false,
        "git_scopes_exported": false,
        "skills": [{"name": "../evil", "hash": ""}],
        "mcp_servers": [],
        "model_profiles": [],
        "redacted_secrets": [],
    }))
    .unwrap();
    write_custom_archive(&evil, &[("manifest.json", &manifest)]);
    assert!(preview_bundle_in(&target, &evil).is_err());
    assert!(import_bundle_in(&target, &evil, "merge").is_err());
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn skill_hash_mismatch_is_reported_as_a_warning() {
    let base = temp_base("hash-mismatch");
    let target = home(&base, "target");
    let archive = base.join("mismatch.aidebundle");
    let manifest = serde_json::to_vec(&serde_json::json!({
        "format_version": 1,
        "created_at": 0,
        "aide_version": "0.0.0",
        "includes_secrets": false,
        "git_scopes_exported": false,
        "skills": [{"name": "alpha", "hash": "deadbeef"}],
        "mcp_servers": [],
        "model_profiles": [],
        "redacted_secrets": [],
    }))
    .unwrap();
    write_custom_archive(
        &archive,
        &[
            ("manifest.json", &manifest),
            ("skills/alpha/SKILL.md", b"---\nname: alpha\n---\npayload"),
        ],
    );
    let preview = preview_bundle_in(&target, &archive).unwrap();
    assert!(preview
        .warnings
        .iter()
        .any(|warning| warning.contains("hash differs")));
    let imported = import_bundle_in(&target, &archive, "merge").unwrap();
    assert_eq!(imported.skills_added, vec!["alpha".to_string()]);
    assert!(imported
        .warnings
        .iter()
        .any(|warning| warning.contains("hash differs")));
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn safety_snapshot_is_created_and_restorable() {
    let base = temp_base("safety");
    let source = home(&base, "source");
    make_skill(&source, "alpha", "v1");
    write_mcp_servers(&source, &[("depot", "npx")]);
    let bundle = exported_bundle(&base, &source);

    let target = home(&base, "target");
    write_mcp_servers(&target, &[("mine", "keep")]);
    let imported = import_bundle_in(&target, &bundle, "merge").unwrap();

    let snapshots = snapshot::list_snapshots_in(&target).unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].id, imported.safety_snapshot);

    snapshot::restore_snapshot_in(&target, &imported.safety_snapshot).unwrap();
    let mcp = mcp::read_central_at(&target.join(".aide/mcp.json")).unwrap();
    assert!(mcp.servers.contains_key("mine"));
    assert!(!mcp.servers.contains_key("depot"));
    // Skills are never touched by a configuration restore.
    assert!(target.join(".agents/skills/alpha/SKILL.md").is_file());
    let _ = fs::remove_dir_all(&base);
}
