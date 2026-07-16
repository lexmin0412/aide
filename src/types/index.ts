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

export type ViewState =
  | { type: "home" }
  | { type: "skill"; skill: SkillInfo }
