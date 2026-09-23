import { useState, useEffect, useCallback, useImperativeHandle, forwardRef, useRef, useMemo } from "react"
import { invoke } from "@tauri-apps/api/core"
import { revealItemInDir } from "@tauri-apps/plugin-opener"
import { FileText, Folder, FolderOpen, Pencil, Trash2, FilePlus, FolderPlus, ExternalLink } from "lucide-react"
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
import { toast } from "@/lib/toast"
import { useTranslation } from "react-i18next"
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
  newFile: () => void
  newFolder: () => void
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

/** Single inline text input used for rename and create, with local value state
 * so keystrokes do not re-render the whole tree. */
function InlineEditInput({
  initialValue,
  rename,
  placeholder,
  className,
  onCommit,
  onCancel,
}: {
  initialValue: string
  rename?: boolean
  placeholder?: string
  className?: string
  onCommit: (name: string) => void
  onCancel: () => void
}) {
  const inputRef = useRef<HTMLInputElement>(null)
  const [val, setVal] = useState(initialValue)

  useEffect(() => {
    const el = inputRef.current
    if (!el) return
    el.focus()
    if (rename) {
      const dotIndex = initialValue.lastIndexOf(".")
      if (dotIndex > 0) el.setSelectionRange(0, dotIndex)
      else el.select()
    } else {
      el.select()
    }
    // Run once on mount; initialValue is fixed for the lifetime of the editor.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return (
    <Input
      ref={inputRef}
      value={val}
      onChange={(e) => setVal(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") { e.preventDefault(); onCommit(val.trim()) }
        if (e.key === "Escape") { e.preventDefault(); onCancel() }
      }}
      onBlur={onCancel}
      className={className}
      placeholder={placeholder}
      onClick={(e) => e.stopPropagation()}
    />
  )
}

export const FileTree = forwardRef<FileTreeHandle, FileTreeProps>(function FileTree(
  { rootPath, onSelectFile, selectedPath, onFileDeleted, onFileRenamed },
  ref
) {
  const { t } = useTranslation()
  const [dirChildren, setDirChildren] = useState<Record<string, FileEntry[]>>({})
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set())
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null)
  const [editing, setEditing] = useState<EditingState | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<FileEntry | null>(null)
  const [keyIndex, setKeyIndex] = useState(-1)
  const containerRef = useRef<HTMLDivElement>(null)

  const loadDir = useCallback(async (path: string) => {
    const entries = await invoke<FileEntry[]>("list_directory", { path })
    setDirChildren((prev) => ({ ...prev, [path]: entries }))
    return entries
  }, [])

  const refresh = useCallback(async () => {
    await Promise.all([rootPath, ...expandedDirs].map((dir) => loadDir(dir)))
  }, [rootPath, expandedDirs, loadDir])

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
    setDirChildren({})
    setKeyIndex(-1)
    // Restore the previously expanded folders for this skill.
    let stored: string[] = []
    try {
      stored = JSON.parse(localStorage.getItem(`filetree-expanded-${rootPath}`) ?? "[]")
    } catch {}
    setExpandedDirs(new Set(stored))
    loadDir(rootPath)
    for (const dir of stored) {
      void loadDir(dir)
    }
    // Run per skill switch; loadDir is stable.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rootPath])

  useEffect(() => {
    localStorage.setItem(`filetree-expanded-${rootPath}`, JSON.stringify([...expandedDirs]))
  }, [expandedDirs, rootPath])

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

  const commitRename = useCallback(async (entry: FileEntry, name: string) => {
    setEditing(null)
    if (!name) return
    const parentDir = entry.path.substring(0, entry.path.lastIndexOf("/"))
    const newPath = parentDir + "/" + name
    if (newPath === entry.path) return
    try {
      await invoke("rename_entry", { oldPath: entry.path, newPath })
      await loadDir(parentDir)
      onFileRenamed?.(entry.path, newPath)
    } catch (e) {
      toast(t("fileTree.renameFailed", { error: String(e) }), "error")
    }
  }, [loadDir, onFileRenamed])

  const commitNewFile = useCallback(async (dirPath: string, name: string) => {
    setEditing(null)
    if (!name) return
    const filePath = dirPath + "/" + name
    try {
      await invoke("create_file", { path: filePath })
      setExpandedDirs((prev) => new Set(prev).add(dirPath))
      await loadDir(dirPath)
      onSelectFile(filePath)
    } catch (e) {
      toast(t("fileTree.createFileFailed", { error: String(e) }), "error")
    }
  }, [loadDir, onSelectFile])

  const commitNewFolder = useCallback(async (dirPath: string, name: string) => {
    setEditing(null)
    if (!name) return
    const dirTarget = dirPath + "/" + name
    try {
      await invoke("create_directory", { path: dirTarget })
      setExpandedDirs((prev) => new Set(prev).add(dirPath))
      await loadDir(dirPath)
    } catch (e) {
      toast(t("fileTree.createFolderFailed", { error: String(e) }), "error")
    }
  }, [loadDir])

  const cancelEdit = useCallback(() => setEditing(null), [])

  const handleDelete = useCallback(async () => {
    if (!deleteTarget) return
    try {
      await invoke("delete_entry", { path: deleteTarget.path })
      const parentDir = deleteTarget.path.substring(0, deleteTarget.path.lastIndexOf("/"))
      await loadDir(parentDir)
      onFileDeleted?.(deleteTarget.path)
      toast(t("fileTree.movedToast", { name: deleteTarget.name }), "success")
    } catch (e) {
      toast(t("fileTree.deleteFailed", { error: String(e) }), "error")
    }
    setDeleteTarget(null)
  }, [deleteTarget, loadDir, onFileDeleted])

  const revealEntry = useCallback(async (entry: FileEntry) => {
    setContextMenu(null)
    try {
      await revealItemInDir(entry.path)
    } catch (e) {
      toast(t("fileTree.revealFailed", { error: String(e) }), "error")
    }
  }, [])

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

  useImperativeHandle(ref, () => ({
    refresh,
    newFile: () => startNewFile(rootPath),
    newFolder: () => startNewFolder(rootPath),
  }), [refresh, startNewFile, startNewFolder, rootPath])

  // Flattened list of currently visible rows, used for arrow-key navigation.
  const flatEntries = useMemo(() => {
    const list: FileEntry[] = []
    const walk = (entries: FileEntry[]) => {
      for (const entry of entries) {
        list.push(entry)
        if (entry.is_dir && expandedDirs.has(entry.path)) walk(dirChildren[entry.path] || [])
      }
    }
    walk(dirChildren[rootPath] || [])
    return list
  }, [dirChildren, expandedDirs, rootPath])

  useEffect(() => {
    if (keyIndex < 0) return
    const el = containerRef.current?.querySelector(`[data-idx="${keyIndex}"]`)
    el?.scrollIntoView({ block: "nearest" })
  }, [keyIndex])

  const onTreeKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (editing) return
    if (e.key === "ArrowDown") {
      e.preventDefault()
      setKeyIndex((prev) => Math.min(prev + 1, flatEntries.length - 1))
    } else if (e.key === "ArrowUp") {
      e.preventDefault()
      setKeyIndex((prev) => Math.max(prev - 1, 0))
    } else if (e.key === "Home") {
      e.preventDefault()
      setKeyIndex(flatEntries.length > 0 ? 0 : -1)
    } else if (e.key === "End") {
      e.preventDefault()
      setKeyIndex(flatEntries.length - 1)
    } else if (e.key === "Enter" && keyIndex >= 0 && keyIndex < flatEntries.length) {
      e.preventDefault()
      const entry = flatEntries[keyIndex]
      if (entry.is_dir) toggleDir(entry)
      else onSelectFile(entry.path)
    }
  }, [editing, flatEntries, keyIndex, toggleDir, onSelectFile])

  const renderTree = (entries: FileEntry[], depth: number, indexRef: { current: number }) => {
    const items: React.ReactNode[] = []
    const dirs = entries.filter((e) => e.is_dir)
    const files = entries.filter((e) => !e.is_dir)
    for (const entry of dirs) {
      const idx = indexRef.current++
      const isExpanded = expandedDirs.has(entry.path)
      const children = dirChildren[entry.path] || []
      const isEditingRename = editing?.type === "rename" && editing.originalPath === entry.path
      const isEditingNewInThisDir = editing && (editing.type === "newFile" || editing.type === "newFolder") && editing.parentDir === entry.path
      items.push(
        <div key={entry.path}>
          <div
            data-idx={idx}
            className={`flex items-center gap-1 px-2 py-1 cursor-pointer text-xs ${
              selectedPath === entry.path
                ? "bg-accent text-accent-foreground"
                : "hover:bg-card/60"
            } ${keyIndex === idx ? "outline outline-1 -outline-offset-1 outline-ring/70" : ""}`}
            style={{ paddingLeft: `${depth * 16 + 8}px` }}
            onClick={() => { setKeyIndex(idx); toggleDir(entry) }}
            onContextMenu={(e) => { e.preventDefault(); e.stopPropagation(); setContextMenu({ x: e.clientX, y: e.clientY, entry }) }}
          >
            <span className="text-[10px] text-muted-foreground w-3">{isExpanded ? "\u25BC" : "\u25B6"}</span>
            {isExpanded ? <FolderOpen size={14} className="text-muted-foreground shrink-0" /> : <Folder size={14} className="text-muted-foreground shrink-0" />}
            {isEditingRename && editing ? (
              <InlineEditInput
                initialValue={editing.value}
                rename
                className="h-5 text-xs px-1 py-0 flex-1 min-w-0"
                onCommit={(name) => commitRename(entry, name)}
                onCancel={cancelEdit}
              />
            ) : (
              <span className="truncate">{entry.name}</span>
            )}
          </div>
          {isExpanded && (
            <>
              {isEditingNewInThisDir && editing && (
                <div style={{ paddingLeft: `${(depth + 1) * 16 + 24}px` }} className="flex items-center gap-1 px-2 py-0.5">
                  <InlineEditInput
                    initialValue=""
                    placeholder={editing.type === "newFile" ? t("fileTree.filename") : t("fileTree.folderName")}
                    className="h-5 text-xs px-1 py-0 flex-1 min-w-0"
                    onCommit={(name) =>
                      editing.type === "newFile" ? commitNewFile(editing.parentDir, name) : commitNewFolder(editing.parentDir, name)
                    }
                    onCancel={cancelEdit}
                  />
                </div>
              )}
              {renderTree(children, depth + 1, indexRef)}
            </>
          )}
        </div>
      )
    }
    for (const entry of files) {
      const idx = indexRef.current++
      const isEditingRename = editing?.type === "rename" && editing.originalPath === entry.path
      const ext = entry.extension?.toLowerCase() || ""
      const isText = TEXT_EXTENSIONS.has(ext)
      const isImage = IMAGE_EXTENSIONS.has(ext)
      items.push(
        <div
          key={entry.path}
          data-idx={idx}
          className={`flex items-center gap-1 px-2 py-1 cursor-pointer text-xs ${
            selectedPath === entry.path
              ? "bg-accent text-accent-foreground"
              : "hover:bg-card/60"
          } ${keyIndex === idx ? "outline outline-1 -outline-offset-1 outline-ring/70" : ""}`}
          style={{ paddingLeft: `${depth * 16 + 24}px` }}
          onClick={() => { setKeyIndex(idx); onSelectFile(entry.path) }}
          onContextMenu={(e) => { e.preventDefault(); e.stopPropagation(); setContextMenu({ x: e.clientX, y: e.clientY, entry }) }}
        >
          {isImage ? <span className="text-sm shrink-0">{'\u{1F5BC}'}</span> : isText ? <FileText size={14} className="text-muted-foreground shrink-0" /> : <FileText size={14} className="text-muted-foreground/50 shrink-0" />}
          {isEditingRename && editing ? (
            <InlineEditInput
              initialValue={editing.value}
              rename
              className="h-5 text-xs px-1 py-0 flex-1 min-w-0"
              onCommit={(name) => commitRename(entry, name)}
              onCancel={cancelEdit}
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
    <div
      ref={containerRef}
      className="py-1 select-none outline-none"
      tabIndex={0}
      onKeyDown={onTreeKeyDown}
    >
      {isEditingNewAtRoot && editing && (
        <div style={{ paddingLeft: 24 }} className="flex items-center gap-1 px-2 py-0.5">
          <InlineEditInput
            initialValue=""
            placeholder={editing.type === "newFile" ? t("fileTree.filename") : t("fileTree.folderName")}
            className="h-5 text-xs px-1 py-0 flex-1 min-w-0"
            onCommit={(name) =>
              editing.type === "newFile" ? commitNewFile(editing.parentDir, name) : commitNewFolder(editing.parentDir, name)
            }
            onCancel={cancelEdit}
          />
        </div>
      )}
      {renderTree(rootLevel, 0, { current: 0 })}

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
                <FilePlus size={14} /> {t("fileTree.newFile")}
              </button>
              <button
                className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 hover:bg-accent hover:text-accent-foreground"
                onClick={() => startNewFolder(contextMenu.entry.path)}
              >
                <FolderPlus size={14} /> {t("fileTree.newFolder")}
              </button>
              <div className="my-1 h-px bg-border" />
            </>
          )}
          <button
            className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 hover:bg-accent hover:text-accent-foreground"
            onClick={() => startRename(contextMenu.entry)}
          >
            <Pencil size={14} /> {t("fileTree.rename")}
          </button>
          <button
            className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 hover:bg-accent hover:text-accent-foreground"
            onClick={() => revealEntry(contextMenu.entry)}
          >
            <ExternalLink size={14} /> {t("fileTree.reveal")}
          </button>
          <button
            className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-destructive hover:bg-destructive/10"
            onClick={() => { setDeleteTarget(contextMenu.entry); setContextMenu(null) }}
          >
            <Trash2 size={14} /> {t("fileTree.delete")}
          </button>
        </div>
      )}

      <Dialog open={!!deleteTarget} onOpenChange={(open) => { if (!open) setDeleteTarget(null) }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("fileTree.deleteTitle", { type: deleteTarget?.is_dir ? t("fileTree.typeFolder") : t("fileTree.typeFile") })}</DialogTitle>
            <DialogDescription>
              {t("fileTree.deleteDesc", { name: deleteTarget?.name })}
              {deleteTarget?.is_dir && t("fileTree.deleteDescDir")}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" size="sm" onClick={() => setDeleteTarget(null)}>{t("common.cancel")}</Button>
            <Button variant="destructive" size="sm" onClick={handleDelete}>{t("fileTree.moveToTrash")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
})
