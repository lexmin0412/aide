import { useState, useCallback, useEffect, useRef } from "react"
import { invoke } from "@tauri-apps/api/core"
import { RefreshCw } from "lucide-react"
import { Button } from "@/components/ui/button"
import { FileTree } from "./FileTree"
import type { FileTreeHandle } from "./FileTree"
import { Editor } from "./Editor"
import type { ToolInfo, EditorTab } from "../types"

export function ConfigPanel() {
  const [tools, setTools] = useState<ToolInfo[]>([])
  const [activeTool, setActiveTool] = useState<string | null>(null)
  const [homeDir, setHomeDir] = useState("")
  const [tabs, setTabs] = useState<EditorTab[]>([])
  const [activeTabPath, setActiveTabPath] = useState<string | null>(null)
  const treeRef = useRef<FileTreeHandle>(null)

  useEffect(() => {
    invoke<ToolInfo[]>("list_tools").then((list) => {
      setTools(list)
      if (list.length > 0) setActiveTool(list[0].key)
    })
    invoke<string>("get_home_dir").then(setHomeDir)
  }, [])

  const activeToolInfo = tools.find((t) => t.key === activeTool)
  const rootPath = activeToolInfo && homeDir ? homeDir + "/" + activeToolInfo.detect_dir : ""

  const openFile = useCallback(
    async (filePath: string) => {
      const existing = tabs.find((t) => t.path === filePath)
      if (existing) {
        setActiveTabPath(filePath)
        if (!existing.is_dirty) {
          try {
            const content = await invoke<string>("read_text_file", { path: filePath })
            setTabs((prev) => prev.map((t) => (t.path === filePath ? { ...t, content } : t)))
          } catch {}
        }
        return
      }
      const name = filePath.split("/").pop() || filePath
      try {
        const content = await invoke<string>("read_text_file", { path: filePath })
        setTabs((prev) => [...prev, { path: filePath, name, content, is_dirty: false, language: "plain" }])
        setActiveTabPath(filePath)
      } catch (e) { console.error("Failed to read file:", e) }
    },
    [tabs]
  )

  const closeTab = useCallback(
    (path: string, e: React.MouseEvent) => {
      e.stopPropagation()
      const idx = tabs.findIndex((t) => t.path === path)
      setTabs((prev) => prev.filter((t) => t.path !== path))
      if (activeTabPath === path) {
        const remaining = tabs.filter((t) => t.path !== path)
        setActiveTabPath(remaining[Math.min(idx, remaining.length - 1)]?.path || null)
      }
    },
    [tabs, activeTabPath]
  )

  const handleEditorChange = useCallback((path: string, content: string) => {
    setTabs((prev) => prev.map((t) => (t.path === path ? { ...t, content, is_dirty: true } : t)))
  }, [])

  const switchTab = useCallback(
    async (path: string) => {
      setActiveTabPath(path)
      const tab = tabs.find((t) => t.path === path)
      if (tab && !tab.is_dirty) {
        try {
          const content = await invoke<string>("read_text_file", { path })
          setTabs((prev) => prev.map((t) => (t.path === path ? { ...t, content } : t)))
        } catch {}
      }
    },
    [tabs]
  )

  const refreshAllTabs = useCallback(async () => {
    for (const tab of tabs) {
      if (!tab.is_dirty) {
        try {
          const content = await invoke<string>("read_text_file", { path: tab.path })
          setTabs((prev) => prev.map((t) => (t.path === tab.path ? { ...t, content } : t)))
        } catch {}
      }
    }
  }, [tabs])

  const handleRefresh = useCallback(async () => {
    await treeRef.current?.refresh()
    await refreshAllTabs()
  }, [refreshAllTabs])

  const handleSave = useCallback(
    async (path: string, content: string) => {
      try {
        await invoke("write_text_file", { path, content })
        setTabs((prev) => prev.map((t) => (t.path === path ? { ...t, content, is_dirty: false } : t)))
      } catch (e) { console.error("Failed to save file:", e) }
    },
    []
  )

  const handleFileDeleted = useCallback(
    (path: string) => {
      const idx = tabs.findIndex((t) => t.path === path)
      if (idx === -1) return
      setTabs((prev) => prev.filter((t) => t.path !== path))
      if (activeTabPath === path) {
        const remaining = tabs.filter((t) => t.path !== path)
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

  const activeTab = tabs.find((t) => t.path === activeTabPath)

  return (
    <div className="h-full flex flex-col">
      <div className="flex items-center gap-2 px-4 py-2 border-b border-border bg-card/30 shrink-0 overflow-x-auto">
        <span className="text-[10px] text-muted-foreground uppercase tracking-wider shrink-0">Tools</span>
        {tools.map((t) => (
          <Button
            key={t.key}
            variant={activeTool === t.key ? "secondary" : "ghost"}
            size="sm"
            className="h-6 text-xs shrink-0"
            onClick={() => { setActiveTool(t.key); setActiveTabPath(null); setTabs([]) }}
          >
            {t.name}
          </Button>
        ))}
      </div>
      <div className="flex-1 flex overflow-hidden">
        {activeToolInfo && rootPath ? (
          <>
            <div className="w-64 bg-card/40 border-r border-border overflow-y-auto shrink-0">
              <div className="flex items-center justify-between px-3 py-2 text-[10px] text-muted-foreground font-mono border-b border-border">
                <span className="truncate">{activeToolInfo.detect_dir}</span>
                <button
                  className="p-0.5 text-muted-foreground hover:text-foreground rounded hover:bg-card/60 shrink-0 ml-1"
                  onClick={handleRefresh}
                  title="Refresh"
                >
                  <RefreshCw size={12} />
                </button>
              </div>
              <FileTree ref={treeRef} rootPath={rootPath} onSelectFile={openFile} selectedPath={activeTabPath} onFileDeleted={handleFileDeleted} onFileRenamed={handleFileRenamed} />
            </div>
            <div className="flex-1 flex flex-col overflow-hidden">
              {tabs.length > 0 && (
                <div className="flex bg-card/30 border-b border-border overflow-x-auto h-9 shrink-0">
                  {tabs.map((tab) => (
                    <div
                      key={tab.path}
                      className={`flex items-center gap-1.5 px-3 h-full text-xs cursor-pointer border-r border-border whitespace-nowrap ${
                        activeTabPath === tab.path ? "bg-background border-b-2 border-b-foreground" : "bg-card/50 hover:bg-card"
                      }`}
                      onClick={() => switchTab(tab.path)}
                    >
                      <span className="max-w-[150px] truncate">{tab.name}</span>
                      {tab.is_dirty && <span className="text-blue-400 font-bold">*</span>}
                      <span className="text-muted-foreground hover:text-foreground ml-1" onClick={(e) => closeTab(tab.path, e)}>×</span>
                    </div>
                  ))}
                </div>
              )}
              <div className="flex-1 overflow-hidden">
                {activeTab ? (
                  <Editor tab={activeTab} onChange={handleEditorChange} onSave={handleSave} />
                ) : (
                  <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
                    Select a file from the sidebar
                  </div>
                )}
              </div>
            </div>
          </>
        ) : (
          <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">Loading...</div>
        )}
      </div>
    </div>
  )
}
