import { useState, useCallback, useEffect } from "react"
import { invoke } from "@tauri-apps/api/core"
import type { SkillInfo, EditorTab } from "../types"
import { FileTree } from "./FileTree"
import { Editor } from "./Editor"

interface SkillDetailProps {
  skill: SkillInfo
  onBack: () => void
}

export function SkillDetail({ skill, onBack }: SkillDetailProps) {
  const [tabs, setTabs] = useState<EditorTab[]>([])
  const [activeTabPath, setActiveTabPath] = useState<string | null>(null)
  const rootPath = skill.is_symlink && skill.target_path ? skill.target_path : skill.path

  const openFile = useCallback(
    async (path: string) => {
      const name = path.split("/").pop() || path
      try {
        const content = await invoke<string>("read_text_file", { path })
        setTabs((prev) => {
          if (prev.some((t) => t.path === path)) return prev
          return [...prev, { path, name, content, is_dirty: false, language: "plain" }]
        })
        setActiveTabPath(path)
      } catch (e) {
        console.error("Failed to read file:", e)
      }
    },
    []
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

  useEffect(() => {
    openFile(rootPath + "/SKILL.md")
  }, [skill.path])

  return (
    <div className="skill-detail">
      <div className="skill-detail-header">
        <button className="skill-back-btn" onClick={onBack}>
          {"\u2190"} Back
        </button>
        <div className="skill-detail-info">
          <span className="skill-detail-name">
            {skill.is_symlink ? "\uD83D\uDD17 " : ""}{skill.display_name}
          </span>
          <span className="skill-detail-path">{skill.path}</span>
        </div>
      </div>
      <div className="skill-detail-body">
        <div className="skill-detail-sidebar">
          <FileTree
            rootPath={rootPath}
            onSelectFile={openFile}
            selectedPath={activeTabPath}
          />
        </div>
        <div className="skill-detail-content">
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
          <div className="skill-detail-editor">
            {activeTab ? (
              <Editor tab={activeTab} onChange={handleEditorChange} onSave={handleSave} />
            ) : (
              <div className="skill-detail-empty">
                <p>Select a file from the sidebar to preview</p>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
