import { useState, useEffect, useCallback, useImperativeHandle, forwardRef, useRef } from "react"
import { invoke } from "@tauri-apps/api/core"
import { FileText, Folder, FolderOpen, Pencil, Trash2, FilePlus, FolderPlus } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { FileEntry } from "../types"

interface FileTreeProps {
  rootPath: string
  onSelectFile: (path: string) => void
  selectedPath: string | null
  onFileDeleted?: (path: string) => void
  onFileRenamed?: (oldPath: string, newPath: string) => void
}

export interface FileTreeHandle {
  refresh: () => Promise<void>
}

const TEXT_EXTENSIONS = new Set([
  "md", "json", "jsonc", "yaml", "yml", "toml",
  "txt", "js", "ts", "jsx", "tsx", "css", "html",
  "sh", "bash", "env",
])

const IMAGE_EXTENSIONS = new Set([
  "png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp",
])

interface ContextMenuState {
  x: number
  y: number
  entry: FileEntry
}

interface EditingState {
  type: "rename" | "newFile" | "newFolder"
  parentDir: string
  value: string
  originalPath?: string
}

export const FileTree = forwardRef<FileTreeHandle, FileTreeProps>(function FileTree(
  { rootPath, onSelectFile, selectedPath, onFileDeleted, onFileRenamed },
  ref
) {
  const [dirChildren, setDirChildren] = useState<Record<string, FileEntry[]>>({})
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set())
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null)
  const [editing, setEditing] = useState<EditingState | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<FileEntry | null>(null)
  const editInputRef = useRef<HTMLInputElement>(null)

  const loadDir = useCallback(async (path: string) => {
    const entries = await invoke<FileEntry[]>("list_directory", { path })
    setDirChildren((prev) => ({ ...prev, [path]: entries }))
    return entries
  }, [])

  const refresh = useCallback(async () => {
    await loadDir(rootPath)
    for (const dirPath of expandedDirs) {
      await loadDir(dirPath)
    }
  }, [rootPath, expandedDirs, loadDir])

  useImperativeHandle(ref, () => ({ refresh }), [refresh])

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

  useEffect(() => {
    if (editing && editInputRef.current) {
      editInputRef.current.focus()
      if (editing.type === "rename") {
        const dotIndex = editing.value.lastIndexOf(".")
        if (dotIndex > 0) editInputRef.current.setSelectionRange(0, dotIndex)
        else editInputRef.current.select()
      } else {
        editInputRef.current.select()
      }
    }
  }, [editing])

  const menuRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!contextMenu) return
    const onPointerDown = (e: PointerEvent) => {
      if (e.button !== 0) return
      if (menuRef.current?.contains(e.target as Node)) return
      setContextMenu(null)
    }
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setContextMenu(null) }
    document.addEventListener("pointerdown", onPointerDown, true)
    document.addEventListener("keydown", onKey)
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true)
      document.removeEventListener("keydown", onKey)
    }
  }, [contextMenu])

  const commitRename = useCallback(async () => {
    if (!editing || editing.type !== "rename" || !editing.originalPath) return
    const parentDir = editing.parentDir
    const newPath = parentDir + "/" + editing.value
    if (newPath === editing.originalPath) { setEditing(null); return }
    try {
      await invoke("rename_entry", { oldPath: editing.originalPath, newPath })
      await loadDir(parentDir)
      onFileRenamed?.(editing.originalPath, newPath)
    } catch (e) { console.error("Failed to rename:", e) }
    setEditing(null)
  }, [editing, loadDir, onFileRenamed])

  const commitNewFile = useCallback(async () => {
    if (!editing || editing.type !== "newFile") return
    const filePath = editing.parentDir + "/" + editing.value
    try {
      await invoke("create_file", { path: filePath })
      if (!expandedDirs.has(editing.parentDir)) {
        setExpandedDirs((prev) => new Set(prev).add(editing.parentDir))
      }
      await loadDir(editing.parentDir)
      onSelectFile(filePath)
    } catch (e) { console.error("Failed to create file:", e) }
    setEditing(null)
  }, [editing, expandedDirs, loadDir, onSelectFile])

  const commitNewFolder = useCallback(async () => {
    if (!editing || editing.type !== "newFolder") return
    const dirPath = editing.parentDir + "/" + editing.value
    try {
      await invoke("create_directory", { path: dirPath })
      if (!expandedDirs.has(editing.parentDir)) {
        setExpandedDirs((prev) => new Set(prev).add(editing.parentDir))
      }
      await loadDir(editing.parentDir)
    } catch (e) { console.error("Failed to create folder:", e) }
    setEditing(null)
  }, [editing, expandedDirs, loadDir])

  const commitEdit = useCallback(() => {
    if (!editing) return
    if (editing.type === "rename") commitRename()
    else if (editing.type === "newFile") commitNewFile()
    else commitNewFolder()
  }, [editing, commitRename, commitNewFile, commitNewFolder])

  const cancelEdit = useCallback(() => setEditing(null), [])

  const handleDelete = useCallback(async () => {
    if (!deleteTarget) return
    try {
      await invoke("delete_entry", { path: deleteTarget.path })
      const parentDir = deleteTarget.path.substring(0, deleteTarget.path.lastIndexOf("/"))
      await loadDir(parentDir)
      onFileDeleted?.(deleteTarget.path)
    } catch (e) { console.error("Failed to delete:", e) }
    setDeleteTarget(null)
  }, [deleteTarget, loadDir, onFileDeleted])

  const startRename = useCallback((entry: FileEntry) => {
    setContextMenu(null)
    setEditing({ type: "rename", parentDir: entry.path.substring(0, entry.path.lastIndexOf("/")), value: entry.name, originalPath: entry.path })
  }, [])

  const startNewFile = useCallback((dirPath: string) => {
    setContextMenu(null)
    setExpandedDirs((prev) => new Set(prev).add(dirPath))
    setEditing({ type: "newFile", parentDir: dirPath, value: "" })
  }, [])

  const startNewFolder = useCallback((dirPath: string) => {
    setContextMenu(null)
    setExpandedDirs((prev) => new Set(prev).add(dirPath))
    setEditing({ type: "newFolder", parentDir: dirPath, value: "" })
  }, [])

  const renderInlineInput = (paddingLeft: number) => (
    <div style={{ paddingLeft: `${paddingLeft}px` }} className="flex items-center gap-1 px-2 py-0.5">
      <Input
        ref={editInputRef}
        value={editing!.value}
        onChange={(e) => setEditing((prev) => prev ? { ...prev, value: e.target.value } : null)}
        onKeyDown={(e) => {
          if (e.key === "Enter") { e.preventDefault(); commitEdit() }
          if (e.key === "Escape") { e.preventDefault(); cancelEdit() }
        }}
        onBlur={cancelEdit}
        className="h-5 text-xs px-1 py-0"
        placeholder={editing!.type === "newFile" ? "filename" : editing!.type === "newFolder" ? "folder name" : undefined}
      />
    </div>
  )

  const renderTree = (entries: FileEntry[], depth: number) => {
    const items: React.ReactNode[] = []
    const dirs = entries.filter((e) => e.is_dir)
    const files = entries.filter((e) => !e.is_dir)
    for (const entry of dirs) {
      const isExpanded = expandedDirs.has(entry.path)
      const children = dirChildren[entry.path] || []
      const isEditingRename = editing?.type === "rename" && editing.originalPath === entry.path
      const isEditingNewInThisDir = editing && (editing.type === "newFile" || editing.type === "newFolder") && editing.parentDir === entry.path
      items.push(
        <div key={entry.path}>
          <div
            className={`flex items-center gap-1 px-2 py-1 cursor-pointer text-xs hover:bg-card/60 ${
              selectedPath === entry.path ? "bg-card/80" : ""
            }`}
            style={{ paddingLeft: `${depth * 16 + 8}px` }}
            onClick={() => toggleDir(entry)}
            onContextMenu={(e) => { e.preventDefault(); e.stopPropagation(); setContextMenu({ x: e.clientX, y: e.clientY, entry }) }}
          >
            <span className="text-[10px] text-muted-foreground w-3">{isExpanded ? "\u25BC" : "\u25B6"}</span>
            {isExpanded ? <FolderOpen size={14} className="text-muted-foreground shrink-0" /> : <Folder size={14} className="text-muted-foreground shrink-0" />}
            {isEditingRename ? (
              <Input
                ref={editInputRef}
                value={editing.value}
                onChange={(e) => setEditing((prev) => prev ? { ...prev, value: e.target.value } : null)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") { e.preventDefault(); commitEdit() }
                  if (e.key === "Escape") { e.preventDefault(); cancelEdit() }
                }}
                onBlur={cancelEdit}
                className="h-5 text-xs px-1 py-0 flex-1 min-w-0"
                onClick={(e) => e.stopPropagation()}
              />
            ) : (
              <span className="truncate">{entry.name}</span>
            )}
          </div>
          {isExpanded && (
            <>
              {isEditingNewInThisDir && editing && renderInlineInput((depth + 1) * 16 + 24)}
              {renderTree(children, depth + 1)}
            </>
          )}
        </div>
      )
    }
    for (const entry of files) {
      const isEditingRename = editing?.type === "rename" && editing.originalPath === entry.path
      const ext = entry.extension?.toLowerCase() || ""
      const isText = TEXT_EXTENSIONS.has(ext)
      const isImage = IMAGE_EXTENSIONS.has(ext)
      items.push(
        <div
          key={entry.path}
          className={`flex items-center gap-1 px-2 py-1 cursor-pointer text-xs hover:bg-card/60 ${
            selectedPath === entry.path ? "bg-card/80" : ""
          }`}
          style={{ paddingLeft: `${depth * 16 + 24}px` }}
          onClick={() => onSelectFile(entry.path)}
          onContextMenu={(e) => { e.preventDefault(); e.stopPropagation(); setContextMenu({ x: e.clientX, y: e.clientY, entry }) }}
        >
          {isImage ? <span className="text-sm shrink-0">{'\u{1F5BC}'}</span> : isText ? <FileText size={14} className="text-muted-foreground shrink-0" /> : <FileText size={14} className="text-muted-foreground/50 shrink-0" />}
          {isEditingRename ? (
            <Input
              ref={editInputRef}
              value={editing.value}
              onChange={(e) => setEditing((prev) => prev ? { ...prev, value: e.target.value } : null)}
              onKeyDown={(e) => {
                if (e.key === "Enter") { e.preventDefault(); commitEdit() }
                if (e.key === "Escape") { e.preventDefault(); cancelEdit() }
              }}
              onBlur={cancelEdit}
              className="h-5 text-xs px-1 py-0 flex-1 min-w-0"
              onClick={(e) => e.stopPropagation()}
            />
          ) : (
            <span className="truncate">{entry.name}</span>
          )}
        </div>
      )
    }
    return items
  }

  const rootLevel = dirChildren[rootPath] || []
  const isEditingNewAtRoot = editing && (editing.type === "newFile" || editing.type === "newFolder") && editing.parentDir === rootPath

  return (
    <div className="py-1 select-none">
      {isEditingNewAtRoot && editing && renderInlineInput(24)}
      {renderTree(rootLevel, 0)}

      {contextMenu && (
        <div
          ref={menuRef}
          className="fixed z-50 min-w-[140px] rounded-md border border-border bg-popover p-1 text-xs shadow-md"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          {contextMenu.entry.is_dir && (
            <>
              <button
                className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 hover:bg-accent hover:text-accent-foreground"
                onClick={() => startNewFile(contextMenu.entry.path)}
              >
                <FilePlus size={14} /> New File
              </button>
              <button
                className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 hover:bg-accent hover:text-accent-foreground"
                onClick={() => startNewFolder(contextMenu.entry.path)}
              >
                <FolderPlus size={14} /> New Folder
              </button>
              <div className="my-1 h-px bg-border" />
            </>
          )}
          <button
            className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 hover:bg-accent hover:text-accent-foreground"
            onClick={() => startRename(contextMenu.entry)}
          >
            <Pencil size={14} /> Rename
          </button>
          <button
            className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-destructive hover:bg-destructive/10"
            onClick={() => { setDeleteTarget(contextMenu.entry); setContextMenu(null) }}
          >
            <Trash2 size={14} /> Delete
          </button>
        </div>
      )}

      <Dialog open={!!deleteTarget} onOpenChange={(open) => { if (!open) setDeleteTarget(null) }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete {deleteTarget?.is_dir ? "Folder" : "File"}</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete <span className="font-mono text-foreground">{deleteTarget?.name}</span>?
              {deleteTarget?.is_dir && " This will delete all contents inside."}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" size="sm" onClick={() => setDeleteTarget(null)}>Cancel</Button>
            <Button variant="destructive" size="sm" onClick={handleDelete}>Delete</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
})
