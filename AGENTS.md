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
│   ├── UnsavedChangesDialog.tsx # Shared "leave with unsaved edits" guard
│   ├── PageError.tsx      # Page-level load failure + retry state
│   └── ui/                # shadcn/ui primitives (button, dialog, input, popover, etc.)
├── hooks/
│   ├── useTabs.ts         # Shared tab state (dirty channel, live-doc readers, leave guards)
│   └── useSidebarWidth.ts # Resizable, persisted sidebar width
├── lib/
│   ├── utils.ts           # cn() utility (clsx + tailwind-merge)
│   ├── fileUtils.ts       # readFileAsTab, getFileMtime, isImageFile helpers
│   ├── editorStateCache.ts# Bounded LRU of parked CodeMirror states (undo/scroll survive tab switches)
│   ├── markdown.ts        # Minimal markdown renderer for preview
│   ├── theme.ts           # Theme preference (system/light/dark) helpers
│   └── toast.tsx          # Global toast emitter + ToastHost (aria-live)
├── test/
│   └── setup.ts           # Vitest + jest-dom setup
├── stores/
│   └── skillStore.ts      # Search query + scroll position persistence
├── types/
│   └── index.ts           # Shared TypeScript types
├── App.tsx                # Root layout (page routing + skill CRUD + palette + theme)
├── App.css                # Global styles + Tailwind theme
└── main.tsx               # Entry point

src-tauri/
└── src/
    ├── lib.rs             # Tauri commands (42 commands)
    ├── main.rs            # Entry point
    ├── storage.rs         # Atomic writes, backups, strict JSON reads (missing vs corrupt)
    ├── adapter/mod.rs     # Tool adapter definitions (43 AI tools)
    ├── registry.rs        # skills.sh registry search + GitHub install
    ├── git_sync.rs        # Scoped git backup/share (publish/pull per remote)
    ├── models.rs          # Model profiles + models.dev registry + per-tool sync
    └── bin/aide-mcp.rs    # MCP stdio server + CLI over the same core (sidecar)
