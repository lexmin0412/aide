import { useState, useCallback, useEffect } from "react"
import { useFileSystem } from "./hooks/useFileSystem"
import { FileTree } from "./components/FileTree"
import { FileList } from "./components/FileList"
import { Editor } from "./components/Editor"
import type { FileEntry, EditorTab } from "./types"
import "./App.css"

function App() {
  const fs = useFileSystem()
  const [selectedPath, setSelectedPath] = useState<string | null>(null)
  const [tabs, setTabs] = useState<EditorTab[]>([])
  const [activeTabPath, setActiveTabPath] = useState<string | null>(null)

  const openFile = useCallback(
    async (path: string) => {
      const existing = tabs.find((t) => t.path === path)
      if (existing) {
        setActiveTabPath(path)
        return
      }

      const name = path.split("/").pop() || path
      const ext = name.split(".").pop()?.toLowerCase() || ""
      let language = "plain"
      if (["json", "jsonc"].includes(ext)) language = "json"
      else if (["md", "markdown"].includes(ext)) language = "markdown"

      try {
        const content = await fs.readFile(path)
        const newTab: EditorTab = { path, name, content, is_dirty: false, language }
        setTabs((prev) => [...prev, newTab])
        setActiveTabPath(path)
      } catch (e) {
        console.error("Failed to read file:", e)
      }
    },
    [tabs, fs]
  )

  const closeTab = useCallback(
    (path: string, e: React.MouseEvent) => {
      e.stopPropagation()
      const idx = tabs.findIndex((t) => t.path === path)
      setTabs((prev) => prev.filter((t) => t.path !== path))
      if (activeTabPath === path) {
        const newTabs = tabs.filter((t) => t.path !== path)
        setActiveTabPath(newTabs[Math.min(idx, newTabs.length - 1)]?.path || null)
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
        await fs.writeFile(path, content)
        setTabs((prev) =>
          prev.map((t) => (t.path === path ? { ...t, content, is_dirty: false } : t))
        )
      } catch (e) {
        console.error("Failed to save file:", e)
      }
    },
    [fs]
  )

  const handleSelectFile = useCallback(
    (entry: FileEntry) => {
      setSelectedPath(entry.path)
      openFile(entry.path)
    },
    [openFile]
  )

  const handleNavigateDir = useCallback(
    async (path: string) => {
      setSelectedPath(null)
      await fs.listDir(path)
    },
    [fs]
  )

  const handleNavigateUp = useCallback(() => {
    const parent = fs.currentDir.substring(0, fs.currentDir.lastIndexOf("/")) || "/"
    handleNavigateDir(parent)
  }, [fs.currentDir, handleNavigateDir])

  const activeTab = tabs.find((t) => t.path === activeTabPath)

  useEffect(() => {
    fs.init().then((home) => {
      const target = home + "/.agents/skills"
      if (home) {
        fs.listDir(target)
          .catch(() => fs.listDir(home))
      }
    })
  }, [])

  return (
    <div className="app">
      <div className="sidebar">
        <div className="sidebar-header">aide</div>
        <FileTree
          fs={fs}
          onSelectDir={(path) => fs.listDir(path)}
          onSelectFile={openFile}
          selectedPath={selectedPath}
        />
      </div>
      <div className="main">
        <div className="tab-bar">
          {tabs.map((tab) => (
            <div
              key={tab.path}
              className={`tab ${activeTabPath === tab.path ? "active" : ""}`}
              onClick={() => setActiveTabPath(tab.path)}
            >
              <span className="tab-name">{tab.name}</span>
              {tab.is_dirty && <span className="tab-dirty">*</span>}
              <span className="tab-close" onClick={(e) => closeTab(tab.path, e)}>
                x
              </span>
            </div>
          ))}
        </div>
        <div className="content">
          {activeTab ? (
            <Editor
              tab={activeTab}
              onChange={handleEditorChange}
              onSave={handleSave}
            />
          ) : (
            <div className="file-list-panel">
              <FileList
                entries={fs.entries}
                currentDir={fs.currentDir}
                onNavigateUp={handleNavigateUp}
                onSelectFile={handleSelectFile}
                onSelectDir={(entry) => {
                  setSelectedPath(entry.path)
                  handleNavigateDir(entry.path)
                }}
                selectedPath={selectedPath}
              />
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

export default App
