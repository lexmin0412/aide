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
