# aide

AI Development Environment config manager.

A unified hub for managing cross-agent AI configurations — Skills, MCP, and model configs — with multi-agent compatibility (Claude Code, OpenCode, Cursor, Zed, etc.).

## Features

- **File Explorer**: Directory tree + file list with natural sorting
- **Code Editor**: CodeMirror 6 with JSON Schema validation, Markdown support, dark theme
- **Multi-Tab**: Open multiple files, Cmd+S to save, dirty indicators
- **Extensible**: Designed for Skills sync, MCP config translation, model config abstraction

## Tech Stack

| Layer | Tech |
|---|---|
| Desktop | Tauri v2 |
| Frontend | React 19 + TypeScript 6 + Vite 8 |
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
├── src/lib.rs     # File CRUD commands
└── src/main.rs    # Entry point
```