```

## Rust Commands (src-tauri/src/lib.rs)

| Command | Params | Returns | Description |
|---|---|---|---|
| `list_skills` | — | `SkillInfo[]` | List skills from ~/.agents/skills |
| `import_skill` | `source: string` | `string` | Copy a local folder into ~/.agents/skills (rejects duplicates, skips node_modules/.git/etc.) |
| `search_registry` | `query: string` | `RegistrySkill[]` | Search the skills.sh community registry (same unauthenticated API as `npx skills`) |
| `install_skill_from_registry` | `id: string` | `RemoteInstallResult` | Install `owner/repo[/skill]` from GitHub (25 MB archive cap, 20 MB/file and 150 MB total extraction caps, provenance in ~/.aide/skill-sources.json) |
| `search_skills` | `query: string` | `SkillSearchMatch[]` | Case-insensitive full-text search across skill text files |
| `list_directory` | `path: string` | `FileEntry[]` | List dir (dirs first, alpha) |
| `read_text_file` | `path: string` | `string` | Read file as UTF-8 |
| `write_text_file` | `path, content: string` | `void` | Atomically write a file (temp + fsync + rename, keeps existing mode) |
| `create_file` | `path: string` | `void` | Create an empty file; fails if the path already exists |
| `create_directory` | `path: string` | `void` | Create dir (recursive); fails if the path is a file |
| `delete_entry` | `path: string` | `void` | Move file/dir to system trash; errors instead of deleting permanently when trash is unavailable |
| `rename_entry` | `old_path, new_path: string` | `void` | Rename/move; refuses to overwrite an existing destination |
| `file_exists` | `path: string` | `boolean` | Check existence |
| `get_file_mtime` | `path: string` | `u64` | File modification time (unix seconds) |
| `get_home_dir` | — | `string` | User home directory |
| `list_tools` | — | `ToolInfo[]` | List supported AI tools |
| `check_sync_statuses` | — | `ToolInfo[]` | Tools with sync status |
| `sync_tool` | `tool_key: string, overwrite?: bool` | `SyncResult` | Sync skills to one tool; tool-only skills are adopted into the central store, conflicting copies always backed up to ~/.aide/sync-backup, and the replaced directory is kept rather than deleted |
| `sync_all_tools` | — | `SyncResult[]` | Sync skills to all tools |
| `list_mcp_servers` | — | `McpServerView[]` | List central MCP servers |
| `save_mcp_servers` | `servers: McpServerView[]` | `void` | Save central MCP config (atomic + backup, 0600) |
| `list_mcp_tools` | — | `McpToolView[]` | List MCP-capable tools |
| `sync_mcp_tool` | `tool_key: string` | `McpSyncResult` | Sync MCP to one tool; unparsable tool configs abort instead of being replaced |
| `sync_mcp_all` | — | `Result<McpSyncResult[]>` | Sync MCP to all tools |
| `import_mcp_all` | — | `Result<ImportResult[]>` | Import MCP configs from all tools (JSON + TOML), per-adapter errors reported |
| `update_skill_tags` | `path, tags: string[]` | `void` | Update tags in SKILL.md frontmatter via YAML parse/serialize (validated before write, 0600 backup) |
| `check_skill_updates` | — | `Result<SkillUpdate[]>` | Registry-installed skills whose upstream repo advanced past the recorded commit |
| `update_skill` | `skill: string` | `RemoteInstallResult` | Reinstall a skill from its recorded upstream id (replaced copy goes to trash) |
| `get_agent_server` | — | `AgentServerStatus` | aide-mcp sidecar availability and whether agent access is enabled (validates the registered command path) |
| `enable_agent_access` | `enable: bool` | `AgentServerStatus` | Add/remove the aide MCP server entry in the central MCP config |
| `list_model_providers` | `force?: bool` | `ProviderSummary[]` | models.dev providers+models with a 24h disk cache (~/.aide/cache) |
| `list_model_profiles` | — | `Result<ModelProfilesConfig>` | Read ~/.aide/models.json (0600); corrupt files are an error, not empty defaults |
| `save_model_profiles` | `config: ModelProfilesConfig` | `void` | Validate and atomically write model profiles (backup + 0600) |
| `sync_model_profile` | `profile_id: string` | `ModelSyncResult[]` | Write one profile into its target tools (claude_code env / opencode provider / codex config.toml) |
| `sync_all_model_profiles` | — | `ModelProfileSyncResult[]` | Sync every profile to its targets |
| `get_git_config` | — | `Result<GitRemotesConfig>` | Read ~/.aide/remotes.json (scopes + skill scope assignments) |
| `save_git_config` | `config: GitRemotesConfig` | `void` | Validate and atomically write the scope config |
| `set_skill_scope` | `skill: string, scope?: string` | `void` | Assign a skill to a git scope (None = default scope) |
| `publish_scope` | `scope: string` | `PublishOutcome` | Mirror the scope's skills into its worktree, commit and push (refuses empty scopes, preserves repo files, rescues teammate skills) |
| `pull_scope` | `scope: string` | `PullOutcome` | Fast-forward pull, then write remote-side skills back (remote wins, only skills of this scope, never deletes local-only) |
| `get_mcp_drift` | — | `Result<McpDrift[]>` | Compare each tool config with the central MCP config + managed manifest (`missing`/`leftover` per tool) |
| `cleanup_mcp_drift` | — | `Result<McpSyncResult[]>` | Re-sync every tool so disabled/deleted servers are removed from its config |

## Conventions

- **English only**: all code, comments, documentation, commit messages, and release notes must be in English
- **No emoji** in code or commit messages
- **No README** files unless explicitly requested
- All Rust commands return `Result<T, String>`
- **Persistence goes through `storage.rs`**: never `fs::write` a user config directly. Use `storage::atomic_write` / `write_json` (temp + fsync + rename + backup) and `storage::read_json` (returns `Err` on corrupt files — never fall back to defaults and overwrite the user's file)
- Frontend invokes via `@tauri-apps/api/core` `invoke()`
- Editor keymap uses `useRef` for callbacks to avoid stale closures
- CSS variables in `:root`/`.dark` for theming (light + dark palettes, oklch color space); theme preference stored in localStorage `theme` (absent = system)
- Context menus use `pointerdown` (capture) + `menuRef.contains()` pattern
- Hooks must always be called unconditionally (Rules of Hooks)
- User-facing async failures go through `toast()` from `@/lib/toast`, never `alert()`
- **Quality gates are blocking**: `pnpm typecheck`, `pnpm lint`, `pnpm test`, `cargo fmt --check`, `cargo clippy --lib --no-deps -- -D warnings`, `cargo test --lib` must pass before a change is considered done

## Development

```bash
pnpm tauri dev      # Start dev server with hot-reload
pnpm build          # TypeScript check + Vite build
pnpm typecheck      # tsc --noEmit
pnpm lint           # ESLint 9 flat config
pnpm test           # Vitest (pure logic + hooks)
pnpm tauri build    # Production build + bundling

cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo clippy --manifest-path src-tauri/Cargo.toml --lib --no-deps -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

CI (`.github/workflows/ci.yml`) runs the same gates on every push/PR.

## Architecture Notes

