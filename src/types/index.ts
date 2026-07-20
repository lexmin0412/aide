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
}

export interface SkillInfo {
  name: string
  display_name: string
  description: string
  path: string
  is_symlink: boolean
  target_path: string | null
  file_count: number
}

export interface ToolInfo {
  key: string
  name: string
  global_skills: string
  detect_dir: string
  project_skills: string
  status: string
}
