//! Append-only audit trail for privileged operations.
//!
//! GUI actions and sidecar agent actions both write here so a user can see what
//! an agent did on their machine. The journal is JSON Lines (one event per
//! line), rotated at 2 MB with one previous file kept. Auditing is best effort:
//! a broken audit log must never fail the operation it records.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

/// Rotate once the live log passes this size. One previous file is kept.
const MAX_AUDIT_BYTES: u64 = 2 * 1024 * 1024;

pub fn audit_path(aide_dir: &Path) -> PathBuf {
    aide_dir.join("audit.jsonl")
}

fn rotated_path(aide_dir: &Path) -> PathBuf {
    let mut name = audit_path(aide_dir).into_os_string();
    name.push(".1");
    PathBuf::from(name)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Record one event under the real `~/.aide/audit.jsonl`.
///
/// Failures are ignored on purpose: auditing is observability, not control flow.
pub fn record(action: &str, details: Value) {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let _ = append(
        &home.join(".aide"),
        action,
        &details,
        now_secs(),
        MAX_AUDIT_BYTES,
    );
}

/// Append one event. `ts`, `max_bytes` are explicit so tests can exercise
/// rotation and timestamps without touching the user's real audit log.
fn append(
    aide_dir: &Path,
    action: &str,
    details: &Value,
    ts: u64,
    max_bytes: u64,
) -> Result<(), String> {
    let path = audit_path(aide_dir);
    rotate_if_needed(aide_dir, &path, max_bytes);
    let entry = json!({
        "ts": ts,
        "action": action,
        "details": redact(details),
    });
    let mut line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
    line.push('\n');
    fs::create_dir_all(aide_dir).map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    file.write_all(line.as_bytes())
        .map_err(|e| format!("write {}: {e}", path.display()))?;
    // The journal contains file paths and agent commands; keep it owner-only.
    crate::storage::restrict_permissions(&path)
}

fn rotate_if_needed(aide_dir: &Path, path: &Path, max_bytes: u64) {
    let too_big = fs::metadata(path)
        .map(|m| m.len() >= max_bytes)
        .unwrap_or(false);
    if !too_big {
        return;
    }
    let rotated = rotated_path(aide_dir);
    // Windows cannot rename over an existing file; dropping the oldest
    // revision first keeps exactly one backup on every platform.
    let _ = fs::remove_file(&rotated);
    let _ = fs::rename(path, &rotated);
}

/// Replace values of secret-looking keys so credentials never land in the log.
fn redact(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    if is_secret_key(key) {
                        (key.clone(), Value::String("[redacted]".to_string()))
                    } else {
                        (key.clone(), redact(value))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact).collect()),
        other => other.clone(),
    }
}

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    let compact: String = lower
        .chars()
        .filter(|c| *c != '-' && *c != '_' && *c != '.')
        .collect();
    lower == "authorization"
        || lower == "proxy-authorization"
        || compact.ends_with("apikey")
        || compact.ends_with("key")
        || compact.ends_with("token")
        || compact.ends_with("secret")
        || compact.ends_with("password")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aide-audit-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn read_lines(aide_dir: &Path) -> Vec<Value> {
        fs::read_to_string(audit_path(aide_dir))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn appends_one_json_line_per_event() {
        let dir = temp_dir("append");
        append(
            &dir,
            "install",
            &json!({"id": "owner/repo", "ok": true}),
            100,
            MAX_AUDIT_BYTES,
        )
        .unwrap();
        append(
            &dir,
            "publish",
            &json!({"scope": "personal"}),
            200,
            MAX_AUDIT_BYTES,
        )
        .unwrap();
        let lines = read_lines(&dir);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["ts"], 100);
        assert_eq!(lines[0]["action"], "install");
        assert_eq!(lines[0]["details"]["id"], "owner/repo");
        assert_eq!(lines[1]["action"], "publish");
        // The file is a valid JSONL journal, not a JSON array.
        assert!(fs::read_to_string(audit_path(&dir))
            .unwrap()
            .ends_with('\n'));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotates_to_a_single_previous_file() {
        let dir = temp_dir("rotate");
        // A tiny limit makes rotation observable without writing megabytes.
        for ts in 0..6 {
            append(&dir, "event", &json!({ "n": ts }), ts, 60).unwrap();
        }
        assert!(audit_path(&dir).exists());
        assert!(rotated_path(&dir).exists());
        // The previous file keeps older events; the live file holds newer ones.
        let current = read_lines(&dir);
        assert!(current.len() < 6);
        let previous: Vec<Value> = fs::read_to_string(rotated_path(&dir))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(!previous.is_empty());
        assert!(previous[0]["details"]["n"].as_u64().unwrap() < 6);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn redacts_secret_looking_keys() {
        let dir = temp_dir("redact");
        append(
            &dir,
            "sync",
            &json!({
                "apiKey": "sk-live",
                "api_key": "sk-live",
                "AIDE_OPENAI_API_KEY": "sk-live",
                "refresh_token": "tok",
                "Authorization": "Bearer x",
                "nested": { "client_secret": "shh", "safe": "visible" },
                "list": [{ "access-key": "ak" }],
                "id": "owner/repo",
            }),
            1,
            MAX_AUDIT_BYTES,
        )
        .unwrap();
        let details = read_lines(&dir)[0]["details"].clone();
        assert_eq!(details["apiKey"], "[redacted]");
        assert_eq!(details["api_key"], "[redacted]");
        assert_eq!(details["AIDE_OPENAI_API_KEY"], "[redacted]");
        assert_eq!(details["refresh_token"], "[redacted]");
        assert_eq!(details["Authorization"], "[redacted]");
        assert_eq!(details["nested"]["client_secret"], "[redacted]");
        assert_eq!(details["nested"]["safe"], "visible");
        assert_eq!(details["list"][0]["access-key"], "[redacted]");
        assert_eq!(details["id"], "owner/repo");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_directory_is_created_and_empty_events_are_recorded() {
        let dir = temp_dir("create").join("nested").join(".aide");
        append(&dir, "enable_agent_access", &json!({}), 7, MAX_AUDIT_BYTES).unwrap();
        assert!(audit_path(&dir).is_file());
        assert_eq!(read_lines(&dir)[0]["action"], "enable_agent_access");
        let _ = fs::remove_dir_all(dir.parent().unwrap().parent().unwrap());
    }
}