- **File CRUD** uses direct Rust `std::fs` operations (unrestricted access); images are served via the Tauri asset protocol (`convertFileSrc`), not base64 IPC
- **Persistence**: every config write goes through `storage.rs` (atomic temp+rename, previous revision copied to `~/.aide/backups`, 0600 for secret-bearing files); corrupt configs surface as errors, so a damaged `mcp.json`/`models.json`/`remotes.json` is never silently replaced by defaults
- **Editor state** is split into two channels: the CodeMirror document is the single source of truth (dirty is reported once per dirty episode, not per keystroke), while `tab.content` is only a flushed snapshot. Save/preview/close/leave read the live document through a registered doc source, so switching tabs never loses edits. Undo history and scroll survive via a bounded LRU EditorState cache (`lib/editorStateCache.ts`, 16 entries)
- **Tab reloads** are mtime-aware: non-dirty tabs only re-read when `get_file_mtime` reports a change
- **Save flow**: Cmd+S → `onSave(path)` → live doc read from the EditorView → `write_text_file` (`storage::atomic_write`)
- **Unsaved changes** are guarded by a single `UnsavedChangesDialog`: back navigation, top-nav away from skills, palette skill switching and ConfigPanel tool switching all go through `useTabs.requestLeave`
- **Markdown preview**: `.md` tabs get an edit/preview toggle; renderer is `lib/markdown.ts` (no markdown dependency; supports headings, lists, code fences, blockquotes, GFM tables, images, and inline formatting)
- **Image viewer**: Editor splits into ImageViewer / TextViewer components (Rules of Hooks)
- **Skill sync** uses symlinks (junctions on Windows, symlinks on Linux) from `~/.agents/skills` to each tool's skills directory. Tool-only skills are adopted into the central store, conflicting tool-side copies are always backed up to `~/.aide/sync-backup/<tool>/`, and the replaced directory is renamed (not deleted); creating the link restores the original on failure
- **MCP sync is a managed upsert merge**: central servers override same-name entries in the tool config and tool-only servers are preserved. `~/.aide/mcp-managed.json` records which names aide wrote per tool, so disabling/deleting a server (or removing a tool from its `targets`) removes exactly those entries on the next sync; nothing aide did not write is ever deleted. `get_mcp_drift` reports tools whose config diverged (`missing`) or still contain no-longer-managed servers (`leftover`), and the UI offers `cleanup_mcp_drift`. Central config at `~/.aide/mcp.json`. Supported formats: JSON/JSONC/TOML; Trae and VS Code paths are per-OS; OpenCode config resolves to `opencode.jsonc` when present (else `opencode.json`, new installs use `.jsonc`) and the same file is used by model sync
- **Long-running commands run off the IPC thread**: installs/updates, publish/pull, registry search, update checks, model provider fetch, MCP sync/drift and full-text search use `#[tauri::command(async)]`; progress reporting is still pending
- **Update checks are deduped and cached**: `check_skill_updates` groups by `(owner, repo, subtree)`, calls the GitHub API once per group and caches HEAD commits for 10 minutes in `~/.aide/cache/github-heads.json`; failures are not cached. Pending: `list_skills` mtime cache (directory rescan is still O(skills × files))
- **Tool adapters**: 43 tools with vercel-labs/skills-verified paths; tools whose global dir is `.agents/skills` are native (no symlink), the rest are linked on sync
- **FileTree context menu**: right-click for Rename, Delete, Reveal in Finder, New File, New Folder; arrow keys navigate, Enter opens/toggles. Create/rename check for existing targets before writing
- **New Skill** scaffolds `<name>/SKILL.md` with frontmatter via `create_directory` + `write_text_file`
- **Tab management**: Cmd+W closes only the active page's tab (`useTabs({ active })`); closing a dirty tab asks for confirmation; Cmd+K opens the search palette (skills + full-text file matches) with stale-response protection
- **Skill import**: local folders are copied into ~/.agents/skills via `import_skill`; registry installs come from GitHub tarballs via `install_skill_from_registry` (no git binary needed); both record provenance in ~/.aide/skill-sources.json, surfaced as `SkillInfo.source`
- **Registry installs** are named from the SKILL.md frontmatter (then the requested skill segment, then the repo name), download a pinned commit, enforce extraction limits, and swap the staged directory into place without deleting the previous copy first
- **Editor theme**: follows the app theme (oneDark in dark mode, palette-driven light theme via CSS variables)
- **Brand**: indigo accent on primary actions/rings/links (`--primary`/`--ring`); header logo + app icon share the 2x2 module-grid mark
- **Model management**: profiles in ~/.aide/models.json (0600) pair a models.dev provider with an API key, optional base URL and model selection; sync targets are only the tools with file-based model config (claude_code settings.json env for anthropic/protocol proxies, opencode provider section, codex config.toml model_providers with env_key); other targets report unsupported
- **Agent access**: aide bundles an `aide-mcp` sidecar (src/bin/aide-mcp.rs) that is both an MCP stdio server (9 skill-domain tools: list/search/search-registry/install/check-updates/update/sync/publish/pull) and a human CLI; enabling agent access registers it in the central MCP config, synced to tools like any other server. `get_agent_server` only reports enabled when the registered command still points at the running binary. Registry installs record the upstream commit so `check_skill_updates` can detect upstream changes via the GitHub API
- **Git backup**: scopes in ~/.aide/remotes.json map skills to separate remotes (personal vs company repos stay isolated). Canonical skills are never restructured; each scope keeps a mirrored worktree in ~/.aide/scopes/<scope>. Publish refuses empty scopes (would clear the remote), only manages skill directories (README/.github survive), and rescues skills pushed by teammates back into the central store; Pull = ff-only + remote-wins write-back limited to the scope's own skills, never deletes local-only skills. Git is resolved once (`AIDE_GIT_BIN` → PATH → Homebrew/usr paths) so Finder-launched apps work; remote URLs and branch refs are validated, and `--` guards keep user input out of option position. Pending: chunked progress for long git runs
- Port: 1430 (Vite) / 1431 (HMR)
