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
│   ├── SkillGrid.tsx      # Skills listing page (grid + search + new skill + empty state)
│   ├── SkillCard.tsx      # Individual skill card
│   ├── SkillDetail.tsx    # Skill detail view (file tree + editor + delete)
│   ├── SyncPanel.tsx      # Skills sync modal (symlink management)
│   ├── MCPPage.tsx        # MCP servers management page
│   ├── ConfigPanel.tsx    # Config file browser per tool
│   ├── Editor.tsx         # CodeMirror 6 + markdown preview + image viewer wrapper
│   ├── CommandPalette.tsx # Cmd+K skill search palette
│   ├── NewSkillDialog.tsx # Create skill scaffold (folder + SKILL.md frontmatter)
│   ├── TabBar.tsx         # Shared editor tab strip
│   ├── Skeleton.tsx       # Loading placeholders
│   ├── ErrorBoundary.tsx  # Global error boundary
│   ├── FileTree.tsx       # Directory tree (context menu, keyboard nav)
│   ├── TagEditor.tsx      # Tag editing content (rendered inside a popover)
│   ├── UpdateDialog.tsx   # Auto-update check & install dialog
│   └── ui/                # shadcn/ui primitives (button, dialog, input, popover, etc.)
├── hooks/
│   ├── useTabs.ts         # Shared tab state (open/close/switch/save, mtime reload)
│   └── useSidebarWidth.ts # Resizable, persisted sidebar width
├── lib/
│   ├── utils.ts           # cn() utility (clsx + tailwind-merge)
│   ├── fileUtils.ts       # readFileAsTab, getFileMtime, isImageFile helpers
│   ├── markdown.ts        # Minimal markdown renderer for preview
│   ├── theme.ts           # Theme preference (system/light/dark) helpers
│   └── toast.tsx          # Global toast emitter + ToastHost
├── stores/
│   └── skillStore.ts      # Search query + scroll position persistence
├── types/
│   └── index.ts           # Shared TypeScript types
├── App.tsx                # Root layout (page routing + skill CRUD + palette + theme)
├── App.css                # Global styles + Tailwind theme
└── main.tsx               # Entry point

src-tauri/
└── src/
    ├── lib.rs             # Tauri commands (40 commands)
    ├── main.rs            # Entry point
    ├── adapter/mod.rs     # Tool adapter definitions (43 AI tools)
    ├── registry.rs        # skills.sh registry search + GitHub install
    ├── git_sync.rs        # Scoped git backup/share (publish/pull per remote)
    ├── registry.rs        # skills.sh registry search + GitHub install
    ├── models.rs          # Model profiles + models.dev registry + per-tool sync
    └── bin/aide-mcp.rs    # MCP stdio server + CLI over the same core (sidecar)
