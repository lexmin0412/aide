//! aide-core: headless domain logic shared by the aide desktop app, the
//! `aide-mcp` sidecar and any future CLI.
//!
//! Nothing in this crate depends on Tauri or on a GUI toolkit: the app crate is
//! a thin `#[tauri::command]` layer over these modules, and the sidecar links
//! only this crate.

pub mod adapter;
pub mod audit;
pub mod doctor;
pub mod files;
pub mod fsutil;
pub mod git_sync;
pub mod mcp;
pub mod models;
pub mod policy;
pub mod project;
pub mod registry;
pub mod skills;
pub mod storage;

pub use doctor::DoctorReport;
pub use files::FileEntry;
pub use skills::{SkillInfo, SkillSearchMatch, SyncResult, ToolInfo};

/// The current user's home directory, or an error when it cannot be resolved.
pub fn home_dir() -> Result<std::path::PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "Cannot find home directory".to_string())
}

/// `~/.aide`, aide's own configuration and cache directory.
pub fn aide_dir() -> Result<std::path::PathBuf, String> {
    Ok(home_dir()?.join(".aide"))
}

/// The user's home directory as a string (the `get_home_dir` command payload).
pub fn home_dir_string() -> String {
    dirs::home_dir()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".to_string())
}
