import { useState } from "react"
import SkillGrid from "./components/SkillGrid"
import SkillDetail from "./components/SkillDetail"
import { ConfigPanel } from "./components/ConfigPanel"
import MCPPage from "./components/MCPPage"
import type { SkillInfo } from "./types"
import "./App.css"

type Page = "skills" | "mcp" | "configs"

export default function App() {
  const [page, setPage] = useState<Page>("skills")
  const [selectedSkill, setSelectedSkill] = useState<SkillInfo | null>(null)

  const tabs: { key: Page; label: string }[] = [
    { key: "skills", label: "Skills" },
    { key: "mcp", label: "MCP" },
    { key: "configs", label: "Configs" },
  ]

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
      </div>
      <div className="flex-1 relative overflow-hidden">
        <div className={`absolute inset-0 ${page === "skills" ? "" : "hidden"}`}>
          {selectedSkill ? (
            <SkillDetail skill={selectedSkill} onBack={() => setSelectedSkill(null)} />
          ) : (
            <SkillGrid onSelectSkill={setSelectedSkill} />
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
