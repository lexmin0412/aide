import { useState, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import SkillGrid from "./components/SkillGrid"
import SkillDetail from "./components/SkillDetail"
import { ConfigPanel } from "./components/ConfigPanel"
import MCPPage from "./components/MCPPage"
import { UpdateDialog } from "./components/UpdateDialog"
import type { SkillInfo } from "./types"
import "./App.css"

type Page = "skills" | "mcp" | "configs"

export default function App() {
  const [page, setPage] = useState<Page>("skills")
  const [selectedSkill, setSelectedSkill] = useState<SkillInfo | null>(null)
  const [gridKey, setGridKey] = useState(0)
  const [showUpdate, setShowUpdate] = useState(false)

  const tabs: { key: Page; label: string }[] = [
    { key: "skills", label: "Skills" },
    { key: "mcp", label: "MCP" },
    { key: "configs", label: "Configs" },
  ]

  const handleSkillDeleted = useCallback(async (path: string) => {
    await invoke("delete_entry", { path })
    setSelectedSkill(null)
    setGridKey((k) => k + 1)
  }, [])

  return (
    <div className="h-full flex flex-col bg-background text-foreground">
      <div className="flex items-center h-10 px-4 border-b border-border bg-card/40 shrink-0 gap-6">
        <div className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">aide</div>
        <div className="flex gap-1">
          {tabs.map((t) => (
            <button
              key={t.key}
              onClick={() => { setPage(t.key); if (t.key === "skills") setSelectedSkill(null) }}
              className={`px-3 h-7 text-xs rounded-md transition-colors ${
                page === t.key
                  ? "bg-secondary text-foreground"
                  : "text-muted-foreground hover:text-foreground hover:bg-secondary/50"
              }`}
            >
              {t.label}
            </button>
          ))}
        </div>
        <button
          onClick={() => setShowUpdate(true)}
          className="ml-auto text-muted-foreground hover:text-foreground transition-colors"
          title="Check for updates"
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
            <polyline points="7 10 12 15 17 10" />
            <line x1="12" y1="15" x2="12" y2="3" />
          </svg>
        </button>
        <a
          href="https://github.com/lexmin0412/aide"
          target="_blank"
          rel="noopener noreferrer"
          className="text-muted-foreground hover:text-foreground transition-colors"
          title="GitHub"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
          </svg>
        </a>
      </div>
      {showUpdate && <UpdateDialog onClose={() => setShowUpdate(false)} />}
      <div className="flex-1 relative overflow-hidden">
        <div className={`absolute inset-0 ${page === "skills" ? "" : "hidden"}`}>
          {selectedSkill ? (
            <SkillDetail skill={selectedSkill} onBack={() => setSelectedSkill(null)} onDelete={handleSkillDeleted} />
          ) : (
            <SkillGrid key={gridKey} onSelectSkill={setSelectedSkill} />
          )}
        </div>
        <div className={`absolute inset-0 ${page === "mcp" ? "" : "hidden"}`}>
          <MCPPage />
        </div>
        <div className={`absolute inset-0 ${page === "configs" ? "" : "hidden"}`}>
          <ConfigPanel />
        </div>
      </div>
    </div>
  )
}
