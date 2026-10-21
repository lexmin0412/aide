import { useState, useCallback, useEffect, useRef } from "react"
import { useTranslation } from "react-i18next"
import { invoke } from "@tauri-apps/api/core"
import { readFileAsTab, getFileMtime } from "@/lib/fileUtils"
import { toast } from "@/lib/toast"
import { evictEditorState } from "@/lib/editorStateCache"
import type { EditorTab } from "../types"

/** Reads the live document out of a mounted editor; returns undefined when the
 * editor is not mounted (the tab cache is authoritative then because it is
 * flushed on every tab switch). */
export type DocReader = () => string | undefined

const POST_UNMOUNT_EVICT_DELAY_MS = 0

/**
 * Tab management shared by SkillDetail and ConfigPanel: open/close/switch,
 * dirty tracking, mtime-aware reload, Cmd+W handling and leave guards.
 *
 * Dirty/content flow (dual channel):
 * - the editor reports `docChanged` only once per dirty episode
 *   (`handleDirtyChange`), so keystrokes never re-render the whole app;
 * - the live document is read through `registerDocSource` whenever content is
 *   actually needed: save, markdown preview, closing a dirty tab and unsaved
 *   change guards;
 * - switching tabs flushes the current document into `tab.content`, and the
 *   Editor keeps a bounded per-path state cache so switching back restores the
 *   exact document/undo history.
 */
