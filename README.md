# aide

AI Development Environment config manager.

A unified hub for managing cross-agent AI configurations — Skills, MCP, and model configs — with multi-agent compatibility (Claude Code, OpenCode, Cursor, Codex, Warp, GitHub Copilot, Trae, VS Code, MiMo Code).

## Features

- **Skills Management**: Install, browse, edit, tag, and sync AI agent skills across 8 tools via symlink
- **MCP Server Management**: Centralized MCP config with import/sync to tool-specific formats
- **Config Browser**: View and edit raw config files for each supported AI tool
- **Code Editor**: CodeMirror 6 with JSON linting, Markdown support, dark theme
- **Image Viewer**: Preview png/jpg/gif/webp/svg/bmp files inline
- **File Tree**: Directory browser with right-click context menu (Rename, Delete, New File/Folder)
- **Multi-Tab**: Open multiple files, Cmd+W to close tab, Cmd+S to save, dirty indicators
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

## Getting Started

```bash
pnpm tauri dev
```

## Project

```bash
src/               # React frontend
src-tauri/         # Rust backend + Tauri config
├── src/lib.rs     # 22 Tauri commands
├── src/adapter/   # Tool adapter definitions
└── src/mcp.rs     # MCP config management
```
