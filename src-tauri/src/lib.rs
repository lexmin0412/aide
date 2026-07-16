use std::path::Path;
use std::time::SystemTime;
use std::{fs, io};

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_at: u64,
    pub extension: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub path: String,
    pub is_symlink: bool,
    pub target_path: Option<String>,
    pub file_count: usize,
}

fn read_dir_entries(path: &Path) -> io::Result<Vec<FileEntry>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        let path = entry.path();
        let modified_at = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        entries.push(FileEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: path.to_string_lossy().to_string(),
            is_dir: path.is_dir(),
            size: meta.len(),
            modified_at,
            extension: path.extension().map(|e| e.to_string_lossy().to_string()),
        });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

fn count_files(path: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !matches!(name.as_str(), "node_modules" | ".git" | ".pnpm" | "dist" | ".next" | "target") {
                    count += 1;
                }
            } else {
                count += 1;
            }
        }
    }
    count
}

fn parse_skill_frontmatter(content: &str) -> (Option<String>, Option<String>) {
    let content = content.trim();
    if !content.starts_with("---") {
        return (None, None);
    }
    let rest = content[3..].trim();
    if let Some(end) = rest.find("---") {
        let yaml_str = &rest[..end];
        if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(yaml_str) {
            let mapping = match &val {
                serde_yaml::Value::Mapping(m) => m,
                _ => return (None, None),
            };
            let name = mapping
                .get(&serde_yaml::Value::String("name".into()))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let desc = mapping
                .get(&serde_yaml::Value::String("description".into()))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            return (name, desc);
        }
    }
    (None, None)
}

#[tauri::command]
fn list_skills() -> Result<Vec<SkillInfo>, String> {
    let skills_dir = dirs::home_dir()
        .ok_or_else(|| "Cannot find home directory".to_string())?
        .join(".agents")
        .join("skills");

    if !skills_dir.exists() {
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();
    for entry in fs::read_dir(&skills_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        let is_symlink = meta.is_symlink();
        let target_path = if is_symlink {
            fs::read_link(&path).ok().map(|p| p.to_string_lossy().to_string())
        } else {
            None
        };

        let (display_name, description) = {
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                if let Ok(content) = fs::read_to_string(&skill_md) {
                    let (n, d) = parse_skill_frontmatter(&content);
                    (n.unwrap_or_else(|| name.clone()), d.unwrap_or_default())
                } else {
                    (name.clone(), String::new())
                }
            } else {
                (name.clone(), String::new())
            }
        };

        let file_count = if is_symlink {
            let real_path = fs::read_link(&path).unwrap_or(path.clone());
            count_files(&real_path)
        } else {
            count_files(&path)
        };

        skills.push(SkillInfo {
            name: name.clone(),
            display_name,
            description,
            path: path.to_string_lossy().to_string(),
            is_symlink,
            target_path,
            file_count,
        });
    }

    skills.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(skills)
}

#[tauri::command]
fn list_directory(path: String) -> Result<Vec<FileEntry>, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {}", path));
    }
    read_dir_entries(dir).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_text_file(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn write_text_file(path: String, content: String) -> Result<(), String> {
    if let Some(parent) = Path::new(&path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, &content).map_err(|e| e.to_string())
}

#[tauri::command]
fn create_file(path: String) -> Result<(), String> {
    if let Some(parent) = Path::new(&path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::File::create(&path).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn create_directory(path: String) -> Result<(), String> {
    fs::create_dir_all(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_entry(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if p.is_dir() {
        fs::remove_dir_all(p).map_err(|e| e.to_string())
    } else {
        fs::remove_file(p).map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn rename_entry(old_path: String, new_path: String) -> Result<(), String> {
    fs::rename(&old_path, &new_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn file_exists(path: String) -> bool {
    Path::new(&path).exists()
}

#[tauri::command]
fn get_home_dir() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_skills,
            list_directory,
            read_text_file,
            write_text_file,
            create_file,
            create_directory,
            delete_entry,
            rename_entry,
            file_exists,
            get_home_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
