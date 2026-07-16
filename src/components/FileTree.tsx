import { useState, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import type { FileEntry } from "../types"

interface FileTreeProps {
  rootPath: string
  onSelectFile: (path: string) => void
  selectedPath: string | null
}

const TEXT_EXTENSIONS = new Set([
  "md", "json", "jsonc", "yaml", "yml", "toml",
  "txt", "js", "ts", "jsx", "tsx", "css", "html",
  "sh", "bash", "env",
])

export function FileTree({ rootPath, onSelectFile, selectedPath }: FileTreeProps) {
  const [dirChildren, setDirChildren] = useState<Record<string, FileEntry[]>>({})
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set())

  const loadDir = useCallback(async (path: string) => {
    const entries = await invoke<FileEntry[]>("list_directory", { path })
    setDirChildren((prev) => ({ ...prev, [path]: entries }))
    return entries
  }, [])

  const toggleDir = useCallback(
    async (entry: FileEntry) => {
      const newExpanded = new Set(expandedDirs)
      if (newExpanded.has(entry.path)) {
        newExpanded.delete(entry.path)
      } else {
        newExpanded.add(entry.path)
        if (!dirChildren[entry.path]) {
          await loadDir(entry.path)
        }
      }
      setExpandedDirs(newExpanded)
    },
    [expandedDirs, dirChildren, loadDir]
  )

  useEffect(() => {
    setDirChildren({})
    setExpandedDirs(new Set())
    loadDir(rootPath)
  }, [rootPath])

  const renderTree = (entries: FileEntry[], depth: number) => {
    const items: React.ReactNode[] = []

    const dirs = entries.filter((e) => e.is_dir)
    const files = entries.filter((e) => !e.is_dir)

    for (const entry of dirs) {
      const isExpanded = expandedDirs.has(entry.path)
      const children = dirChildren[entry.path] || []
      items.push(
        <div key={entry.path}>
          <div
            className={`tree-item ${selectedPath === entry.path ? "selected" : ""}`}
            style={{ paddingLeft: `${depth * 16 + 8}px` }}
            onClick={() => toggleDir(entry)}
          >
            <span className="tree-icon">{isExpanded ? "\u25BE" : "\u25B8"}</span>
            <span className="tree-folder-icon">{isExpanded ? "\uD83D\uDCC2" : "\uD83D\uDCC1"}</span>
            <span className="tree-name">{entry.name}</span>
          </div>
          {isExpanded && renderTree(children, depth + 1)}
        </div>
      )
    }

    for (const entry of files) {
      const ext = entry.extension?.toLowerCase() || ""
      const isText = TEXT_EXTENSIONS.has(ext)
      items.push(
        <div key={entry.path}>
          <div
            className={`tree-item tree-file ${selectedPath === entry.path ? "selected" : ""}`}
            style={{ paddingLeft: `${depth * 16 + 24}px` }}
            onClick={() => onSelectFile(entry.path)}
          >
            <span className="file-icon">{isText ? "\uD83D\uDCC4" : "\uD83D\uDCC1"}</span>
            <span className="tree-name">{entry.name}</span>
            {!isText && <span className="file-badge binary">bin</span>}
          </div>
        </div>
      )
    }

    return items
  }

  const rootLevel = dirChildren[rootPath] || []
  return <div className="file-tree">{renderTree(rootLevel, 0)}</div>
}
