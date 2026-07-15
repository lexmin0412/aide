# AGENTS.md — aide

## Tech Stack

- **Frontend**: React 19 + TypeScript 6 + Vite 8
- **Backend**: Tauri v2 (Rust)
- **Editor**: CodeMirror 6
- **Package Manager**: pnpm 11

## Project Structure

```
src/
├── components/         # React components
│   ├── Editor.tsx      # CodeMirror editor wrapper
│   ├── FileList.tsx    # File listing panel
│   └── FileTree.tsx    # Directory tree sidebar
├── hooks/
│   └── useFileSystem.ts  # Tauri invoke wrappers
├── types/
│   └── index.ts        # Shared TypeScript types
├── App.tsx             # Main layout (sidebar + tabs + editor)
├── App.css             # All styles (VS Code dark theme)
└── main.tsx            # Entry point

src-tauri/
└── src/
    ├── lib.rs          # Tauri commands (file CRUD + utils)
    └── main.rs         # Entry point
```

## Rust Commands (src-tauri/src/lib.rs)

| Command | Params | Returns | Description |
|---|---|---|---|
| `list_directory` | `path: string` | `FileEntry[]` | List dir (dirs first, alpha) |
| `read_text_file` | `path: string` | `string` | Read file as UTF-8 |
| `write_text_file` | `path, content: string` | `void` | Write file (creates parent dirs) |
| `create_file` | `path: string` | `void` | Create empty file |
| `create_directory` | `path: string` | `void` | Create dir (recursive) |
| `delete_entry` | `path: string` | `void` | Delete file/dir (recursive) |
| `rename_entry` | `old_path, new_path: string` | `void` | Rename/move |
| `file_exists` | `path: string` | `boolean` | Check existence |
| `get_home_dir` | — | `string` | User home directory |

## Conventions

- **No comments** in source code unless explaining a non-obvious decision
- **No emoji** in code or commit messages
- **No README** files unless explicitly requested
- All Rust commands return `Result<T, String>`
- Frontend invokes via `@tauri-apps/api/core` `invoke()`
- Editor keymap uses `useRef` for callbacks to avoid stale closures
- CSS variables in `:root` for theming (dark theme)

## Development

```bash
pnpm tauri dev      # Start dev server with hot-reload
pnpm build          # TypeScript check + Vite build
```

## Architecture Notes

- **File CRUD** uses direct Rust `std::fs` operations (unrestricted access)
- **Editor state** tied to `tab.path` — switching tabs recreates the EditorView
- **Save flow**: Cmd+S → `onSaveRef.current(path, content)` → `fs::write`
- Port: 1430 (Vite) / 1431 (HMR)
