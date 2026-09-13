import { useState, useCallback, useEffect, useRef } from "react"
import { invoke } from "@tauri-apps/api/core"
import { readFileAsTab, getFileMtime } from "@/lib/fileUtils"
import { toast } from "@/lib/toast"
import type { EditorTab } from "../types"

/**
 * Tab management shared by SkillDetail and ConfigPanel: open/close/switch,
 * dirty tracking, mtime-aware reload and Cmd+W handling. Closing a dirty tab
 * is gated behind a confirmation so unsaved edits are never silently dropped.
 */
export function useTabs() {
  const [tabs, setTabs] = useState<EditorTab[]>([])
  const [activeTabPath, setActiveTabPath] = useState<string | null>(null)
  const [pendingClosePath, setPendingClosePath] = useState<string | null>(null)
  const handleSaveRef = useRef<(path: string, content: string) => Promise<void>>(async () => {})

  const doClose = useCallback(
    (path: string) => {
      const idx = tabs.findIndex((t) => t.path === path)
      const remaining = tabs.filter((t) => t.path !== path)
      setTabs(remaining)
      if (activeTabPath === path) {
        setActiveTabPath(remaining[Math.min(idx, remaining.length - 1)]?.path || null)
      }
    },
    [tabs, activeTabPath]
  )

  const openFile = useCallback(
    async (filePath: string) => {
      const existing = tabs.find((t) => t.path === filePath)
      setActiveTabPath(filePath)
      if (existing) {
        // Reload in place only when the file changed on disk.
        if (existing.is_dirty || existing.is_image) return
        try {
          const mtime = await getFileMtime(filePath)
          if (mtime === existing.mtime) return
          const fresh = await readFileAsTab(filePath)
          setTabs((prev) =>
            prev.map((t) =>
              t.path === filePath && !t.is_dirty ? { ...t, content: fresh.content, mtime: fresh.mtime } : t
            )
          )
        } catch {}
        return
      }
      try {
        const tab = await readFileAsTab(filePath)
        setTabs((prev) => (prev.some((t) => t.path === filePath) ? prev : [...prev, tab]))
      } catch (e) {
        toast(`Failed to open file: ${e}`, "error")
      }
    },
    [tabs]
  )

  const closeTab = useCallback(
    (path: string, e?: React.MouseEvent) => {
      e?.stopPropagation()
      const tab = tabs.find((t) => t.path === path)
      if (tab?.is_dirty) {
        setPendingClosePath(path)
        return
      }
      doClose(path)
    },
    [tabs, doClose]
  )

  const dirtyPendingTab = pendingClosePath
    ? tabs.find((t) => t.path === pendingClosePath) || null
    : null

  const resolvePendingClose = useCallback(
    async (save: boolean) => {
      const path = pendingClosePath
      setPendingClosePath(null)
      if (!path) return
      if (save) {
        const tab = tabs.find((t) => t.path === path)
        if (tab) await handleSaveRef.current(path, tab.content)
      }
      doClose(path)
    },
    [pendingClosePath, tabs, doClose]
  )

  const cancelPendingClose = useCallback(() => setPendingClosePath(null), [])

  useEffect(() => {
    if (tabs.length === 0) return
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "w") {
        e.preventDefault()
        if (activeTabPath) closeTab(activeTabPath)
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [tabs.length, activeTabPath, closeTab])

  const handleEditorChange = useCallback((path: string, content: string) => {
    setTabs((prev) => prev.map((t) => (t.path === path ? { ...t, content, is_dirty: true } : t)))
  }, [])

  const switchTab = useCallback(
    async (path: string) => {
      setActiveTabPath(path)
      const tab = tabs.find((t) => t.path === path)
      if (!tab || tab.is_dirty || tab.is_image) return
      try {
        const mtime = await getFileMtime(path)
        if (mtime === tab.mtime) return
        const fresh = await readFileAsTab(path)
        setTabs((prev) =>
          prev.map((t) => (t.path === path && !t.is_dirty ? { ...t, content: fresh.content, mtime: fresh.mtime } : t))
        )
      } catch {}
    },
    [tabs]
  )

  const refreshAllTabs = useCallback(async () => {
    for (const tab of tabs) {
      if (tab.is_dirty || tab.is_image) continue
      try {
        const fresh = await readFileAsTab(tab.path)
        setTabs((prev) =>
          prev.map((t) => (t.path === tab.path ? { ...t, content: fresh.content, mtime: fresh.mtime } : t))
        )
      } catch {}
    }
  }, [tabs])

  const handleSave = useCallback(async (path: string, content: string) => {
    try {
      await invoke("write_text_file", { path, content })
      const mtime = await getFileMtime(path)
      setTabs((prev) => prev.map((t) => (t.path === path ? { ...t, content, is_dirty: false, mtime } : t)))
    } catch (e) {
      toast(`Failed to save file: ${e}`, "error")
    }
  }, [])

  handleSaveRef.current = handleSave

  const handleFileDeleted = useCallback(
    (path: string) => {
      const idx = tabs.findIndex((t) => t.path === path)
      if (idx === -1) return
      const remaining = tabs.filter((t) => t.path !== path)
      setTabs(remaining)
      if (activeTabPath === path) {
        setActiveTabPath(remaining[Math.min(idx, remaining.length - 1)]?.path || null)
      }
    },
    [tabs, activeTabPath]
  )

  const handleFileRenamed = useCallback(
    (oldPath: string, newPath: string) => {
      setTabs((prev) =>
        prev.map((t) => {
          if (t.path === oldPath) {
            const name = newPath.split("/").pop() || newPath
            return { ...t, path: newPath, name }
          }
          if (t.path.startsWith(oldPath + "/")) {
            const suffix = t.path.slice(oldPath.length)
            const updatedPath = newPath + suffix
            const name = updatedPath.split("/").pop() || updatedPath
            return { ...t, path: updatedPath, name }
          }
          return t
        })
      )
      if (activeTabPath === oldPath) setActiveTabPath(newPath)
      else if (activeTabPath?.startsWith(oldPath + "/")) {
        setActiveTabPath(newPath + activeTabPath.slice(oldPath.length))
      }
    },
    [activeTabPath]
  )

  const clearTabs = useCallback(() => {
    setTabs([])
    setActiveTabPath(null)
    setPendingClosePath(null)
  }, [])

  const activeTab = tabs.find((t) => t.path === activeTabPath)

  return {
    tabs,
    activeTabPath,
    activeTab,
    openFile,
    closeTab,
    switchTab,
    handleEditorChange,
    refreshAllTabs,
    handleSave,
    handleFileDeleted,
    handleFileRenamed,
    clearTabs,
    setActiveTabPath,
    dirtyPendingTab,
    resolvePendingClose,
    cancelPendingClose,
  }
}
