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
      if (newExpanded.has(entry.path)) newExpanded.delete(entry.path)
      else {
        newExpanded.add(entry.path)
        if (!dirChildren[entry.path]) await loadDir(entry.path)
      }
      setExpandedDirs(newExpanded)
    },
    [expandedDirs, dirChildren, loadDir]
  )

  useEffect(() => {
    setDirChildren({}); setExpandedDirs(new Set())
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
            className={`flex items-center gap-1 px-2 py-1 cursor-pointer text-xs hover:bg-card/60 ${
              selectedPath === entry.path ? "bg-card/80" : ""
            }`}
            style={{ paddingLeft: `${depth * 16 + 8}px` }}
            onClick={() => toggleDir(entry)}
          >
            <span className="text-[10px] text-muted-foreground w-3">{isExpanded ? "▼" : "▶"}</span>
            <span className="text-sm">{isExpanded ? "📂" : "📁"}</span>
            <span className="truncate">{entry.name}</span>
          </div>
          {isExpanded && renderTree(children, depth + 1)}
        </div>
      )
    }
    for (const entry of files) {
      const ext = entry.extension?.toLowerCase() || ""
      const isText = TEXT_EXTENSIONS.has(ext)
      items.push(
        <div
          key={entry.path}
          className={`flex items-center gap-1 px-2 py-1 cursor-pointer text-xs hover:bg-card/60 ${
            selectedPath === entry.path ? "bg-card/80" : ""
          }`}
          style={{ paddingLeft: `${depth * 16 + 24}px` }}
          onClick={() => onSelectFile(entry.path)}
        >
          <span className="text-sm">{isText ? "📄" : "📁"}</span>
          <span className="truncate">{entry.name}</span>
          {!isText && <span className="px-1 rounded bg-red-500/15 text-red-400 text-[9px] ml-auto">bin</span>}
        </div>
      )
    }
    return items
  }

  const rootLevel = dirChildren[rootPath] || []
  return <div className="py-1">{renderTree(rootLevel, 0)}</div>
}
