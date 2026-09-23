export interface FileEntry {
  name: string
  path: string
  is_dir: boolean
  size: number
  modified_at: number
  extension: string | null
}

export interface EditorTab {
  path: string
  name: string
  content: string
  is_dirty: boolean
  language: string
  is_image?: boolean
  /** File modification time (unix seconds) at last read/save, used to skip redundant reloads. */
  mtime?: number
}

export interface SkillInfo {
  name: string
  display_name: string
  description: string
  tags: string[]
  path: string
  is_symlink: boolean
  target_path: string | null
  file_count: number
  /** Install provenance, e.g. "owner/repo" from the skills registry. */
  source?: string | null
  /** Explicitly assigned git scope; None means the default scope applies. */
  scope?: string | null
}

export interface GitRemoteInfo {
  name: string
  url: string
  branch: string
  is_default: boolean
}

export interface GitRemotesConfig {
  remotes: GitRemoteInfo[]
  skill_scopes: Record<string, string>
}

export interface GitPublishResult {
  committed: boolean
  pushed: boolean
  skills: string[]
  message: string
}

export interface GitPullResult {
  updated: string[]
}

export interface SkillUpdateInfo {
  skill: string
  id: string
  source: string
  installed_commit: string
  latest_commit: string
}

export interface AgentServerStatus {
  enabled: boolean
  binary_available: boolean
  command: string | null
}

export interface ModelSummary {
  id: string
  name: string
  context: number | null
}

export interface ProviderSummary {
  id: string
  name: string
  npm: string | null
  base_url: string | null
  models: ModelSummary[]
}

export interface ModelProfile {
  id: string
  name: string
  provider: string
  base_url: string
  api_key: string
  model: string
  models: string[]
  targets: string[]
}

export interface ModelProfilesConfig {
  profiles: ModelProfile[]
}

export interface ModelSyncResult {
  tool: string
  ok: boolean
  message: string
}

export interface ModelProfileSyncResult {
  profile_id: string
  profile_name: string
  results: ModelSyncResult[]
}

export interface ToolInfo {
  key: string
  name: string
  global_skills: string
  detect_dir: string
  project_skills: string
  status: string
}