```

## Rust Commands (src-tauri/src/lib.rs)

| Command | Params | Returns | Description |
|---|---|---|---|
| `list_skills` | — | `SkillInfo[]` | List skills from ~/.agents/skills |
| `import_skill` | `source: string` | `string` | Copy a local folder into ~/.agents/skills (rejects duplicates, skips node_modules/.git/etc.) |
| `search_registry` | `query: string` | `RegistrySkill[]` | Search the skills.sh community registry (same unauthenticated API as `npx skills`) |
| `install_skill_from_registry` | `id: string` | `RemoteInstallResult` | Install `owner/repo[/skill]` from GitHub (25 MB archive cap, provenance in ~/.aide/skill-sources.json) |
| `search_skills` | `query: string` | `SkillSearchMatch[]` | Case-insensitive full-text search across skill text files |
| `list_directory` | `path: string` | `FileEntry[]` | List dir (dirs first, alpha) |
| `read_text_file` | `path: string` | `string` | Read file as UTF-8 |
| `write_text_file` | `path, content: string` | `void` | Write file (creates parent dirs) |
| `create_file` | `path: string` | `void` | Create empty file |
| `create_directory` | `path: string` | `void` | Create dir (recursive) |
| `delete_entry` | `path: string` | `void` | Move file/dir to system trash (falls back to permanent delete) |
| `rename_entry` | `old_path, new_path: string` | `void` | Rename/move |
| `file_exists` | `path: string` | `boolean` | Check existence |
| `get_file_mtime` | `path: string` | `u64` | File modification time (unix seconds) |
| `get_home_dir` | — | `string` | User home directory |
| `list_tools` | — | `ToolInfo[]` | List supported AI tools |
| `check_sync_statuses` | — | `ToolInfo[]` | Tools with sync status |
| `sync_tool` | `tool_key: string, overwrite?: bool` | `SyncResult` | Sync skills to one tool; conflicting tool-side copies are backed up to ~/.aide/sync-backup unless `overwrite` |
| `sync_all_tools` | — | `SyncResult[]` | Sync skills to all tools |
| `list_mcp_servers` | — | `McpServerView[]` | List central MCP servers |
| `save_mcp_servers` | `servers: McpServerView[]` | `void` | Save central MCP config |
| `list_mcp_tools` | — | `McpToolView[]` | List MCP-capable tools |
| `sync_mcp_tool` | `tool_key: string` | `McpSyncResult` | Sync MCP to one tool |
| `sync_mcp_all` | — | `McpSyncResult[]` | Sync MCP to all tools |
| `import_mcp_all` | — | `ImportResult[]` | Import MCP configs from all tools |
| `update_skill_tags` | `path, tags: string[]` | `void` | Update tags in SKILL.md frontmatter (drops nested metadata.tags) |
| `check_skill_updates` | — | `SkillUpdate[]` | Registry-installed skills whose upstream repo advanced past the recorded commit |
| `update_skill` | `skill: string` | `RemoteInstallResult` | Reinstall a skill from its recorded upstream id (replaced copy goes to trash) |
| `get_agent_server` | — | `AgentServerStatus` | aide-mcp sidecar availability and whether agent access is enabled |
| `enable_agent_access` | `enable: bool` | `AgentServerStatus` | Add/remove the aide MCP server entry in the central MCP config |
| `list_model_providers` | `force?: bool` | `ProviderSummary[]` | models.dev providers+models with a 24h disk cache (~/.aide/cache) |
| `list_model_profiles` | — | `ModelProfilesConfig` | Read ~/.aide/models.json (0600) |
| `save_model_profiles` | `config: ModelProfilesConfig` | `void` | Validate and write model profiles |
| `sync_model_profile` | `profile_id: string` | `ModelSyncResult[]` | Write one profile into its target tools (claude_code env / opencode provider / codex config.toml) |
| `sync_all_model_profiles` | — | `ModelProfileSyncResult[]` | Sync every profile to its targets |
| `get_git_config` | — | `GitRemotesConfig` | Read ~/.aide/remotes.json (scopes + skill scope assignments) |
| `save_git_config` | `config: GitRemotesConfig` | `void` | Validate and write the scope config |
| `set_skill_scope` | `skill: string, scope?: string` | `void` | Assign a skill to a git scope (None = default scope) |
| `publish_scope` | `scope: string` | `PublishOutcome` | Mirror the scope's skills into its worktree, commit and push (auto-rebase retry) |
| `pull_scope` | `scope: string` | `PullOutcome` | Fast-forward pull, then write remote-side skills back (remote wins, never deletes local-only) |

## Conventions

- **English only**: all code, comments, documentation, commit messages, and release notes must be in English
- **No emoji** in code or commit messages
- **No README** files unless explicitly requested
- All Rust commands return `Result<T, String>`
- Frontend invokes via `@tauri-apps/api/core` `invoke()`
- Editor keymap uses `useRef` for callbacks to avoid stale closures
- CSS variables in `:root`/`.dark` for theming (light + dark palettes, oklch color space); theme preference stored in localStorage `theme` (absent = system)
- Context menus use `pointerdown` (capture) + `menuRef.contains()` pattern
- Hooks must always be called unconditionally (Rules of Hooks)
- User-facing async failures go through `toast()` from `@/lib/toast`, never `alert()`

## Development

```bash
pnpm tauri dev      # Start dev server with hot-reload
pnpm build          # TypeScript check + Vite build
pnpm tauri build    # Production build + bundling
```

## Architecture Notes

- **File CRUD** uses direct Rust `std::fs` operations (unrestricted access); images are served via the Tauri asset protocol (`convertFileSrc`), not base64 IPC
- **Editor state** tied to `tab.path` — switching tabs recreates the EditorView; undo history/scroll are preserved via a per-path `EditorState` cache in Editor.tsx
- **Tab reloads** are mtime-aware: non-dirty tabs only re-read when `get_file_mtime` reports a change
- **Save flow**: Cmd+S → `onSaveRef.current(path, content)` → `fs::write`
- **Markdown preview**: `.md` tabs get an edit/preview toggle; renderer is `lib/markdown.ts` (no markdown dependency)
- **Image viewer**: Editor splits into ImageViewer / TextViewer components (Rules of Hooks)
- **Skill sync** uses symlinks (junctions on Windows) from `~/.agents/skills` to each tool's skills directory; conflicting tool-side skills are moved to `~/.aide/sync-backup/<tool>/<skill>-<timestamp>` unless overwrite is requested
- **MCP sync is an upsert merge**: central servers override same-name entries in the tool config, tool-only servers are preserved; central config at `~/.aide/mcp.json`
- **Tool adapters**: 43 tools with vercel-labs/skills-verified paths; tools whose global dir is `.agents/skills` are native (no symlink), the rest are linked on sync
- **FileTree context menu**: right-click for Rename, Delete, Reveal in Finder, New File, New Folder; arrow keys navigate, Enter opens/toggles
- **New Skill** scaffolds `<name>/SKILL.md` with frontmatter via `create_directory` + `write_text_file`
- **Tab management**: Cmd+W closes the active tab; closing a tab with unsaved edits asks for confirmation; Cmd+K opens the search palette (skills + full-text file matches)
- **Skill import**: local folders are copied into ~/.agents/skills via `import_skill`; registry installs come from GitHub tarballs via `install_skill_from_registry` (no git binary needed); both record provenance in ~/.aide/skill-sources.json, surfaced as `SkillInfo.source`
- **Editor theme**: follows the app theme (oneDark in dark mode, palette-driven light theme via CSS variables)
- **Brand**: indigo accent on primary actions/rings/links (`--primary`/`--ring`); header logo + app icon share the 2x2 module-grid mark
- **Model management**: profiles in ~/.aide/models.json (0600) pair a models.dev provider with an API key, optional base URL and model selection; sync targets are only the tools with file-based model config (claude_code settings.json env for anthropic/protocol proxies, opencode provider section, codex config.toml model_providers with env_key); other targets report unsupported
- **Agent access**: aide bundles an `aide-mcp` sidecar (src/bin/aide-mcp.rs) that is both an MCP stdio server (8 skill-domain tools: list/search/install/update/publish/pull) and a human CLI; enabling agent access registers it in the central MCP config, synced to tools like any other server. Registry installs record the upstream commit so `check_skill_updates` can detect upstream changes via the GitHub API
- **Git backup**: scopes in ~/.aide/remotes.json map skills to separate remotes (personal vs company repos stay isolated). Canonical skills are never restructured; each scope keeps a mirrored worktree in ~/.aide/scopes/<scope>. Publish = mirror + commit + push (rebase retry on non-fast-forward); Pull = ff-only pull + remote-wins write-back that never deletes local-only skills. Git shells out to the system `git` so SSH agents/credential helpers apply
- Port: 1430 (Vite) / 1431 (HMR)
