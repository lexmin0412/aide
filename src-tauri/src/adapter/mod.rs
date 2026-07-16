pub struct ToolInfo {
    pub key: &'static str,
    pub name: &'static str,
    /// Global skills directory (relative to ~)
    pub global_skills: &'static str,
    /// Directory used to detect whether the tool is installed
    pub detect_dir: &'static str,
    /// Project-level skills directory (relative to project root)
    pub project_skills: &'static str,
}

pub fn all_tools() -> Vec<ToolInfo> {
    vec![
        // Tools with .agents/skills paths already support the standard natively.
        ToolInfo { key: "opencode", name: "OpenCode", global_skills: ".agents/skills", detect_dir: ".config/opencode", project_skills: ".agents/skills" },
        ToolInfo { key: "claude_code", name: "Claude Code", global_skills: ".claude/skills", detect_dir: ".claude", project_skills: ".claude/skills" },
        ToolInfo { key: "codex", name: "Codex", global_skills: ".agents/skills", detect_dir: ".codex", project_skills: ".agents/skills" },
        ToolInfo { key: "trae", name: "Trae", global_skills: ".trae/skills", detect_dir: ".trae", project_skills: ".trae/skills" },
        ToolInfo { key: "cursor", name: "Cursor", global_skills: ".cursor/skills", detect_dir: ".cursor", project_skills: ".agents/skills" },
        ToolInfo { key: "warp", name: "Warp", global_skills: ".agents/skills", detect_dir: ".warp", project_skills: ".agents/skills" },
        ToolInfo { key: "github_copilot", name: "GitHub Copilot", global_skills: ".copilot/skills", detect_dir: ".copilot", project_skills: ".agents/skills" },
        ToolInfo { key: "mimocode", name: "MiMo Code", global_skills: ".agents/skills", detect_dir: ".config/mimocode", project_skills: ".agents/skills" },
    ]
}
