# aide

[简体中文](./README.zh-CN.md) | English

AI Development Environment config manager.

A unified hub for managing cross-agent AI configurations — Skills, MCP, and model configs — with multi-agent compatibility (Claude Code, OpenCode, Cursor, Codex, Gemini CLI, Windsurf, Zed, Cline, Roo Code, Kilo Code, Goose, Warp, GitHub Copilot, Trae, Qwen Code, iFlow CLI, Kimi Code, MiniMax Code, CodeBuddy, Qoder, Lingma, MiMo Code, and more).

## Features

- **Skills Management**: Install, browse, edit, tag, and sync AI agent skills across 40+ tools via symlink/junction
- **MCP Server Management**: Centralized MCP config with import/sync to tool-specific formats (JSON, JSONC, TOML)
- **Model Management**: Provider profiles backed by models.dev (213+ providers) with locally-stored API keys, synced to Claude Code / OpenCode / Codex
- **Project Profiles**: Per-repository `<project>/.aide/project.json` selecting skills, MCP servers and tools; one sync links skills into the shared and per-tool project directories and merges MCP entries — only links/entries aide itself created are ever removed
- **Scoped Git Backup**: Publish/pull skills to separate git remotes (personal vs company) with safe, non-destructive conflict handling
- **Agent Access**: Bundled `aide-mcp` sidecar exposes skills to agents over MCP (list/search/install/update/publish/pull)
- **Safety**: atomic config writes with backups, path-scoped file commands, 0600 tool configs, and a redacted audit log with optional agent allowlists
- **Config Browser**: View and edit raw config files for each supported AI tool
- **Code Editor**: CodeMirror 6 with JSON linting, Markdown support, dark theme
- **Image Viewer**: Preview png/jpg/gif/webp/svg/bmp files inline
- **File Tree**: Directory browser with right-click context menu (Rename, Delete, New File/Folder)
- **Multi-Tab**: Open multiple files, Cmd+W to close tab, Cmd+S to save, dirty indicators with unsaved-change guards
- **Auto-Update**: In-app update check, download, and install via tauri-plugin-updater

## Tech Stack

| Layer | Tech |
|---|---|
| Desktop | Tauri v2 |
| Frontend | React 19 + TypeScript 6 + Vite 8 |
| Styling | Tailwind CSS 4 + shadcn/ui |
| Editor | CodeMirror 6 |
| Backend | Rust (`std::fs` CRUD) |
| Package | pnpm 11 |

## Development

```bash
pnpm tauri dev      # Start dev server with hot-reload
pnpm build          # TypeScript check + Vite build
pnpm typecheck      # tsc --noEmit
pnpm lint           # ESLint (flat config)
pnpm test           # Vitest
pnpm tauri build    # Production build + bundling
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo clippy --manifest-path src-tauri/Cargo.toml --lib --no-deps -- -D warnings
```

## Project

```bash
src/                     # React frontend
src-tauri/               # Tauri app crate + Cargo workspace root
├── core/                # aide-core: headless logic (skills, MCP, models, git, project, doctor)
├── src/lib.rs           # 47 thin Tauri commands delegating to aide-core
├── src/main.rs          # Tauri entry point
└── src/bin/aide-mcp.rs  # MCP/CLI sidecar built on aide-core (no Tauri)
```
