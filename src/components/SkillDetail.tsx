import { useState, useCallback, useEffect, useRef } from "react"
import { invoke } from "@tauri-apps/api/core"
import { RefreshCw } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { FileTree } from "./FileTree"
import type { FileTreeHandle } from "./FileTree"
import { Editor } from "./Editor"
import { readFileAsTab } from "@/lib/fileUtils"
import type { SkillInfo, EditorTab } from "../types"

interface SkillDetailProps {
  skill: SkillInfo
  onBack: () => void
}

export default function SkillDetail({ skill, onBack }: SkillDetailProps) {
  const [tabs, setTabs] = useState<EditorTab[]>([])
  const [activeTabPath, setActiveTabPath] = useState<string | null>(null)
  const rootPath = skill.is_symlink && skill.target_path ? skill.target_path : skill.path
  const treeRef = useRef<FileTreeHandle>(null)

  const initRef = useRef(false)
  useEffect(() => {
    if (initRef.current) return
    initRef.current = true
    const skillMd = rootPath + "/SKILL.md"
    invoke<string>("read_text_file", { path: skillMd })
      .then((content) => {
        const name = skillMd.split("/").pop() || skillMd
        setTabs((prev) => [...prev, { path: skillMd, name, content, is_dirty: false, language: "plain" }])
        setActiveTabPath(skillMd)
      })
      .catch(() => {})
  }, [])

  const openFile = useCallback(
    async (filePath: string) => {
      const existing = tabs.find((t) => t.path === filePath)
      if (existing) {
        setActiveTabPath(filePath)
        if (!existing.is_dirty && !existing.is_image) {
          try {
            const tab = await readFileAsTab(filePath)
            setTabs((prev) => prev.map((t) => (t.path === filePath ? { ...t, content: tab.content } : t)))
          } catch {}
        }
        return
      }
      try {
        const tab = await readFileAsTab(filePath)
        setTabs((prev) => [...prev, tab])
        setActiveTabPath(filePath)
      } catch (e) { console.error("Failed to read file:", e) }
    },
    [tabs]
  )

  const closeTab = useCallback(
    (path: string, e?: React.MouseEvent) => {
      e?.stopPropagation()
      const idx = tabs.findIndex((t) => t.path === path)
      setTabs((prev) => prev.filter((t) => t.path !== path))
      if (activeTabPath === path) {
        const remaining = tabs.filter((t) => t.path !== path)
        setActiveTabPath(remaining[Math.min(idx, remaining.length - 1)]?.path || null)
      }
    },
    [tabs, activeTabPath]
  )

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
      if (tab && !tab.is_dirty && !tab.is_image) {
        try {
          const updated = await readFileAsTab(path)
          setTabs((prev) => prev.map((t) => (t.path === path ? { ...t, content: updated.content } : t)))
        } catch {}
      }
    },
    [tabs]
  )

  const refreshAllTabs = useCallback(async () => {
    for (const tab of tabs) {
      if (!tab.is_dirty && !tab.is_image) {
        try {
          const updated = await readFileAsTab(tab.path)
          setTabs((prev) => prev.map((t) => (t.path === tab.path ? { ...t, content: updated.content } : t)))
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
      <div className="flex items-center gap-3 px-5 py-2.5 border-b border-border bg-card/40">
        <Button variant="ghost" size="sm" onClick={onBack}>← Back</Button>
        <div className="flex flex-col min-w-0">
          <div className="text-sm font-medium truncate">
            {skill.is_symlink && "🔗 "}{skill.display_name}
          </div>
          <div className="text-[10px] text-muted-foreground font-mono truncate">{skill.path}</div>
        </div>
        {skill.is_symlink && skill.target_path && (
          <Badge variant="outline" className="font-mono text-[10px] ml-auto">→ {skill.target_path}</Badge>
        )}
      </div>
      <div className="flex-1 flex overflow-hidden">
        <div className="w-64 bg-card/40 border-r border-border overflow-y-auto shrink-0">
          <div className="flex items-center justify-between px-3 py-2 text-[10px] text-muted-foreground font-mono border-b border-border">
            <span className="truncate">{rootPath.split("/").pop()}</span>
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
      </div>
    </div>
  )
}
