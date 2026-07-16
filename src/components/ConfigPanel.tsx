import { useState, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import type { ToolInfo, EditorTab } from "../types"
import { FileTree } from "./FileTree"
import { Editor } from "./Editor"

export function ConfigPanel() {
  const [tools, setTools] = useState<ToolInfo[]>([])
  const [activeTool, setActiveTool] = useState<string | null>(null)
  const [tabs, setTabs] = useState<EditorTab[]>([])
  const [activeTabPath, setActiveTabPath] = useState<string | null>(null)

  useEffect(() => {
    invoke<ToolInfo[]>("list_tools").then((list) => {
      setTools(list)
      if (list.length > 0) setActiveTool(list[0].key)
    })
  }, [])

  const [homeDir, setHomeDir] = useState("")

  useEffect(() => {
    invoke<string>("get_home_dir").then(setHomeDir)
  }, [])

  const activeToolInfo = tools.find((t) => t.key === activeTool)
  const rootPath = activeToolInfo && homeDir ? homeDir + "/" + activeToolInfo.detect_dir : ""

  const openFile = useCallback(
    async (filePath: string) => {
      const existing = tabs.find((t) => t.path === filePath)
      if (existing) {
        setActiveTabPath(filePath)
        return
      }
      const name = filePath.split("/").pop() || filePath
      try {
        const content = await invoke<string>("read_text_file", { path: filePath })
        setTabs((prev) => [...prev, { path: filePath, name, content, is_dirty: false, language: "plain" }])
        setActiveTabPath(filePath)
      } catch (e) {
        console.error("Failed to read file:", e)
      }
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
    setTabs((prev) =>
      prev.map((t) => (t.path === path ? { ...t, content, is_dirty: true } : t))
    )
  }, [])

  const handleSave = useCallback(
    async (path: string, content: string) => {
      try {
        await invoke("write_text_file", { path, content })
        setTabs((prev) =>
          prev.map((t) => (t.path === path ? { ...t, content, is_dirty: false } : t))
        )
      } catch (e) {
        console.error("Failed to save file:", e)
      }
    },
    []
  )

  const activeTab = tabs.find((t) => t.path === activeTabPath)

  return (
    <div className="config-panel">
      <div className="config-toolbar">
        <div className="config-toolbar-label">Tools</div>
        <div className="config-tool-list">
          {tools.map((tool) => (
            <button
              key={tool.key}
              className={`config-tool-btn ${activeTool === tool.key ? "active" : ""}`}
              onClick={() => {
                setActiveTool(tool.key)
                setActiveTabPath(null)
              }}
            >
              {tool.name}
            </button>
          ))}
        </div>
      </div>
      <div className="config-body">
        {activeToolInfo && (
          <>
            <div className="config-sidebar">
              <div className="config-sidebar-header">
                {activeToolInfo.detect_dir}
              </div>
              <FileTree
                rootPath={rootPath}
                onSelectFile={openFile}
                selectedPath={activeTabPath}
              />
            </div>
            <div className="config-content">
              {tabs.length > 0 && (
                <div className="tab-bar">
                  {tabs.map((tab) => (
                    <div
                      key={tab.path}
                      className={`tab ${activeTabPath === tab.path ? "active" : ""}`}
                      onClick={() => setActiveTabPath(tab.path)}
                    >
                      <span className="tab-name">{tab.name}</span>
                      {tab.is_dirty && <span className="tab-dirty">*</span>}
                      <span className="tab-close" onClick={(e) => closeTab(tab.path, e)}>x</span>
                    </div>
                  ))}
                </div>
              )}
              <div className="config-editor">
                {activeTab ? (
                  <Editor tab={activeTab} onChange={handleEditorChange} onSave={handleSave} />
                ) : (
                  <div className="config-empty">Select a file to preview</div>
                )}
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  )
}
