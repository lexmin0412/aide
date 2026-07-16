import { useState } from "react"
import { SkillGrid } from "./components/SkillGrid"
import { SkillDetail } from "./components/SkillDetail"
import { ConfigPanel } from "./components/ConfigPanel"
import { MCPPage } from "./components/MCPPage"
import type { SkillInfo } from "./types"
import "./App.css"

type Page = "skills" | "configs" | "mcp"

function App() {
  const [page, setPage] = useState<Page>("skills")
  const [selectedSkill, setSelectedSkill] = useState<SkillInfo | null>(null)

  return (
    <div className="app">
      <div className="app-nav">
        <div className="app-nav-brand">aide</div>
        <div className="app-nav-tabs">
          <button className={`app-nav-tab ${page === "skills" ? "active" : ""}`}
            onClick={() => { setPage("skills"); setSelectedSkill(null) }}>
            Skills
          </button>
          <button className={`app-nav-tab ${page === "mcp" ? "active" : ""}`}
            onClick={() => setPage("mcp")}>
            MCP
          </button>
          <button className={`app-nav-tab ${page === "configs" ? "active" : ""}`}
            onClick={() => setPage("configs")}>
            Configs
          </button>
        </div>
      </div>
      <div className="app-page">
        <div className={`app-panel ${page === "skills" ? "active" : ""}`}>
          {!selectedSkill ? (
            <SkillGrid onSelectSkill={setSelectedSkill} />
          ) : (
            <SkillDetail skill={selectedSkill} onBack={() => setSelectedSkill(null)} />
          )}
        </div>
        <div className={`app-panel ${page === "mcp" ? "active" : ""}`}>
          <MCPPage />
        </div>
        <div className={`app-panel ${page === "configs" ? "active" : ""}`}>
          <ConfigPanel />
        </div>
      </div>
    </div>
  )
}

export default App