export function useTabs(options?: { active?: boolean }) {
  const { t } = useTranslation()
  const active = options?.active ?? true
  const [tabs, setTabs] = useState<EditorTab[]>([])
  const [activeTabPath, setActiveTabPath] = useState<string | null>(null)
  const [pendingClosePath, setPendingClosePath] = useState<string | null>(null)
  const [pendingLeave, setPendingLeave] = useState<{ tabs: EditorTab[]; proceed: () => void } | null>(null)
  const docReaders = useRef(new Map<string, DocReader>())
  const handleSaveRef = useRef<(path: string) => Promise<boolean>>(async () => false)

  // Latest state for callbacks that stay identity-stable (shortcuts, guards).
  const tabsRef = useRef(tabs)
  tabsRef.current = tabs
  const activeTabPathRef = useRef(activeTabPath)
  activeTabPathRef.current = activeTabPath

  /** Live document for a tab: mounted editor first, flushed snapshot second. */
  const readLiveDoc = useCallback((tab: EditorTab): string => {
    const reader = docReaders.current.get(tab.path)
    const live = reader?.()
    return live !== undefined ? live : tab.content
  }, [])

  /** Copy the mounted editor's document into tab.content (one state update). */
  const flushTabDoc = useCallback((path: string) => {
    const reader = docReaders.current.get(path)
    if (!reader) return
    const live = reader()
    if (live === undefined) return
    setTabs((prev) => {
      const tab = prev.find((t) => t.path === path)
      if (!tab || tab.content === live) return prev
      return prev.map((t) => (t.path === path ? { ...t, content: live } : t))
    })
  }, [])

  const doClose = useCallback((path: string) => {
    const current = tabsRef.current
    const idx = current.findIndex((t) => t.path === path)
    if (idx === -1) return
    const remaining = current.filter((t) => t.path !== path)
    setTabs(remaining)
    setActiveTabPath((prev) =>
      prev === path ? remaining[Math.min(idx, remaining.length - 1)]?.path ?? null : prev
    )
    docReaders.current.delete(path)
    // The Editor parks its state during the unmount commit that follows this
    // update; evict on the next tick so the closed tab does not stay cached.
    setTimeout(() => evictEditorState(path), POST_UNMOUNT_EVICT_DELAY_MS)
  }, [])

  const openFile = useCallback(
    async (filePath: string) => {
      const previousPath = activeTabPathRef.current
      if (previousPath && previousPath !== filePath) flushTabDoc(previousPath)
      setActiveTabPath(filePath)
      const existing = tabsRef.current.find((t) => t.path === filePath)
      if (existing) {
        // Reload in place only when the file changed on disk.
        if (existing.is_dirty || existing.is_image) return
        try {
          const mtime = await getFileMtime(filePath)
          if (mtime === existing.mtime) return
          const fresh = await readFileAsTab(filePath)
          setTabs((prev) =>
            prev.map((t) =>
              t.path === filePath && !t.is_dirty
                ? { ...t, content: fresh.content, mtime: fresh.mtime, rev: (t.rev ?? 0) + 1 }
                : t
            )
          )
        } catch {}
        return
      }
      try {
        const tab = await readFileAsTab(filePath)
        setTabs((prev) => (prev.some((t) => t.path === filePath) ? prev : [...prev, tab]))
      } catch (e) {
        toast(t("editor.openFailed", { error: String(e) }), "error")
      }
    },
    [flushTabDoc, t]
  )

  const closeTab = useCallback(
    (path: string, e?: React.MouseEvent) => {
      e?.stopPropagation()
      const tab = tabsRef.current.find((t) => t.path === path)
      if (tab?.is_dirty) {
        setPendingClosePath(path)
        return
      }
      doClose(path)
    },
    [doClose]
  )

  const dirtyPendingTab = pendingClosePath
    ? tabs.find((t) => t.path === pendingClosePath) || null
    : null

  const saveTab = useCallback(
    async (path: string, content: string): Promise<boolean> => {
      try {
        await invoke("write_text_file", { path, content })
        const mtime = await getFileMtime(path).catch(() => 0)
        setTabs((prev) => prev.map((t) => (t.path === path ? { ...t, content, is_dirty: false, mtime } : t)))
        return true
      } catch (e) {
        toast(t("editor.saveFailed", { error: String(e) }), "error")
        return false
      }
    },
    [t]
  )

  /** Save a tab using its live document (never a stale content snapshot). */
  const handleSave = useCallback(
    async (path: string): Promise<boolean> => {
      const tab = tabsRef.current.find((t) => t.path === path)
      if (!tab) return false
      return saveTab(path, readLiveDoc(tab))
    },
    [readLiveDoc, saveTab]
  )
  handleSaveRef.current = handleSave

  const resolvePendingClose = useCallback(
    async (save: boolean) => {
      const path = pendingClosePath
      if (!path) return
      // Keep the dialog open when the save fails so the user can retry or discard.
      if (save && !(await handleSaveRef.current(path))) return
      setPendingClosePath(null)
      doClose(path)
    },
    [pendingClosePath, doClose]
  )

  const cancelPendingClose = useCallback(() => setPendingClosePath(null), [])

  const handleDirtyChange = useCallback((path: string, dirty: boolean) => {
    setTabs((prev) => {
      const tab = prev.find((t) => t.path === path)
      if (!tab || tab.is_dirty === dirty) return prev
      return prev.map((t) => (t.path === path ? { ...t, is_dirty: dirty } : t))
    })
  }, [])

  const switchTab = useCallback(
    async (path: string) => {
      const previousPath = activeTabPathRef.current
      if (previousPath && previousPath !== path) flushTabDoc(previousPath)
      setActiveTabPath(path)
      const tab = tabsRef.current.find((t) => t.path === path)
      if (!tab || tab.is_dirty || tab.is_image) return
      try {
        const mtime = await getFileMtime(path)
        if (mtime === tab.mtime) return
        const fresh = await readFileAsTab(path)
        setTabs((prev) =>
          prev.map((t) =>
            t.path === path && !t.is_dirty
              ? { ...t, content: fresh.content, mtime: fresh.mtime, rev: (t.rev ?? 0) + 1 }
              : t
          )
        )
      } catch {}
    },
    [flushTabDoc]
  )

  const refreshAllTabs = useCallback(async () => {
    for (const tab of tabsRef.current) {
      if (tab.is_dirty || tab.is_image) continue
      try {
        const fresh = await readFileAsTab(tab.path)
        setTabs((prev) =>
          prev.map((t) =>
            t.path === tab.path
              ? { ...t, content: fresh.content, mtime: fresh.mtime, rev: (t.rev ?? 0) + 1 }
              : t
          )
        )
      } catch {}
    }
  }, [])

  const handleFileDeleted = useCallback((path: string) => {
    const current = tabsRef.current
    const remaining = current.filter((t) => t.path !== path && !t.path.startsWith(path + "/"))
    if (remaining.length === current.length) return
    setTabs(remaining)
    setActiveTabPath((prev) => {
      if (prev === path || prev?.startsWith(path + "/")) {
        return remaining[remaining.length - 1]?.path ?? null
      }
      return prev
    })
    for (const tab of current) {
      if (tab.path === path || tab.path.startsWith(path + "/")) docReaders.current.delete(tab.path)
    }
    setTimeout(() => evictEditorState(path, { subtree: true }), POST_UNMOUNT_EVICT_DELAY_MS)
  }, [])

  const handleFileRenamed = useCallback((oldPath: string, newPath: string) => {
    // Capture live documents before the Editor re-mounts under the new paths.
    const remap = (path: string): string => {
      if (path === oldPath) return newPath
      if (path.startsWith(oldPath + "/")) return newPath + path.slice(oldPath.length)
      return path
    }
    const liveDocs = new Map<string, string>()
    for (const tab of tabsRef.current) {
      const reader = docReaders.current.get(tab.path)
      const live = reader?.()
      if (live !== undefined) liveDocs.set(tab.path, live)
    }
    setTabs((prev) =>
      prev.map((t) => {
        const updatedPath = remap(t.path)
        if (updatedPath === t.path) return t
        return {
          ...t,
          path: updatedPath,
          name: updatedPath.split("/").pop() || updatedPath,
          content: liveDocs.get(t.path) ?? t.content,
        }
      })
    )
    setActiveTabPath((prev) => (prev ? remap(prev) : prev))
    for (const path of liveDocs.keys()) docReaders.current.delete(path)
    setTimeout(() => evictEditorState(oldPath, { subtree: true }), POST_UNMOUNT_EVICT_DELAY_MS)
  }, [])

  /**
   * Unsaved-changes guard shared by back navigation, page navigation, palette
   * skill switching and ConfigPanel tool switching. Runs `proceed` immediately
   * when no tab is dirty, otherwise shows the dialog through `pendingLeaveTabs`.
   */
  const requestLeave = useCallback((proceed: () => void) => {
    const dirty = tabsRef.current.filter((t) => t.is_dirty)
    if (dirty.length === 0) {
      proceed()
      return
    }
    setPendingLeave({ tabs: dirty, proceed })
  }, [])

  const resolveLeave = useCallback(
    async (save: boolean) => {
      const pending = pendingLeave
      if (!pending) return
      setPendingLeave(null)
      if (save) {
        for (const tab of pending.tabs) {
          if (!(await handleSaveRef.current(tab.path))) {
            // Restore the dialog: a failed save must not turn into data loss.
            setPendingLeave(pending)
            return
          }
        }
      }
      pending.proceed()
    },
    [pendingLeave]
  )

  const cancelLeave = useCallback(() => setPendingLeave(null), [])

  const clearTabs = useCallback(() => {
    const paths = tabsRef.current.map((t) => t.path)
    setTabs([])
    setActiveTabPath(null)
    setPendingClosePath(null)
    setPendingLeave(null)
    for (const path of paths) {
      docReaders.current.delete(path)
      setTimeout(() => evictEditorState(path), POST_UNMOUNT_EVICT_DELAY_MS)
    }
  }, [])

  /** Editor registers its live-document reader here (null on unmount). */
  const registerDocSource = useCallback((path: string, reader: DocReader | null) => {
    if (reader) docReaders.current.set(path, reader)
    else docReaders.current.delete(path)
  }, [])

  // Cmd+W is registered by the active editor pane only (pages stay mounted
  // hidden), so it never closes tabs in two panes at once.
  useEffect(() => {
    if (!active || tabs.length === 0) return
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "w") {
        e.preventDefault()
        const path = activeTabPathRef.current
        if (path) closeTab(path)
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [active, tabs.length, closeTab])

  const activeTab = tabs.find((t) => t.path === activeTabPath)
  const dirtyTabs = tabs.filter((t) => t.is_dirty)

  return {
    tabs,
    activeTabPath,
    activeTab,
    dirtyTabs,
    openFile,
    closeTab,
    switchTab,
    handleDirtyChange,
    refreshAllTabs,
    handleSave,
    handleFileDeleted,
    handleFileRenamed,
    clearTabs,
    registerDocSource,
    setActiveTabPath,
    dirtyPendingTab,
    resolvePendingClose,
    cancelPendingClose,
    pendingLeaveTabs: pendingLeave?.tabs ?? null,
    requestLeave,
    resolveLeave,
    cancelLeave,
  }
}
