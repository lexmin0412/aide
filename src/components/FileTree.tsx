import { useState, useEffect, useCallback } from "react"
import type { FileEntry } from "../types"
import { useFileSystem } from "../hooks/useFileSystem"

interface FileTreeProps {
  fs: ReturnType<typeof useFileSystem>
  onSelectDir: (path: string) => void
  onSelectFile: (path: string) => void
  selectedPath: string | null
}

export function FileTree({ fs, onSelectDir, selectedPath }: FileTreeProps) {
  const [dirChildren, setDirChildren] = useState<Record<string, FileEntry[]>>({})
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set())

  const loadDir = useCallback(
    async (path: string) => {
      const entries = await fs.listDir(path)
      setDirChildren((prev) => ({
        ...prev,
        [path]: entries,
      }))
      return entries
    },
    [fs]
  )

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
      onSelectDir(entry.path)
    },
    [expandedDirs, dirChildren, loadDir, onSelectDir]
  )

  useEffect(() => {
    fs.init().then((home) => {
      loadDir(home)
    })
  }, [])

  const renderTree = (entries: FileEntry[], depth: number) => {
    return entries
      .filter((e) => e.is_dir)
      .map((entry) => {
        const isExpanded = expandedDirs.has(entry.path)
        const children = dirChildren[entry.path] || []
        return (
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
            {isExpanded && children.length > 0 && renderTree(children, depth + 1)}
          </div>
        )
      })
  }

  const rootLevel = Object.values(dirChildren).flat()
  return <div className="file-tree">{renderTree(rootLevel, 0)}</div>
}
