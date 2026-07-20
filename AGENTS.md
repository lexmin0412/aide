# AGENTS.md — aide

## Tech Stack

- **Frontend**: React 19 + TypeScript 6 + Vite 8
- **Backend**: Tauri v2 (Rust)
- **Styling**: Tailwind CSS 4 + shadcn/ui (base-nova style)
- **Editor**: CodeMirror 6
- **Icons**: Lucide React + inline SVG
- **Package Manager**: pnpm 11

## Project Structure

```
src/
├── components/
│   ├── SkillGrid.tsx      # Skills listing page (grid + search)
│   ├── SkillCard.tsx      # Individual skill card
│   ├── SkillDetail.tsx    # Skill detail view (file tree + editor + delete)
│   ├── SyncPanel.tsx      # Skills sync modal (symlink management)
│   ├── MCPPage.tsx        # MCP servers management page
│   ├── ConfigPanel.tsx    # Config file browser per tool
│   ├── Editor.tsx         # CodeMirror 6 + image viewer wrapper
│   ├── FileTree.tsx       # Directory tree with context menu
│   └── ui/                # shadcn/ui primitives (button, dialog, input, etc.)
├── lib/
│   ├── utils.ts           # cn() utility (clsx + tailwind-merge)
│   └── fileUtils.ts       # readFileAsTab, isImageFile helpers
├── types/
│   └── index.ts           # Shared TypeScript types
├── App.tsx                # Root layout (page routing + skill CRUD)
├── App.css                # Global styles + Tailwind theme
└── main.tsx               # Entry point

src-tauri/
└── src/
    ├── lib.rs             # Tauri commands (21 commands)
    ├── main.rs            # Entry point
    ├── adapter/mod.rs     # Tool adapter definitions (8 AI tools)
    └── mcp.rs             # MCP config management
```

## Rust Commands (src-tauri/src/lib.rs)

| Command | Params | Returns | Description |
|---|---|---|---|
| `list_skills` | — | `SkillInfo[]` | List skills from ~/.agents/skills |
| `list_directory` | `path: string` | `FileEntry[]` | List dir (dirs first, alpha) |
| `read_text_file` | `path: string` | `string` | Read file as UTF-8 |
| `read_binary_base64` | `path: string` | `string` | Read file as base64 data URL |
| `write_text_file` | `path, content: string` | `void` | Write file (creates parent dirs) |
| `create_file` | `path: string` | `void` | Create empty file |
| `create_directory` | `path: string` | `void` | Create dir (recursive) |
| `delete_entry` | `path: string` | `void` | Delete file/dir/symlink (recursive) |
| `rename_entry` | `old_path, new_path: string` | `void` | Rename/move |
| `file_exists` | `path: string` | `boolean` | Check existence |
| `get_home_dir` | — | `string` | User home directory |
| `list_tools` | — | `ToolInfo[]` | List supported AI tools |
| `check_sync_statuses` | — | `ToolInfo[]` | Tools with sync status |
| `sync_tool` | `tool_key: string` | `SyncResult` | Sync skills to one tool |
| `sync_all_tools` | — | `SyncResult[]` | Sync skills to all tools |
| `list_mcp_servers` | — | `McpServerView[]` | List central MCP servers |
| `save_mcp_servers` | `servers: McpServerView[]` | `void` | Save central MCP config |
| `list_mcp_tools` | — | `McpToolView[]` | List MCP-capable tools |
| `sync_mcp_tool` | `tool_key: string` | `McpSyncResult` | Sync MCP to one tool |
| `sync_mcp_all` | — | `McpSyncResult[]` | Sync MCP to all tools |
| `import_mcp_all` | — | `ImportResult[]` | Import MCP configs from all tools |

## Conventions

- **No comments** in source code unless explaining a non-obvious decision
- **No emoji** in code or commit messages
- **No README** files unless explicitly requested
- All Rust commands return `Result<T, String>`
- Frontend invokes via `@tauri-apps/api/core` `invoke()`
- Editor keymap uses `useRef` for callbacks to avoid stale closures
- CSS variables in `:root` for theming (dark theme, oklch color space)
- Context menus use `pointerdown` (capture) + `menuRef.contains()` pattern
- Hooks must always be called unconditionally (Rules of Hooks)

## Development

```bash
pnpm tauri dev      # Start dev server with hot-reload
pnpm build          # TypeScript check + Vite build
```

## Architecture Notes

- **File CRUD** uses direct Rust `std::fs` operations (unrestricted access)
- **Editor state** tied to `tab.path` — switching tabs recreates the EditorView
- **Save flow**: Cmd+S → `onSaveRef.current(path, content)` → `fs::write`
- **Image viewer**: Editor splits into ImageViewer / TextViewer components (Rules of Hooks)
- **Skill sync** uses symlinks from `~/.agents/skills` to each tool's skills directory
- **MCP central config** stored at `~/.aide/mcp.json`, synced per tool format
- **FileTree context menu**: right-click for Rename, Delete, New File, New Folder
- **Tab management**: Cmd+W intercepts close-tab when tabs are open
- Port: 1430 (Vite) / 1431 (HMR)
