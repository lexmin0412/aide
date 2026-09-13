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

/// Tool paths follow the community-maintained agent matrix from
/// vercel-labs/skills (github.com/vercel-labs/skills, src/agents.ts).
/// Tools whose global skills dir is `.agents/skills` read the central store
/// natively and need no symlink; the rest are linked on sync.
pub fn all_tools() -> Vec<ToolInfo> {
    vec![
        // ── Mainstream coding agents ──
        ToolInfo { key: "claude_code", name: "Claude Code", global_skills: ".claude/skills", detect_dir: ".claude", project_skills: ".claude/skills" },
        ToolInfo { key: "opencode", name: "OpenCode", global_skills: ".config/opencode/skills", detect_dir: ".config/opencode", project_skills: ".agents/skills" },
        ToolInfo { key: "codex", name: "Codex", global_skills: ".codex/skills", detect_dir: ".codex", project_skills: ".agents/skills" },
        ToolInfo { key: "gemini_cli", name: "Gemini CLI", global_skills: ".gemini/skills", detect_dir: ".gemini", project_skills: ".agents/skills" },
        ToolInfo { key: "github_copilot", name: "GitHub Copilot", global_skills: ".copilot/skills", detect_dir: ".copilot", project_skills: ".agents/skills" },
        ToolInfo { key: "cursor", name: "Cursor", global_skills: ".cursor/skills", detect_dir: ".cursor", project_skills: ".agents/skills" },
        ToolInfo { key: "windsurf", name: "Windsurf", global_skills: ".codeium/windsurf/skills", detect_dir: ".codeium/windsurf", project_skills: ".windsurf/skills" },
        ToolInfo { key: "zed", name: "Zed", global_skills: ".agents/skills", detect_dir: ".config/zed", project_skills: ".agents/skills" },
        ToolInfo { key: "cline", name: "Cline", global_skills: ".agents/skills", detect_dir: ".cline", project_skills: ".agents/skills" },
        ToolInfo { key: "roo_code", name: "Roo Code", global_skills: ".roo/skills", detect_dir: ".roo", project_skills: ".roo/skills" },
        ToolInfo { key: "kilo_code", name: "Kilo Code", global_skills: ".kilo/skills", detect_dir: ".kilo", project_skills: ".agents/skills" },
        ToolInfo { key: "goose", name: "Goose", global_skills: ".config/goose/skills", detect_dir: ".config/goose", project_skills: ".goose/skills" },
        ToolInfo { key: "droid", name: "Droid", global_skills: ".factory/skills", detect_dir: ".factory", project_skills: ".agents/skills" },
        ToolInfo { key: "amp", name: "Amp", global_skills: ".config/agents/skills", detect_dir: ".config/amp", project_skills: ".agents/skills" },
        ToolInfo { key: "crush", name: "Crush", global_skills: ".config/crush/skills", detect_dir: ".config/crush", project_skills: ".crush/skills" },
        ToolInfo { key: "continue", name: "Continue", global_skills: ".continue/skills", detect_dir: ".continue", project_skills: ".continue/skills" },
        ToolInfo { key: "augment", name: "Augment", global_skills: ".augment/skills", detect_dir: ".augment", project_skills: ".augment/skills" },
        ToolInfo { key: "warp", name: "Warp", global_skills: ".agents/skills", detect_dir: ".warp", project_skills: ".agents/skills" },
        ToolInfo { key: "aider_desk", name: "AiderDesk", global_skills: ".aider-desk/skills", detect_dir: ".aider-desk", project_skills: ".aider-desk/skills" },
        ToolInfo { key: "openhands", name: "OpenHands", global_skills: ".openhands/skills", detect_dir: ".openhands", project_skills: ".openhands/skills" },
        // ── CLI-first agents ──
        ToolInfo { key: "zcode", name: "ZCode", global_skills: ".zcode/skills", detect_dir: ".zcode", project_skills: ".zcode/skills" },
        ToolInfo { key: "qwen_code", name: "Qwen Code", global_skills: ".qwen/skills", detect_dir: ".qwen", project_skills: ".agents/skills" },
        ToolInfo { key: "iflow_cli", name: "iFlow CLI", global_skills: ".iflow/skills", detect_dir: ".iflow", project_skills: ".iflow/skills" },
        ToolInfo { key: "kiro_cli", name: "Kiro CLI", global_skills: ".kiro/skills", detect_dir: ".kiro", project_skills: ".kiro/skills" },
        ToolInfo { key: "grok", name: "Grok Build", global_skills: ".grok/skills", detect_dir: ".grok", project_skills: ".grok/skills" },
        ToolInfo { key: "kimi_code", name: "Kimi Code CLI", global_skills: ".agents/skills", detect_dir: ".kimi", project_skills: ".agents/skills" },
        ToolInfo { key: "minimax_code", name: "MiniMax Code", global_skills: ".minimax/skills", detect_dir: ".minimax", project_skills: ".minimax/skills" },
        ToolInfo { key: "forgecode", name: "ForgeCode", global_skills: ".forge/skills", detect_dir: ".forge", project_skills: ".forge/skills" },
        ToolInfo { key: "kode", name: "Kode", global_skills: ".kode/skills", detect_dir: ".kode", project_skills: ".kode/skills" },
        ToolInfo { key: "neovate", name: "Neovate", global_skills: ".neovate/skills", detect_dir: ".neovate", project_skills: ".neovate/skills" },
        // ── IDE / Chinese-market agents ──
        ToolInfo { key: "trae", name: "Trae", global_skills: ".trae/skills", detect_dir: ".trae", project_skills: ".trae/skills" },
        ToolInfo { key: "trae_cn", name: "Trae CN", global_skills: ".trae-cn/skills", detect_dir: ".trae-cn", project_skills: ".trae/skills" },
        ToolInfo { key: "mimocode", name: "MiMo Code", global_skills: ".agents/skills", detect_dir: ".config/mimocode", project_skills: ".agents/skills" },
        ToolInfo { key: "codebuddy", name: "CodeBuddy", global_skills: ".codebuddy/skills", detect_dir: ".codebuddy", project_skills: ".codebuddy/skills" },
        ToolInfo { key: "qoder", name: "Qoder", global_skills: ".qoder/skills", detect_dir: ".qoder", project_skills: ".qoder/skills" },
        ToolInfo { key: "qoder_cn", name: "Qoder CN", global_skills: ".qoder-cn/skills", detect_dir: ".qoder-cn", project_skills: ".qoder/skills" },
        ToolInfo { key: "lingma", name: "Lingma", global_skills: ".lingma/skills", detect_dir: ".lingma", project_skills: ".lingma/skills" },
        ToolInfo { key: "junie", name: "Junie", global_skills: ".junie/skills", detect_dir: ".junie", project_skills: ".junie/skills" },
        ToolInfo { key: "antigravity", name: "Antigravity", global_skills: ".gemini/antigravity/skills", detect_dir: ".gemini/antigravity", project_skills: ".agents/skills" },
        ToolInfo { key: "zencoder", name: "Zencoder", global_skills: ".zencoder/skills", detect_dir: ".zencoder", project_skills: ".zencoder/skills" },
        ToolInfo { key: "adal", name: "AdaL", global_skills: ".adal/skills", detect_dir: ".adal", project_skills: ".adal/skills" },
        ToolInfo { key: "codestudio", name: "Code Studio", global_skills: ".codestudio/skills", detect_dir: ".codestudio", project_skills: ".codestudio/skills" },
        ToolInfo { key: "pochi", name: "Pochi", global_skills: ".pochi/skills", detect_dir: ".pochi", project_skills: ".pochi/skills" },
    ]
}
